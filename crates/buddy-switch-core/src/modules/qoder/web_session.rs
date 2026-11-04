//! Qoder **网页会话**（Cookie）的本地缓存（issue #111 · S4）。
//!
//! 登录窗只在用户点「登录」时才有 Cookie；一旦窗口关掉、应用重启，进程内的 cookie
//! store 也就没了。不落盘的话，**每次要看逐包明细都得让用户重新登录一次** —— 那不是
//! 「一次授权长期可用」，而是把成本转嫁给用户。
//!
//! ## 三条边界（都别越）
//!
//! 1. ★★ **这是完整身份凭据**：拿到它等于拿到该 Qoder 账号的网页登录态。所以
//!    - 只经 [`super::store::write_json`] 落盘（走 `atomic_write_bytes` ⇒ 临时文件即 0600 + rename）；
//!    - **不导出、不进备份、不进日志、不进错误消息**（导出走的是账号库，本文件是独立文件，
//!      刻意不与账号库合并 —— 合并会被 `export_import` 一并带走）。
//! 2. ★ **有保鲜期**：上游会话会过期，本地留 7 天就够。过期不是「无效」，是「不再信任，
//!    请重新登录」—— 两者在 UI 上的动作完全不同。
//! 3. ★ **归属必须记下来**：Cookie 是浏览器级的，同一台机器上可能先后登过不同账号。
//!    读取方必须比对 `uid`，不一致就当作没有（见 [`WebSession::matches_uid`]）。
//!
//! ## 为什么不做加密
//!
//! PRD 原写「走 `at_rest` 加密落盘」。落地时核实：`at_rest` 是**解上游客户端信封**的
//! 模块（`decrypt_field` / `fetch_build_key`），不是本仓自己的对称加密；本仓也没有
//! 机器绑定的密钥材料 —— 账号库里的 `accessToken` / `refreshToken` 本身就是明文 JSON
//! （仅靠 0600 挡同机其他用户）。在这种情况下给 Cookie 单独加一层「密钥和密文放在
//! 同一个目录」的加密是**安全剧场**，还多一条必须维护的解密路径。
//! ⇒ 与账号库**同等防护等级**（0600 + 原子写），并在文档里写明这一点。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::paths;
use super::region::QoderRegion;
use super::store;

/// 本地缓存的最长保鲜期：7 天。
///
/// 取值理由：Qoder 网页会话的实际有效期由上游决定（本仓不掌握），7 天是「明显短于
/// 任何正常会话、又足够覆盖日常使用」的折中。过期后不删文件（留作诊断），但
/// [`WebSession::is_fresh`] 判否 ⇒ 调用方按「未登录」处理。
pub const MAX_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// 一份网页会话。
///
/// ⚠️ `cookie` 是 `k=v; k2=v2` 形状的 **`Cookie:` 头**，不是单个 cookie 的值。
/// `Debug` 刻意手写（见下）：派生 `Debug` 会把整串 cookie 打进 `{:?}`，
/// 一旦有人把它塞进日志就全泄了。
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebSession {
    /// 归属账号 uid（**必填**；空串视为无效会话）。
    pub uid: String,
    /// `Cookie:` 头原文。
    pub cookie: String,
    /// 会话里出现过哪些 cookie 名（诊断用，**不含值**）。
    #[serde(default)]
    pub cookie_names: Vec<String>,
    /// 落盘时刻（毫秒）。
    pub saved_at_ms: i64,
    /// 最近一次**被上游接受**的时刻（毫秒）。`0` = 存下来后还没验证过。
    #[serde(default)]
    pub verified_at_ms: i64,
}

impl std::fmt::Debug for WebSession {
    /// 手写 `Debug`：只出 uid 与 cookie 的**条数**，永不出原文。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSession")
            .field("uid", &self.uid)
            .field("cookies", &self.cookie_names.len())
            .field("saved_at_ms", &self.saved_at_ms)
            .field("verified_at_ms", &self.verified_at_ms)
            .finish()
    }
}

impl WebSession {
    /// 是否还在保鲜期内（`now_ms` 由调用方给，便于测试注入）。
    pub fn is_fresh(&self, now_ms: i64) -> bool {
        if self.cookie.trim().is_empty() || self.uid.trim().is_empty() {
            return false;
        }
        let age = now_ms.saturating_sub(self.saved_at_ms);
        // 负年龄（系统时钟被回拨）⇒ **按新鲜处理**：真实使用中它只可能是几秒的漂移，
        // 而「因为时钟跳了一下就让用户重新登录」是更糟的体验。
        age < MAX_AGE_MS
    }

    /// 归属是否就是 `uid`（大小写不敏感；空值一律判否 —— fail-closed）。
    pub fn matches_uid(&self, uid: &str) -> bool {
        let expected = uid.trim();
        !expected.is_empty()
            && !self.uid.trim().is_empty()
            && self.uid.trim().eq_ignore_ascii_case(expected)
    }
}

/// 会话文件的路径（与账号库同目录，按区域分家）。
pub fn file_for(region: QoderRegion) -> PathBuf {
    paths::qoder_dir().join(format!("web_session.{}.json", region.as_str()))
}

