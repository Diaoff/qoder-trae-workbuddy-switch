//! 定时任务排程配置与「按点独立排程」纯函数。
//!
//! 对照参考实现 `internal/config/schedule.go` + `internal/scheduler/scheduler.go`：
//! WorkBuddy 侧的六类任务（签到 / 旅行 / 活跃上报 / 保活 / 开学季 / 夜猫子）各自独立时点、
//! 独立开关，同一时点上的多类任务并行执行；本产品另加第七类 **Trae 自动签到**
//! （`trae_checkin`，第二条产品线，默认关闭，见 [`default_schedule`]）。
//!
//! **粒度是「时:分」而非整点**（issue #27）：每类任务的时点表是 [`TimeOfDay`] 列表，
//! 序列化为 `"HH:MM"` 字符串（`checkin_times: ["09:00", "21:00"]`）。用户可以把签到定在
//! 09:37 这类非整点，让每日触发时刻不再是一个可预期的固定整点。
//! 旧格式 `*_hours: [9, 21]`（整点整数）**仍可读**，读入即等价于 `["09:00", "21:00"]`，
//! 下次保存时自然迁移为新键（见 [`times_from`]）。
//!
//! **默认值预置 + 缺失键保留**：`ScheduleConfig` 先取默认值再被输入覆盖，因此键缺席
//! （或为 `null`）时保留默认——尤其 `*_enabled` 缺省必须为 `true`，否则老配置会因字段
//! 缺失被静默关闭。禁用一律走 `*_enabled=false`，不用哨兵时点（`[-1]` 之类）表意。
//!
//! 持久化到 `~/.buddy-switch/schedule_config.json`；读写沿用 config 模块的
//! `store_dir()` / `atomic_write` 约定。

use serde_json::{json, Value};
use std::path::PathBuf;

use crate::modules::config::{atomic_write, store_dir};
use crate::modules::cst;

/// 一天中的某个**时点**（`HH:MM`，内部存自 00:00 起的分钟数）。
///
/// ## 为什么需要这个类型（issue #27）
///
/// 排程此前的时点表是 `Vec<u32>` 的**小时**列表（`[9, 21]` ⇒ 09:00 / 21:00 两个整点）。
/// 用户反馈「只能设整点」：既没法把签到定在 09:37，也没法让每天的时刻错开一点。
/// 直接把 `Vec<u32>` 的含义改成「分钟数」是不行的——`[9, 21]` 会被读成 00:09 / 00:21，
/// **静默改掉所有老用户的签到时间**。因此引入显式类型 + 显式序列化格式：
///
/// - 内部：`0..=1439` 的分钟数，排序即「一天内的先后」，去重 / 比较都按它；
/// - 线上：`"HH:MM"` 字符串（人可读、跨语言无歧义，前端直接就是 `<input type="time">` 的值）；
/// - 兼容：老键 `*_hours` 的整点仍被读成 `hour:00`，见 [`times_from`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeOfDay(u32);

impl TimeOfDay {
    /// 由「时」「分」构造；`hour > 23` 或 `minute > 59` 返回 `None`。
    ///
    /// **这是唯一的构造入口之一**（另一个是 [`TimeOfDay::parse`]）：越界值在类型层面
    /// 就不存在，因此调用方（含配置读取）不需要再补一层「范围校验」。
    pub fn from_hm(hour: u32, minute: u32) -> Option<Self> {
        if hour > 23 || minute > 59 {
            return None;
        }
        Some(Self(hour * 60 + minute))
    }

    /// 解析 `H:MM` / `HH:MM`（分钟可写 1–2 位，如 `9:5`）；其余形态一律 `None`。
    ///
    /// 刻意**不接受** `HH:MM:SS`、`24:00`、负数、带空格的分段：上游（前端控件）只产出
    /// `HH:MM`，宽松解析只会让「看起来存进去了、实际按另一个时刻触发」的错更难发现。
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (hour, minute) = text.split_once(':')?;
        let digits_ok = |part: &str| {
            !part.is_empty() && part.len() <= 2 && part.bytes().all(|b| b.is_ascii_digit())
        };
        if !digits_ok(hour) || !digits_ok(minute) {
            return None;
        }
        Self::from_hm(hour.parse().ok()?, minute.parse().ok()?)
    }

    /// 自 00:00 起的分钟数。
    pub fn minutes_since_midnight(self) -> u32 {
        self.0
    }

    /// 时（0–23）。
    pub fn hour(self) -> u32 {
        self.0 / 60
    }

    /// 分（0–59）。
    pub fn minute(self) -> u32 {
        self.0 % 60
    }
}

impl std::fmt::Display for TimeOfDay {
    /// 恒为 `HH:MM` 两位补零形态（`"09:05"`），与 [`TimeOfDay::parse`] 互逆。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}", self.hour(), self.minute())
    }
}

