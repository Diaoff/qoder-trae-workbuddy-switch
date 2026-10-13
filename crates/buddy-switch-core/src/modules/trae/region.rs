//! Trae 的两个**正交维度**：区域（region）与程序（program）。
//!
//! ## 为什么需要这一层（现状缺陷）
//!
//! 改造前只有一根轴（[`super::variant::TraeVariant`]），两个取值
//! `TraeWork` / `Trae` **都是国内构建**，于是：
//!
//! - `data_dir_names` 把国内与国外的候选名混在同一张表里
//!   （`["TRAE SOLO CN", "TRAE SOLO"]`、`["Trae CN", "Trae"]`），并按顺序取
//!   **第一个存在的**目录 ⇒ 装了国际版也永远探测不到，用户在界面上看不见、管不了；
//! - 「一条程序」与「一个账号体系」被混为一谈：国内的两条程序其实共用
//!   `api.trae.cn` 同一套账号，却各自维护一份账号库。
//!
//! ## 两根轴各自负责什么
//!
//! | 轴 | 取值 | 决定 |
//! |:---|:---|:---|
//! | [`TraeRegion`] | `Cn` / `Global` | **持久化**：账号库 / 登录态快照 / 设备标识 / 冷却 / 端点。即「账号属于哪套账号体系」 |
//! | [`TraeProgram`] | `TraeWork` / `TraeCode` | **执行**：把登录态写进哪个客户端目录、启动哪个 exe |
//!
//! 关键是**持久化轴是区域而不是程序**：CN 与国际是两套互不相通的账号体系
//! （JWT 不通用，端点不同），而同一区域内的两条程序共用同一套账号 ——
//! 同一个 Trae 账号可以分别启用在国内的 TraeWork 与 TraeCode 上。
//!
//! ## 程序位（program instance）与其官方名
//!
//! 取值来自**两处权威来源**（本机 2026-09-21 实测），不是我们起的名字：
//!
//! - 客户端 `<安装根>\resources\app\product.json` 的 `nameAlias` / `packageType`；
//! - 系统注册表 `Uninstall` 项的 `DisplayName`（用户在「应用和功能」里看到的名字）。
//!
//! | 区域 | 程序 | userData 目录 | exe / 进程 | `nameAlias` | `packageType` | 注册表显示名 |
//! |:---|:---|:---|:---|:---|:---|:---|
//! | CN | TraeWork | `TRAE SOLO CN` | `TRAE SOLO CN.exe` | `TraeWork CN` | `SOLO_CN` | TraeWork CN (User) |
//! | CN | TraeCode | `Trae CN` | `Trae CN.exe` | `TraeCode CN` | `TRAE_CN` | TraeCode CN (User) |
//! | CN | **Trae**（合并版） | `Trae CN` | `Trae CN.exe` | `TraeCode CN` | `TRAE_CN` | TraeCode CN (User) |
//! | Global | TraeWork | `TRAE SOLO` | `TRAE SOLO.exe` | `TraeWork` | `SOLO_I18N` | TraeWork (User) |
//! | Global | TraeCode | `Trae` | `Trae.exe` | `TraeCode`（待实测） | `TRAE_I18N`（推断，见下） | 本机未安装 |
//! | Global | **Trae**（合并版） | `Trae` | `Trae.exe` | `TraeCode`（待实测） | `TRAE_I18N`（推断，见下） | 本机未安装 |
//!
//! ⚠️ **候选名必须互不重叠**：`TRAE SOLO CN` 与 `TRAE SOLO` 只差一个后缀，
//! `Trae CN` 与 `Trae` 是包含关系。用「包含匹配」判程序位会互相抢
//! （出现「装了 A、读了 B 的目录」），因此 [`ProgramSpec`] 的候选名一律
//! **精确逐字比对**，不重叠由单测钉住。
//!
//! ### 唯一登记在案的重叠：`Trae`（合并版）× `TraeCode`（issue #20）
//!
//! 2026-10-13 客户端把 TraeWork 与 TraeCode 合并成**一个**客户端，落点就是
//! `Trae CN` / `Trae` —— 与旧的 **TraeCode** 逐字同名（本机 `migration-job.json`
//! 实证：`sourceAppName = "TRAE SOLO CN"`、`targetDataPath = …\Trae CN`）。
//! 也就是说 `Trae` 与 `TraeCode` **物理上是同一个客户端**，候选名必然重叠。
//!
//! 这不是缺陷，而是「合并」这一事实的投影；两条程序位因此**刻意共享**
//! 同一份候选名、同一个 [`super::variant::TraeVariant`] 与同一个快照目录
//! （见 [`super::paths::profiles_dir_for_program`]）—— 否则会把同一个客户端的
//! 登录态拆进两套快照/两本账号库。重叠由单测**显式白名单**钉住（白名单外新增重叠仍会红）。
//!
//! 另一条重叠（`TRAE SOLO` ∈ TraeWork ∩ Global TraeWork）是**待修缺陷**，
//! 与上面这条性质不同，见 [`super::variant::TRAE_WORK_SPEC`] 的说明。
//!
//! ## 程序位是否都真的接上了（2026-09-28 issue #3；2026-10-13 issue #20）
//!
//! 本模块一直有 4 个 [`ProgramSpec`]，但**第 4 个（Global × TraeCode）曾经接不上**：
//! `platform::variant_for_program` 对它返回 `None`，于是界面上那一格永远是
//! 「未检测到」。issue #3 报的「不支持 TraeCode」就是这一格。
//!
//! 现在 6 个程序位（3 程序 × 2 区域）都映射到 [`super::variant::TraeVariant`] 的一个取值
//! （`TraeWork` / `Trae` / `Global` / `GlobalTraeCode` —— 合并版 `Trae` 与遗留
//! `TraeCode` **共用**后两者），探测、启动、切换、登录态快照四件事因此都按程序位分家。
//! 判据见 `platform::tests::全部程序位已建模且自洽`。
//!
//! ⚠️ 唯一**推断**（非实测）的是 `TraeCode` 国际版的 `packageType`：本机未装该客户端。
//! 它只被 [`super::variant::OAuthLine::from_package_type`] 消费，而那个分派
//! **只区分 `SOLO_*` 与非 `SOLO_*`** ⇒ 任何非 `SOLO_*` 取值都落 TRAE 线（IDE 该用的
//! 那条钥匙），所以推断错也**不会**把请求打到错的域或换错钥匙。
//!
//! ## 展示名：区域前缀 + 程序名
//!
//! 程序位的展示名按用户确认的写法（国际版带 `AI` 后缀，国内版不带）：
//! `TraeWork` / `TraeCode` / `TraeWork AI` / `Trae AI`。
//! 区域前缀（[`TraeRegion::display_name`]，`国内版` / `国际版`）由调用方按需拼接 ——
//! 界面上是「国内版 TraeWork」「国际版 TraeWork AI」，日志里也能拼成无歧义的全名。
//!
//! ## 与 [`super::variant`] 的关系（过渡期）
//!
//! 区域端点**不在这里另立一份**：本模块的 [`region_endpoints`] 直接读
//! [`super::variant::variant_spec`] 里那张已实测/已更正的端点表，
//! 避免同一事实出现两个来源。等调用点全部迁到区域轴后，端点表本身也会搬进来。

