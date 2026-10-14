//! 给上游客户端装 / 卸本工具的 hook（P15 · S3）。
//!
//! ## 它做什么
//!
//! 往客户端 `settings.json` 的 `hooks` 字段里，为每个关心的事件追加一条
//! 「调用本工具、把事件转给我们」的命令。客户端（Claude Code 兼容的 command hooks）
//! 会在每次会话 / 工具调用时同步执行它。
//!
//! ## ★ 三条纪律（`P15-SPIKE-RESULTS §6 D41`）
//!
//! 1. **不新造写入路径**：一律走 [`super::client_settings::patch_settings`] ——
//!    它自带**格式指纹保持**（不重排用户的 JSON 排版）、**写前自动备份**、
//!    **进程守卫**（客户端在跑就拒写，否则会被它的内存态覆盖）。
//!    本模块只负责算出「合并后的 `hooks` 应该长什么样」，落盘交给它。
//! 2. **合并不是覆盖**：用户可能已经装了别人的 hook。合并规则是
//!    「剔除**自己的**旧条目（幂等）→ 保留**别人的** → 追加自己的一条」，
//!    见 [`merge_hooks`]（纯函数，逐形态单测）。
//! 3. **可一键还原**：卸载 = 再合并一次、只是不追加（[`strip_our_hooks`]），
//!    且每一步都有 `client_settings` 的自动备份兜底。
//!
//! ## 为什么事件集里**没有** `PreCompact`
//!
//! [`super::agent_watch::HookKind`] 把 `PreCompact` 映射成 `Unknown` 并**整条丢弃**。
//! 注册一个我们明确不处理的事件，只会让客户端在每次上下文压缩时白起一个进程 ——
//! 所以**只注册状态机用得上的那 9 个**。

use std::path::Path;

use serde_json::{json, Map, Value};

use super::client_settings::{patch_settings, read_settings};

/// 要注册到客户端的事件。
///
/// 与客户端支持的事件集对齐，**只取状态机用得上的**（见模块头对 `PreCompact` 的说明）。
pub const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "Stop",
    "SessionEnd",
];

/// handler 的超时（秒）。
///
/// 与外部参考实现取同一个值：本命令只做「读 stdin + 转发一条本地 HTTP」，
/// 正常在毫秒级完成；3 s 是留给杀软 / 磁盘抖动这类一次性卡顿的余量。
pub const HOOK_TIMEOUT_SECONDS: u64 = 3;

/// 客户端侧显示的来源名（出现在客户端的 hook 列表里）。
pub const HOOK_STATUS_MESSAGE: &str = "Buddy Switch";

/// 构造要写进客户端配置的那条命令行。
///
/// ⚠️ 路径**必须加引号**：Windows 上安装目录常含空格（`C:\Program Files\...`），
/// 不加引号会被客户端按空格切成两个参数。
pub fn hook_command(exe: &Path, source: &str) -> String {
    format!("\"{}\" hook --source {source}", exe.display())
}

/// 这条命令是不是**我们自己**装的。
///
/// 判据锚在「可执行文件是我们的」+「这是 hook 子命令」两件事上，
/// **不锚**在路径上（用户可以把应用装到任何地方、也可以改名后再改名回来）。
pub fn is_our_hook(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    (lower.contains("buddyswitch") || lower.contains("buddy-switch"))
        && lower.contains(" hook")
        && lower.contains("--source")
}

/// 一个 handler 对象（写进 `hooks.<Event>[].hooks[]`）。
fn handler(command: &str, event: &str) -> Value {
    let mut handler = json!({
        "type": "command",
        "command": command,
        "timeout": HOOK_TIMEOUT_SECONDS,
        "statusMessage": HOOK_STATUS_MESSAGE,
    });
    // `SessionEnd` 是同步的：客户端在退出路径上等它跑完，此时起后台进程没意义。
    // 其余事件加 `async`，避免我们的转发拖慢客户端的工具调用（与外部参考实现同款）。
    if event != "SessionEnd" {
        handler["async"] = json!(true);
    }
    handler
}