impl serde::Serialize for TimeOfDay {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// 一次性生成「定时任务类型标识」的**唯一来源**。
///
/// 关键设计：枚举变体、[`ScheduleTask::all`]、标签 [`ScheduleTask::as_str`]，以及按变体
/// 分派的 [`ScheduleTask::enabled`] / [`ScheduleTask::times`]，**全部由同一份声明生成**，
/// 因此「任务集合」不可能与枚举漂移——新增/删除一类任务只需改动下面
/// `define_schedule_tasks!` 调用里的**一行**，四处自动同步。
///
/// 为什么必须这样：此前 `all()` 是**手工列举的固定数组**，与枚举是**两个来源**；把某一类
/// 任务从 `all()` 删掉而枚举仍在时，编译器无从察觉，该任务会**静默消失**且测试全绿
/// （与「`set_credits` 存在却无人调用」是同一失效模式）。现在删掉任一行会让枚举失去该
/// 变体，而所有 `match ScheduleTask`（含 `buddy-switch-server` 的派发，**无通配**）随之
/// **无法编译**——把「集合完整性」从运行时自证升级为编译期强制。
macro_rules! define_schedule_tasks {
    ($( $variant:ident => $label:literal, $enabled:ident, $times:ident, $legacy:ident ; )+) => {
        /// 定时任务的类型标识。
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum ScheduleTask {
            $( $variant, )+
        }

        impl ScheduleTask {
            /// 全部任务（顺序即声明顺序）。**由宏与枚举一并生成，二者不可能漂移。**
            pub fn all() -> Vec<ScheduleTask> {
                vec![ $( ScheduleTask::$variant, )+ ]
            }

            /// 稳定标识（用于日志 / 汇总）。
            pub fn as_str(self) -> &'static str {
                match self {
                    $( ScheduleTask::$variant => $label, )+
                }
            }

            /// 按稳定标识解析任务；未知标识返回 `None`。
            ///
            /// 与 [`ScheduleTask::as_str`] 由**同一份声明**生成，二者不可能漂移；供
            /// 「立即执行」这类按名派发的入口使用（不要把标识另抄一份字面量去 match）。
            pub fn parse(label: &str) -> Option<ScheduleTask> {
                match label {
                    $( $label => Some(ScheduleTask::$variant), )+
                    _ => None,
                }
            }

            /// 该任务是否在配置中被显式启用。
            pub fn enabled(self, cfg: &ScheduleConfig) -> bool {
                match self {
                    $( ScheduleTask::$variant => cfg.$enabled, )+
                }
            }

            /// 该任务配置的时点表；任务被禁用时返回空切片
            /// （`next_fire` 对空列表返回 `None`）。
            pub fn times(self, cfg: &ScheduleConfig) -> &[TimeOfDay] {
                if !self.enabled(cfg) {
                    return &[];
                }
                self.times_unchecked(cfg)
            }

            /// 该任务的时点表，**不看启用开关**。
            ///
            /// 供配置读取 / 校验使用：被禁用任务的时点同样是落盘数据，非法值不能因为
            /// 「现在关着」就被放过（否则用户下次打开开关时拿到的是一个坏配置）。
            pub fn times_unchecked(self, cfg: &ScheduleConfig) -> &[TimeOfDay] {
                match self {
                    $( ScheduleTask::$variant => &cfg.$times, )+
                }
            }

            /// 写入该任务的时点表（配置读取时按任务遍历用）。
            pub fn set_times(self, cfg: &mut ScheduleConfig, times: Vec<TimeOfDay>) {
                match self {
                    $( ScheduleTask::$variant => cfg.$times = times, )+
                }
            }

            /// 写入该任务的启用开关（配置读取时按任务遍历用）。
            pub fn set_enabled(self, cfg: &mut ScheduleConfig, enabled: bool) {
                match self {
                    $( ScheduleTask::$variant => cfg.$enabled = enabled, )+
                }
            }

            /// 该任务的时点表在 JSON / 接口里的字段名（`checkin_times` 等）。
            ///
            /// 与 [`ScheduleTask::times_unchecked`] 由**同一份声明**生成（`stringify!`），
            /// 因此「字段名」与「字段本身」不会各写一份而漂移；错误文案里的字段名即由此而来。
            pub fn times_field(self) -> &'static str {
                match self {
                    $( ScheduleTask::$variant => stringify!($times), )+
                }
            }

            /// 该任务时点表的**旧字段名**（`checkin_hours` 等，整点整数格式）。
            ///
            /// 只用于读取老配置（见 [`times_from`]）；保存时永远写 [`ScheduleTask::times_field`]。
            pub fn legacy_times_field(self) -> &'static str {
                match self {
                    $( ScheduleTask::$variant => stringify!($legacy), )+
                }
            }

            /// 该任务的启用开关字段名（`checkin_enabled` 等），用于错误文案指向开关。
            pub fn switch_field(self) -> &'static str {
                match self {
                    $( ScheduleTask::$variant => stringify!($enabled), )+
                }
            }
        }
    };
}

// ⚠️ 这里是「有哪些定时任务」的**唯一来源**：新增/删除一类任务只改这一处。
// 每行 = 变体 => 标签, 启用开关字段, 时点表字段, 旧时点表字段（整点格式）。漏改必然导致编译失败。
define_schedule_tasks! {
    Checkin => "checkin", checkin_enabled, checkin_times, checkin_hours;
    Travel => "travel", travel_enabled, travel_times, travel_hours;
    Activity => "activity", activity_enabled, activity_times, activity_hours;
    Keepalive => "keepalive", keepalive_enabled, keepalive_times, keepalive_hours;
    School => "school", school_enabled, school_times, school_hours;
    Cat => "cat", cat_enabled, cat_times, cat_hours;
    // Trae 分区（第二条产品线）的自动签到。**与上面的 `checkin` 是两件事**：
    // 前者签 WorkBuddy 的账号库，本任务签 Trae 的区域账号库（两本互不相通）。
    // 粒度刻意独立 —— 用户完全可能只想自动签其中一个产品。
    TraeCheckin => "trae_checkin", trae_checkin_enabled, trae_checkin_times, trae_checkin_hours;
    // Trae 账号的 **JWT 自动续期**（`refresh_token` → 新 JWT）。
    //
    // 与 `trae_checkin` 分开而不是塞进它：签到是「每天签一次就够」的动作，而续期是
    // **凭据维护** —— 账号没到期时它什么也不做，到期了就必须有人换。合在一起会让
    // 「只想保活、不想签到」的用户被迫打开自动签到（那是本仓反复消灭的假控件形态）。
    TraeJwtRefresh => "trae_jwt_refresh", trae_jwt_refresh_enabled, trae_jwt_refresh_times, trae_jwt_refresh_hours;
}

/// 排程配置（`~/.buddy-switch/schedule_config.json`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleConfig {
    pub checkin_times: Vec<TimeOfDay>,
    pub travel_times: Vec<TimeOfDay>,
    pub activity_times: Vec<TimeOfDay>,
    pub keepalive_times: Vec<TimeOfDay>,
    pub school_times: Vec<TimeOfDay>,
    pub cat_times: Vec<TimeOfDay>,
    /// Trae 自动签到的时点（第二条产品线，区域账号库）。
    pub trae_checkin_times: Vec<TimeOfDay>,
    /// Trae 账号 JWT 自动续期的时点（第二条产品线，区域账号库）。
    pub trae_jwt_refresh_times: Vec<TimeOfDay>,
    pub checkin_enabled: bool,
    pub travel_enabled: bool,
    pub activity_enabled: bool,
    pub keepalive_enabled: bool,
    pub school_enabled: bool,
    pub cat_enabled: bool,
    /// Trae 自动签到开关。**默认关闭**，见 [`default_schedule`] 的说明。
    pub trae_checkin_enabled: bool,
    /// Trae JWT 自动续期开关。**默认开启**，见 [`default_schedule`] 的说明。
    pub trae_jwt_refresh_enabled: bool,
    pub activity_report_count: u32,
}

