//! 应用级偏好（L0）—— **只放「宿主自己也要读」的那一类**。
//!
//! ## 边界：偏好存哪，看**谁需要读它**
//!
//! | 偏好 | 读者 | 载体 |
//! |:---|:---|:---|
//! | 主题 / 语言 / 作用域快捷键 / 自动检查更新 | 只有渲染层 | `localStorage`（见 `lib/theme.ts` 等） |
//! | **关闭窗口时是否隐藏到托盘** | **宿主（Rust）** | 本模块的 `app_prefs.json` |
//!
//! 「关闭窗口」这件事发生在 `WindowEvent::CloseRequested` 里，那一刻**没有渲染层参与**
//! —— `prevent_close()` 必须在 Rust 侧当场决定，晚了窗口已经没了。所以这一项不能像
//! 主题那样留在 `localStorage`（宿主读不到它）。
//!
//! ⚠️ **别把纯前端偏好搬进来**：每加一项都要多一对命令 + 四处接线（`commands.rs` /
//! `generate_handler!` / `api.ts` / 门禁豁免表），而收益是零 —— 渲染层读 `localStorage`
//! 是同步的，走命令反而要处理「首帧还没读回来」。
//!
//! ## 与 `github_config.json` 的关系
//!
//! 两者都是 store 根下的小 JSON，但**归属不同域**：更新源配置归 `update` 模块
//! （含 owner/repo/proxy，是网络配置），本模块归「本应用的运行方式」。合并只会让
//! 两个域的归一化规则互相牵连。

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::modules::config::{atomic_write, store_dir};

/// 偏好文件（与 `github_config.json` 同级，都在 store 根下）。
pub fn app_prefs_file() -> PathBuf {
    store_dir().join("app_prefs.json")
}

/// 默认值。
///
/// `close_to_tray = true` 是**改设置之前就有的行为**（`tray.rs` 里
/// `CloseRequested` 一律 `prevent_close` + `hide`）—— 默认值必须与它逐字一致，
/// 否则升级后所有老用户的「点关闭」行为会**静默改变**。
///
/// `watch_agent_sessions = false`：P15 的外部 Agent 会话监听**默认关**。
/// 开启它要**往别的客户端写 hook 配置**，属于必须由用户显式同意的动作
/// （`P15-SPIKE-RESULTS §6 D41`）—— 默认开等于替用户改了别人的配置。
pub fn default_app_prefs() -> Value {
    json!({ "close_to_tray": true, "watch_agent_sessions": false })
}

/// 归一化：只认已知键；值必须是布尔，否则回落默认值；未知键丢弃。
///
/// 归一化而不是原样透传，理由与 `normalize_switch_config` 相同：配置文件是用户
/// 可手改的，一个 `"false"`（字符串）如果被当成真值传给 `if`，就会出现
/// 「设置里显示已关闭、实际仍然隐藏到托盘」这种**两边都以为自己对**的状态。
fn normalize_app_prefs(input: &Value) -> Value {
    let defaults = default_app_prefs();
    let mut merged = defaults.clone();
    if let Some(obj) = input.as_object() {
        for key in defaults.as_object().unwrap().keys() {
            if let Some(value) = obj.get(key).and_then(Value::as_bool) {
                merged[key.as_str()] = json!(value);
            }
        }
    }
    merged
}

/// 读取偏好；文件缺失 / 损坏 / 字段非法一律回落默认值（**永不失败**）。
pub fn load_app_prefs() -> Value {
    let f = app_prefs_file();
    if let Ok(text) = std::fs::read_to_string(&f) {
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            return normalize_app_prefs(&value);
        }
    }
    default_app_prefs()
}

/// 保存偏好，返回**归一化后的回读值**（调用方按回读值渲染，不假装写入成功）。
pub fn save_app_prefs(prefs: &Value) -> std::io::Result<Value> {
    let merged = normalize_app_prefs(prefs);
    std::fs::create_dir_all(store_dir())?;
    atomic_write(
        &app_prefs_file(),
        &serde_json::to_string_pretty(&merged).unwrap_or_default(),
    )?;
    Ok(merged)
}

