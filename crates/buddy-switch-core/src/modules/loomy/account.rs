//! Loomy 的账号模型与账号库读写。
//!
//! ## 存储形态
//!
//! 与 Qoder 侧同构：**按区域分家的一个 JSON 文件**（[`super::paths::accounts_file_for`]，
//! Loomy 当前只有一个区域），里面是一个账号数组。账号以 `uid` 为唯一键。
//!
//! ## 两套形态，**不要混**
//!
//! | 形态 | 用途 | 含令牌明文 |
//! |:--|:--|:--|
//! | [`RawAccount`] | **落盘** | 是（它本来就是凭据库） |
//! | [`account_view`] | **下发给前端** | **否**（只给 `has*` 布尔与掩码） |
//!
//! ⚠️ 前端**永远拿不到**令牌明文 —— 与账号卡片「不展示凭据」的产品约定一致，
//! 也避免截图 / 投屏泄露（本项目用户经常贴截图报障）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::modules::loomy::paths;
use crate::modules::loomy::region::LoomyRegion;

/// 账号来源：面板粘贴凭据导入。
pub const SOURCE_IMPORT: &str = "import";
/// 账号来源：面板短信验证码登录。
pub const SOURCE_SMS_LOGIN: &str = "sms-login";
/// 账号来源：面板微信扫码登录（二期）。
pub const SOURCE_WECHAT_LOGIN: &str = "wechat-login";

/// 一条账号记录（**落盘形态**，含令牌明文）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RawAccount {
    /// 账号唯一 id（userid 或派生值；见 `credential.rs` 的 uid 口径）。
    pub uid: String,
    /// 讯飞 session（业务凭据）。
    #[serde(default)]
    pub access_token: String,
    /// 讯飞 userid（可空 —— 派生 uid 的账号可能没有）。
    #[serde(default)]
    pub userid: String,
    /// 手机号（可空）。
    #[serde(default)]
    pub phone: String,
    /// 昵称（可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    /// 过期时刻（毫秒时间戳；0 = 未知）。
    #[serde(default)]
    pub expires_at: i64,
    /// 来源（[`SOURCE_IMPORT`] / [`SOURCE_SMS_LOGIN`] / [`SOURCE_WECHAT_LOGIN`]）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 用户备注。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remark: Option<String>,
    /// 自定义分组（自由文本；缺省 = 未分组）—— 与 WorkBuddy / Qoder 的 `group` 同语义。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// 永久积分（最近一次查询的快照）。
    #[serde(default)]
    pub credits: i64,
    /// 每日池余额（最近一次查询的快照；当天作废）。
    #[serde(default)]
    pub daily_credits: i64,
    /// 签到本地盖章（`YYYY-MM-DD HH:MM:SS`，本地时区；展示与调度去重提示，非判据）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checkin: Option<String>,
    /// 加入时刻（ISO-8601）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_at: Option<String>,
    /// 最近更新时刻（ISO-8601）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
/// 账号库文件（一个区域一本）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountsFile {
    /// 账号列表（顺序即界面顺序）。
    #[serde(default)]
    pub accounts: Vec<RawAccount>,
}

/// 读账号库（容错：文件缺失 / 损坏 ⇒ 空库）。
pub fn load_accounts_for(region: LoomyRegion) -> AccountsFile {
    load_accounts_checked_for(region).unwrap_or_default()
}

/// 读账号库并如实报错（写路径用：必须知道是「在空库上新建」还是「在损坏库上覆盖」）。
pub fn load_accounts_checked_for(region: LoomyRegion) -> Result<AccountsFile, String> {
    let path = paths::accounts_file_for(region);
    Ok(
        crate::modules::qoder::store::read_json_checked::<AccountsFile>(&path)?
            .unwrap_or_default(),
    )
}

/// 写账号库（原子写 + 先建目录）。复用 `qoder::store` 的原子写实现（同款不重写）。
pub fn save_accounts_for(region: LoomyRegion, file: &AccountsFile) -> Result<(), String> {
    crate::modules::qoder::store::write_json(&paths::accounts_file_for(region), file)
}

/// 查一个账号。
pub fn find_account_for(region: LoomyRegion, uid: &str) -> Option<RawAccount> {
    load_accounts_for(region)
        .accounts
        .into_iter()
        .find(|account| account.uid == uid)
}