/// 落盘到指定路径（测试注入用；生产走 [`save`]）。
///
/// 覆盖写入时保留 `verified_at_ms`：重新登录后还没验证过上游，但「上次验证时刻」
/// 仍是有效的诊断信息。
pub fn save_to(path: &Path, session: &WebSession) -> Result<(), String> {
    let previous = load_from(path);
    let mut next = session.clone();
    if next.verified_at_ms == 0 {
        if let Some(previous) = previous {
            if previous.matches_uid(&next.uid) {
                next.verified_at_ms = previous.verified_at_ms;
            }
        }
    }
    store::write_json(path, &next)
}

/// 读取指定路径（文件不存在 / 损坏 ⇒ `None`，不报错）。
pub fn load_from(path: &Path) -> Option<WebSession> {
    store::read_json_checked::<WebSession>(path)
        .ok()
        .flatten()
        .filter(|session| !session.cookie.trim().is_empty() && !session.uid.trim().is_empty())
}

/// 删除指定路径（不存在 ⇒ 成功，幂等）。
pub fn clear_at(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(path).map_err(|error| format!("删除网页会话失败: {error}"))
}

/// 落盘（生产入口）。
pub fn save(region: QoderRegion, session: &WebSession) -> Result<(), String> {
    save_to(&file_for(region), session)
}

/// 读取（生产入口；**不判保鲜期**，由调用方用 [`WebSession::is_fresh`]）。
pub fn load(region: QoderRegion) -> Option<WebSession> {
    load_from(&file_for(region))
}

/// 清除（生产入口；幂等）。
pub fn clear(region: QoderRegion) -> Result<(), String> {
    clear_at(&file_for(region))
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn session(uid: &str, now_ms: i64) -> WebSession {
        WebSession {
            uid: uid.to_string(),
            cookie: "a=1; b=2".to_string(),
            cookie_names: vec!["a".into(), "b".into()],
            saved_at_ms: now_ms,
            verified_at_ms: 0,
        }
    }

    #[test]
    fn roundtrip_keeps_cookie_but_debug_never_leaks_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("web_session.cn.json");
        let saved = session("u1", 1_000);
        save_to(&path, &saved).unwrap();

        let loaded = load_from(&path).expect("应当读回");
        assert_eq!(loaded.uid, "u1");
        assert_eq!(loaded.cookie, "a=1; b=2");

        // ★ 证伪点：把 Debug 改回 derive ⇒ 这条红，且日志里会打出完整 cookie。
        let debug = format!("{loaded:?}");
        assert!(
            !debug.contains("a=1"),
            "Debug 不得输出 cookie 原文: {debug}"
        );
        assert!(
            !debug.contains("b=2"),
            "Debug 不得输出 cookie 原文: {debug}"
        );
        assert!(debug.contains("cookies: 2"));
    }

    #[test]
    fn freshness_is_bounded_and_clock_skew_is_forgiven() {
        let now = 10 * MAX_AGE_MS;
        assert!(session("u1", now).is_fresh(now), "刚存下来的必须新鲜");
        assert!(
            session("u1", now - MAX_AGE_MS + 1).is_fresh(now),
            "差 1ms 到期仍算新鲜"
        );
        assert!(
            !session("u1", now - MAX_AGE_MS).is_fresh(now),
            "恰好到期即不新鲜"
        );
        // 时钟回拨（负年龄）不得让用户被迫重新登录
        assert!(
            session("u1", now + 60_000).is_fresh(now),
            "负年龄按新鲜处理"
        );

        // 缺 cookie / 缺 uid 一律不新鲜（fail-closed）
        let mut empty = session("u1", now);
        empty.cookie = "  ".into();
        assert!(!empty.is_fresh(now));
        let mut no_uid = session("u1", now);
        no_uid.uid = String::new();
        assert!(!no_uid.is_fresh(now));
    }

    #[test]
    fn uid_match_is_fail_closed_and_case_insensitive() {
        let saved = session("ABC-123", 0);
        assert!(saved.matches_uid("abc-123"));
        assert!(!saved.matches_uid("other"));
        assert!(!saved.matches_uid(""));
        assert!(!session("", 0).matches_uid(""));
    }

    #[test]
    fn resave_keeps_verified_at_and_clear_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("web_session.cn.json");

        let mut first = session("u1", 1_000);
        first.verified_at_ms = 1_500;
        save_to(&path, &first).unwrap();

        // 重新登录（新 cookie、还没验证）⇒ 保留上一次的 verifiedAt
        let mut second = session("u1", 2_000);
        second.cookie = "c=3".into();
        save_to(&path, &second).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.cookie, "c=3");
        assert_eq!(loaded.verified_at_ms, 1_500, "同账号重存应保留上次验证时刻");

        // 换账号 ⇒ 不得继承别人的验证时刻
        let mut third = session("u2", 3_000);
        third.cookie = "d=4".into();
        save_to(&path, &third).unwrap();
        assert_eq!(load_from(&path).unwrap().verified_at_ms, 0);

        clear_at(&path).unwrap();
        assert!(load_from(&path).is_none());
        clear_at(&path).expect("再删一次必须成功（幂等）");
    }

    #[test]
    fn corrupt_file_reads_as_none_instead_of_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("web_session.cn.json");
        std::fs::write(&path, "{not json").unwrap();
        assert!(load_from(&path).is_none(), "损坏按「没有」处理，不 panic");
        // 空 cookie 的文件也当没有（半写状态）
        std::fs::write(&path, r#"{"uid":"u1","cookie":"  ","savedAtMs":0}"#).unwrap();
        assert!(load_from(&path).is_none());
    }
}