use super::variant::{variant_spec, EndpointSet, TraeVariant};

/// 账号体系所在区域。
///
/// 它是**持久化轴**：账号库文件后缀、登录态快照目录、设备标识、冷却账本、
/// 以及全部联网端点都由它决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraeRegion {
    /// 国内版（`api.trae.cn` / `api.trae.com.cn`，账号体系与国内主体一致）。
    Cn,
    /// 国际版（客户端 `packageType = SOLO_I18N`，发布者 SPRING (SG) PTE. LTD）。
    Global,
}

impl TraeRegion {
    /// 稳定标识（文件名后缀、序列化、前端传参）。
    pub fn as_str(self) -> &'static str {
        match self {
            TraeRegion::Cn => "cn",
            TraeRegion::Global => "global",
        }
    }

    /// 界面上的区域前缀（与 WorkBuddy 的「国内版 / 国际版」同形）。
    pub fn display_name(self) -> &'static str {
        match self {
            TraeRegion::Cn => "国内版",
            TraeRegion::Global => "国际版",
        }
    }

    /// 从字符串解析区域，**宽容**并兼容历史取值。
    ///
    /// ⚠️ 历史值 `trae_work` / `trae_cn` **都映射到 [`TraeRegion::Cn`]**：
    /// 它们是改造前的「产品线」标识，而那两个产品线**都是国内构建**。
    /// 老前端/老链接带着这两个值进来时必须落到国内版，**不能**落到国际版 ——
    /// 否则会在国际版里读一个空账号库，用户看到的是「账号凭空消失」。
    pub fn parse(s: &str) -> Option<TraeRegion> {
        match s.trim().to_ascii_lowercase().as_str() {
            "cn" | "china" | "domestic" | "trae_work" | "trae_cn" | "traework" | "trae" => {
                Some(TraeRegion::Cn)
            }
            "global" | "intl" | "international" | "sg" | "ai" => Some(TraeRegion::Global),
            _ => None,
        }
    }

    /// 全部区域（遍历用，顺序稳定）。
    pub fn all() -> [TraeRegion; 2] {
        [TraeRegion::Cn, TraeRegion::Global]
    }
}

