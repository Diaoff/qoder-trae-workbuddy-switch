//! 统一网关的 API Key 存储（`~/.buddy-switch/gateway_unified_keys.json`）。
//!
//! # 与两个既有 Key 库的关系：**独立的第三份**
//!
//! WB（`gateway_keys.json`，键绑 region）与 Trae（`trae/api_gateway_keys.json`，键绑 variant）
//! 各持一份；统一入口**不复用它们**，理由有两条：
//!
//! 1. **语义不同**：前两者的归属是一个产品线内的域/程序位；统一入口的归属是
//!    **命名空间集合**（一把 Key 可同时授权 `wb-*` 与 `trae-*`，这是「合并入口」的全部意义）。
//! 2. **越权风险**：复用任何一份都会让「改统一入口的 Key」与「改那个产品线自己的 Key」
//!    互相影响 —— 用户在两处看到的是同一把 Key，却只有一处能改，排查时无法推理。
//!
//! ⇒ 老 Key **不能**用于统一入口（会 401）。这是有意的：三个入口的 Key 库互不牵连，
//! 关掉统一入口不影响任何既有客户端。
//!
//! # 与 WB 那份的三处差异（其余逐字同构，便于对照阅读）
//!
//! | 处 | WB | 统一入口 |
//! |:--|:--|:--|
//! | 归属 | `region: Region` | `namespaces: Vec<String>` + `biz_namespace: Option<String>` |
//! | 前缀 | `sk-wb-` | `sk-uni-` |
//! | `create` | 不会失败 | **会失败**（要校验命名空间在册、业务转发只能是 `wb-*`） |
//!
//! 其余（只存 sha256 + 前 4 / 后 4 位、明文只返回一次、每次操作实时读盘、
//! 吊销可逆 / 删除只允许已吊销）**逐条照搬** —— 这些是既有实现踩出来的，不重新发明。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use buddy_switch_core::modules::config as core_config;
use buddy_switch_core::modules::gateway_namespace;

use super::config::unified_keys_file;
use crate::apikey::{constant_time_eq, sha256_hex};

/// 统一网关 Key 的前缀（与 `sk-wb-` / `sk-trae-` 同族，一眼认出来源）。
pub const UNIFIED_KEY_PREFIX: &str = "sk-uni-";

/// 单条统一密钥记录（明文不落库）。
///
/// ⚠️ **不加 `deny_unknown_fields`**：将来前端加了字段而宿主还没升级时，
/// 严格模式会让**整份台账解析失败** ⇒ 回落空表 ⇒ 用户的全部 Key「凭空消失」。
/// 未知字段原样保留，只忽略即可。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedApiKeyRecord {
    /// 稳定标识。
    pub id: String,
    /// 用户起的名字。
    pub name: String,
    /// 前缀，如 `sk-uni-a1b2`（列表脱敏展示用）。
    pub prefix: String,
    /// 尾部 4 位（如 `9f3c`）—— 与 `prefix` 同为**非敏感**片段，
    /// 列表里显示成 `sk-uni-a1b2••••••••9f3c`，用户才能确认是哪一把。
    #[serde(default)]
    pub suffix: String,
    /// `sha256(hex)` of 完整明文。
    pub hash: String,
    /// ★ 授权使用的命名空间集合（空数组 ⇒ **无授权**，fail-closed，不等于「全开」）。
    #[serde(default)]
    pub namespaces: Vec<String>,
    /// 业务转发（`/biz/*path`）用哪个命名空间。仅 `wb-*` 合法；`None` ⇒ 该端点返回 400。
    ///
    /// 为什么落在 Key 上而不是请求头：业务转发的**真实调用方是客户端自身**，
    /// 它加不了自定义头（见 `UNIFIED-GATEWAY-ARCH.md` §4.5）。
    #[serde(default)]
    pub biz_namespace: Option<String>,
    /// 创建时间（毫秒）。
    pub created_at: i64,
    /// 吊销时间；`None` = 启用中。
    pub revoked_at: Option<i64>,
    /// 最近使用时间（60 秒节流写盘）。
    #[serde(default)]
    pub last_used_at: Option<i64>,
}

