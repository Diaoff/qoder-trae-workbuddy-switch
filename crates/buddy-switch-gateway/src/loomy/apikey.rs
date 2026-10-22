//! Loomy 独立网关的多 API Key 存储（`sk-loomy-` 前缀 + 三态动作）。
//!
//! 与另外三条产品线（`sk-wb-` / `sk-trae-` / `sk-qoder-` / `sk-uni-`）**同款结构**：
//! 明文不落库、只存 `sha256`；`prefix` / `suffix` 供脱敏展示
//! （`prefix••••••••suffix`，**不含省略号**）；三态动作 **吊销 → 恢复 → 删除**，
//! 且**已吊销的行必须同时给「恢复 + 删除」两个出口**（本仓产品纪律：只给删除会让
//! 「误吊销」只能靠删号重来）。
//!
//! ## 与 Qoder 侧的**两处刻意差异**
//!
//! 1. **没有 `region` 字段**：Loomy 只有一套账号体系（单作用域 `loomy-cn`），
//!    给 Key 加一个恒为 `cn` 的归属轴就是**假控件**的孪生形态（数据里多一列永远不变的值）。
//!    将来出国际版时再加 variant 字段 —— 那时它是真的可变量。
//! 2. **没有 legacy 回落**：Loomy 网关是新增能力，不存在「设置里的旧 apiKey」可迁。
//!
//! ## 前缀为什么是 `sk-loomy-`
//!
//! 四条产品线的 Key 前缀是**同一族**，看一眼就知道该配哪个地址：
//! `sk-wb-`（WorkBuddy）· `sk-trae-`（Trae）· `sk-qoder-`（Qoder）· `sk-loomy-`（Loomy）
//! · `sk-uni-`（统一网关）。
//!
//! ⚠️ **校验只比对 `sha256(完整明文)`，不看前缀** ⇒ 前缀改名不废老 Key。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// 新 Key 的前缀字面量（创建时 `sk-loomy-<secret 前 4 位>`）。
pub const KEY_PREFIX: &str = "sk-loomy-";

/// 明文 Key 的密钥段长度（32 位十六进制，与另三条产品线一致）。
const SECRET_HEX_LEN: usize = 32;

/// 单条 Loomy API Key（明文不落库）。
///
/// 磁盘 / 线上形状为 camelCase（`createdAt` / `revokedAt` / `lastUsedAt`），前端类型逐字对齐。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoomyApiKeyRecord {
    pub id: String,
    pub name: String,
    /// 前缀（脱敏展示用），如 `sk-loomy-a1b2`。⚠️ **不含省略号**。
    pub prefix: String,
    /// 尾部 4 位（如 `9f3c`）。列表显示 `prefix••••••••suffix`。
    #[serde(default)]
    pub suffix: String,
    /// `sha256(hex)` of 完整明文 Key。
    pub hash: String,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
    #[serde(default)]
    pub last_used_at: Option<i64>,
}

impl LoomyApiKeyRecord {
    /// 脱敏展示（**不含 hash**，可安全下发到前端）。
    pub fn masked(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "prefix": self.prefix,
            "suffix": self.suffix,
            "createdAt": self.created_at,
            "revokedAt": self.revoked_at,
            "revoked": self.revoked_at.is_some(),
            "lastUsedAt": self.last_used_at,
        })
    }

    /// 是否已吊销。
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }
}

/// `sha256` 十六进制小写。
fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn random_id() -> String {
    // 新 Key 的 id 与 secret 都来自 UUID（v4）：id 不泄密，secret 是 32 位十六进制。
    uuid::Uuid::new_v4().simple().to_string()
}

/// 读写 `loomy/gateway_keys.json` 的多 Key 存储。
///
/// **无进程内缓存**：每次操作都读盘 —— 独立监听与宿主路由两份实例创建即可见、
/// 吊销即时生效（与另三条产品线同一纪律）。
pub struct LoomyApiKeyStore {
    path: PathBuf,
}

impl LoomyApiKeyStore {
    /// 新建存储（不读取文件；每次操作实时读盘）。
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 存储文件路径（供诊断 / 展示）。
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// 读取 Key 列表（文件缺失 / 损坏 ⇒ 空表）。
    pub fn load(&self) -> Vec<LoomyApiKeyRecord> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str::<Vec<LoomyApiKeyRecord>>(&text).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn save(&self, records: &[LoomyApiKeyRecord]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let text = serde_json::to_string_pretty(records).map_err(|error| error.to_string())?;
        buddy_switch_core::modules::config::atomic_write(&self.path, &text)
            .map_err(|error| error.to_string())
    }

    /// 创建一把新 Key：返回 `(记录, 明文)` —— **明文只在这一刻出现一次**。
    pub fn create(&self, name: String) -> Result<(LoomyApiKeyRecord, String), String> {
        let secret = format!("{KEY_PREFIX}{}", &random_id()[..SECRET_HEX_LEN]);
        let record = LoomyApiKeyRecord {
            id: random_id(),
            name: if name.trim().is_empty() {
                "未命名 Key".to_string()
            } else {
                name.trim().to_string()
            },
            prefix: format!(
                "{KEY_PREFIX}{}",
                &secret[KEY_PREFIX.len()..KEY_PREFIX.len() + 4]
            ),
            suffix: secret.chars().skip(secret.chars().count() - 4).collect(),
            hash: sha256_hex(&secret),
            created_at: now_ms(),
            revoked_at: None,
            last_used_at: None,
        };
        let mut records = self.load();
        records.push(record.clone());
        self.save(&records)?;
        Ok((record, secret))
    }