impl Default for TraeRegion {
    /// 默认国内版：与改造前的默认变体（`trae_work`）落在**同一个账号库**上，
    /// 因此「不传 region」对老调用点的语义就是「行为与升级前一模一样」。
    fn default() -> Self {
        TraeRegion::Cn
    }
}

/// Trae 的两条程序线。
///
/// 它是**执行轴**：只决定「登录态写进哪个客户端目录、启动哪个 exe」，
/// 不参与账号归属。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraeProgram {
    /// TraeWork 程序线（客户端 `TRAE SOLO*`，官方别名 `TraeWork*`）。
    ///
    /// 自 2026-10-13 起客户端已把它**并入** [`TraeProgram::Trae`]；本取值只服务
    /// **未升级的老版本**（本机 `D:\Programs\TRAE SOLO CN\` 已被卸载、只剩 debug.log）。
    TraeWork,
    /// Trae 代码编辑器程序线（客户端 `Trae*`，官方别名 `TraeCode*`）。
    ///
    /// ⚠️ 与 [`TraeProgram::Trae`] **是同一个客户端**（同名、同 userData、同 exe）——
    /// 保留它是为了让合并前写入的数据（`trae_code` 标识、`profiles_trae_cn` 快照）
    /// 继续可读。新增逻辑请优先用 [`TraeProgram::Trae`]。
    TraeCode,
    /// **合并后的单一客户端**（issue #20）：CN `Trae CN` / 国际 `Trae`。
    ///
    /// ## 依据（本机实测，不是推断）
    ///
    /// 客户端 2026-10-13 把 TraeWork 与 TraeCode 合并成一条产品线，并把旧数据迁进
    /// 合并后的客户端。证据是 `%APPDATA%\Trae CN\migration-job.json`（逐字）：
    ///
    /// ```json
    /// { "sourceDataPath": "…\\AppData\\Roaming\\TRAE SOLO CN",
    ///   "sourceInstallPath": "D:\\Programs\\TRAE SOLO CN",
    ///   "sourceAppName": "TRAE SOLO CN",
    ///   "targetDataPath": "…\\AppData\\Roaming\\Trae CN",
    ///   "removeApp": true, "removeData": false, "status": "completed" }
    /// ```
    ///
    /// 即：`TRAE SOLO CN`（TraeWork）被卸载、数据并入 `Trae CN`。
    ///
    /// ## ⚠️ 它与 [`TraeProgram::TraeCode`] 同名 —— 这是刻意的
    ///
    /// 合并后的客户端**就是** `Trae CN` / `Trae`，与旧 TraeCode 逐字同名，无法（也无需）
    /// 用名字区分。因此两者共享候选名、共享 [`super::variant::TraeVariant`]
    /// 与共享快照目录；本取值是**面向用户的正确名字**（客户端现在就叫 Trae，
    /// 不再叫 TraeCode），`TraeCode` 则是历史标识的兼容别名。
    Trae,
}