/// 默认排程：WorkBuddy 六类与参考实现 `DefaultSchedule` 逐字一致。
///
/// 时点仍是整点（`09:00` / `21:00` 等）：分钟粒度是**新增能力**，不是**新默认值**——
/// 改默认会让老用户（以及「参考实现逐字一致」这条约定）的触发时刻无故漂移。
///
/// ## 唯一的偏离：Trae 自动签到**默认关闭**
///
/// 其余六类默认启用是**历史既定事实**（参考实现如此，改了会让老用户的功能静默消失）。
/// Trae 这条是本产品新增的能力，且它会**对外发请求**：默认打开等于「升级后凭空开始
/// 拿用户的 Trae 凭据去签到」——那不是用户授权过的行为。时点仍预置 09:00/21:00，
/// 用户打开开关即可用，不需要先配时间。
///
/// ## 第二处偏离：Trae JWT 自动续期**默认开启**
///
/// 与上一条**方向相反**，但判据是同一个：这件事用户**已经授权过**。
///
/// - `trae_checkin` 是**替用户去领东西**（claim 积分）——用户没要求过，故默认关；
/// - JWT 续期是**维持用户自己已经给出来的登录态**：`refresh_token` 的**唯一用途**就是
///   换新 JWT，而它是用户走「网页登录」添加账号时明确交出来的（添加账号对话框的原话：
///   「网页登录的账号可以自动续期」），卡片上也有「自动刷新 JWT」标记。
///   ⇒ 默认关闭等于**界面承诺了一件事、后台从不兑现**：账号静默过期、网关调用突然 401，
///   而用户看到的只是「怎么又要手动刷新」（这正是本轮修复的缺陷）。
///
/// 时点取 09:00/21:00：告警窗口是 24h（[`crate::modules::trae::TRAE_JWT_WARN_HOURS`]），
/// 一天两次检查 ⇒ 账号进入窗口后最迟约 12h 内被处理，仍留半天余量；
/// 应用没开在那个时点由**启动补跑**兜住（见 `scheduler::STARTUP_TASKS`）。
/// 用户可用 `trae_jwt_refresh_enabled = false` 关掉。
pub fn default_schedule() -> ScheduleConfig {
    /// 整点 `h:00`（默认表全部是整点，故集中在这里构造，避免每行都写 `from_hm(..).expect`）。
    fn at(hour: u32) -> TimeOfDay {
        TimeOfDay::from_hm(hour, 0).expect("默认时点的时/分都是合法值")
    }
    ScheduleConfig {
        checkin_times: vec![at(9), at(21)],
        travel_times: vec![at(9), at(21)],
        activity_times: vec![at(10)],
        keepalive_times: vec![at(22)],
        school_times: vec![at(12)],
        cat_times: vec![at(1)],
        trae_checkin_times: vec![at(9), at(21)],
        trae_jwt_refresh_times: vec![at(9), at(21)],
        checkin_enabled: true,
        travel_enabled: true,
        activity_enabled: true,
        keepalive_enabled: true,
        school_enabled: true,
        cat_enabled: true,
        trae_checkin_enabled: false,
        trae_jwt_refresh_enabled: true,
        // 领猫前置需 5 次对话；5 连发把 chat_5 刷满。显式缺省 = 5，但 0/负数归一为 1（旧行为）。
        activity_report_count: 5,
    }
}

/// `schedule_config.json` 路径。
pub fn schedule_config_file() -> PathBuf {
    store_dir().join("schedule_config.json")
}

/// 从已解析的 JSON 读时点表；`null` / 非数组 / 空数组一律视为「未配置」→ 保留默认。
///
/// ## 两个键名：新 `*_times` 优先，旧 `*_hours` 兼容
///
/// - `*_times`：`["09:37", "21:00"]`，分钟粒度（issue #27）；
/// - `*_hours`：`[9, 21]`，**旧格式**，等价于 `["09:00", "21:00"]`。
///
/// 旧键只在**新键缺席**时读取：老配置文件升级后照常工作，而一旦用户在新界面上保存过
/// 一次，落盘的就是 `*_times`，旧键随之消失（[`schedule_to_value`] 只输出新键）。
/// 若两个键同时存在，**以新键为准**——新键代表用户最近一次的显式选择。
///
/// 数组内的每个值**必须是合法时点**：小时越界、分钟越界、`"24:00"`、`"09:60"`、非字符串
/// 一律**报错**（错误文案指向对应 `*_enabled` 开关），绝不静默丢弃——否则「全部时点非法」
/// 会得到空表，使该任务被静默关闭、永不触发。
fn times_from(
    map: &serde_json::Map<String, Value>,
    field: &str,
    legacy_field: &str,
    switch_key: &str,
    default: &[TimeOfDay],
) -> Result<Vec<TimeOfDay>, String> {
    // 新键优先。`Some(items)` 才继续；键缺席 / 为 `null` / 不是数组 → 尝试旧键。
    if let Some(items) = map.get(field).and_then(Value::as_array) {
        if items.is_empty() {
            return Ok(default.to_vec());
        }
        let mut times = Vec::with_capacity(items.len());
        for item in items {
            // 只认字符串：数字（`9`）在新键里是**格式错误**而不是「09:00」，
            // 因为新键的契约就是 `HH:MM`；放行数字会让两种写法长期并存、歧义丛生。
            let Some(text) = item.as_str() else {
                return Err(time_error(field, switch_key, item));
            };
            match TimeOfDay::parse(text) {
                Some(time) => times.push(time),
                None => return Err(time_error(field, switch_key, item)),
            }
        }
        return Ok(times);
    }

    // 旧键（整点整数）：仅在新键缺席时读取，等价于 `hour:00`。
    let Some(Value::Array(items)) = map.get(legacy_field) else {
        return Ok(default.to_vec());
    };
    if items.is_empty() {
        return Ok(default.to_vec());
    }
    let mut times = Vec::with_capacity(items.len());
    for item in items {
        // `as_i64` 仅对「可表示为 i64 的整数」返回 Some；浮点 / 非数字 / 超范围 → None → 报错。
        match item.as_i64().and_then(|h| u32::try_from(h).ok()) {
            Some(hour) if hour <= 23 => {
                times.push(TimeOfDay::from_hm(hour, 0).expect("hour <= 23 时构造必定成功"))
            }
            _ => return Err(time_error(legacy_field, switch_key, item)),
        }
    }
    Ok(times)
}

/// 时点非法的统一错误文案：指出字段与合法格式，并**指向对应的 `*_enabled` 开关**
/// （关闭任务走开关，而不是靠哨兵时点猜）。
fn time_error(field: &str, switch_key: &str, value: impl std::fmt::Display) -> String {
    format!(
        "{field}: {value} 不是合法时点（HH:MM，00:00-23:59）；如要关闭该任务请设 schedule.{switch_key}=false"
    )
}

/// 把输入 JSON 归一化为 [`ScheduleConfig`]（默认值预置 → 逐类按宏声明读取时点/开关）。
///
/// 失败仅有一种原因：某任务的时点非法（格式错 / 越界）。此时错误文案**指向该任务的
/// `*_enabled` 开关**，引导用户改用开关而不是靠哨兵值猜。
pub fn schedule_from_value(input: &Value) -> Result<ScheduleConfig, String> {
    let defaults = default_schedule();
    let mut cfg = defaults.clone();
    if let Some(map) = input.as_object() {
        // 逐类任务读写**全部由宏声明驱动**：字段名、旧键名、开关名都来自
        // `define_schedule_tasks!` 那一行，因此不存在「新增一类任务但漏了读写」的
        // 空档（此前这里是 8 段手写赋值 + 8 项手写开关表，正是漂移的来源）。
        for task in ScheduleTask::all() {
            let times = times_from(
                map,
                task.times_field(),
                task.legacy_times_field(),
                task.switch_field(),
                task.times_unchecked(&defaults),
            )?;
            task.set_times(&mut cfg, times);

            if let Some(enabled) = map.get(task.switch_field()).and_then(Value::as_bool) {
                task.set_enabled(&mut cfg, enabled);
            }
        }

        // 0 / 负数 → 1（兼容旧行为：每号每天 1 条上报点亮连登）；缺省保留默认 5。
        if let Some(count) = map.get("activity_report_count").and_then(Value::as_i64) {
            cfg.activity_report_count = if count <= 0 { 1 } else { count as u32 };
        }
    }

    // 这里**没有**「再校验一遍时点范围」的末道防线，也不该有：`TimeOfDay` 只能经
    // `from_hm` / `parse` 构造，越界值在类型层面就不存在（此前那层校验是恒真的，
    // 只会让人误以为范围检查发生在这里）。真正的逐项校验发生在 `times_from` 里，
    // 那里才有「哪个键、第几项、该改哪个开关」的上下文。
    Ok(cfg)
}