    /// 校验一把明文 Key：命中且未吊销 ⇒ 记录，并**顺手 touch** `lastUsedAt`。
    pub fn verify(&self, presented: &str) -> Option<LoomyApiKeyRecord> {
        let trimmed = presented.trim();
        if trimmed.is_empty() {
            return None;
        }
        let hash = sha256_hex(trimmed);
        let records = self.load();
        let record = records
            .iter()
            .find(|record| record.hash == hash && !record.is_revoked())?;
        self.touch(&record.id);
        Some(record.clone())
    }

    /// 记录一次使用（写盘失败**忽略** —— 使用时间戳是统计信息，不能阻断请求）。
    pub fn touch(&self, id: &str) {
        let mut records = self.load();
        if let Some(record) = records.iter_mut().find(|record| record.id == id) {
            record.last_used_at = Some(now_ms());
            let _ = self.save(&records);
        }
    }

    /// 吊销（置 `revoked_at`；已吊销 ⇒ 可读错误）。
    pub fn revoke(&self, id: &str) -> Result<(), String> {
        let mut records = self.load();
        let record = records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| "找不到该 Key".to_string())?;
        if record.is_revoked() {
            return Err("该 Key 已是吊销状态".to_string());
        }
        record.revoked_at = Some(now_ms());
        self.save(&records)
    }

    /// 恢复吊销（清 `revoked_at`；未吊销 ⇒ 可读错误）。
    pub fn restore(&self, id: &str) -> Result<(), String> {
        let mut records = self.load();
        let record = records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| "找不到该 Key".to_string())?;
        if !record.is_revoked() {
            return Err("该 Key 未被吊销，无需恢复".to_string());
        }
        record.revoked_at = None;
        self.save(&records)
    }

    /// 删除（**仅已吊销可删** —— 未吊销必须先吊销，防误删在用凭据）。
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let records = self.load();
        let record = records
            .iter()
            .find(|record| record.id == id)
            .ok_or_else(|| "找不到该 Key".to_string())?;
        if !record.is_revoked() {
            return Err("只能删除已吊销的 Key：请先吊销，再删除".to_string());
        }
        let remaining: Vec<LoomyApiKeyRecord> = records
            .into_iter()
            .filter(|record| record.id != id)
            .collect();
        self.save(&remaining)
    }

    /// 列表（脱敏）。
    pub fn list(&self) -> Vec<Value> {
        self.load().iter().map(LoomyApiKeyRecord::masked).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (LoomyApiKeyStore, PathBuf) {
        let path = std::env::temp_dir().join(format!("loomy-keys-test-{}.json", random_id()));
        (LoomyApiKeyStore::new(path.clone()), path)
    }

    /// 三态动作的合同：吊销 → 恢复 → 删除；未吊销不可删；记录里**永远没有明文**。
    ///
    /// 变异：把 `delete` 的「仅已吊销可删」守卫删掉 ⇒ 本测试红。
    #[test]
    fn key_lifecycle_three_states_and_no_plaintext_on_disk() {
        let (store, path) = temp_store();
        let (record, secret) = store.create("测试".into()).unwrap();
        assert!(secret.starts_with(KEY_PREFIX), "{secret}");
        assert!(record.prefix.starts_with(KEY_PREFIX));

        // 明文**不落盘**：磁盘文件里不得出现 secret 本体。
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(!on_disk.contains(&secret), "磁盘出现明文 Key");

        assert!(store.verify(&secret).is_some());
        assert!(store.load()[0].last_used_at.is_some());

        assert!(store.delete(&record.id).is_err(), "未吊销不可删");
        store.revoke(&record.id).unwrap();
        assert!(store.revoke(&record.id).is_err(), "重复吊销必须报错");
        assert!(store.verify(&secret).is_none(), "吊销后必须拒绝");
        store.restore(&record.id).unwrap();
        assert!(store.verify(&secret).is_some(), "恢复后必须重新可用");
        assert!(store.restore(&record.id).is_err(), "未吊销无需恢复");
        store.revoke(&record.id).unwrap();
        store.delete(&record.id).unwrap();
        assert!(store.load().is_empty());

        let _ = std::fs::remove_file(&path);
    }

    /// 脱敏列表**不含 hash / 明文**；且**没有 `region` 字段**（单作用域平台不该多一列恒值）。
    ///
    /// 变异：往记录里加 `region` 并序列化 ⇒ 本条红。
    #[test]
    fn masked_list_has_no_secret_hash_or_region() {
        let (store, path) = temp_store();
        let (_record, secret) = store.create("m".into()).unwrap();
        let list = store.list();
        let text = serde_json::to_string(&list).unwrap();
        assert!(!text.contains(&secret));
        assert!(!text.contains("hash"), "脱敏列表不得带 hash 字段");
        assert!(!text.contains("region"), "Loomy 单作用域，Key 不该有归属轴");
        assert!(list[0]["prefix"].as_str().unwrap().starts_with(KEY_PREFIX));
        let _ = std::fs::remove_file(&path);
    }

    /// 明文形态与另三条产品线**同族**：`sk-loomy-` + 32 位 hex。
    #[test]
    fn plaintext_shape_matches_the_other_product_lines() {
        let (store, path) = temp_store();
        let (record, secret) = store.create("shape".into()).unwrap();
        assert!(secret.starts_with("sk-loomy-"), "{secret}");
        assert_eq!(
            secret.len(),
            "sk-loomy-".len() + SECRET_HEX_LEN,
            "明文长度应为前缀 + 32 位 hex：{secret}"
        );
        let hex = &secret["sk-loomy-".len()..];
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()), "{hex}");
        assert_eq!(record.prefix, format!("sk-loomy-{}", &hex[..4]));
        let _ = std::fs::remove_file(&path);
    }
}