impl TraeProgram {
    /// 稳定标识。
    pub fn as_str(self) -> &'static str {
        match self {
            TraeProgram::TraeWork => "trae_work",
            TraeProgram::TraeCode => "trae_code",
            TraeProgram::Trae => "trae",
        }
    }

    /// 从字符串解析程序线（宽容，接受客户端 exe / 目录名的常见写法）。
    pub fn parse(s: &str) -> Option<TraeProgram> {
        let lowered = s.trim().to_ascii_lowercase();
        let lowered = lowered.strip_suffix(".exe").unwrap_or(&lowered);
        if lowered.contains("solo") || lowered.contains("work") {
            return Some(TraeProgram::TraeWork);
        }
        // `code` 必须**先于**裸 `trae` 判定：`trae_code` 是遗留 IDE 程序位的标识，
        // 而裸 `trae` 自 issue #20 起指合并后的客户端 —— 两者是**不同槽位**
        // （虽然物理上同一个客户端），顺序写反会让 `trae_code` 落进 `Trae`。
        if lowered.contains("code") {
            return Some(TraeProgram::TraeCode);
        }
        // 裸 `trae`（含 `trae cn` / `trae cn.exe`）＝ 合并后的客户端。
        // 客户端自己的目录名/ exe 名就是这个，故它是最常见的输入。
        if lowered == "trae" || lowered == "trae cn" {
            return Some(TraeProgram::Trae);
        }
        // 其余含 `trae` 的历史写法（`trae_cn` / `global_trae_code` 已在上面的
        // `code` 分支处理）一律落遗留 TraeCode，**不猜**成合并版。
        if lowered.contains("trae") {
            return Some(TraeProgram::TraeCode);
        }
        None
    }

    /// 全部程序线（遍历用，顺序稳定：遗留两条在前，合并版在后）。
    pub fn all() -> [TraeProgram; 3] {
        [
            TraeProgram::TraeWork,
            TraeProgram::TraeCode,
            TraeProgram::Trae,
        ]
    }
}

/// 一个**程序位**（区域 × 程序）的全部差异。
#[derive(Debug, Clone, Copy)]
pub struct ProgramSpec {
    /// 所属区域。
    pub region: TraeRegion,
    /// 所属程序线。
    pub program: TraeProgram,
    /// 界面展示名（国际版带 `AI` 后缀），见模块头「展示名」。
    pub display_name: &'static str,
    /// 客户端 `product.json` 的 `nameAlias`（官方自称，用于诊断与核对）。
    pub name_alias: &'static str,
    /// 客户端 `product.json` 的 `packageType`；未知时为 `None`（**不猜**）。
    pub package_type: Option<&'static str>,
    /// userData 目录名候选（`%APPDATA%\<名字>`）。**精确匹配，不做包含匹配**。
    pub data_dir_names: &'static [&'static str],
    /// 可执行文件名候选（精确匹配）。
    pub exe_names: &'static [&'static str],
    /// 进程名候选（Windows 精简名，不带 `.exe`）。
    pub proc_names: &'static [&'static str],
}

const CN_TRAE_WORK: ProgramSpec = ProgramSpec {
    region: TraeRegion::Cn,
    program: TraeProgram::TraeWork,
    display_name: "TraeWork",
    name_alias: "TraeWork CN",
    package_type: Some("SOLO_CN"),
    data_dir_names: &["TRAE SOLO CN"],
    exe_names: &["TRAE SOLO CN.exe"],
    proc_names: &["TRAE SOLO CN"],
};

const CN_TRAE_CODE: ProgramSpec = ProgramSpec {
    region: TraeRegion::Cn,
    program: TraeProgram::TraeCode,
    display_name: "TraeCode",
    name_alias: "TraeCode CN",
    package_type: Some("TRAE_CN"),
    data_dir_names: &["Trae CN"],
    exe_names: &["Trae CN.exe"],
    proc_names: &["Trae CN"],
};

/// **合并后的 CN 客户端**（issue #20）：客户端名就是 `Trae CN`。
///
/// 与 [`CN_TRAE_CODE`] **逐字段相同**（同名、同目录、同 exe、同 `packageType`）——
/// 因为合并后的客户端**就是**那一个。差异只在**展示名**（客户端现在叫 `Trae`）
/// 与 `program` 标识（`trae`）。候选名重叠已在模块头登记。
const CN_TRAE: ProgramSpec = ProgramSpec {
    region: TraeRegion::Cn,
    program: TraeProgram::Trae,
    display_name: "Trae",
    // 客户端 `product.json` 的 `nameAlias` 实测仍是 `TraeCode CN`
    // （合并版 `D:\Programs\Trae CN\resources\app\product.json`，本机 2026-10-13 读取）
    // —— 上游没改这个字段，我们不替它改。
    name_alias: "TraeCode CN",
    package_type: Some("TRAE_CN"),
    data_dir_names: &["Trae CN"],
    exe_names: &["Trae CN.exe"],
    proc_names: &["Trae CN"],
};