/// 把 [`ScheduleConfig`] 序列化为落盘 / 接口返回用的 JSON（只含已知字段）。
///
/// 时点一律输出为 `*_times` 的 `"HH:MM"` 字符串；**旧键 `*_hours` 不再输出** ——
/// 这正是老配置「保存一次即完成迁移」的机制。
pub fn schedule_to_value(cfg: &ScheduleConfig) -> Value {
    json!({
        "checkin_times": cfg.checkin_times,
        "travel_times": cfg.travel_times,
        "activity_times": cfg.activity_times,
        "keepalive_times": cfg.keepalive_times,
        "school_times": cfg.school_times,
        "cat_times": cfg.cat_times,
        "trae_checkin_times": cfg.trae_checkin_times,
        "trae_jwt_refresh_times": cfg.trae_jwt_refresh_times,
        "checkin_enabled": cfg.checkin_enabled,
        "travel_enabled": cfg.travel_enabled,
        "activity_enabled": cfg.activity_enabled,
        "keepalive_enabled": cfg.keepalive_enabled,
        "school_enabled": cfg.school_enabled,
        "cat_enabled": cfg.cat_enabled,
        "trae_checkin_enabled": cfg.trae_checkin_enabled,
        "trae_jwt_refresh_enabled": cfg.trae_jwt_refresh_enabled,
        "activity_report_count": cfg.activity_report_count,
    })
}

/// 读取排程配置；文件缺失 / 损坏 / 含非法时点时回落到默认，保证调度器始终可用。
pub fn load_schedule_config() -> ScheduleConfig {
    let file = schedule_config_file();
    if file.exists() {
        if let Ok(text) = std::fs::read_to_string(&file) {
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                if let Ok(cfg) = schedule_from_value(&value) {
                    return cfg;
                }
            }
        }
    }
    default_schedule()
}

/// 保存排程配置：先校验归一化，成功才落盘（只保留已知字段）；返回归一化后的配置。
pub fn save_schedule_config(input: &Value) -> Result<ScheduleConfig, String> {
    let cfg = schedule_from_value(input)?;
    std::fs::create_dir_all(store_dir()).map_err(|e| e.to_string())?;
    let content = serde_json::to_string_pretty(&schedule_to_value(&cfg)).unwrap_or_default();
    atomic_write(&schedule_config_file(), &content).map_err(|e| e.to_string())?;
    Ok(cfg)
}

/// 在给定时点表里挑「**现在之后**最近的时点」的毫秒时间戳。
///
/// - 今天该时点仍晚于 `now_ms` → 取今天；
/// - 该时点已过（或恰等于 `now_ms`）→ 取次日同一时点；
/// - 跨月 / 跨年 / 闰日由 `NaiveDate::succ_opt` 处理，不做手工进位；
/// - `times` 为空（任务禁用）→ `None`。
///
/// 分钟粒度与整点走同一条路径：`TimeOfDay` 自带时分，`cst_hm_ms` 一次算出当天时刻，
/// 不存在「先算整点再加偏移」那种会在跨日边界上算错的写法。
pub fn next_fire(times: &[TimeOfDay], now_ms: i64) -> Option<i64> {
    let today = cst::cst_date(now_ms);
    let mut earliest: Option<i64> = None;
    for time in times {
        let at = |date| cst::cst_hm_ms(date, time.hour(), time.minute());
        let mut candidate = at(today);
        if !matches!(candidate, Some(ts) if ts > now_ms) {
            candidate = today.succ_opt().and_then(at);
        }
        if let Some(ts) = candidate {
            earliest = Some(match earliest {
                Some(existing) => existing.min(ts),
                None => ts,
            });
        }
    }
    earliest
}