/// 新增 / 覆盖一条账号（按 `uid` 去重；Loomy 无 domain 轴，不做归属校验）。
///
/// 返回写入结果（新增还是覆盖）；uid 为空 ⇒ `Err`（uid 是账号库的唯一键）。
pub fn upsert_account_for(
    region: LoomyRegion,
    mut account: RawAccount,
) -> Result<UpsertOutcome, String> {
    if account.uid.trim().is_empty() {
        return Err("账号缺少 uid —— 已拒绝写入（uid 是账号库的唯一键）。".to_string());
    }
    let mut file = load_accounts_checked_for(region)?;
    let now = crate::modules::qoder::store::now_iso();
    let outcome = match file
        .accounts
        .iter_mut()
        .find(|existing| existing.uid == account.uid)
    {
        Some(existing) => {
            // 保留首次加入时刻；更新其余字段。
            account.added_at = existing.added_at.clone().or(account.added_at.take());
            account.updated_at = Some(now);
            *existing = account;
            UpsertOutcome::Replaced
        }
        None => {
            account.added_at = account.added_at.or(Some(now.clone()));
            account.updated_at = Some(now);
            file.accounts.push(account);
            UpsertOutcome::Added
        }
    };
    save_accounts_for(region, &file)?;
    Ok(outcome)
}

/// 一条账号被写入后的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    /// 新增。
    Added,
    /// 覆盖了同 uid 的既有记录。
    Replaced,
}

/// 更新备注（唯一的「只改一个字段」入口；避免调用方走 upsert 把令牌一起覆盖）。
pub fn set_remark_for(
    region: LoomyRegion,
    uid: &str,
    remark: Option<String>,
) -> Result<bool, String> {
    let mut file = load_accounts_checked_for(region)?;
    let Some(account) = file.accounts.iter_mut().find(|a| a.uid == uid) else {
        return Ok(false);
    };
    account.remark = remark;
    account.updated_at = Some(crate::modules::qoder::store::now_iso());
    save_accounts_for(region, &file)?;
    Ok(true)
}

/// 更新分组（自由文本；与备注同款「只改一个字段」入口）。
pub fn set_group_for(
    region: LoomyRegion,
    uid: &str,
    group: Option<String>,
) -> Result<bool, String> {
    let mut file = load_accounts_checked_for(region)?;
    let Some(account) = file.accounts.iter_mut().find(|a| a.uid == uid) else {
        return Ok(false);
    };
    account.group = group;
    account.updated_at = Some(crate::modules::qoder::store::now_iso());
    save_accounts_for(region, &file)?;
    Ok(true)
}

/// 回写刷新得到的新 session（只改令牌与过期时刻，避免覆盖其它字段）。
///
/// ⚠️ Loomy 没有 refresh 端点，本函数目前只被**短信重登**路径使用（换新 session）。
pub fn set_session_for(
    region: LoomyRegion,
    uid: &str,
    access_token: &str,
    expires_at: i64,
) -> Result<bool, String> {
    let mut file = load_accounts_checked_for(region)?;
    let Some(account) = file.accounts.iter_mut().find(|a| a.uid == uid) else {
        return Ok(false);
    };
    account.access_token = access_token.to_string();
    account.expires_at = expires_at;
    account.updated_at = Some(crate::modules::qoder::store::now_iso());
    save_accounts_for(region, &file)?;
    Ok(true)
}

/// 删除一条账号（Loomy 一期不做软删除 / 回收站 —— 单作用域平台，误删可重新导入或重登；
/// Qoder 的回收站属「多区客户端会话同步」场景，Loomy 无该需求）。
pub fn delete_account_for(region: LoomyRegion, uid: &str) -> Result<bool, String> {
    let mut file = load_accounts_checked_for(region)?;
    let before = file.accounts.len();
    file.accounts.retain(|account| account.uid != uid);
    if file.accounts.len() == before {
        return Ok(false);
    }
    save_accounts_for(region, &file)?;
    Ok(true)
}

/// 展示名回落：昵称 > 手机号 > userid > uid。
///
/// `pub`：批量任务（签到 / 采样）的结果里要带可读名字，否则前端只能显示裸 uid。
pub fn display_name(account: &RawAccount) -> String {
    if let Some(nickname) = account.nickname.as_deref().filter(|s| !s.is_empty()) {
        return nickname.to_string();
    }
    if !account.phone.is_empty() {
        return format!("Loomy {}", account.phone);
    }
    if !account.userid.is_empty() {
        return format!("Loomy {}", account.userid);
    }
    format!("Loomy {}", account.uid)
}