const GLOBAL_TRAE_WORK: ProgramSpec = ProgramSpec {
    region: TraeRegion::Global,
    program: TraeProgram::TraeWork,
    display_name: "TraeWork AI",
    name_alias: "TraeWork",
    package_type: Some("SOLO_I18N"),
    data_dir_names: &["TRAE SOLO"],
    exe_names: &["TRAE SOLO.exe"],
    proc_names: &["TRAE SOLO"],
};

/// 国际版 TraeCode：**本机未安装**，`nameAlias` / `packageType` 无从实测，
/// 因此 `package_type` 留 `None`、别名按同族命名规则推断并在此显式标注「待实测」。
/// 猜测只影响诊断文案；候选名（目录 / exe / 进程）是客户端通用命名，风险可控。
///
/// ⚠️ 它与 [`super::variant::GLOBAL_TRAE_CODE_SPEC`] 的 `package_type` **不是同一件事**：
/// 那边的字段是 `&'static str`（授权页分派必须要一个具体值），填的是同一推断值
/// `TRAE_I18N`；本字段是**诊断用**、允许诚实留空。两处都写了「未实测」，
/// 谁先拿到真实客户端就一起更正。
const GLOBAL_TRAE_CODE: ProgramSpec = ProgramSpec {
    region: TraeRegion::Global,
    program: TraeProgram::TraeCode,
    display_name: "Trae AI",
    name_alias: "TraeCode（待实测）",
    package_type: None,
    data_dir_names: &["Trae"],
    exe_names: &["Trae.exe"],
    proc_names: &["Trae"],
};

/// **合并后的国际客户端**（issue #20）：客户端名 `Trae` / `Trae.exe`。
///
/// 与 [`GLOBAL_TRAE_CODE`] 逐字段相同（同上：合并后的客户端就是那一个）。
/// 展示名沿用「国际版带 `AI` 后缀」的用户约定（与 `TraeWork AI` / `Trae AI` 同款），
/// 因此它与 [`GLOBAL_TRAE_CODE`] 的展示名也相同 —— 两者本就是同一个客户端。
const GLOBAL_TRAE: ProgramSpec = ProgramSpec {
    region: TraeRegion::Global,
    program: TraeProgram::Trae,
    display_name: "Trae AI",
    name_alias: "TraeCode（待实测）",
    package_type: None,
    data_dir_names: &["Trae"],
    exe_names: &["Trae.exe"],
    proc_names: &["Trae"],
};

/// 全部程序位（3 程序 × 2 区域 = 6 条），顺序稳定：先区域、后程序线。
///
/// ⚠️ 其中 `Trae` 与 `TraeCode` 两条**候选名逐字相同**（同一个客户端，见模块头
/// 「唯一登记在案的重叠」）⇒ 本函数的返回值**含重复候选名**，把它摊平成候选表时
/// 必须先按序去重（`platform::tests::flattened_candidates_match_variant_table` 就是这么做且注明了理由）。
pub fn all_program_specs() -> [&'static ProgramSpec; 6] {
    [
        &CN_TRAE_WORK,
        &CN_TRAE_CODE,
        &CN_TRAE,
        &GLOBAL_TRAE_WORK,
        &GLOBAL_TRAE_CODE,
        &GLOBAL_TRAE,
    ]
}

/// 取某个程序位的描述符。
pub fn program_spec(region: TraeRegion, program: TraeProgram) -> &'static ProgramSpec {
    match (region, program) {
        (TraeRegion::Cn, TraeProgram::TraeWork) => &CN_TRAE_WORK,
        (TraeRegion::Cn, TraeProgram::TraeCode) => &CN_TRAE_CODE,
        (TraeRegion::Cn, TraeProgram::Trae) => &CN_TRAE,
        (TraeRegion::Global, TraeProgram::TraeWork) => &GLOBAL_TRAE_WORK,
        (TraeRegion::Global, TraeProgram::TraeCode) => &GLOBAL_TRAE_CODE,
        (TraeRegion::Global, TraeProgram::Trae) => &GLOBAL_TRAE,
    }
}

