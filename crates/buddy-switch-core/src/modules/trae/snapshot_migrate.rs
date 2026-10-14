//! 客户端**合并**后的登录态快照搬运（issue #20）。
//!
//! ## 背景（本机实证，不是推断）
//!
//! 2026-10-13 客户端把 TraeWork 与 TraeCode 合并成**一个**客户端：CN 叫 `Trae CN`、
//! 国际版叫 `Trae`。客户端自己留了一份合并记录，落在**目标客户端**的 userData 根下
//! （`%APPDATA%\Trae CN\migration-job.json`，逐字）：
//!
//! ```json
//! { "sourceDataPath": "…\\AppData\\Roaming\\TRAE SOLO CN",
//!   "sourceInstallPath": "D:\\Programs\\TRAE SOLO CN",
//!   "sourceAppName": "TRAE SOLO CN",
//!   "targetDataPath": "…\\AppData\\Roaming\\Trae CN",
//!   "removeApp": true, "removeData": false, "status": "completed" }
//! ```
//!
//! 即：`TRAE SOLO CN`（TraeWork）被卸载、数据并入 `Trae CN`。
//!
//! ## 为什么快照必须跟着搬
//!
//! 快照是**某个客户端 userData 的文件副本**，按程序位分家存放
//! （见 [`super::paths::profiles_dir_for_program`]）。合并前用户管的是 TraeWork，
//! 快照自然落在 TraeWork 槽位；合并后活的客户端是 `Trae CN`，它的槽位里**一个快照都没有**
//! ⇒ 用户点「切换账号」得到「该账号的登录态快照不存在，请先保存该登录态」。
//!
//! 那句话**逻辑上没错**（那个槽位确实没有），但对用户不成立 —— 他早就存过了。
//! 更糟的是 TraeWork 槽位此刻指向一个**死掉的 userData**（`removeData: false`
//! 让它留在盘上，但客户端已卸载）⇒ 从 TraeWork 那侧切过去会「成功」却写进死目录。
//! 所以正确做法是把旧槽位的快照**搬进合并槽位**，而不是让用户去点另一个按钮。
//!
//! ## 三条硬约束（与 [`super::region_migrate`] 同款纪律）
//!
//! 1. **只增不改**：目标槽位已有同 uid 时**一律跳过**，绝不覆盖。搬运是"把漏掉的捡起来"，
//!    不是"用旧的换掉新的"——目标槽位里那份可能更新。
//! 2. **只复制，不动源**：源槽位原样保留（它是这次操作的天然备份，也是「降级回旧版本
//!    客户端仍能用」的前提）。
//! 3. **可重复执行**：并集语义天然幂等；第二次跑不新增任何东西，也不产生第二份备份。
//!
//! ## ★ 必须由客户端的合并记录**门控**（不能无条件搬）
//!
//! 无条件把 TraeWork 的快照搬进 `Trae`/`TraeCode` 槽位是**危险**的：在**没有合并**的机器上
//! 两条客户端并存，TraeWork 的快照会被灌进 TraeCode 的槽位 ——
//! 那就是「把一个客户端未知格式的登录态写进另一个客户端」
//! （[`super::paths::profiles_dir_for_program`] 模块头点名过这类事故）。
//!
//! 因此判据取**客户端自己写的** `migration-job.json`：只有 `status == "completed"`
//! 且源客户端能识别成**另一条**程序线时才搬。没有这个文件 ⇒ 不搬（等价于今天的行为）。
//!
//! ⚠️ 该文件是**客户端私有格式**（未公开文档）。所以本模块对它是**只读 + 宽容**：
//! 读不到 / 解析不了 / 字段缺失 / `status` 不是 `completed` ⇒ 一律**不搬**，
//! 并记进 [`SnapshotCarryReport::notes`]。客户端哪天改了这个文件，退化的结果
//! 只是「回到需要手动保存登录态」，不会写坏任何东西。
//!
//! ## 关于槽位里的 `.source` 标记（**刻意原样复制，不要"修正"它**）
//!
//! `backup_to_slot_for` 会在槽位根写一个 `.source` 记录快照的**来源 userData**。
//! 本模块用 [`super::profile::copy_dir_recursive`] 整目录复制，所以搬过去的快照里
//! `.source` 仍写着 `…\TRAE SOLO CN`（旧客户端），而它现在住在合并版槽位里。
//!
//! 这**不是**遗留问题：那个值记录的正是「这份快照抓自哪个客户端」，而事实就是如此。
//! 全仓**没有任何地方读它做校验**（只在备份时写），因此不会误判；反过来，
//! 把它改写成目标客户端的路径才是**说谎** —— 这份文件的内容确实来自旧客户端。