/// 宿主专用：窗口被要求关闭时，该隐藏到托盘（true）还是真正退出（false）。
///
/// 每次读盘而不是进程内缓存：这个函数只在一件事（点关闭）上被调用，
/// 读一个小 JSON 的成本可以忽略；缓存反而会引入「改了设置但没生效」的窗口期。
pub fn close_to_tray() -> bool {
    load_app_prefs()
        .get("close_to_tray")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// 宿主专用：是否开启外部 Agent 会话监听（即「给客户端装 hook」）。
///
/// 与 [`close_to_tray`] 同款：每次读盘、不缓存 —— 它只在「装 / 卸 hook」这个
/// 用户显式动作上被读，读一个小 JSON 的成本可以忽略。
pub fn watch_agent_sessions() -> bool {
    load_app_prefs()
        .get("watch_agent_sessions")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认值**只含已知键**，且每一项的默认值都是刻意选的（改它 = 改行为）。
    #[test]
    fn 默认值只含已知键() {
        let d = default_app_prefs();
        assert_eq!(
            d["close_to_tray"],
            json!(true),
            "老行为，必须与升级前逐字一致"
        );
        assert_eq!(
            d["watch_agent_sessions"],
            json!(false),
            "要写别的客户端配置的动作，必须默认关"
        );
        assert_eq!(
            d.as_object().unwrap().len(),
            2,
            "每加一个键都要同步这条断言"
        );
    }

    #[test]
    fn 归一化只认已知键且非布尔回落默认() {
        // 已知键 + 合法布尔 ⇒ 采纳
        let ok = normalize_app_prefs(&json!({ "close_to_tray": false }));
        assert_eq!(ok["close_to_tray"], json!(false));
        // 字符串 "false" 不是布尔 ⇒ **回落 true**（不能把假值当真值用）
        let stringy = normalize_app_prefs(&json!({ "close_to_tray": "false" }));
        assert_eq!(stringy["close_to_tray"], json!(true));
        // 未知键丢弃（防止把别的模块的配置误存进来）
        let extra = normalize_app_prefs(&json!({ "close_to_tray": false, "theme": "dark" }));
        assert_eq!(extra.as_object().unwrap().len(), 2, "只认已知的两个键");
        assert!(extra.get("theme").is_none());
        // 非对象输入 ⇒ 全默认
        assert_eq!(normalize_app_prefs(&json!("nope")), default_app_prefs());
        assert_eq!(normalize_app_prefs(&json!(null)), default_app_prefs());
    }

    /// 真值表：`close_to_tray()` 是宿主的**唯一**判据，错一格就是「点了关闭没反应」
    /// 或「点了关闭整个应用没了」。逐格断言，不靠默认值蒙。
    #[test]
    fn close_to_tray_真值表() {
        let cases: [(Value, bool); 5] = [
            (json!({ "close_to_tray": true }), true),
            (json!({ "close_to_tray": false }), false),
            (json!({ "close_to_tray": "no" }), true), // 脏值 ⇒ 默认（隐藏）
            (json!({}), true),                        // 缺字段 ⇒ 默认（隐藏）
            (json!([]), true),                        // 非对象 ⇒ 默认（隐藏）
        ];
        for (input, want) in cases {
            let got = normalize_app_prefs(&input)
                .get("close_to_tray")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            assert_eq!(got, want, "输入 {input} 应得 {want}");
        }
    }

    /// `watch_agent_sessions()` 真值表：默认**关**，脏值一律回落到关。
    ///
    /// 这条比 `close_to_tray` 更要紧：它是「要不要去动**别人客户端的**配置文件」的判据，
    /// 任何一格错成 `true` 都意味着**用户没同意就被写了 hook**。
    ///
    /// 可证伪性：把 `unwrap_or(false)` 改成 `unwrap_or(true)` ⇒ 后三格变红。
    #[test]
    fn watch_agent_sessions_真值表() {
        let cases: [(Value, bool); 5] = [
            (json!({ "watch_agent_sessions": true }), true),
            (json!({ "watch_agent_sessions": false }), false),
            (json!({ "watch_agent_sessions": "true" }), false), // 字符串不是布尔 ⇒ 关
            (json!({}), false),                                 // 缺字段 ⇒ 关
            (json!([]), false),                                 // 非对象 ⇒ 关
        ];
        for (input, want) in cases {
            let got = normalize_app_prefs(&input)
                .get("watch_agent_sessions")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            assert_eq!(got, want, "输入 {input} 应得 {want}");
        }
    }
}