impl UnifiedApiKeyRecord {
    /// 是否处于启用态。
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// 该 Key 是否被授权使用某个命名空间（**精确匹配**，与命名空间表的解析口径一致）。
    pub fn authorizes(&self, namespace: &str) -> bool {
        self.namespaces.iter().any(|id| id == namespace)
    }

    /// 脱敏展示（**不含 `hash`**）。
    pub fn masked(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "prefix": self.prefix,
            "suffix": self.suffix,
            "namespaces": self.namespaces,
            "bizNamespace": self.biz_namespace,
            "createdAt": self.created_at,
            "revokedAt": self.revoked_at,
            "revoked": self.revoked_at.is_some(),
            "lastUsedAt": self.last_used_at,
        })
    }
}

/// 读写 `gateway_unified_keys.json` 的 Key 存储。
///
/// 与 WB 那份一样**每次操作都读盘、无进程内缓存** —— 宿主的管理面与独立监听是两份实例，
/// 缓存会让「刚创建的 Key」在另一份里看不见。
pub struct UnifiedApiKeyStore {
    path: PathBuf,
}

impl Default for UnifiedApiKeyStore {
    fn default() -> Self {
        Self::new(unified_keys_file())
    }
}

impl UnifiedApiKeyStore {
    /// 使用指定文件（便于测试）。
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load(&self) -> Vec<UnifiedApiKeyRecord> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str::<Vec<UnifiedApiKeyRecord>>(&text).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn save(&self, records: &[UnifiedApiKeyRecord]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let content = serde_json::to_string_pretty(records).map_err(|error| error.to_string())?;
        core_config::atomic_write(&self.path, &content).map_err(|error| error.to_string())
    }

    /// 校验一份授权集（创建与修改共用，**唯一判断点**）。
    ///
    /// 规则：
    /// - 每个 id 必须**在册**（拼错的 id 静默丢弃 = 用户「加了却没生效」，故明确报错）；
    /// - 至少一条（空授权集的 Key 能创建出来，但什么都调不动 —— 是纯粹的坑）；
    /// - `biz_namespace` 若有，必须**同时**在本 Key 的授权集内、且是 **WorkBuddy** 命名空间
    ///   （业务转发只有 WB 有这条通道；Trae 业务域不可配）。
    pub fn validate_authorization(
        namespaces: &[String],
        biz_namespace: Option<&str>,
    ) -> Result<(), String> {
        if namespaces.is_empty() {
            return Err("至少要授权一个命名空间".to_string());
        }
        for id in namespaces {
            if gateway_namespace::parse(id).is_none() {
                return Err(format!(
                    "命名空间「{id}」不在册。当前在册：{}",
                    super::config::known_ids().join(" / ")
                ));
            }
        }
        if let Some(biz) = biz_namespace {
            let spec = gateway_namespace::parse(biz).ok_or_else(|| {
                format!(
                    "业务转发命名空间「{biz}」不在册。当前在册：{}",
                    super::config::known_ids().join(" / ")
                )
            })?;
            if !namespaces.iter().any(|id| id == biz) {
                return Err(format!(
                    "业务转发命名空间「{biz}」必须也在本 Key 的授权集内"
                ));
            }
            if gateway_namespace::wb_region_of(spec).is_none() {
                return Err(format!(
                    "业务转发命名空间「{biz}」不是 WorkBuddy 命名空间 —— 业务转发只有 WorkBuddy 有这条通道"
                ));
            }
        }
        Ok(())
    }