use std::path::Path;

use serde_json::{json, Value};

use super::paths;
use super::platform;
use super::profile;
use super::region::{TraeProgram, TraeRegion};
use super::store;

/// 客户端自己留下的合并记录文件名（落在**目标客户端** userData 根下）。
const MIGRATION_JOB_FILE: &str = "migration-job.json";

/// 槽位根下**不是账号快照**的保留名 —— 搬运时必须跳过。
///
/// - `current_account.txt`：本应用自己的记账（「当前登录的是谁」），**文件**不是目录；
/// - `last`：切换前的现场快照（回滚用），是**目录**，所以只靠 `is_dir()` 挡不住它。
///
/// ⚠️ 用**黑名单**而不是「只搬纯数字 uid」：槽位名可以由用户在设置页任意命名
/// （`trae_backup_profile` 的 `userId` 参数），白名单会把那些合法槽位静默丢掉。
const RESERVED_SLOT_NAMES: &[&str] = &["last", "current_account.txt"];

/// 一次「源程序 → 目标程序」的搬运明细。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotCarry {
    /// 所属区域。
    pub region: TraeRegion,
    /// 源程序位（快照搬出来的那一条）。
    pub from: TraeProgram,
    /// 目标程序位（搬进去的那一条）。
    pub to: TraeProgram,
    /// 账号槽位名（userId）。
    pub slot: String,
    /// 本次复制的文件数。
    pub files: u64,
}

/// 搬运结果（报告形态，前端/日志直接可读）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotCarryReport {
    /// 实际搬运的记录（逐账号）。
    pub carries: Vec<SnapshotCarry>,
    /// 因目标槽位已有同 uid 而**跳过**（只增不改）的账号数。
    pub slots_kept: usize,
    /// 逐个槽位的失败原因（`"<槽位>（<原因>）"`）。非空即「有东西没搬成」。
    pub errors: Vec<String>,
    /// 诊断备注（为什么没搬 / 认出了什么）。仅用于排查，不影响行为。
    pub notes: Vec<String>,
}

impl SnapshotCarryReport {
    /// 本次是否真的搬了东西（`false` = 幂等空转）。
    pub fn changed(&self) -> bool {
        !self.carries.is_empty()
    }

    /// 复制的文件总数。
    pub fn files_copied(&self) -> u64 {
        self.carries.iter().map(|c| c.files).sum()
    }

    /// 线上形态（camelCase，与其余 Trae 报告同形）。
    pub fn to_json(&self) -> Value {
        json!({
            "slotsCopied": self.carries.len(),
            "slotsKept": self.slots_kept,
            "filesCopied": self.files_copied(),
            "carried": self
                .carries
                .iter()
                .map(|c| json!({
                    "region": c.region.as_str(),
                    "from": c.from.as_str(),
                    "to": c.to.as_str(),
                    "slot": c.slot,
                    "files": c.files,
                }))
                .collect::<Vec<_>>(),
            "errors": self.errors,
            "notes": self.notes,
            "changed": self.changed(),
        })
    }
}

