//! 网关错误类型与「上游类别 → HTTP 状态码 / 协议错误体」映射（P0-7）。

use serde_json::{json, Value};

use buddy_switch_core::modules::product::Scope;
use buddy_switch_core::modules::region::{self, Region, RegionMismatch};
use buddy_switch_core::modules::upstream::UpstreamErrorKind;

/// 网关统一错误。
#[derive(Debug, Clone)]
pub enum GatewayError {
    /// 未携带/无效/已吊销 Key → 401
    Unauthorized(String),
    /// 该 region 无可用凭据 → 401。
    ///
    /// ★ `cause` **只影响文案，不影响** [`Self::type_name`]（四类共用 `no_credential`）
    /// —— 客户端按类别名分流，改它等于改对外契约。
    NoCredential {
        /// 归属域。
        region: Region,
        /// 具体原因。四类状态的**可行动指引完全不同**，见 [`NoCredentialCause`]。
        cause: NoCredentialCause,
    },
    /// 选中的账号只有加密信封凭据 → 401（可读指引；见 `account::envelope_token_error`）
    EncryptedCredential { region: Region },
    /// 凭据 region 不符 → 401（安全红线 F，含修复指引）
    RegionMismatch(RegionMismatch),
    /// 上游失败 → 按 kind 映射
    Upstream {
        kind: UpstreamErrorKind,
        status: u16,
        message: String,
    },
    /// 请求参数不合法 → 400
    BadRequest(String),
    /// 模型被用户禁用 → **403**（明确拒绝，不是笼统 4xx 里的 400；P11 §3.18.1）。
    ///
    /// 与 `/v1/models` 的过滤**同源**（读同一份 `disabled` 存储 + 同一个 [`crate::disabled_models::is_disabled`]）——
    /// 否则禁用就是「假开关」。
    ModelDisabled { scope: Scope, model: String },
    /// 网关自身错误（端口占用/目录失败/内部）→ 502（不返回裸堆栈）
    Internal(String),
}

/// 「该 region 没有可用凭据」的**具体原因**。
///
/// # 为什么需要它（issue #26，2026-10-14）
///
/// 改造前 `NoCredential` 只有 `region` 一个字段 ⇒ 下面**四类状态逐字共用同一句**
/// 「未找到已登录的 X 账号，请在桌面端登录该版本后重试」：
///
/// | 状态 | 真实处境 | 那句指引 |
/// |:--|:--|:--|
/// | 认证文件缺失 | 桌面端确实没登录 | ✅ 有效 |
/// | 账号库为空 | 同上（但换登录**真的**能修） | ✅ 有效 |
/// | 指定账号不在库 | 账号被删了，登录无关 | ❌ 无效 |
/// | 池里有号但全不可用 | **已经登录了**，号被禁用/冷却/零积分 | ❌ 无效 |
///
/// 后两类下「去桌面端重新登录」是**无效指引**：报障人因此「切换过多个账号、
/// 桌面端和 Buddy Switch 都试过」仍然解决不了 —— 被文案带偏，而不是他操作不对。
/// 更糟的是 Docker / headless 部署**根本没有桌面端**，那句话对这类用户是死路。
///
/// ⇒ 原因必须自证，且指引必须指向**真正能解的那一步**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoCredentialCause {
    /// `current` 策略：本机该 region 的认证文件缺失，或存在但读不出凭据。
    ///
    /// （`read_auth_file_for` 把「文件存在但不可解析」视为该位置的**权威**结果，
    /// 不回退下一候选；`imported_account_from_root` 取不到 `access_token` 也返回 `None`。
    /// 两者都落到这一档。）
    AuthFileMissing,
    /// 该 region 账号库为空（`max_credits` 策略，或 relay 同步后池内该区域 0 条）。
    StoreEmpty,
    /// `pinned` 策略：被指定的账号不在该 region 账号库中（多半已被删除）。
    PinnedMissing,
    /// relay 兜底：池里该 region **有**账号，但此刻一个都不可用。
    ///
    /// 计数来自池快照（`Pool::summary`），用于把「有 12 个号但 0 个可用」这类
    /// 事实**直接摆给用户**，而不是让他猜。
    PoolExhausted {
        /// 该区域池内账号总数。
        total: usize,
        /// 其中已禁用（三振终态）的数量。
        disabled: usize,
        /// 其中冷却中的数量。
        cooling: usize,
        /// 其中余额为 0 的数量。
        zero_credits: usize,
    },
}

