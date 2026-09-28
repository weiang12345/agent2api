//! ZCode（智谱 / Z.AI 的编码代理客户端）适配：**国内版 / 国际版**两个 provider。
//!
//! ── 上游长什么样 ────────────────────────────────────────────
//! ZCode 的「编码套餐」（Coding Plan / Start Plan）可以在客户端里用**订阅
//! 登录态**调用，不必去开放平台申请 API Key。本家复刻的就是这条链路：
//!
//! ```text
//!   zcode 平面（登录 / 领取 / 客户端配置）        推理平面
//!   https://zcode.z.ai                     国内 → https://open.bigmodel.cn
//!   （两地相同）                            国际 → https://api.z.ai
//! ```
//!
//! 推理是 **OpenAI 兼容**协议（`{openai_base}/chat/completions`），
//! 与 raccoon / autoclaw 同构（无状态、Bearer 鉴权、SSE 回写模型名），
//! 因此本家不需要 `is_stateful` 那条「适配器自己发一次请求」的路子。
//!
//! ── 与前端预设 `glm` / `glm-cn` 的分工（别当成重复建设）──────
//! `ui/preset-providers.js` 里已有两项预设指向同一批端点，但那条路要用户
//! 自己提供 **API Key**（走开放平台计费）。本家补的是**订阅登录态**那条通道：
//! OAuth 登录 → JWT → 转发。两者并存，各取所需。
//!
//! ── 本家没有签到，改接「周末套餐领取」──────────────────────
//! 其余各家都有每日签到（`core::auto_checkin`），ZCode 的运营玩法是限时发放的
//! 体验套餐。因此本家进定时任务框架的是 [`claim`] 而不是签到 —— 调度形状相同
//! （每天/按窗口跑一次、结果进同一套汇总），协议完全不同。
//!
//! ── 子模块与当前进度 ────────────────────────────────────────
//!   region.rs       地区（域名 / 身份 / 环境变量 / 账号 id 前缀）—— 已完成
//!   models.rs       模型清单（静态表，两地共用）—— 已完成
//!   claim.rs        周末套餐领取（探测 / 领取 / 失败分类 / 调度语义）—— 已完成
//!   adapter.rs      `ProviderAdapter` 实现（OpenAI 兼容转发）—— 已完成
//!   credentials.rs  凭证（访问令牌 + 套餐 JWT + 设备标识）—— 已完成
//!   oauth.rs        CLI 轮询登录（init / poll / 授权地址中转页）—— 已完成
//!   coding_key.rs   编码套餐凭证换取（OAuth 令牌 → 能用于推理的 API Key）—— 已完成
//!
//! 登录链已接进 `core/login/zcode.rs`（`start_zcode_login` / `run_zcode_login`，
//! 与 Qoder 那支同构：启动任务 → 轮询 → 落账号），界面上「添加账号」的两个
//! 入口（网页登录 / 填写凭证）都可用。
//!
//! ── 两件**已知未做**的事（别误以为已经覆盖）───────────────────
//!   1. **凭证续期**：上游没有续期协议 —— OAuth 令牌用坏了只能重新登录
//!      （参考实现实测：JWT 只带 `iat` 不带 `exp`，8 天前的仍能用，过期只以
//!      401 暴露）。适配器的 `refresh_access_token` 因此如实报错，见 `adapter.rs`。
//!   2. **客户端请求签名**（Client Request Signing V4）：上游的
//!      `GET {zcode}/api/v1/agent/configs` 此刻回 `codingPlanSignature.enable: true`，
//!      官方客户端会给编码套餐的推理请求加 Ed25519 签名 + 工作量证明头。
//!      参考实现对它**全程 fail-open**（握手失败、连续两次 401 `VERIFY_*`
//!      都退回未签名），且实测未签名请求的拒绝理由是鉴权（`Authentication
//!      Failed`）而不是签名 —— 因此本家先不做，靠 `coding_key` 把凭证换对。
//!      哪天真被 `VERIFY_SIGNATURE_INVALID` 拒了，再照参考实现的
//!      `src/proxy/client-signing.ts` 补。

pub mod adapter;
pub mod claim;
pub mod coding_key;
pub mod credentials;
pub mod models;
pub mod oauth;
pub mod region;