/// 从**目标客户端** userData 里读出「这次合并是从哪条程序线来的」。
///
/// 返回 `None` 的**每一种情况都是"不搬"**，且都不算错误（见模块头「必须由客户端的
/// 合并记录门控」）：文件不在、读不了、不是 JSON、`status` 不是 `completed`、
/// 三个候选字段都取不到名字。
fn completed_migration_source(target_data_dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(target_data_dir.join(MIGRATION_JOB_FILE)).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;

    // 只认「已完成」。缺 `status` 或其它值 ⇒ 不搬：宁可不动，也不要在客户端
    // 迁移失败 / 中断（`status` 可能是 `failed` / `running`）时把旧快照灌进新槽位。
    if value.get("status").and_then(Value::as_str) != Some("completed") {
        return None;
    }

    // 优先用客户端自述的应用名（`sourceAppName`，本机实测为 `TRAE SOLO CN`）；
    // `sourceWin32RegValueName` 是同一个事实的另一种写法（卸载项名）。
    for key in ["sourceAppName", "sourceWin32RegValueName"] {
        if let Some(name) = value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            return Some(name.to_string());
        }
    }

    // 再退一步：用源安装目录的末段名（`D:\Programs\TRAE SOLO CN` → `TRAE SOLO CN`）。
    // 只在前两个字段都缺失时用，因为路径的末段名不保证等于产品名。
    value
        .get("sourceInstallPath")
        .and_then(Value::as_str)
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// 把**遗留程序线**的登录态快照搬进**合并版**程序位的槽位（幂等、只增不改、只复制）。
///
/// 逐区域判定：目标取 [`TraeProgram::Trae`]（合并版），源由该客户端 userData 下的
/// `migration-job.json` 决定（见 [`completed_migration_source`]）。
///
/// **不返回 `Result`**：本操作是**尽力而为**的附属迁移，单个槽位复制失败不该让
/// 整次「遗留迁移」（含账号库合并）报错 —— 但失败必须**可见**，落在
/// [`SnapshotCarryReport::errors`] 里（调用方回传给前端并写日志）。
pub fn carry_merged_snapshots() -> SnapshotCarryReport {
    let mut report = SnapshotCarryReport::default();

    for region in TraeRegion::all() {
        let target_program = TraeProgram::Trae;
        let Some(target_data_dir) = platform::user_data_dir_for_program(region, target_program)
        else {
            continue;
        };
        // ★ **门控**：只有客户端自己记录了「合并已完成」才动数据（见模块头）。
        let Some(source_name) = completed_migration_source(&target_data_dir) else {
            continue;
        };
        // 记录里的源客户端名必须能认出程序线 —— 认不出就不搬（宽容失败方向）。
        let Some(recorded) = TraeProgram::parse(&source_name) else {
            report.notes.push(format!(
                "{}：合并记录里的源客户端 `{source_name}` 认不出是哪条程序线，未搬运",
                region.as_str()
            ));
            continue;
        };

        let target_root = paths::profiles_dir_for_program(region, target_program);
        report.notes.push(format!(
            "{}：客户端已记录合并完成（源 `{source_name}` ⇒ {:?}），把遗留快照搬进 {:?}",
            region.as_str(),
            recorded,
            target_root
        ));

        let mut carried_here = 0usize;
        // ## 为什么搬**所有遗留程序位**，而不是只搬记录里那一条
        //
        // `migration-job.json` 只记了 TraeWork（`TRAE SOLO CN`）—— 因为 TraeCode 的
        // userData 恰好**就是**合并后的目标目录，客户端没必要为它写迁移项。
        // 但合并后的客户端是两条线的并集，两份遗留快照对它**都**有效：
        // 客户端自己已经把两边的数据合进了同一个 userData。
        // ⇒ 只搬记录里那一条，会让「用过 TraeCode 的机器」上的快照永远搬不过来。
        //
        // ⚠️ 安全性仍由上面的门控保证：门控为真 ⇒ 本机已合并 ⇒ 不存在另一条独立的
        // 遗留客户端（合并版复用了同名目录），因此不会把 A 客户端的登录态灌进 B。
        for source_program in TraeProgram::all() {
            if source_program == target_program {
                continue;
            }
            let source_root = paths::profiles_dir_for_program(region, source_program);
            // ⚠️ 这里**不能**用 `!source_root.is_dir()` 当「源槽位不存在」的判据：
            // `profiles_dir_for_program` 内部会 `create_dir_all`（`paths::ensured`），
            // 所以它返回的路径**恒为已存在的目录** —— 那个判断恒假，是死代码。
            // 「源槽位里有没有东西」由下面的 `read_dir` 是否产出条目决定：空目录 ⇒ 0 条搬运
            // ⇒ `changed()` 为假 ⇒ 调用方不提示。代价是源槽位目录会被顺带创建出来
            // （与本模块其余路径一致，无副作用）。
            if source_root == target_root {
                continue;
            }

            let entries = match std::fs::read_dir(&source_root) {
                Ok(entries) => entries,
                Err(error) => {
                    report.errors.push(format!(
                        "读取源槽位 {} 失败（{error}）",
                        source_root.display()
                    ));
                    continue;
                }
            };

            for entry in entries.flatten() {
                let slot = entry.file_name().to_string_lossy().to_string();
                // 跳过保留名与非法名（见 `RESERVED_SLOT_NAMES` / `paths::safe_slot_name`）。
                if RESERVED_SLOT_NAMES.contains(&slot.as_str()) || !paths::safe_slot_name(&slot) {
                    continue;
                }
                let from = entry.path();
                // 只搬**目录**：槽位根下除保留名外可能还有零散文件（如 `.source` 之外的杂物），
                // 它们不是快照，搬过去只会让目标槽位多出无法识别的条目。
                if !from.is_dir() {
                    continue;
                }
                let to = target_root.join(&slot);
                if to.exists() {
                    // ★ 只增不改：目标已有同 uid ⇒ 保留目标那份。
                    report.slots_kept += 1;
                    continue;
                }
                match profile::copy_dir_recursive(&from, &to) {
                    Ok(files) => {
                        carried_here += 1;
                        report.carries.push(SnapshotCarry {
                            region,
                            from: source_program,
                            to: target_program,
                            slot,
                            files,
                        });
                    }
                    Err(error) => report.errors.push(format!("{slot}（{error}）")),
                }
            }
        }

        // 日志按**区域**记（不是全局 `report.changed()`）：否则 A 区搬了东西会让
        // B 区也写一条「复制 0 个」的噪声日志。
        if carried_here > 0 {
            store::append_log(
                &paths::log_file_for_region(region, "switcher.log"),
                &format!(
                    "客户端合并快照搬运：{region:?} 的遗留程序位 -> {target_program:?}，\
                     复制 {carried_here} 个账号槽位"
                ),
            );
        }
    }

    report
}