impl GatewayError {
    /// 上游类别 → HTTP 状态码（A-3.5）。
    pub fn status(&self) -> u16 {
        match self {
            GatewayError::Unauthorized(_)
            | GatewayError::NoCredential { .. }
            | GatewayError::EncryptedCredential { .. }
            | GatewayError::RegionMismatch(_) => 401,
            GatewayError::Upstream { kind, .. } => match kind {
                UpstreamErrorKind::HardCredit => 402,
                UpstreamErrorKind::SoftRate => 429,
                UpstreamErrorKind::SessionDead => 401,
                UpstreamErrorKind::NotFound => 502,
                UpstreamErrorKind::Server => 502,
                UpstreamErrorKind::Client => 400,
            },
            GatewayError::BadRequest(_) => 400,
            // 403 = 明确拒绝（凭据有效、请求合法，但该模型被用户禁用）。
            GatewayError::ModelDisabled { .. } => 403,
            GatewayError::Internal(_) => 502,
        }
    }

    /// 错误类别名（`type`/`code` 字段用）。
    pub fn type_name(&self) -> &'static str {
        match self {
            GatewayError::Unauthorized(_) => "unauthorized",
            GatewayError::NoCredential { .. } => "no_credential",
            GatewayError::EncryptedCredential { .. } => "encrypted_credential",
            GatewayError::RegionMismatch(_) => "region_mismatch",
            GatewayError::Upstream { kind, .. } => match kind {
                UpstreamErrorKind::HardCredit => "hard_credit",
                UpstreamErrorKind::SoftRate => "soft_rate",
                UpstreamErrorKind::SessionDead => "session_dead",
                UpstreamErrorKind::NotFound => "not_found",
                UpstreamErrorKind::Server => "server",
                UpstreamErrorKind::Client => "client",
            },
            GatewayError::BadRequest(_) => "bad_request",
            GatewayError::ModelDisabled { .. } => "model_disabled",
            GatewayError::Internal(_) => "internal",
        }
    }

    /// 可读消息（不含堆栈）。
    pub fn message(&self) -> String {
        match self {
            GatewayError::Unauthorized(message) => message.clone(),
            GatewayError::NoCredential { region, cause } => no_credential_message(*region, *cause),
            // ★ 与 NoCredential 刻意分开：这条的用户**其实已经登录了**，
            // 说「未找到已登录的账号」会把人引到错的方向（去重新登录桌面端）。
            // 真正要做的是让该账号在本应用里重新取得**明文**凭据。
            GatewayError::EncryptedCredential { region } => format!(
                "选中的{}账号凭据是 WorkBuddy 加密信封态，网关无法用它发起请求；\
                 请在 Buddy Switch 中对该账号重新登录（或 OAuth 扫码添加）以取得明文凭据",
                region_display(*region)
            ),
            GatewayError::RegionMismatch(mismatch) => mismatch.message(),
            GatewayError::Upstream { message, .. } => message.chars().take(400).collect(),
            GatewayError::BadRequest(message) => message.clone(),
            // 与 Trae 侧共用同一句文案（`disabled_models::disabled_message`），避免两处漂移。
            GatewayError::ModelDisabled { scope, model } => {
                crate::disabled_models::disabled_message(*scope, model)
            }
            GatewayError::Internal(message) => message.clone(),
        }
    }

    /// OpenAI 端点错误体：`{"error":{"message","type","code"}}`。
    pub fn openai_body(&self) -> Value {
        let kind = self.type_name();
        json!({ "error": { "message": self.message(), "type": kind, "code": kind } })
    }

    /// Anthropic 端点错误体：`{"type":"error","error":{"type","message"}}`。
    pub fn anthropic_body(&self) -> Value {
        json!({ "type": "error", "error": { "type": self.type_name(), "message": self.message() } })
    }
}

/// `no_credential` 的分档文案（**唯一出口**）。
///
/// 两条规则，缺一不可：
/// 1. **说清事实** —— 有几个号、几个不可用、分别为什么，而不是笼统的「未找到」；
/// 2. **指引必须真能解** —— 只在「确实没登录」时才让人去登录；对 Docker / headless
///    （本机没有桌面客户端）必须给出替代做法，否则那句话是死路。
///
/// 反面教材（改造前，见 [`NoCredentialCause`] 的文档）：四类状态共用一句
/// 「请在桌面端登录该版本后重试」，把「号被禁用」的用户引向重新登录。
fn no_credential_message(region: Region, cause: NoCredentialCause) -> String {
    let name = region_display(region);
    match cause {
        NoCredentialCause::AuthFileMissing => format!(
            "未找到已登录的{name}账号，请在桌面端登录该版本后重试；\
             若本机没有该版本的桌面客户端（如 Docker / headless 部署），\
             请在「网关 → 账号池」用扫码方式添加账号"
        ),
        NoCredentialCause::StoreEmpty => format!(
            "本机{name}账号库为空，且认证文件也取不到凭据；\
             请先登录该版本桌面端并导入账号，或在本应用内扫码添加账号"
        ),
        NoCredentialCause::PinnedMissing => format!(
            "「指定账号」指向的账号不在本机{name}账号库中（可能已被删除）；\
             请在「网关 → 账号池」重新指定，或改为「不指定」由网关自动择优"
        ),
        NoCredentialCause::PoolExhausted {
            total,
            disabled,
            cooling,
            zero_credits,
        } => format!(
            "本机{name}账号库有 {total} 个账号，但此刻 0 个可用：\
             {disabled} 个已禁用、{cooling} 个冷却中、{zero_credits} 个余额为 0。\
             重新登录 / 切换账号不会解除禁用与冷却 —— \
             请在「网关 → 账号池」查看逐账号原因（禁用是终态，需余额刷新成功或人工复活）"
        ),
    }
}