/// 汇总全部时点，返回「最近一次唤醒时刻」与该时刻到期的任务列表。
///
/// 同一时点上的多类任务一并返回（调用方据此**并行**派发，互不阻塞）；全部任务被
/// 显式禁用时返回 `(None, [])`。
pub fn next_wake(cfg: &ScheduleConfig, now_ms: i64) -> (Option<i64>, Vec<ScheduleTask>) {
    let slots: Vec<(i64, ScheduleTask)> = ScheduleTask::all()
        .into_iter()
        .filter_map(|task| next_fire(task.times(cfg), now_ms).map(|at| (at, task)))
        .collect();
    let Some(earliest) = slots.iter().map(|(at, _)| *at).min() else {
        return (None, Vec::new());
    };
    let kinds = slots
        .into_iter()
        .filter(|(at, _)| *at == earliest)
        .map(|(_, task)| task)
        .collect();
    (Some(earliest), kinds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::cst::cst_ms;

    /// 测试用的时点构造：`t(9, 37)` == 09:37。非法值直接 panic（测试里就该炸）。
    fn t(hour: u32, minute: u32) -> TimeOfDay {
        TimeOfDay::from_hm(hour, minute).expect("测试时点必须合法")
    }

    fn hours_of(kinds: &[ScheduleTask]) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = kinds.iter().map(|k| k.as_str()).collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn defaults_match_reference_and_enable_all_tasks() {
        let cfg = default_schedule();
        // 默认仍是**整点**：分钟粒度是新增能力，不是新默认值（改默认会让老用户的
        // 触发时刻无故漂移，也破坏「与参考实现逐字一致」这条约定）。
        assert_eq!(cfg.checkin_times, vec![t(9, 0), t(21, 0)]);
        assert_eq!(cfg.travel_times, vec![t(9, 0), t(21, 0)]);
        assert_eq!(cfg.activity_times, vec![t(10, 0)]);
        assert_eq!(cfg.keepalive_times, vec![t(22, 0)]);
        assert_eq!(cfg.school_times, vec![t(12, 0)]);
        assert_eq!(cfg.cat_times, vec![t(1, 0)]);
        assert_eq!(cfg.activity_report_count, 5);
        assert_eq!(cfg.trae_checkin_times, vec![t(9, 0), t(21, 0)]);
        assert_eq!(cfg.trae_jwt_refresh_times, vec![t(9, 0), t(21, 0)]);
        // WorkBuddy 的六类默认全启用（与参考实现逐字一致，改了会让老用户的功能静默消失）。
        // Trae 两条**方向相反**，各有理由（见 `default_schedule` 的文档）：
        //   · `trae_checkin` 默认**关**（替用户去领东西，他没要求过，故 opt-in）；
        //   · `trae_jwt_refresh` 默认**开**（维持用户网页登录时自己交出来的 refresh_token）。
        for task in ScheduleTask::all() {
            let expected = !matches!(task, ScheduleTask::TraeCheckin);
            assert_eq!(
                task.enabled(&cfg),
                expected,
                "默认启用状态不符: {}",
                task.as_str()
            );
        }
        assert!(
            !cfg.trae_checkin_enabled,
            "Trae 自动签到必须**默认关闭**：默认打开等于升级后凭空拿用户凭据去签到"
        );
        assert!(
            cfg.trae_jwt_refresh_enabled,
            "Trae JWT 续期必须**默认开启**：界面（卡片标记 + 添加账号对话框）早就承诺\
             「网页登录的账号可以自动续期」，默认关掉等于承诺了却不兑现"
        );
        // 时点表仍预置好，用户打开开关即可用，不必先配时间。
        assert!(
            ScheduleTask::TraeCheckin.times(&cfg).is_empty(),
            "关闭的任务不参与排程（times() 返回空切片）"
        );
    }

    /// `TimeOfDay` 的解析 / 显示必须互逆，且拒绝一切「看起来像时间」的畸形串。
    ///
    /// 可证伪性：若 `parse` 放宽到接受 `"24:00"` 或 `"09:60"`，第三条断言会红；若 `Display`
    /// 不补零（输出 `"9:5"`），往返断言会红 —— 而前端 `<input type="time">` 只认 `HH:MM`，
    /// 不补零会让「存进去的值」和「控件显示的值」对不上。
    #[test]
    fn time_of_day_parse_and_display_are_inverse() {
        assert_eq!(TimeOfDay::parse("09:37"), Some(t(9, 37)));
        assert_eq!(TimeOfDay::parse("9:37"), Some(t(9, 37)));
        assert_eq!(TimeOfDay::parse("9:5"), Some(t(9, 5)));
        // 两端空白容忍（配置文件是手改得动的）。
        assert_eq!(TimeOfDay::parse(" 21:00 "), Some(t(21, 0)));
        assert_eq!(t(9, 5).to_string(), "09:05");
        assert_eq!(t(23, 59).to_string(), "23:59");
        for text in [
            "", "9", "24:00", "09:60", "-1:00", "09:37:00", "09 : 37", "ab:cd", "::",
        ] {
            assert_eq!(TimeOfDay::parse(text), None, "必须拒绝畸形时点: {text:?}");
        }
        assert_eq!(TimeOfDay::from_hm(24, 0), None);
        assert_eq!(TimeOfDay::from_hm(0, 60), None);
        assert_eq!(t(0, 0).minutes_since_midnight(), 0);
        assert_eq!(t(23, 59).minutes_since_midnight(), 1439);
    }

    #[test]
    fn missing_keys_keep_defaults_especially_enabled_flags() {
        // 空对象：缺失键必须保留默认（尤其 *_enabled 不能因缺失变 false）。
        let cfg = schedule_from_value(&json!({})).expect("empty config is valid");
        assert_eq!(cfg, default_schedule());

        // 只给一个无关键，其余仍全部默认启用。
        let cfg = schedule_from_value(&json!({"activity_report_count": 2})).unwrap();
        assert_eq!(cfg.activity_report_count, 2);
        assert!(cfg.checkin_enabled && cfg.travel_enabled && cfg.school_enabled);
    }

    #[test]
    fn empty_and_null_times_fall_back_to_defaults() {
        let cfg = schedule_from_value(&json!({
            "checkin_times": [],
            "travel_times": null,
            "school_times": [],
            // 旧键的空数组同理（老配置里出现过 `[]`，不能因此把任务静默关掉）。
            "cat_hours": [],
        }))
        .unwrap();
        assert_eq!(cfg.checkin_times, vec![t(9, 0), t(21, 0)]);
        assert_eq!(cfg.travel_times, vec![t(9, 0), t(21, 0)]);
        assert_eq!(cfg.school_times, vec![t(12, 0)]);
        assert_eq!(cfg.cat_times, vec![t(1, 0)]);
    }

    #[test]
    fn explicit_false_wins_for_every_switch() {
        let cfg = schedule_from_value(&json!({
            "checkin_enabled": false,
            "travel_enabled": false,
            "activity_enabled": false,
            "keepalive_enabled": false,
            "school_enabled": false,
            "cat_enabled": false,
            "trae_checkin_enabled": false,
            "trae_jwt_refresh_enabled": false,
        }))
        .unwrap();
        for task in ScheduleTask::all() {
            assert!(
                !task.enabled(&cfg),
                "显式 false 必须生效: {}",
                task.as_str()
            );
            assert!(task.times(&cfg).is_empty());
        }

        // 反向探针（**必需**）：Trae 签到的默认值是 `false`，因此只测「显式 false」时，
        // 这一项**即使后端根本没读这个键**也会通过 —— 那是假绿。
        // 用显式 true 才能证明该字段真的被读进配置。
        let all_on = schedule_from_value(&json!({
            "checkin_enabled": true,
            "travel_enabled": true,
            "activity_enabled": true,
            "keepalive_enabled": true,
            "school_enabled": true,
            "cat_enabled": true,
            "trae_checkin_enabled": true,
            "trae_jwt_refresh_enabled": true,
        }))
        .unwrap();
        for task in ScheduleTask::all() {
            assert!(
                task.enabled(&all_on),
                "显式 true 必须生效: {}",
                task.as_str()
            );
        }
    }

    /// issue #27 的**主路径**：`*_times` 的分钟必须被原样读进配置。
    ///
    /// 可证伪性：若后端仍按整点截断（例如把 `"09:37"` 存成 09:00），
    /// 第一条断言会红；若只读旧键 `*_hours`，`*_times` 会被忽略、落回默认 09:00/21:00。
    #[test]
    fn minute_granularity_is_read_from_times_key() {
        let cfg = schedule_from_value(&json!({
            "checkin_times": ["09:37", "21:05"],
            "trae_checkin_times": ["08:15"],
        }))
        .unwrap();
        assert_eq!(cfg.checkin_times, vec![t(9, 37), t(21, 5)]);
        assert_eq!(cfg.trae_checkin_times, vec![t(8, 15)]);
        // 未提交的任务仍保留默认（不是被清空）。
        assert_eq!(cfg.school_times, vec![t(12, 0)]);
    }

    /// 旧格式 `*_hours` 必须继续可读，且**新键优先**、保存时只写新键。
    ///
    /// 这是升级路径的核心断言：老用户的 `checkin_hours: [9, 21]` 不能变成
    /// 「未配置 ⇒ 落默认」而丢值，也不能因为键改名而让任务静默停摆。
    #[test]
    fn legacy_hours_are_read_as_whole_hours_and_migrated_on_save() {
        // (1) 只有旧键 ⇒ 等价于 `["09:00", "21:00"]`。
        let cfg = schedule_from_value(&json!({"checkin_hours": [9, 21]})).unwrap();
        assert_eq!(cfg.checkin_times, vec![t(9, 0), t(21, 0)]);

        // (2) 两键并存 ⇒ 新键为准（新键代表用户最近一次的显式选择）。
        let cfg = schedule_from_value(&json!({
            "checkin_times": ["09:37"],
            "checkin_hours": [9, 21],
        }))
        .unwrap();
        assert_eq!(cfg.checkin_times, vec![t(9, 37)]);

        // (3) 落盘只输出新键：旧键消失 = 「保存一次即完成迁移」。
        let value = schedule_to_value(&default_schedule());
        assert!(value.get("checkin_times").is_some());
        assert!(
            value.get("checkin_hours").is_none(),
            "保存时不得再输出旧键，否则新旧两套字段会长期并存"
        );
        assert_eq!(value["checkin_times"], json!(["09:00", "21:00"]));

        // (4) 往返：`to_value` 的输出必须能被 `from_value` 原样读回。
        let cfg = default_schedule();
        assert_eq!(schedule_from_value(&schedule_to_value(&cfg)).unwrap(), cfg);
    }

    #[test]
    fn invalid_time_errors_and_points_to_enabled_switch() {
        // 新键：小时越界。
        let err =
            schedule_from_value(&json!({"checkin_times": ["24:00"]})).expect_err("24:00 必须报错");
        assert!(err.contains("checkin_times"), "{err}");
        assert!(
            err.contains("checkin_enabled"),
            "错误文案必须指向 *_enabled 开关: {err}"
        );

        // 新键：分钟越界（若被放行会被 chrono 进位成下一个小时，静默改掉触发时刻）。
        let err = schedule_from_value(&json!({"cat_times": ["09:60"]})).unwrap_err();
        assert!(err.contains("cat_times"), "{err}");
        assert!(err.contains("cat_enabled"), "{err}");

        // 新键：数字是格式错误（新键的契约是 `HH:MM` 字符串）。
        let err = schedule_from_value(&json!({"school_times": [12]})).unwrap_err();
        assert!(err.contains("school_times"), "{err}");

        // 旧键：负数 / 越界仍按旧文案报错并指向开关。
        let err = schedule_from_value(&json!({"travel_hours": [9, -1]})).unwrap_err();
        assert!(err.contains("travel_hours"), "{err}");
        assert!(err.contains("travel_enabled"), "{err}");
    }

    #[test]
    fn activity_report_count_non_positive_normalizes_to_one() {
        assert_eq!(
            schedule_from_value(&json!({"activity_report_count": 0}))
                .unwrap()
                .activity_report_count,
            1
        );
        assert_eq!(
            schedule_from_value(&json!({"activity_report_count": -3}))
                .unwrap()
                .activity_report_count,
            1
        );
        assert_eq!(
            schedule_from_value(&json!({"activity_report_count": 8}))
                .unwrap()
                .activity_report_count,
            8
        );
        // 缺省保留默认 5。
        assert_eq!(
            schedule_from_value(&json!({}))
                .unwrap()
                .activity_report_count,
            5
        );
    }

    #[test]
    fn next_fire_picks_nearest_upcoming_time_today() {
        // 今天 08:00 → [09:00, 21:00] 今天 09:00。
        let now = cst_ms(2026, 9, 16, 8, 0);
        assert_eq!(
            next_fire(&[t(9, 0), t(21, 0)], now),
            Some(cst_ms(2026, 9, 16, 9, 0))
        );
    }

    /// 分钟粒度在**同一个小时内**也必须分得清：09:00 与 09:30 是两个不同的触发时刻。
    ///
    /// 可证伪性：若 `next_fire` 仍按小时比较（把 09:30 归成 09:00），
    /// 第二条断言会返回今天 09:00（已过）而不是今天 09:30。
    #[test]
    fn next_fire_respects_minutes_within_the_same_hour() {
        let times = [t(9, 0), t(9, 30)];
        // 08:50 → 09:00
        assert_eq!(
            next_fire(&times, cst_ms(2026, 9, 16, 8, 50)),
            Some(cst_ms(2026, 9, 16, 9, 0))
        );
        // 09:15 → 09:30（09:00 已过）
        assert_eq!(
            next_fire(&times, cst_ms(2026, 9, 16, 9, 15)),
            Some(cst_ms(2026, 9, 16, 9, 30))
        );
        // 09:37 → 次日 09:00
        assert_eq!(
            next_fire(&times, cst_ms(2026, 9, 16, 9, 37)),
            Some(cst_ms(2026, 9, 17, 9, 0))
        );
        // 非整点时点同样成立。
        assert_eq!(
            next_fire(&[t(9, 37)], cst_ms(2026, 9, 16, 9, 30)),
            Some(cst_ms(2026, 9, 16, 9, 37))
        );
    }

    #[test]
    fn next_fire_wraps_to_next_day_when_all_points_passed() {
        // 今天 22:00 → 今天时点（9/21）全过 → 次日 09:00。
        let now = cst_ms(2026, 9, 16, 22, 0);
        assert_eq!(
            next_fire(&[t(9, 0), t(21, 0)], now),
            Some(cst_ms(2026, 9, 17, 9, 0))
        );

        // 恰好等于某时点 → 不算「之后」，取次日（与参考实现 !After 语义一致）。
        let exact = cst_ms(2026, 9, 16, 9, 0);
        assert_eq!(
            next_fire(&[t(9, 0)], exact),
            Some(cst_ms(2026, 9, 17, 9, 0))
        );
        // 分钟级同理：09:37:00 整不算「之后」。
        let exact_minute = cst_ms(2026, 9, 16, 9, 37);
        assert_eq!(
            next_fire(&[t(9, 37)], exact_minute),
            Some(cst_ms(2026, 9, 17, 9, 37))
        );
    }

    #[test]
    fn next_fire_crosses_month_and_year_boundaries() {
        assert_eq!(
            next_fire(&[t(9, 0)], cst_ms(2026, 9, 30, 23, 59)),
            Some(cst_ms(2026, 10, 1, 9, 0))
        );
        assert_eq!(
            next_fire(&[t(1, 0)], cst_ms(2026, 12, 31, 20, 0)),
            Some(cst_ms(2027, 1, 1, 1, 0))
        );
        // 空列表（禁用）→ None。
        assert_eq!(next_fire(&[], cst_ms(2026, 9, 16, 8, 0)), None);
    }

    #[test]
    fn next_wake_groups_multiple_tasks_on_same_hour() {
        // 默认配置下 checkin / travel / trae_jwt_refresh 都在 9 点；08:00 唤醒时三者同槽。
        let cfg = default_schedule();
        let (at, kinds) = next_wake(&cfg, cst_ms(2026, 9, 16, 8, 0));
        assert_eq!(at, Some(cst_ms(2026, 9, 16, 9, 0)));
        assert_eq!(
            hours_of(&kinds),
            vec!["checkin", "trae_jwt_refresh", "travel"]
        );
    }

    #[test]
    fn next_wake_returns_earliest_across_task_families() {
        // 08:00：cat=1(次日) / activity=10 / school=12 / checkin&travel&trae_jwt_refresh=9 /
        // keepalive=22 → 9 点。
        let cfg = default_schedule();
        let (at, kinds) = next_wake(&cfg, cst_ms(2026, 9, 16, 8, 0));
        assert_eq!(at, Some(cst_ms(2026, 9, 16, 9, 0)));
        assert_eq!(
            hours_of(&kinds),
            vec!["checkin", "trae_jwt_refresh", "travel"]
        );
    }

    /// 分钟粒度必须真的影响**唤醒时刻**，而不只是配置里多存了两个数字。
    #[test]
    fn next_wake_honours_minute_level_times() {
        let mut cfg = default_schedule();
        // 把签到挪到 09:37，其余任务全部关掉 ⇒ 下一次唤醒必须是 09:37。
        cfg.checkin_times = vec![t(9, 37)];
        for task in ScheduleTask::all() {
            if task != ScheduleTask::Checkin {
                match task {
                    ScheduleTask::Travel => cfg.travel_enabled = false,
                    ScheduleTask::Activity => cfg.activity_enabled = false,
                    ScheduleTask::Keepalive => cfg.keepalive_enabled = false,
                    ScheduleTask::School => cfg.school_enabled = false,
                    ScheduleTask::Cat => cfg.cat_enabled = false,
                    ScheduleTask::TraeCheckin => cfg.trae_checkin_enabled = false,
                    ScheduleTask::TraeJwtRefresh => cfg.trae_jwt_refresh_enabled = false,
                    ScheduleTask::Checkin => {}
                }
            }
        }
        let (at, kinds) = next_wake(&cfg, cst_ms(2026, 9, 16, 9, 0));
        assert_eq!(at, Some(cst_ms(2026, 9, 16, 9, 37)));
        assert_eq!(hours_of(&kinds), vec!["checkin"]);
    }

    #[test]
    fn disabling_activity_does_not_affect_school_or_cat() {
        // 六类开关互相独立：单独关闭 activity 不应影响其它任务的排程。
        let mut cfg = default_schedule();
        cfg.activity_times = vec![t(10, 0)];
        let (_, all_kinds) = next_wake(&cfg, cst_ms(2026, 9, 16, 9, 30));
        assert!(all_kinds.contains(&ScheduleTask::Activity));

        cfg.activity_enabled = false;
        let (_, kinds) = next_wake(&cfg, cst_ms(2026, 9, 16, 9, 30));
        assert!(
            !kinds.contains(&ScheduleTask::Activity),
            "关闭 activity 后不应再出现该任务"
        );
        // school / cat 仍在候选（其自身开关未动）。
        assert!(ScheduleTask::School.enabled(&cfg));
        assert!(ScheduleTask::Cat.enabled(&cfg));
        assert!(ScheduleTask::School.times(&cfg).contains(&t(12, 0)));
        assert_eq!(ScheduleTask::Cat.times(&cfg), &[t(1, 0)]);
        // 最近的唤醒时点 = 各启用任务的最近时点，school(12:00) 与 cat(次日 01:00) 均在其中。
        let (at, _) = next_wake(&cfg, cst_ms(2026, 9, 16, 9, 30));
        assert_eq!(at, Some(cst_ms(2026, 9, 16, 12, 0)));
    }

    #[test]
    fn all_tasks_disabled_yields_no_wake() {
        let mut cfg = default_schedule();
        cfg.checkin_enabled = false;
        cfg.travel_enabled = false;
        cfg.activity_enabled = false;
        cfg.keepalive_enabled = false;
        cfg.school_enabled = false;
        cfg.cat_enabled = false;
        cfg.trae_jwt_refresh_enabled = false;
        let (at, kinds) = next_wake(&cfg, cst_ms(2026, 9, 16, 8, 0));
        assert_eq!(at, None);
        assert!(kinds.is_empty());
    }

    #[test]
    fn save_rejects_invalid_and_persists_valid() {
        // save 的纯校验路径（不触盘）：值非法直接 Err。
        assert!(save_schedule_config(&json!({"travel_times": ["25:00"]})).is_err());
        assert!(save_schedule_config(&json!({"travel_hours": [30]})).is_err());
    }

    #[test]
    fn all_invalid_times_error_instead_of_silent_empty() {
        // 负数（旧键）：全部非法 → 必须报错，而不是静默丢弃得到空表（任务会被静默关闭、永不触发）。
        let err =
            schedule_from_value(&json!({"checkin_hours": [-1, -2]})).expect_err("负数小时必须报错");
        assert!(err.contains("checkin_hours"), "{err}");
        assert!(
            err.contains("checkin_enabled"),
            "错误文案必须指向 *_enabled 开关: {err}"
        );

        // 浮点（旧键，非整数）：同样报错并指向开关。
        let err =
            schedule_from_value(&json!({"cat_hours": [1.5, 2.5]})).expect_err("浮点小时必须报错");
        assert!(err.contains("cat_hours"), "{err}");
        assert!(err.contains("cat_enabled"), "{err}");

        // 混合（一个合法 + 一个非法）：仍报错，绝不静默丢弃非法项。
        assert!(
            schedule_from_value(&json!({"travel_hours": [9, -1]})).is_err(),
            "含非法项必须整体报错"
        );
        // 新键同样不允许「部分合法就放行」。
        assert!(
            schedule_from_value(&json!({"travel_times": ["09:00", "24:01"]})).is_err(),
            "含非法时点必须整体报错"
        );

        // 合法边界必须通过：旧键 0 / 23，新键 00:00 / 23:59。
        let cfg = schedule_from_value(&json!({"school_hours": [0, 23]})).unwrap();
        assert_eq!(cfg.school_times, vec![t(0, 0), t(23, 0)]);
        let cfg = schedule_from_value(&json!({"school_times": ["00:00", "23:59"]})).unwrap();
        assert_eq!(cfg.school_times, vec![t(0, 0), t(23, 59)]);
    }

    /// 表驱动：把宏 [`define_schedule_tasks!`] 生成的**四张映射表**（`as_str` / `enabled` /
    /// `times` / `times_field`+`switch_field`）逐类钉死，防止「宏正确地生成了错误映射」——
    /// 即某类任务读到了**另一类**的开关/时点/字段名，从而按错的配置静默运行
    /// （编译与既有测试都发现不了）。
    ///
    /// 关键：配置取值必须**互相可区分**，否则「读错字段」会得到相同观察结果、测不出。
    /// - **label**：逐类断言 `as_str()` 等于自己的字面量。
    /// - **times**：八份 `*_times` 设成互不相同的列表，逐类断言 `times()` 等于自己的列表；
    ///   任一跨字段错读必然得到别人的列表 → 暴露。
    /// - **字段名**：`times_field` / `switch_field` 必须各自指向自己的键（错误文案靠它定位）。
    /// - **enabled**：布尔只有两态，单份配置区分不了 8 个字段，故遍历「只开一个开关」的
    ///   八份配置，断言**被启用的任务集合**恰为该开关的属主；任一跨字段错读都会在某份
    ///   配置里被拆穿。
    ///
    /// 断言的是「等于它自己那一列」（随字段变化），而非某个常量——否则改错映射仍可能蒙对。
    #[test]
    fn schedule_task_mappings_are_pinned_per_variant() {
        // 全关 + 八份互异的时点列表（当作「每类自己的那一列」的探针）。
        let off = ScheduleConfig {
            checkin_times: vec![t(1, 0)],
            travel_times: vec![t(2, 0)],
            activity_times: vec![t(3, 0)],
            keepalive_times: vec![t(4, 0)],
            school_times: vec![t(5, 0)],
            cat_times: vec![t(6, 0)],
            trae_checkin_times: vec![t(7, 0)],
            trae_jwt_refresh_times: vec![t(8, 0)],
            checkin_enabled: false,
            travel_enabled: false,
            activity_enabled: false,
            keepalive_enabled: false,
            school_enabled: false,
            cat_enabled: false,
            trae_checkin_enabled: false,
            trae_jwt_refresh_enabled: false,
            activity_report_count: 5,
        };

        // (1) label + times + 三个字段名：逐类断言等于「它自己那一列」。
        let expected: [(ScheduleTask, &str, &[TimeOfDay], &str, &str, &str); 8] = [
            (
                ScheduleTask::Checkin,
                "checkin",
                off.checkin_times.as_slice(),
                "checkin_times",
                "checkin_enabled",
                "checkin_hours",
            ),
            (
                ScheduleTask::Travel,
                "travel",
                off.travel_times.as_slice(),
                "travel_times",
                "travel_enabled",
                "travel_hours",
            ),
            (
                ScheduleTask::Activity,
                "activity",
                off.activity_times.as_slice(),
                "activity_times",
                "activity_enabled",
                "activity_hours",
            ),
            (
                ScheduleTask::Keepalive,
                "keepalive",
                off.keepalive_times.as_slice(),
                "keepalive_times",
                "keepalive_enabled",
                "keepalive_hours",
            ),
            (
                ScheduleTask::School,
                "school",
                off.school_times.as_slice(),
                "school_times",
                "school_enabled",
                "school_hours",
            ),
            (
                ScheduleTask::Cat,
                "cat",
                off.cat_times.as_slice(),
                "cat_times",
                "cat_enabled",
                "cat_hours",
            ),
            (
                ScheduleTask::TraeCheckin,
                "trae_checkin",
                off.trae_checkin_times.as_slice(),
                "trae_checkin_times",
                "trae_checkin_enabled",
                "trae_checkin_hours",
            ),
            (
                ScheduleTask::TraeJwtRefresh,
                "trae_jwt_refresh",
                off.trae_jwt_refresh_times.as_slice(),
                "trae_jwt_refresh_times",
                "trae_jwt_refresh_enabled",
                "trae_jwt_refresh_hours",
            ),
        ];
        // `times()` 仅在任务启用时才返回自己的列表，故 times 断言用「全启用」配置。
        let on = ScheduleConfig {
            checkin_enabled: true,
            travel_enabled: true,
            activity_enabled: true,
            keepalive_enabled: true,
            school_enabled: true,
            cat_enabled: true,
            trae_checkin_enabled: true,
            trae_jwt_refresh_enabled: true,
            ..off.clone()
        };
        for (task, label, own_times, times_field, switch_field, legacy_field) in expected {
            assert_eq!(
                task.as_str(),
                label,
                "as_str 映射错误（应等于自己的标签）: {label}"
            );
            assert_eq!(
                task.times(&on),
                own_times,
                "times 映射错误（应等于自己的 *_times）: {label}"
            );
            assert_eq!(
                task.times_unchecked(&off),
                own_times,
                "times_unchecked 必须不看开关、仍返回自己的时点表: {label}"
            );
            assert_eq!(
                task.times_field(),
                times_field,
                "times_field 映射错误: {label}"
            );
            assert_eq!(
                task.switch_field(),
                switch_field,
                "switch_field 映射错误: {label}"
            );
            assert_eq!(
                task.legacy_times_field(),
                legacy_field,
                "legacy_times_field 映射错误（读老配置靠它）: {label}"
            );
        }

        // (2) enabled：布尔仅两态，一份配置区分不了 8 个字段；遍历「只开一个开关」的八份配置，
        //     断言「被启用的任务集合」恰为该开关的属主。
        let only: [(&str, ScheduleConfig); 8] = [
            (
                "checkin",
                ScheduleConfig {
                    checkin_enabled: true,
                    ..off.clone()
                },
            ),
            (
                "travel",
                ScheduleConfig {
                    travel_enabled: true,
                    ..off.clone()
                },
            ),
            (
                "activity",
                ScheduleConfig {
                    activity_enabled: true,
                    ..off.clone()
                },
            ),
            (
                "keepalive",
                ScheduleConfig {
                    keepalive_enabled: true,
                    ..off.clone()
                },
            ),
            (
                "school",
                ScheduleConfig {
                    school_enabled: true,
                    ..off.clone()
                },
            ),
            (
                "cat",
                ScheduleConfig {
                    cat_enabled: true,
                    ..off.clone()
                },
            ),
            (
                "trae_checkin",
                ScheduleConfig {
                    trae_checkin_enabled: true,
                    ..off.clone()
                },
            ),
            (
                "trae_jwt_refresh",
                ScheduleConfig {
                    trae_jwt_refresh_enabled: true,
                    ..off.clone()
                },
            ),
        ];
        for (owner_label, cfg) in only {
            let enabled_labels: Vec<&str> = ScheduleTask::all()
                .into_iter()
                .filter(|task| task.enabled(&cfg))
                .map(|task| task.as_str())
                .collect();
            assert_eq!(
                enabled_labels,
                vec![owner_label],
                "enabled 映射错误：只有 {owner_label} 开关打开时，被启用的任务必须恰为该类"
            );
        }

        // (3) 集合完整性：`all()` 恰好是这八类、不多不少（顺序 = 宏声明顺序）。
        let labels: Vec<&str> = ScheduleTask::all()
            .into_iter()
            .map(|task| task.as_str())
            .collect();
        assert_eq!(
            labels,
            vec![
                "checkin",
                "travel",
                "activity",
                "keepalive",
                "school",
                "cat",
                "trae_checkin",
                "trae_jwt_refresh"
            ],
            "all() 必须恰好包含全部八类且顺序稳定"
        );
    }

    /// `parse` 必须与 `as_str` **逐类可逆**，且未知标识返回 `None`。
    ///
    /// 该断言可证伪：若宏里某类任务的标签字面量被写错（`as_str` 与 `parse` 用的是同一个
    /// 字面量，所以真正的风险是「新增了任务但漏登记」或「未知串被当成某类」），
    /// 第一类断言会红；把 `parse` 改成「未知串回落到某类」会让第二类断言红。
    #[test]
    fn parse_is_the_inverse_of_as_str_and_rejects_unknown() {
        for task in ScheduleTask::all() {
            assert_eq!(
                ScheduleTask::parse(task.as_str()),
                Some(task),
                "parse 必须能还原自己的标识: {}",
                task.as_str()
            );
        }
        // 未知标识（含大小写差异与空串）一律 None，绝不静默回落到某一类。
        for unknown in ["", "Checkin", "CHECKIN", "activity-report", "signin"] {
            assert_eq!(
                ScheduleTask::parse(unknown),
                None,
                "未知标识必须被拒绝: {unknown}"
            );
        }
    }
}