/// ## 为什么测试是 Windows-only
///
/// 与仓库既有约定一致：依赖「客户端 userData 目录」的用例一律 `#[cfg(windows)]`
/// （`test_support::TempEnv::empty()` / `appdata()` 本身也只在 Windows 上提供）。
#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// 造一个「目标客户端 userData + 合并记录 + 源槽位快照」的现场。
    struct Fixture {
        _env: crate::modules::trae::test_support::TempEnv,
        /// 临时 home 下的 `profiles*` 根。
        profiles_root: std::path::PathBuf,
    }

    impl Fixture {
        /// `source_app` 为合并记录里写的源客户端名；`None` 表示**不写**该记录。
        fn new(source_app: Option<&str>, status: &str) -> Self {
            let env = crate::modules::trae::test_support::TempEnv::empty();
            // 目标客户端 userData：CN 合并版 = `%APPDATA%\Trae CN`。
            let target = env.appdata().join("Trae CN");
            std::fs::create_dir_all(&target).expect("目标 userData 应能创建");
            if let Some(source) = source_app {
                std::fs::write(
                    target.join(MIGRATION_JOB_FILE),
                    serde_json::json!({
                        "status": status,
                        "sourceAppName": source,
                        "sourceInstallPath": format!("D:\\Programs\\{source}"),
                    })
                    .to_string(),
                )
                .expect("合并记录应能写入");
            }
            let profiles_root = crate::modules::trae::paths::trae_dir();
            Self {
                _env: env,
                profiles_root,
            }
        }

        /// 在某个程序位的槽位根下铺一个账号快照。
        fn seed(&self, region: TraeRegion, program: TraeProgram, slot: &str, file: &str) {
            let dir = self
                .profiles_root
                .join(profile_dir_name(region, program))
                .join(slot);
            std::fs::create_dir_all(&dir).expect("快照目录应能创建");
            std::fs::write(dir.join(file), b"x").expect("快照文件应能写入");
        }

        fn slot_dir(
            &self,
            region: TraeRegion,
            program: TraeProgram,
            slot: &str,
        ) -> std::path::PathBuf {
            self.profiles_root
                .join(profile_dir_name(region, program))
                .join(slot)
        }
    }

    /// 与 `paths::profiles_dir_for_program` 同源的目录名（测试里只需要名字）。
    ///
    /// ⚠️ 合并版（`Trae`）有**自己的**目录（`profiles_trae_merged` /
    /// `profiles_global_trae_merged`），与遗留 `TraeCode` 的目录**不同** ——
    /// 那正是 issue #20 要修的事（用户报障「两条程序位共用同一个快照目录」）。
    /// 这个映射必须与生产实现逐字对齐，否则用例会在错的目录上断言。
    fn profile_dir_name(region: TraeRegion, program: TraeProgram) -> &'static str {
        match (region, program) {
            (TraeRegion::Cn, TraeProgram::TraeWork) => "profiles",
            (TraeRegion::Cn, TraeProgram::TraeCode) => "profiles_trae_cn",
            (TraeRegion::Cn, TraeProgram::Trae) => "profiles_trae_merged",
            (TraeRegion::Global, TraeProgram::TraeWork) => "profiles_global_trae_work",
            (TraeRegion::Global, TraeProgram::TraeCode) => "profiles_global_trae_code",
            (TraeRegion::Global, TraeProgram::Trae) => "profiles_global_trae_merged",
        }
    }

    /// ★ 有合并记录（`completed`）时，TraeWork 的账号快照必须被搬进合并版槽位。
    ///
    /// 反例（改坏会红）：把 `RESERVED_SLOT_NAMES` 的过滤去掉 ⇒ `last/` 也被当成账号搬过去；
    /// 或者把门控去掉 ⇒ 下面那条「没有合并记录就不搬」的用例会红。
    #[test]
    fn 有合并记录时把遗留快照搬进合并槽位() {
        let fixture = Fixture::new(Some("TRAE SOLO CN"), "completed");
        fixture.seed(TraeRegion::Cn, TraeProgram::TraeWork, "3604620555324748", "Preferences");
        fixture.seed(TraeRegion::Cn, TraeProgram::TraeWork, "1189017012674171", "Preferences");
        // 保留名：`last` 是**目录**，靠 `is_dir()` 挡不住，必须靠黑名单。
        fixture.seed(TraeRegion::Cn, TraeProgram::TraeWork, "last", "Preferences");

        let report = carry_merged_snapshots();

        assert_eq!(report.carries.len(), 2, "应搬两个账号槽位：{report:?}");
        assert!(report.errors.is_empty(), "不应有失败：{:?}", report.errors);
        assert!(
            !report.carries.iter().any(|c| c.slot == "last"),
            "`last` 是保留名，绝不能当成账号搬过去"
        );
        for uid in ["3604620555324748", "1189017012674171"] {
            assert!(
                fixture
                    .slot_dir(TraeRegion::Cn, TraeProgram::Trae, uid)
                    .join("Preferences")
                    .is_file(),
                "账号 {uid} 的快照应已出现在合并版槽位"
            );
        }
        // 只复制、不动源：源槽位仍在。
        assert!(
            fixture
                .slot_dir(TraeRegion::Cn, TraeProgram::TraeWork, "3604620555324748")
                .is_dir(),
            "源槽位必须原样保留"
        );
        assert_eq!(report.slots_kept, 0);
    }

    /// ★ 门控通过时必须把**两个遗留程序位**的快照都搬进来（issue #20）。
    ///
    /// 反例（改坏会红）：只搬 `migration-job.json` 里记的那一条（`TRAE SOLO CN` ⇒ TraeWork）
    /// —— 那样「只用过 TraeCode 的机器」上的快照永远搬不过来，用户照样切不了账号。
    /// 记录里只有 TraeWork 是因为 TraeCode 的 userData **恰好就是**合并后的目标目录，
    /// 客户端没必要为它写迁移项（见 `carry_merged_snapshots` 的说明）。
    #[test]
    fn 门控通过时两个遗留程序位的快照都要搬() {
        let fixture = Fixture::new(Some("TRAE SOLO CN"), "completed");
        fixture.seed(TraeRegion::Cn, TraeProgram::TraeWork, "u-work", "Preferences");
        fixture.seed(TraeRegion::Cn, TraeProgram::TraeCode, "u-code", "Preferences");

        let report = carry_merged_snapshots();

        assert_eq!(report.carries.len(), 2, "两个遗留槽位都要搬：{report:?}");
        assert!(
            fixture
                .slot_dir(TraeRegion::Cn, TraeProgram::Trae, "u-work")
                .is_dir(),
            "TraeWork 的快照没搬进合并槽位"
        );
        assert!(
            fixture
                .slot_dir(TraeRegion::Cn, TraeProgram::Trae, "u-code")
                .is_dir(),
            "TraeCode 的快照没搬进合并槽位"
        );
        // 报告里两条的来源必须**分别标对** —— 否则日志与提示会把来源说错。
        let mut froms: Vec<&str> = report.carries.iter().map(|c| c.from.as_str()).collect();
        froms.sort_unstable();
        assert_eq!(froms, vec!["trae_code", "trae_work"]);
    }

    /// ★ 没有合并记录（或状态不是 `completed`）时**一个字节都不能搬**。
    ///
    /// 这是本模块最危险的一条边界：机器上两条客户端并存时，无条件搬运会把
    /// TraeWork 的登录态灌进 TraeCode 的槽位 —— 用户切过去会把错的登录态写进错客户端。
    #[test]
    fn 无合并记录或未完成时绝不搬运() {
        for (source, status, why) in [
            (None, "completed", "没有合并记录"),
            (Some("TRAE SOLO CN"), "failed", "合并失败"),
            (Some("TRAE SOLO CN"), "running", "合并进行中"),
        ] {
            let fixture = Fixture::new(source, status);
            fixture.seed(TraeRegion::Cn, TraeProgram::TraeWork, "3604620555324748", "Preferences");

            let report = carry_merged_snapshots();

            assert!(
                !report.changed(),
                "{why} 时不得搬运，实际搬了 {:?}",
                report.carries
            );
            assert!(
                !fixture
                    .slot_dir(TraeRegion::Cn, TraeProgram::Trae, "3604620555324748")
                    .exists(),
                "{why} 时目标槽位不得出现任何东西"
            );
        }
    }

    /// ★ 幂等 + **只增不改**：目标已有同 uid 时跳过，不覆盖。
    #[test]
    fn 搬运幂等且目标已有同uid时只增不改() {
        let fixture = Fixture::new(Some("TRAE SOLO CN"), "completed");
        fixture.seed(TraeRegion::Cn, TraeProgram::TraeWork, "3604620555324748", "Preferences");
        // 目标槽位**已经**有一份（内容不同）—— 它必须原封不动。
        fixture.seed(TraeRegion::Cn, TraeProgram::Trae, "3604620555324748", "Local State");
        let existing = fixture
            .slot_dir(TraeRegion::Cn, TraeProgram::Trae, "3604620555324748")
            .join("Local State");

        let first = carry_merged_snapshots();
        assert!(!first.changed(), "目标已有同 uid ⇒ 本次不该搬任何东西");
        assert_eq!(first.slots_kept, 1, "应记一次「跳过」：{first:?}");
        assert!(
            existing.is_file() && !existing.with_file_name("Preferences").exists(),
            "目标槽位原有内容必须原样保留（只增不改）"
        );

        // 第二个账号：第一次搬进去，第二次必须空转。
        fixture.seed(TraeRegion::Cn, TraeProgram::TraeWork, "1189017012674171", "Preferences");
        let second = carry_merged_snapshots();
        assert_eq!(second.carries.len(), 1, "第二次应搬走新增的那个：{second:?}");
        let third = carry_merged_snapshots();
        assert!(!third.changed(), "第三次必须完全空转：{third:?}");
        assert_eq!(third.slots_kept, 2);
    }

    /// 源客户端名认不出程序线时**不搬**，但要在 `notes` 里留痕（便于排查）。
    #[test]
    fn 源客户端名认不出时不搬并留痕() {
        let fixture = Fixture::new(Some("Some Other App"), "completed");
        fixture.seed(TraeRegion::Cn, TraeProgram::TraeWork, "3604620555324748", "Preferences");

        let report = carry_merged_snapshots();

        assert!(!report.changed(), "认不出的源不得搬运");
        assert!(
            report.notes.iter().any(|n| n.contains("Some Other App")),
            "应留下「认不出」的备注：{:?}",
            report.notes
        );
        assert!(report.errors.is_empty(), "认不出不是错误：{:?}", report.errors);
    }

    /// 报告形态必须是 camelCase 且字段齐全（线上契约）。
    #[test]
    fn 报告形态字段齐全且不泄漏snake_case() {
        let fixture = Fixture::new(Some("TRAE SOLO CN"), "completed");
        fixture.seed(TraeRegion::Cn, TraeProgram::TraeWork, "3604620555324748", "Preferences");
        let value = carry_merged_snapshots().to_json();
        for key in ["slotsCopied", "slotsKept", "filesCopied", "carried", "errors", "notes", "changed"] {
            assert!(value.get(key).is_some(), "报告缺少字段 {key}");
        }
        assert!(value.get("slots_copied").is_none(), "不得泄漏 snake_case");
        assert_eq!(value.get("changed").and_then(Value::as_bool), Some(true));
        assert_eq!(value.get("slotsCopied").and_then(Value::as_u64), Some(1));
        let carried = value.get("carried").and_then(Value::as_array).unwrap();
        for key in ["region", "from", "to", "slot", "files"] {
            assert!(carried[0].get(key).is_some(), "搬运条目缺少字段 {key}");
        }
        assert_eq!(carried[0].get("from").and_then(Value::as_str), Some("trae_work"));
        assert_eq!(carried[0].get("to").and_then(Value::as_str), Some("trae"));
    }
}
