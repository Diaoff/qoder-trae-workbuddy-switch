//! Loomy 的**全部路径派生**。
//!
//! 与 `qoder/paths.rs` 同款纪律：**路径派生唯一规则**（`product.rs` 模块头）——
//! 作用域字符串 → 解析成枚举 → **查表**派生路径。本模块的每个函数都只收
//! [`LoomyRegion`]（强类型枚举），**不收 `&str`**。
//!
//! 数据目录布局（`~/.buddy-switch/loomy/`）：
//!
//! ```text
//! loomy/
//! ├── settings.json          # 模块设置（单区不分家）
//! ├── accounts.cn.json       # 账号库
//! └── checkin_ledger.cn.json # 今日已签到台账（跨运行累积）
//! ```

use std::path::PathBuf;

use crate::modules::config::store_dir;
use crate::modules::loomy::region::{LoomyRegion};

/// 本模块的数据目录：`<store_dir>/loomy/`。
pub fn loomy_dir() -> PathBuf {
    store_dir().join("loomy")
}

/// 把 `name.ext` 变成 `name.{region}.ext`（插入位置在扩展名之前）。
fn scoped_file_name(name_with_ext: &str, region: LoomyRegion) -> String {
    match name_with_ext.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}.{}.{ext}", region.as_str()),
        None => format!("{name_with_ext}.{}", region.as_str()),
    }
}

/// 按区域分家的数据文件路径（Loomy 单区，但保持与 Qoder 对称的派生形态）。
fn scoped_file_for(name_with_ext: &str, region: LoomyRegion) -> PathBuf {
    loomy_dir().join(scoped_file_name(name_with_ext, region))
}

/// 账号库。
pub fn accounts_file_for(region: LoomyRegion) -> PathBuf {
    scoped_file_for("accounts.json", region)
}

/// 今日已签到台账（跨运行累积，按 uid 记）。
pub fn checkin_ledger_file_for(region: LoomyRegion) -> PathBuf {
    scoped_file_for("checkin_ledger.json", region)
}

/// 模块设置（**不分家**：单区域平台没有区域差异可分）。
pub fn settings_file() -> PathBuf {
    loomy_dir().join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_carry_region_infix_before_extension() {
        let name = accounts_file_for(LoomyRegion::Cn)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        assert_eq!(name, "accounts.cn.json");

        let ledger = checkin_ledger_file_for(LoomyRegion::Cn)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        assert_eq!(ledger, "checkin_ledger.cn.json");
    }

    #[test]
    fn paths_live_under_the_loomy_dir() {
        let dir = loomy_dir();
        assert!(dir.ends_with("loomy"), "{}", dir.display());
        assert!(accounts_file_for(LoomyRegion::Cn).starts_with(&dir));
        assert!(settings_file().starts_with(&dir));
    }
}