/// region 展示名（错误文案用）。
pub fn region_display(region: Region) -> &'static str {
    region::region_spec(region).display_name
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(kind: UpstreamErrorKind) -> GatewayError {
        GatewayError::Upstream {
            kind,
            status: 0,
            message: "boom".to_string(),
        }
    }

    #[test]
    fn upstream_kind_status_mapping_matches_design() {
        assert_eq!(kind(UpstreamErrorKind::HardCredit).status(), 402);
        assert_eq!(kind(UpstreamErrorKind::SoftRate).status(), 429);
        assert_eq!(kind(UpstreamErrorKind::SessionDead).status(), 401);
        assert_eq!(kind(UpstreamErrorKind::NotFound).status(), 502);
        assert_eq!(kind(UpstreamErrorKind::Server).status(), 502);
        assert_eq!(kind(UpstreamErrorKind::Client).status(), 400);
    }

    #[test]
    fn local_error_status_mapping() {
        assert_eq!(GatewayError::Unauthorized("x".into()).status(), 401);
        assert_eq!(
            GatewayError::NoCredential {
                region: Region::Cn,
                cause: NoCredentialCause::StoreEmpty,
            }
            .status(),
            401
        );
        assert_eq!(GatewayError::BadRequest("x".into()).status(), 400);
        assert_eq!(GatewayError::Internal("x".into()).status(), 502);
    }

    /// 信封凭据必须与「未找到已登录账号」**分开**。
    ///
    /// 混用会把排查引向错的方向：这条的用户其实**已经登录了**，让他「去桌面端登录」
    /// 是无效指引；真正要做的是让该账号在本应用里重新取得**明文**凭据。
    #[test]
    fn encrypted_credential_is_a_distinct_readable_401() {
        let error = GatewayError::EncryptedCredential { region: Region::Cn };
        assert_eq!(error.status(), 401);
        assert_eq!(error.type_name(), "encrypted_credential");
        assert_ne!(
            error.type_name(),
            GatewayError::NoCredential {
                region: Region::Cn,
                cause: NoCredentialCause::StoreEmpty,
            }
            .type_name(),
            "不得与 no_credential 混为一谈"
        );

        let message = error.message();
        assert!(message.contains("加密信封"), "文案应可读：{message}");
        assert!(message.contains("WorkBuddy"), "应点名是哪个区域：{message}");
        assert!(
            message.contains("重新登录"),
            "应给出可行动的做法：{message}"
        );
    }

    /// 四类原因的文案必须**两两不同**，且都必须点名区域。
    ///
    /// 阳性对照：改造前四类共用一句（逐字相同）⇒ 本用例必红。
    #[test]
    fn no_credential_causes_are_distinct_and_named() {
        let region = Region::Cn;
        let causes = [
            NoCredentialCause::AuthFileMissing,
            NoCredentialCause::StoreEmpty,
            NoCredentialCause::PinnedMissing,
            NoCredentialCause::PoolExhausted {
                total: 12,
                disabled: 9,
                cooling: 3,
                zero_credits: 0,
            },
        ];
        let messages: Vec<String> = causes
            .iter()
            .map(|cause| {
                GatewayError::NoCredential {
                    region,
                    cause: *cause,
                }
                .message()
            })
            .collect();

        for (cause, message) in causes.iter().zip(&messages) {
            assert!(
                message.contains("WorkBuddy"),
                "{cause:?} 的文案必须点名区域：{message}"
            );
        }
        for i in 0..messages.len() {
            for j in (i + 1)..messages.len() {
                assert_ne!(
                    messages[i], messages[j],
                    "{:?} 与 {:?} 的文案逐字相同 —— 用户无法据此判断该做什么",
                    causes[i], causes[j]
                );
            }
        }
    }

    /// ★ issue #26 的核心护栏：**「池里有号但一个都不可用」不得再让人去重新登录**。
    ///
    /// 真实报障（2026-10-14）：`切换过多个账号 桌面端或者buddyswitch都试过解决不了`。
    /// 该用户的处境是「号在库里、但全被禁用 / 冷却」，而改造前的文案与「从没登录过」
    /// **逐字相同** ⇒ 他被引去做了一件**不可能有用**的事。
    ///
    /// 断言两侧：① 事实必须摆出来（总数 + 分类计数）；② 那句无效指引**必须消失**。
    #[test]
    fn pool_exhausted_message_states_facts_and_drops_the_relogin_advice() {
        let message = GatewayError::NoCredential {
            region: Region::Cn,
            cause: NoCredentialCause::PoolExhausted {
                total: 12,
                disabled: 9,
                cooling: 3,
                zero_credits: 0,
            },
        }
        .message();

        assert!(message.contains("12 个账号"), "应给出账号总数：{message}");
        assert!(message.contains("9 个已禁用"), "应给出禁用计数：{message}");
        assert!(message.contains("3 个冷却中"), "应给出冷却计数：{message}");
        assert!(
            !message.contains("请在桌面端登录该版本"),
            "「池里有号但全不可用」时不得再让人去桌面端登录（无效指引）：{message}"
        );
    }

    /// 「确实没登录」那一档**必须保留**桌面端指引 —— 否则是把有效指引一起删掉。
    ///
    /// 这是上一条断言的**阳性对照**：没有它，把四类文案全改成「请联系客服」
    /// 也能让上一条变绿。
    #[test]
    fn auth_file_missing_message_keeps_the_desktop_advice() {
        let message = GatewayError::NoCredential {
            region: Region::Cn,
            cause: NoCredentialCause::AuthFileMissing,
        }
        .message();
        assert!(
            message.contains("请在桌面端登录该版本后重试"),
            "真·未登录时必须保留桌面端指引：{message}"
        );
    }

    /// Docker / headless 用户**没有桌面端** ⇒ 文案必须给替代做法。
    ///
    /// 反例：只有「请在桌面端登录该版本后重试」一句时，容器部署的用户
    /// 照着做**做不到**（容器里没有图形客户端）。
    #[test]
    fn headless_deployments_get_a_usable_alternative() {
        for cause in [
            NoCredentialCause::AuthFileMissing,
            NoCredentialCause::StoreEmpty,
        ] {
            let message = GatewayError::NoCredential {
                region: Region::Cn,
                cause,
            }
            .message();
            assert!(
                message.contains("扫码"),
                "{cause:?} 应给无桌面客户端场景的替代做法：{message}"
            );
        }
    }

    /// `type_name` 是**对外分流契约**，四类原因必须共用 `no_credential`。
    ///
    /// 客户端（以及 `docs/redesign/P11-ARCH.md` §1.5）按类别名分流；
    /// 顺手把它拆成四个新类别名会让既有客户端全部走进未知分支。
    #[test]
    fn no_credential_type_name_stays_stable_across_causes() {
        for cause in [
            NoCredentialCause::AuthFileMissing,
            NoCredentialCause::StoreEmpty,
            NoCredentialCause::PinnedMissing,
            NoCredentialCause::PoolExhausted {
                total: 1,
                disabled: 1,
                cooling: 0,
                zero_credits: 0,
            },
        ] {
            let error = GatewayError::NoCredential {
                region: Region::Cn,
                cause,
            };
            assert_eq!(error.type_name(), "no_credential", "{cause:?} 不得改类别名");
            assert_eq!(error.status(), 401, "{cause:?} 必须是 401");
        }
    }

    #[test]
    fn region_mismatch_is_unauthorized_and_upstream_messages_are_bounded() {
        let mismatch = GatewayError::RegionMismatch(RegionMismatch {
            actual_domain: "www.workbuddy.ai".to_string(),
            expected_file: "workbuddy-desktop.info".to_string(),
            env_var: "WORKBUDDY_AUTH_FILE".to_string(),
            actual_region: Region::Global,
            expected_region: Region::Cn,
        });
        assert_eq!(mismatch.status(), 401);
        assert_eq!(mismatch.type_name(), "region_mismatch");
        let upstream = GatewayError::Upstream {
            kind: UpstreamErrorKind::Server,
            status: 500,
            message: "x".repeat(1000),
        };
        assert_eq!(upstream.message().chars().count(), 400);
    }

    #[test]
    fn error_bodies_have_expected_shape() {
        let err = kind(UpstreamErrorKind::HardCredit);
        let openai = err.openai_body();
        assert_eq!(openai["error"]["type"], "hard_credit");
        assert_eq!(openai["error"]["code"], "hard_credit");
        assert!(openai["error"]["message"].is_string());

        let anthropic = err.anthropic_body();
        assert_eq!(anthropic["type"], "error");
        assert_eq!(anthropic["error"]["type"], "hard_credit");
        assert!(anthropic["error"]["message"].is_string());
    }
}