    /// 生成 Key：明文 `sk-uni-` + 32 位 hex；返回 (记录, 明文)。**明文仅此一次返回**。
    ///
    /// 与 WB 那份不同，本方法**会失败**（授权集不合法）—— 故先校验再落盘，
    /// 避免「创建成功但一把废 Key」。
    pub fn create(
        &self,
        name: String,
        namespaces: Vec<String>,
        biz_namespace: Option<String>,
    ) -> Result<(UnifiedApiKeyRecord, String), String> {
        Self::validate_authorization(&namespaces, biz_namespace.as_deref())?;
        let secret = uuid::Uuid::new_v4().simple().to_string(); // 32 hex
        let plaintext = format!("{UNIFIED_KEY_PREFIX}{secret}");
        let prefix = format!("{UNIFIED_KEY_PREFIX}{}", &secret[..4]);
        let suffix = secret[secret.len() - 4..].to_string();
        let record = UnifiedApiKeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            prefix,
            suffix,
            hash: sha256_hex(&plaintext),
            namespaces,
            biz_namespace,
            created_at: core_config::now_ms(),
            revoked_at: None,
            last_used_at: None,
        };
        let mut records = self.load();
        records.push(record.clone());
        self.save(&records)?;
        Ok((record, plaintext))
    }

    /// 常量时间比较哈希；无效 / 不存在 / **已吊销** 一律 `None`。
    pub fn verify(&self, presented: &str) -> Option<UnifiedApiKeyRecord> {
        let presented = presented.trim();
        if presented.is_empty() {
            return None;
        }
        let digest = sha256_hex(presented);
        for record in self.load() {
            if constant_time_eq(record.hash.as_bytes(), digest.as_bytes()) {
                if !record.is_active() {
                    return None;
                }
                return Some(record);
            }
        }
        None
    }

    /// 更新最近使用时间（60 秒节流，避免每个请求都写盘）。
    pub fn touch(&self, id: &str) {
        let mut records = self.load();
        let now = core_config::now_ms();
        let mut changed = false;
        for record in records.iter_mut() {
            if record.id == id {
                let stale = record
                    .last_used_at
                    .map(|last| now - last > 60_000)
                    .unwrap_or(true);
                if stale {
                    record.last_used_at = Some(now);
                    changed = true;
                }
                break;
            }
        }
        if changed {
            let _ = self.save(&records);
        }
    }

    /// 吊销（置 `revoked_at`，**不物理删除** —— 恢复是同一把 Key，客户端无需改配置）。
    pub fn revoke(&self, id: &str) -> Result<(), String> {
        let mut records = self.load();
        if !records.iter().any(|record| record.id == id) {
            return Err("密钥不存在".to_string());
        }
        for record in records.iter_mut() {
            if record.id == id {
                record.revoked_at = Some(core_config::now_ms());
                break;
            }
        }
        self.save(&records)
    }

    /// 撤销吊销（清 `revoked_at` ⇒ 同一把 Key **重新可用**）。
    ///
    /// 与 [`Self::delete`] 同构但方向相反：**只接受已吊销的** Key ——
    /// 对一把本来就能用的 Key「恢复」是空操作，静默成功会让用户以为它曾被吊销过。
    ///
    /// 恢复的判据看**行为**（`verify` 重新通过），不是看字段被清。
    pub fn restore(&self, id: &str) -> Result<(), String> {
        let mut records = self.load();
        match records.iter().find(|record| record.id == id) {
            None => return Err("密钥不存在".to_string()),
            Some(record) if record.is_active() => return Err("该密钥未吊销，无需恢复".to_string()),
            Some(_) => {}
        }
        for record in records.iter_mut() {
            if record.id == id {
                record.revoked_at = None;
                break;
            }
        }
        self.save(&records)
    }

    /// 物理删除（**仅允许删除已吊销的** Key）。
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut records = self.load();
        let before = records.len();
        match records.iter().find(|record| record.id == id) {
            None => return Err("密钥不存在".to_string()),
            Some(record) if record.is_active() => return Err("请先吊销该密钥再删除".to_string()),
            Some(_) => {}
        }
        records.retain(|record| record.id != id);
        debug_assert!(records.len() < before);
        self.save(&records)
    }

    /// 列出全部 Key（含已吊销）。
    pub fn list(&self) -> Vec<UnifiedApiKeyRecord> {
        self.load()
    }

    /// 存储文件路径（测试与诊断用）。
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (UnifiedApiKeyStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "buddy-switch-unified-keys-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (UnifiedApiKeyStore::new(dir.join("keys.json")), dir)
    }

    fn all_four() -> Vec<String> {
        vec![
            "wb-cn".into(),
            "wb-global".into(),
            "trae-cn".into(),
            "trae-global".into(),
        ]
    }

    /// ★ 一把 Key 可**同时**授权两个产品线的命名空间 —— 这是「合并入口」的全部意义。
    #[test]
    fn one_key_can_authorize_both_products() {
        let (store, dir) = temp_store();
        let (record, plaintext) = store
            .create("主力".into(), all_four(), Some("wb-cn".into()))
            .unwrap();
        assert!(plaintext.starts_with(UNIFIED_KEY_PREFIX));
        assert_eq!(plaintext.len(), UNIFIED_KEY_PREFIX.len() + 32);
        assert_eq!(record.prefix.len(), UNIFIED_KEY_PREFIX.len() + 4);
        assert_eq!(record.suffix.len(), 4);
        for ns in ["wb-cn", "wb-global", "trae-cn", "trae-global"] {
            assert!(record.authorizes(ns), "应授权 {ns}");
        }
        assert!(!record.authorizes("traework-cn"), "未授权的不该被认");
        assert!(!record.authorizes("wb-cnx"), "必须是精确匹配");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// 明文只存 sha256：台账里**不得**出现明文，脱敏出口**不得**含 hash。
    #[test]
    fn plaintext_is_never_persisted_and_masked_hides_the_hash() {
        let (store, dir) = temp_store();
        let (record, plaintext) = store
            .create("k".into(), vec!["wb-cn".into()], None)
            .unwrap();
        let raw = std::fs::read_to_string(store.path()).unwrap();
        assert!(!raw.contains(&plaintext), "台账里不得出现明文");
        assert!(raw.contains(&record.hash), "台账里应当只有 hash");
        let masked = record.masked();
        assert!(masked.get("hash").is_none(), "脱敏出口不得含 hash");
        assert_eq!(masked["namespaces"][0], "wb-cn");
        assert_eq!(masked["revoked"], false);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// 验证：正确明文通过；改一个字符 / 空串 / 未知串一律不过。
    #[test]
    fn verify_accepts_only_the_exact_plaintext() {
        let (store, dir) = temp_store();
        let (_record, plaintext) = store
            .create("k".into(), vec!["wb-cn".into()], None)
            .unwrap();
        assert!(store.verify(&plaintext).is_some());
        assert!(store.verify(&format!("{plaintext}x")).is_none());
        assert!(store.verify("").is_none());
        assert!(store.verify("sk-uni-0000").is_none());
        // 前后空白被裁剪（客户端复制粘贴常带空格）
        assert!(store.verify(&format!("  {plaintext}\n")).is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// ★ 三态动作：吊销可逆（同一把 Key 重新可用）· 删除**仅限已吊销**。
    ///
    /// 恢复的判据锚**行为**（`verify` 重新通过），不是看字段被清。
    #[test]
    fn revoke_is_reversible_and_delete_requires_revocation() {
        let (store, dir) = temp_store();
        let (record, plaintext) = store
            .create("k".into(), vec!["wb-cn".into()], None)
            .unwrap();

        // 启用中：不能直接删
        assert!(
            store.delete(&record.id).is_err(),
            "启用中的密钥不该能直接删"
        );
        assert!(
            store.restore(&record.id).is_err(),
            "未吊销的密钥「恢复」应报错而不是空操作"
        );

        // 吊销 ⇒ verify 立刻不过
        store.revoke(&record.id).unwrap();
        assert!(store.verify(&plaintext).is_none(), "吊销后必须立刻失效");

        // 恢复 ⇒ 同一把明文重新可用（客户端无需改配置）
        store.restore(&record.id).unwrap();
        assert!(
            store.verify(&plaintext).is_some(),
            "恢复的判据是 verify 重新通过"
        );

        // 再吊销后删除
        store.revoke(&record.id).unwrap();
        assert!(store.verify(&plaintext).is_none());
        store.delete(&record.id).unwrap();
        assert!(store.list().is_empty());

        // 幂等 / 不存在
        assert!(store.delete(&record.id).is_err(), "删除不存在的密钥应报错");
        assert!(store.revoke(&record.id).is_err(), "吊销不存在的密钥应报错");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// ★ 授权集校验：空集 / 不在册 / 业务转发不合法（不在授权集内、或不是 WB）都拒绝。
    #[test]
    fn authorization_validation_rejects_the_four_bad_shapes() {
        // ① 空授权集
        assert!(UnifiedApiKeyStore::validate_authorization(&[], None).is_err());
        // ② 不在册
        let error =
            UnifiedApiKeyStore::validate_authorization(&["wb-cnx".into()], None).unwrap_err();
        assert!(error.contains("wb-cnx"), "应回显出错的那个 id：{error}");
        // ③ 业务转发不在授权集内
        let error =
            UnifiedApiKeyStore::validate_authorization(&["wb-cn".into()], Some("wb-global"))
                .unwrap_err();
        assert!(error.contains("必须也在本 Key 的授权集内"), "{error}");
        // ④ 业务转发是 Trae 命名空间（Trae 业务域不可配）
        let error = UnifiedApiKeyStore::validate_authorization(
            &["wb-cn".into(), "trae-cn".into()],
            Some("trae-cn"),
        )
        .unwrap_err();
        assert!(error.contains("只有 WorkBuddy"), "{error}");

        // 合法：两条都在授权集内且是 WB
        UnifiedApiKeyStore::validate_authorization(
            &["wb-cn".into(), "wb-global".into()],
            Some("wb-global"),
        )
        .unwrap();
    }

    /// `create` 的校验发生在**落盘之前** —— 不能留下「一把废 Key」。
    #[test]
    fn create_validates_before_writing_anything() {
        let (store, dir) = temp_store();
        assert!(store.create("bad".into(), vec![], None).is_err());
        assert!(store
            .create("bad".into(), vec!["nope".into()], None)
            .is_err());
        assert!(store.list().is_empty(), "校验失败不该落盘任何记录");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// 每次操作实时读盘：**另一份实例**刚创建的 Key 必须立刻可见（无进程内缓存）。
    #[test]
    fn a_second_instance_sees_new_keys_immediately() {
        let (store, dir) = temp_store();
        let other = UnifiedApiKeyStore::new(dir.join("keys.json"));
        let (_record, plaintext) = store
            .create("k".into(), vec!["wb-cn".into()], None)
            .unwrap();
        assert!(
            other.verify(&plaintext).is_some(),
            "无缓存 ⇒ 另一份实例必须立刻看到新 Key（宿主的管理面与独立监听是两份实例）"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// 台账损坏 ⇒ 回落空表（不 panic）；未知字段被忽略（老日志兼容）。
    #[test]
    fn corrupt_ledger_falls_back_to_empty_and_unknown_fields_are_ignored() {
        let (store, dir) = temp_store();
        std::fs::write(dir.join("keys.json"), "not json").unwrap();
        assert!(store.list().is_empty());

        std::fs::write(
            dir.join("keys.json"),
            r#"[{"id":"a","name":"n","prefix":"sk-uni-0000","hash":"h","created_at":1,"revoked_at":null,"future_field":42}]"#,
        )
        .unwrap();
        let records = store.list();
        assert_eq!(records.len(), 1, "未知字段不该让整份台账解析失败");
        assert_eq!(records[0].suffix, "", "缺失字段回落空串（显示时省略尾部）");
        assert!(
            records[0].namespaces.is_empty(),
            "缺失授权集 = 无授权（fail-closed）"
        );
        assert!(!records[0].authorizes("wb-cn"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// `touch` 更新最近使用时间，且**不会**影响吊销态 / 授权集。
    #[test]
    fn touch_updates_last_used_without_touching_the_rest() {
        let (store, dir) = temp_store();
        let (record, _plaintext) = store
            .create("k".into(), vec!["wb-cn".into(), "trae-cn".into()], None)
            .unwrap();
        assert!(record.last_used_at.is_none());
        store.touch(&record.id);
        let after = store
            .list()
            .into_iter()
            .find(|r| r.id == record.id)
            .unwrap();
        assert!(after.last_used_at.is_some(), "touch 应写入 last_used_at");
        assert_eq!(after.namespaces, vec!["wb-cn", "trae-cn"], "授权集不该被动");
        assert!(after.is_active());
        // 不存在的 id：静默无操作（不 panic、不新建）
        store.touch("nonexistent");
        assert_eq!(store.list().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