/// 合并的**唯一**实现（纯函数）。
///
/// `command = Some(c)` ⇒ 「装」：剔除自己的旧条目后追加 `c`；
/// `command = None` ⇒ 「卸」：只剔除，不追加。
///
/// ## 逐形态处理（用户配置里真实存在的两种写法）
///
/// - **分组形态** `{ matcher, hooks: [...] }` —— 主流写法，且**可能一个分组里混装**
///   我们与别人的 hook ⇒ 必须逐条过滤、且过滤后**不能留下空分组**
///   （空分组会被客户端当成「匹配任何 matcher 但没有 handler」）。
/// - **裸 handler 形态** `{ type: "command", command, ... }` —— 客户端早期写法 / 手写产物。
///
/// 除这两种以外的值（字符串、数字、null）**原样保留**：我们不认识的形态宁可不动，
/// 也不能替用户删掉。
pub fn rewrite_hooks(existing: &Value, command: Option<&str>) -> Value {
    let mut doc = match existing {
        Value::Object(map) => Value::Object(map.clone()),
        _ => json!({}),
    };
    let mut hooks: Map<String, Value> = doc
        .get("hooks")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    for event in HOOK_EVENTS {
        let groups = hooks
            .get(*event)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut kept: Vec<Value> = Vec::new();
        for group in groups {
            match group {
                Value::Object(ref map) if map.get("hooks").is_some_and(Value::is_array) => {
                    let inner = map["hooks"].as_array().cloned().unwrap_or_default();
                    let survivors: Vec<Value> = inner
                        .into_iter()
                        .filter(|item| !is_our_hook(command_of(item)))
                        .collect();
                    // 过滤后为空 ⇒ 整个分组丢掉（见文档：空分组是有害的残留）
                    if !survivors.is_empty() {
                        let mut next = map.clone();
                        next.insert("hooks".to_string(), Value::Array(survivors));
                        kept.push(Value::Object(next));
                    }
                }
                Value::Object(ref map) => {
                    if !is_our_hook(map.get("command").and_then(Value::as_str).unwrap_or_default())
                    {
                        kept.push(group.clone());
                    }
                }
                other => kept.push(other.clone()),
            }
        }

        if let Some(command) = command {
            kept.push(json!({ "matcher": "", "hooks": [handler(command, event)] }));
        }

        if kept.is_empty() {
            // 卸载后该事件上一条 hook 都不剩 ⇒ 删掉这个键，而不是留一个 `[]`
            // （空数组在客户端配置里没有意义，还会让用户以为「装过东西」）。
            hooks.remove(*event);
        } else {
            hooks.insert((*event).to_string(), Value::Array(kept));
        }
    }

    if hooks.is_empty() {
        // 一条都不剩 ⇒ 把 `hooks` 键整个删掉（不留空对象）
        if let Some(map) = doc.as_object_mut() {
            map.remove("hooks");
        }
    } else {
        doc["hooks"] = Value::Object(hooks);
    }
    doc
}

/// 「装」：见 [`rewrite_hooks`]。
pub fn merge_hooks(existing: &Value, command: &str) -> Value {
    rewrite_hooks(existing, Some(command))
}

/// 「卸」：见 [`rewrite_hooks`]。
pub fn strip_our_hooks(existing: &Value) -> Value {
    rewrite_hooks(existing, None)
}

/// handler 里的命令串（两种形态通吃）。
fn command_of(item: &Value) -> &str {
    item.get("command").and_then(Value::as_str).unwrap_or_default()
}

/// 某个事件的 handler 里，有没有**我们**的条目。
fn event_has_ours(stored: &Value, event: &str) -> bool {
    let Some(groups) = stored
        .get("hooks")
        .and_then(Value::as_object)
        .and_then(|hooks| hooks.get(event))
        .and_then(Value::as_array)
    else {
        return false;
    };
    groups.iter().any(|group| match group {
        Value::Object(map) if map.get("hooks").is_some_and(Value::is_array) => map["hooks"]
            .as_array()
            .is_some_and(|inner| inner.iter().any(|item| is_our_hook(command_of(item)))),
        other => is_our_hook(command_of(other)),
    })
}

/// 读槽位当前的配置（`values`），未安装 ⇒ 明确的错误。
fn load_values(slot_key: &str) -> Result<Value, String> {
    let current = read_settings(slot_key)?;
    current
        .get("values")
        .filter(|values| !values.is_null())
        .cloned()
        .ok_or_else(|| {
            format!("槽位「{slot_key}」没有可读的配置文件，无法安装 hook（该客户端未安装？）")
        })
}

/// 从**已读出的**配置值判断哪些事件装了我们的 hook（供调用方避免二次读盘）。
pub fn installed_events_of(values: &Value) -> Vec<String> {
    HOOK_EVENTS
        .iter()
        .filter(|event| event_has_ours(values, event))
        .map(|event| (*event).to_string())
        .collect()
}

/// 该槽位上，哪些事件已经装了我们的 hook（按 [`HOOK_EVENTS`] 的顺序）。
///
/// 诊断用：设置页据此告诉用户「装了几条」，而不是给一个含糊的「已开启」。
pub fn installed_events(slot_key: &str) -> Result<Vec<String>, String> {
    let current = read_settings(slot_key)?;
    let Some(values) = current.get("values").filter(|values| !values.is_null()) else {
        return Ok(Vec::new());
    };
    Ok(installed_events_of(values))
}

