//! Loomy（讯飞办公助手）—— 第四条产品线。
//!
//! 与 WorkBuddy / Trae / Qoder 平行，**不复用**彼此的平台模块。
//! 协议事实与建模决策见 `docs/specs/LOOMY-PLATFORM.md`；
//! 适配器蓝本为 `../free-llm-router/src/providers/loomy.ts`（协议已实测，非本仓文件）。
//!
//! ## 模块地图
//!
//! | 文件 | 被依赖方向 |
//! |:--|:--|
//! | [`region`] | 区域枚举 + 上游常量表（两基址 / 错误码 / AK·SK / TTL）—— 最底层 |
//! | [`paths`] | 账号库路径派生（枚举唯一入口） |
//! | [`account`] | 账号模型 + 账号库读写 + 线上视图 |
//! | [`credential`] | 凭据解析与判别特征（防误吞别家凭据） |
//! | [`signing`] | 讯飞账号（CAccount）HMAC-SHA1 签名（仅登录用） |
//! | [`models`] | 模型目录解析（三形态）+ 倍率拆解 |
//! | [`chat`] | 对话请求头与错误分类 |
//! | [`credits`] | 两池积分解析 |
//! | [`checkin`] |每日赠送额度签到（幂等） |
//! | [`oauth`] | 短信验证码登录 |
//! | [`refresh`] | 刻意**不存在**：Loomy 无续期端点，过期只能重登 |
pub mod account;
pub mod api;
pub mod chat;
pub mod checkin;
pub mod credential;
pub mod credits;
pub mod models;
pub mod oauth;
pub mod paths;
pub mod region;
pub mod signing;