/// 某个区域下的全部程序位（界面按区域渲染程序控件时遍历它）。
pub fn programs_of(region: TraeRegion) -> [&'static ProgramSpec; 3] {
    match region {
        TraeRegion::Cn => [&CN_TRAE_WORK, &CN_TRAE_CODE, &CN_TRAE],
        TraeRegion::Global => [&GLOBAL_TRAE_WORK, &GLOBAL_TRAE_CODE, &GLOBAL_TRAE],
    }
}

/// 区域对应的端点集合。
///
/// **单一来源**：直接读 [`super::variant::variant_spec`] 的 CN / 国际端点表
/// （国内值已实测跑通、国际值取自国际版客户端自述），本模块不另立一份。
pub fn region_endpoints(region: TraeRegion) -> &'static EndpointSet {
    let spec = variant_spec(TraeVariant::TraeWork);
    match region {
        TraeRegion::Cn => &spec.cn_endpoints,
        TraeRegion::Global => spec
            .global_endpoints
            .as_ref()
            .expect("国际版端点必须在表里登记（见 variant 模块的国际化端点单测）"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 区域标识与展示名稳定() {
        assert_eq!(TraeRegion::Cn.as_str(), "cn");
        assert_eq!(TraeRegion::Global.as_str(), "global");
        assert_eq!(TraeRegion::Cn.display_name(), "国内版");
        assert_eq!(TraeRegion::Global.display_name(), "国际版");
        // 默认必须落在与改造前同一份账号库上，否则老用户会看到账号凭空消失。
        assert_eq!(TraeRegion::default(), TraeRegion::Cn);
    }

    /// 历史取值 `trae_work` / `trae_cn` **必须都解析成国内版**：
    /// 它们是改造前的产品线标识，而两个产品线都是国内构建。
    /// 若谁把 `trae_cn` 解析成国际版，老链接会在国际版里读空账号库。
    #[test]
    fn 历史产品线标识一律解析为国内版() {
        for s in ["trae_work", "trae_cn", "CN", "trae", "cn"] {
            assert_eq!(TraeRegion::parse(s), Some(TraeRegion::Cn), "解析失败: {s}");
        }
        for s in ["global", "intl", "international", "SG"] {
            assert_eq!(TraeRegion::parse(s), Some(TraeRegion::Global), "解析失败: {s}");
        }
        assert_eq!(TraeRegion::parse("doubao"), None);
    }

    #[test]
    fn 程序线解析接受客户端命名() {
        assert_eq!(TraeProgram::parse("TRAE SOLO CN.exe"), Some(TraeProgram::TraeWork));
        assert_eq!(TraeProgram::parse("solo"), Some(TraeProgram::TraeWork));
        assert_eq!(TraeProgram::parse("trae_work"), Some(TraeProgram::TraeWork));
        // ⚠️ `Trae CN.exe` 自 issue #20 起落**合并版** `Trae` —— 客户端现在就叫这个名字，
        //    而遗留 TraeCode 与它**同名**（物理上同一个客户端），只能靠标识
        //    `trae_code` 区分。名字层面无法分辨两者，`parse` 取当前（合并版）为准。
        assert_eq!(TraeProgram::parse("Trae CN.exe"), Some(TraeProgram::Trae));
        assert_eq!(TraeProgram::parse("trae_code"), Some(TraeProgram::TraeCode));
        assert_eq!(TraeProgram::parse("doubao"), None);
    }

    /// ★ 裸 `trae` / `trae cn` 必须解析成**合并后的客户端**，而 `trae_code` 仍是遗留槽位
    /// （issue #20）。
    ///
    /// 反例（改坏会红）：把 `contains("trae")` 放在 `contains("code")` 之前 ——
    /// `trae_code` 会落进 `Trae`，于是「切到遗留 IDE 程序位」变成「切到合并版」。
    /// 两者物理上同一个客户端，故**不会报错**，只会让 `program` 标识静默漂移。
    #[test]
    fn 裸名trae解析为合并版而trae_code仍是遗留() {
        for s in ["trae", "Trae", "TRAE", "trae cn", "Trae CN", "Trae CN.exe"] {
            assert_eq!(TraeProgram::parse(s), Some(TraeProgram::Trae), "解析失败: {s}");
        }
        for s in ["trae_code", "trae_code_global", "global_trae_code", "trae_cn", "traecode"] {
            assert_eq!(
                TraeProgram::parse(s),
                Some(TraeProgram::TraeCode),
                "遗留标识必须落 TraeCode，不能被裸名规则抢走: {s}"
            );
        }
        // 三个程序位的标识两两不同 —— `parse` 与 `as_str` 必须互为往返。
        for program in TraeProgram::all() {
            assert_eq!(TraeProgram::parse(program.as_str()), Some(program));
        }
    }

    /// 展示名按用户确认的写法：国际版带 `AI`，国内版不带。
    #[test]
    fn 程序位展示名符合约定() {
        use TraeProgram::{Trae, TraeCode, TraeWork};
        use TraeRegion::{Cn, Global};
        assert_eq!(program_spec(Cn, TraeWork).display_name, "TraeWork");
        assert_eq!(program_spec(Cn, TraeCode).display_name, "TraeCode");
        assert_eq!(program_spec(Global, TraeWork).display_name, "TraeWork AI");
        assert_eq!(program_spec(Global, TraeCode).display_name, "Trae AI");
        // 合并后的客户端：客户端自己就叫 `Trae`（CN）/ `Trae`（国际，按约定带 AI 后缀）。
        assert_eq!(program_spec(Cn, Trae).display_name, "Trae");
        assert_eq!(program_spec(Global, Trae).display_name, "Trae AI");
    }

    /// ★★ 合并版程序位（`Trae`）与遗留 TraeCode **必须指向同一个客户端**（issue #20）。
    ///
    /// 依据是 `migration-job.json`（模块头引了原文）：合并把 `TRAE SOLO CN` 的数据
    /// 并进了 `Trae CN`，而 `Trae CN` 正是遗留 TraeCode 的落点。
    ///
    /// 这条断言的意义：三者（候选名 / `packageType` / 展示名归属）一旦分家，
    /// 就会出现「同一台机器上 Trae 与 TraeCode 探测到不同客户端」，
    /// 或「合并版的登录态写进了另一套快照」——两种都不报错，只是数据串了。
    #[test]
    fn 合并版与遗留traecode指向同一客户端() {
        for region in TraeRegion::all() {
            let legacy = program_spec(region, TraeProgram::TraeCode);
            let merged = program_spec(region, TraeProgram::Trae);
            assert_eq!(
                merged.data_dir_names, legacy.data_dir_names,
                "{region:?} 的合并版 userData 候选与遗留 TraeCode 不一致"
            );
            assert_eq!(merged.exe_names, legacy.exe_names, "{region:?} 的 exe 候选不一致");
            assert_eq!(merged.proc_names, legacy.proc_names, "{region:?} 的进程名候选不一致");
            assert_eq!(merged.package_type, legacy.package_type, "{region:?} 的 packageType 不一致");
            // 真正要钉的是**程序位标识**不同 —— 否则前端拿不到区分两者的依据，
            // 「哪一枚按钮点下去写哪个客户端」这件事就没有载体。
            assert_ne!(merged.program, legacy.program);
            assert_ne!(merged.program.as_str(), legacy.program.as_str());
            // 展示名：CN 上两者必须可区分（`TraeCode` vs `Trae`）。
            //
            // ⚠️ Global 上**刻意不断言**不同：用户确认的约定是「国际版带 `AI` 后缀」，
            //    而那条约定给国际版 TraeCode 的名字本来就是 `Trae AI`（见模块头）。
            //    合并版与它是同一个客户端 ⇒ 同名是**正确的**，断言不同反而会红。
            //    （Global 那一行因此会看到两枚 `Trae AI` 按钮，这是「3 程序位」形态的
            //    必然结果，已在 `issue #20` 的设计选择里确认过。）
            if region == TraeRegion::Cn {
                assert_ne!(
                    merged.display_name, legacy.display_name,
                    "CN 的合并版与遗留位展示名相同 —— 界面上会分不清谁是谁"
                );
            }
        }
    }

    /// 程序位的候选名（目录 / exe / 进程）**必须两两互不重叠**，唯一例外**显式白名单**。
    ///
    /// 这是本模块存在的理由之一：改造前国内/国外候选名混在一张表里按序取首个，
    /// 用包含关系匹配就会「装了 A、读了 B 的目录」。谁把它们合并回去，这里会红。
    ///
    /// ## 白名单：合并版 `Trae` × 遗留 `TraeCode`（issue #20）
    ///
    /// 两者**物理上是同一个客户端**（同名 `Trae CN` / `Trae`，见模块头），
    /// 因此候选名必然重叠。这里断的是「重叠集合**恰好等于**这 6 项」，
    /// 而不是「随便重叠都行」——白名单外的任何新增重叠仍会红。
    #[test]
    fn 程序位候选名互不重叠除登记在案的重叠() {
        let specs = all_program_specs();
        let mut overlaps: Vec<String> = Vec::new();
        for (i, a) in specs.iter().enumerate() {
            for b in specs.iter().skip(i + 1) {
                for (kind, left, right) in [
                    ("userData 目录名", a.data_dir_names, b.data_dir_names),
                    ("exe 名", a.exe_names, b.exe_names),
                    ("进程名", a.proc_names, b.proc_names),
                ] {
                    for name in left {
                        if right.contains(name) {
                            overlaps.push(format!(
                                "{kind}:{name}:{:?}/{:?}:{:?}/{:?}",
                                a.region, a.program, b.region, b.program
                            ));
                        }
                    }
                }
            }
        }
        overlaps.sort();
        let mut expected = vec![
            "exe 名:Trae CN.exe:Cn/TraeCode:Cn/Trae",
            "exe 名:Trae.exe:Global/TraeCode:Global/Trae",
            "userData 目录名:Trae CN:Cn/TraeCode:Cn/Trae",
            "userData 目录名:Trae:Global/TraeCode:Global/Trae",
            "进程名:Trae CN:Cn/TraeCode:Cn/Trae",
            "进程名:Trae:Global/TraeCode:Global/Trae",
        ];
        expected.sort();
        assert_eq!(
            overlaps, expected,
            "候选名重叠集合变了 —— 白名单外的新重叠必须先修掉（或显式登记理由）"
        );

        // 反向：CN 与 Global 的同名程序**不能**共用候选名（差一个后缀也算）。
        assert_ne!(
            program_spec(TraeRegion::Cn, TraeProgram::TraeWork).data_dir_names,
            program_spec(TraeRegion::Global, TraeProgram::TraeWork).data_dir_names
        );
    }

    /// `programs_of` 必须给出该区域的**三条**程序线（遗留两条 + 合并版），
    /// 且每条都归属本区域。
    #[test]
    fn 按区域取程序位() {
        for region in TraeRegion::all() {
            let got = programs_of(region);
            assert_eq!(got.len(), 3, "每个区域应有 3 条程序线（Work / Code / 合并版 Trae）");
            for spec in got {
                assert_eq!(spec.region, region, "程序位归属漂了: {:?}", spec.program);
            }
            // 合并版必须在列表里 —— 漏掉它就等于「合并后的客户端在界面上不存在」，
            // 正是 issue #20 报的那个症状。
            assert!(
                got.iter().any(|s| s.program == TraeProgram::Trae),
                "{region:?} 的程序位列表缺少合并版 Trae"
            );
        }
    }

    /// 区域端点是**单一来源**：国内必须等于已实测的 CN 值，国际必须等于
    /// 国际版客户端自述值，且两者主机全不同（混用会把请求打到错的域）。
    #[test]
    fn 区域端点取自既有端点表() {
        let cn = region_endpoints(TraeRegion::Cn);
        assert_eq!(cn.account_base, "https://api.trae.cn");
        assert_eq!(cn.icube_base, "https://api.trae.com.cn");

        let global = region_endpoints(TraeRegion::Global);
        assert_eq!(global.account_base, "https://grow-normal.trae.ai");
        assert_eq!(global.icube_base, "https://icube-normal.trae.ai");
        assert_ne!(global.account_base, cn.account_base);

        // 授权页域（`bootConfig.consoleHost`）同样按区域分家 —— 它不是 API 域，
        // 但漏了分家的后果更隐蔽：国际版用户在**国内**授权页上登录，
        // 页面走完也不回调，症状与「回调端口没监听」几乎一样。
        assert_eq!(cn.console_base, "https://www.trae.cn");
        assert_eq!(global.console_base, "https://www.trae.ai");
        assert_ne!(global.console_base, cn.console_base);
    }
}