/// 装：合并后写回。返回 [`patch_settings`] 的结果（含备份 id / 路径）。
///
/// ⚠️ 客户端**正在运行**时 `patch_settings` 会拒写（进程守卫）——
/// 这是**刻意的**：客户端退出时会把内存态写回，我们的改动会被静默覆盖。
pub fn install(slot_key: &str, command: &str) -> Result<Value, String> {
    let values = load_values(slot_key)?;
    let merged = merge_hooks(&values, command);
    let hooks = merged.get("hooks").cloned().unwrap_or_else(|| json!({}));
    patch_settings(slot_key, &json!({ "hooks": hooks }))
}

/// 卸：剔除我们的条目后写回。
pub fn uninstall(slot_key: &str) -> Result<Value, String> {
    let values = load_values(slot_key)?;
    let stripped = strip_our_hooks(&values);
    let patch = match stripped.get("hooks") {
        Some(hooks) => json!({ "hooks": hooks }),
        // 一条都不剩 ⇒ 显式置空对象（而不是留 undefined）
        None => json!({ "hooks": {} }),
    };
    patch_settings(slot_key, &patch)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "\"C:\\Program Files\\Buddy Switch\\BuddySwitch.exe\" hook --source workbuddy";

    fn others_hook() -> Value {
        json!({
            "matcher": "",
            "hooks": [{ "type": "command", "command": "/usr/bin/python3 /home/u/other.py" }]
        })
    }

    /// ★ 空配置 ⇒ 9 个事件各一条，且**只碰 `hooks` 键**。
    #[test]
    fn install_into_empty_config_touches_only_hooks() {
        let merged = merge_hooks(&json!({ "sandbox": { "enabled": true } }), CMD);
        assert_eq!(merged["sandbox"]["enabled"], json!(true), "别的键必须原样保留");
        let hooks = merged["hooks"].as_object().expect("hooks 必须是对象");
        assert_eq!(hooks.len(), HOOK_EVENTS.len());
        for event in HOOK_EVENTS {
            let groups = hooks[*event].as_array().expect("每事件一个数组");
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0]["hooks"][0]["command"], json!(CMD));
            assert_eq!(groups[0]["hooks"][0]["type"], json!("command"));
        }
    }

    /// `SessionEnd` 同步、其余带 `async`（否则会拖慢客户端的工具调用）。
    #[test]
    fn only_session_end_is_synchronous() {
        let merged = merge_hooks(&json!({}), CMD);
        assert!(merged["hooks"]["SessionEnd"][0]["hooks"][0].get("async").is_none());
        for event in ["PreToolUse", "Stop", "Notification"] {
            assert_eq!(merged["hooks"][event][0]["hooks"][0]["async"], json!(true));
        }
    }

    /// ★ **幂等**：装两次的结果与装一次逐字相同。
    ///
    /// 可证伪性：把 `rewrite_hooks` 里的「剔除自己的旧条目」删掉 ⇒ 变红（事件上会叠两条）。
    #[test]
    fn installing_twice_is_idempotent() {
        let once = merge_hooks(&json!({}), CMD);
        let twice = merge_hooks(&once, CMD);
        assert_eq!(once, twice, "重复安装不得叠加条目");
    }

    /// ★ **保留别人的 hook**（合并不是覆盖）。
    ///
    /// 可证伪性：把「保留别人的」改成「清空整个事件数组」⇒ 变红。
    #[test]
    fn other_peoples_hooks_survive() {
        let existing = json!({ "hooks": { "Stop": [others_hook()] } });
        let merged = merge_hooks(&existing, CMD);
        let groups = merged["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(groups.len(), 2, "别人的 + 我们的 = 2 条");
        assert_eq!(groups[0]["hooks"][0]["command"], json!("/usr/bin/python3 /home/u/other.py"));
    }

    /// ★ **同一分组内混装**时只剔掉我们那条，分组本身要留下。
    ///
    /// 可证伪性：把逐条过滤改成「整个分组丢掉」⇒ 别人的 hook 被误删，变红。
    #[test]
    fn shared_group_keeps_other_handlers() {
        let existing = json!({ "hooks": { "Stop": [{
            "matcher": "",
            "hooks": [
                { "type": "command", "command": "/usr/bin/python3 /home/u/other.py" },
                { "type": "command", "command": CMD },
            ]
        }] } });
        let merged = merge_hooks(&existing, CMD);
        let groups = merged["hooks"]["Stop"].as_array().unwrap();
        let handlers = groups[0]["hooks"].as_array().unwrap();
        assert_eq!(handlers.len(), 1, "我们那条应被剔除，别人的留下");
        assert_eq!(handlers[0]["command"], json!("/usr/bin/python3 /home/u/other.py"));
    }

    /// 裸 handler 形态（无 `hooks` 数组的扁平写法）也要能识别与剔除。
    #[test]
    fn bare_handler_form_is_recognized() {
        let existing = json!({ "hooks": { "Stop": [
            { "type": "command", "command": CMD },
            { "type": "command", "command": "/usr/bin/python3 /home/u/other.py" },
        ] } });
        let merged = merge_hooks(&existing, CMD);
        let groups = merged["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(groups.len(), 2, "别人的留下 + 我们新加的一条");
        assert_eq!(groups[0]["command"], json!("/usr/bin/python3 /home/u/other.py"));
        assert_eq!(groups[1]["hooks"][0]["command"], json!(CMD));
    }

    /// ★ **卸载只删我们的**，且删干净后**不留空数组键**。
    ///
    /// 可证伪性：把 `hooks.remove(*event)` 改成写 `Value::Array(vec![])` ⇒ 变红。
    #[test]
    fn uninstall_removes_our_entries_and_empty_keys() {
        let installed = merge_hooks(&json!({}), CMD);
        let stripped = strip_our_hooks(&installed);
        assert!(
            stripped.get("hooks").is_none(),
            "一条都不剩时必须删掉 hooks 键，而不是留空对象/空数组：{stripped}"
        );
        // 幂等：再卸一次同样干净
        assert_eq!(strip_our_hooks(&stripped), stripped);
    }

    /// 卸载**不碰**别人的 hook（装 → 卸 一轮后回到原样）。
    #[test]
    fn install_then_uninstall_restores_others() {
        let original = json!({ "hooks": { "Stop": [others_hook()] }, "sandbox": {} });
        let installed = merge_hooks(&original, CMD);
        let restored = strip_our_hooks(&installed);
        assert_eq!(restored, original, "装/卸一轮后必须逐字回到原状");
    }

    /// 不认识的形态（字符串等）**原样保留**（不能替用户删看不懂的东西）。
    #[test]
    fn unknown_shapes_are_preserved() {
        let existing = json!({ "hooks": { "Stop": ["weird", 42, null] } });
        let merged = merge_hooks(&existing, CMD);
        let groups = merged["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(groups[0], json!("weird"));
        assert_eq!(groups[1], json!(42));
        assert_eq!(groups[2], json!(null));
    }

    /// `hooks` 是脏类型（字符串）⇒ 当作空对象处理，不 panic。
    #[test]
    fn dirty_hooks_value_degrades_to_empty() {
        let merged = merge_hooks(&json!({ "hooks": "oops" }), CMD);
        assert_eq!(merged["hooks"].as_object().unwrap().len(), HOOK_EVENTS.len());
        let stripped = strip_our_hooks(&json!({ "hooks": "oops" }));
        assert!(stripped.get("hooks").is_none());
    }

    /// ★ 自己的 hook 识别：认「我们的可执行文件 + hook 子命令」，**不认**路径。
    ///
    /// 可证伪性：把判据放宽成「含 hook 就算」⇒ 第一条断言变红（会把别人的 hook 删掉）。
    #[test]
    fn our_hook_detection_is_specific() {
        assert!(is_our_hook(CMD));
        assert!(is_our_hook("\"/opt/buddy-switch\" hook --source workbuddy"));
        assert!(!is_our_hook("/usr/bin/python3 /home/u/other.py"));
        assert!(!is_our_hook("agent-studio-runtime hook --source workbuddy"));
        assert!(!is_our_hook("\"C:\\x\\BuddySwitch.exe\" --version"));
    }

    /// `hook_command` 对含空格的路径加引号（Windows 安装目录的常态）。
    #[test]
    fn hook_command_quotes_the_executable_path() {
        let command = hook_command(Path::new("C:\\Program Files\\Buddy Switch\\BuddySwitch.exe"), "workbuddy");
        assert!(command.starts_with('"'), "含空格的路径必须加引号：{command}");
        assert!(command.ends_with("hook --source workbuddy"));
        assert!(is_our_hook(&command), "自己构造的命令必须被自己认出来");
    }

    /// `installed_events` 的判据（纯逻辑部分）：装过的认得出，没装的不认。
    #[test]
    fn installed_events_detects_our_entries() {
        let installed = merge_hooks(&json!({}), CMD);
        assert_eq!(
            HOOK_EVENTS.iter().filter(|e| event_has_ours(&installed, e)).count(),
            HOOK_EVENTS.len()
        );
        let stripped = strip_our_hooks(&installed);
        assert_eq!(
            HOOK_EVENTS.iter().filter(|e| event_has_ours(&stripped, e)).count(),
            0
        );
    }
}