fn display_opt(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

/// 账号 → 前端下发形态（**不含令牌明文**）。
pub fn account_view(account: &RawAccount, region: LoomyRegion) -> Value {
    let has_credential = !account.access_token.is_empty();
    json!({
        "uid": account.uid,
        "name": display_name(account),
        "nickname": display_opt(&account.nickname),
        "region": region.as_str(),
        "source": display_opt(&account.source),
        "remark": display_opt(&account.remark),
        "group": display_opt(&account.group),
        "expiresAt": account.expires_at,
        // 令牌**永不下发明文**：只给「有没有」与掩码。
        "hasCredential": has_credential,
        "accessTokenMasked": if account.access_token.is_empty() {
            Value::Null
        } else {
            Value::String(crate::modules::qoder::store::mask(&account.access_token))
        },
        "userid": account.userid,
        "phone": account.phone,
        "credits": account.credits,
        "dailyCredits": account.daily_credits,
        "lastCheckin": display_opt(&account.last_checkin),
        "addedAt": display_opt(&account.added_at),
        "updatedAt": display_opt(&account.updated_at),
    })
}

/// 整个区域的账号列表（下发形态）。
pub fn accounts_view_for(region: LoomyRegion) -> Value {
    let file = load_accounts_for(region);
    let views: Vec<Value> = file
        .accounts
        .iter()
        .map(|account| account_view(account, region))
        .collect();
    json!({
        "region": region.as_str(),
        "accounts": views,
    })
}

/// 账号 → 选号池所需的元数据（`uid` → 有无可用凭据）。
///
/// 供网关选号时判断「这个账号能不能用来转发」，**不读令牌明文**。
pub fn pool_meta_for(region: LoomyRegion) -> BTreeMap<String, bool> {
    load_accounts_for(region)
        .accounts
        .into_iter()
        .map(|account| (account.uid.clone(), !account.access_token.is_empty()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn sample_account(uid: &str) -> RawAccount {
        RawAccount {
            uid: uid.to_string(),
            access_token: "a".repeat(32),
            userid: uid.to_string(),
            phone: String::new(),
            nickname: None,
            expires_at: 0,
            source: Some(SOURCE_IMPORT.to_string()),
            remark: None,
            group: None,
            credits: 0,
            daily_credits: 0,
            last_checkin: None,
            added_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn account_view_never_leaks_the_session_token() {
        let account = sample_account("10086");
        let view = account_view(&account, LoomyRegion::Cn);
        let text = view.to_string();
        assert!(!text.contains(&"a".repeat(32)), "泄露了 session 明文：{text}");
        assert!(view["hasCredential"].as_bool().unwrap());
        assert!(view["accessTokenMasked"]
            .as_str()
            .map(|masked| masked.contains('…'))
            .unwrap_or(false));
    }

    #[test]
    fn display_name_prefers_nickname_then_phone_then_userid() {
        let mut account = sample_account("10086");
        assert_eq!(display_name(&account), "Loomy 10086");
        account.phone = "13812345678".to_string();
        assert_eq!(display_name(&account), "Loomy 13812345678");
        account.nickname = Some("小讯".to_string());
        assert_eq!(display_name(&account), "小讯");
    }
}

#[cfg(test)]
mod io_tests {
    use super::tests::sample_account;
    use super::*;

    // ⚠️ 本模块的 IO 测试直接写真实账号库路径（同 region = 同一文件），并行会互相覆盖
    // ⇒ 全部 IO 用例合并在**一个** test 函数里串行执行。
    #[test]
    fn io_round_trip_upsert_delete_and_uid_guard() {
        let region = LoomyRegion::Cn;
        // 隔离 home：写 `~/.buddy-switch/loomy/` 的测试绝不触碰真实用户数据
        // （与 backup.rs 的 isolated_home 同款；guard 持锁至 drop ⇒ 与其它测试串行）。
        let home = std::env::temp_dir().join(format!(
            "buddy-switch-loomy-account-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&home).expect("create isolated home");
        let _guard = crate::modules::config::HomeOverrideGuard::set(&home);
        let mut first = sample_account("20001");
        first.nickname = Some("一号".to_string());
        assert_eq!(upsert_account_for(region, first.clone()), Ok(UpsertOutcome::Added));
        let mut second = sample_account("20001");
        second.nickname = Some("改名".to_string());
        assert_eq!(
            upsert_account_for(region, second),
            Ok(UpsertOutcome::Replaced)
        );
        let file = load_accounts_for(region);
        assert_eq!(file.accounts.len(), 1, "同 uid 去重");
        assert_eq!(file.accounts[0].nickname.as_deref(), Some("改名"));
        assert!(file.accounts[0].added_at.is_some(), "首次加入时刻被保留");
        // 清理（测试直接写真实账号库目录 —— 与 qoder 模块单测同款做法）。
        let _ = std::fs::remove_file(paths::accounts_file_for(region));

        // 空 uid 拒写。
        {
            let mut bad = sample_account("");
            bad.uid = "  ".to_string();
            assert!(upsert_account_for(region, bad).is_err());
        }
        upsert_account_for(region, sample_account("30001")).unwrap();
        upsert_account_for(region, sample_account("30002")).unwrap();
        assert_eq!(delete_account_for(region, "30001"), Ok(true));
        let file = load_accounts_for(region);
        assert_eq!(file.accounts.len(), 1);
        assert_eq!(file.accounts[0].uid, "30002");
        assert_eq!(delete_account_for(region, "30001"), Ok(false), "重复删除返回 false");
        let _ = std::fs::remove_file(paths::accounts_file_for(region));
    }
}
