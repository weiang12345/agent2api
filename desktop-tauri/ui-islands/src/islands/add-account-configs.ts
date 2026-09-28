/**
 * Agent2API · 「登录 / 添加账号」弹窗里各家的表单配置（数据层，无 JSX）。
 *
 * 逐字合并旧实现的三份来源：add-provider-forms.js 的 ADD_FORMS、以及
 * add-qoder.js / add-cline.js / add-accio.js / add-zcode.js 四份**纯常量**配置
 * （它们各自是独立文件、注册到 window 上，只被 add-provider-forms.js 读 ——
 * 迁到岛上后同一份数据在一个模块里更省事，因此不再挂 window，见最终报告）。
 *
 * 文案、字段名、上限、方法与裁剪规则都逐字保留：这些是能跑通上游的既有实现，
 * 迁的只是界面外壳。WorkBuddy 不在这张表里 —— 它的块结构特殊（账号版本 +
 * 打开方式 + 第三方入口开关），单独一个组件，见 add-provider-blocks.tsx。
 */

import { shared } from './add-account-bridge'

/* ─── 类型 ───────────────────────────────── */

/** 一个手填字段：key 直接对应请求体键名（inputKey 只改 DOM id 的中间段） */
export type FieldSpec = {
  key: string
  inputKey?: string
  label: string
  optional?: boolean
  rows?: number
  placeholder: string
}

export type RegionOption = { value: string; label: string }
export type MethodMode = { value: string; label: string; hint?: string }

export type WebLoginConfig = {
  /** 说明里的行内标记（原样注入，与旧实现的 noteOf 同口径） */
  noteHtml?: string
  button: string
  hint?: string
  busyText?: string
  /** 有这一级时多一个「打开方式」分段（没有的家只给内嵌窗口） */
  modes?: MethodMode[]
  /** 这一家的网页登录只在某个地区可用（当前没有家用它，结构上保留） */
  region?: string
  /** 写死的变体选择器（传给壳侧 start_login 的第一个参数） */
  edition?: string
}

export type SmsLoginConfig = { noteHtml?: string }

export type OauthLoginConfig = {
  title?: string
  noteHtml?: string
  hint?: string
  modes?: MethodMode[]
}

export type ProviderConfig = {
  provider: string
  label: string
  /** 手填段的主按钮文案（缺省「添加 X 账号」） */
  addButton?: string
  manualTitle?: string
  manualNote?: string
  manualNoteHtml?: string
  fields: FieldSpec[]
  webLogin?: WebLoginConfig
  smsLogin?: SmsLoginConfig
  oauthLogin?: OauthLoginConfig
  regionOptions?: RegionOption[]
  /** false = 这一家没有「从本机导入桌面端登录态」这一条来源 */
  desktop?: boolean
  desktopNote?: string
  desktopHint?: string
  /** 登录态是 Electron safeStorage 密文、解密要走 DPAPI（仅 Windows） */
  desktopWindowsOnly?: boolean
}

/* ─── 长度上限（与后端逐条对齐，前端先挡一次）────────────── */

/** 备注名长度上限（与后端 truncate_chars(name, 100) 一致） */
export const MAX_NAME_LENGTH = 100
/** uid / deviceId 长度上限（与后端 MAX_USER_UID_LENGTH / MAX_IDENTITY_LENGTH 一致） */
export const MAX_IDENTITY_LENGTH = 256
/** 单行控件的长度上限：备注名按 100，其余标识字段按 256 */
export const maxLengthOf = (field: FieldSpec): number =>
  (field.key === 'name' ? MAX_NAME_LENGTH : MAX_IDENTITY_LENGTH)

/* ─── 各家配置 ─────────────────────────────── */

const RACCOON: ProviderConfig = {
  // raccoon_accounts::add_raccoon_account：token / access_token、refreshToken / refresh_token、name
  provider: 'raccoon',
  label: '小浣熊',
  addButton: '添加小浣熊账号',
  // 网页登录（后端 providers::raccoon::oauth）：官方登录页 + 一次性授权码回调。
  // 只给内嵌窗口，没有「打开方式」这一级 —— 回调是自定义协议深链
  // （office-raccoon://auth/callback），系统浏览器模式下要靠系统注册该协议才回得来
  // （那是小浣熊官方客户端装的，装了才有），给了这个选项只会让用户选完永远等不到回调。
  webLogin: {
    noteHtml: '在<strong>内嵌窗口</strong>里完成官方登录，成功后自动加入账号列表。',
    button: '打开网页登录',
    hint: '内嵌窗口打开；完成后自动加入列表，关窗即取消等待',
    busyText: '等待小浣熊登录完成…',
  },
  manualTitle: '粘贴 token / refreshToken',
  manualNoteHtml: 'refreshToken 可选，填了到期可自动续期；两者都能从<a href="#" class="raccoon-hint-link" data-raccoon-hint>小浣熊客户端登录态文件</a>里取到。',
  fields: [
    { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空则用凭证里的账号名' },
    { key: 'token', label: 'token', rows: 3, placeholder: '粘贴 access_token（一长串 JWT）' },
    { key: 'refreshToken', inputKey: 'refresh', label: 'refreshToken', rows: 2, optional: true, placeholder: '可选' },
  ],
  desktopNote: '读本机客户端当前登录态，每次实时读取（删掉这条记录不影响客户端登录态）。客户端重新登录后点「刷新 Token」同步。',
}

const CATPAW: ProviderConfig = {
  // catpaw_accounts::add_catpaw_account：token / accessToken / access_token / auth_token
  // （即 X-Passport-Token）、uid / userId / loginName、name；没有刷新机制
  provider: 'catpaw',
  label: 'CatPaw',
  // 网页登录：美团 passport 授权页 + 上游把 token 推回本机网关的 loopback 回调。
  // 回调落点与「哪个浏览器」无关，因此系统浏览器完全走得通，而且它是内嵌窗口走不通
  // 时的兜底（美团 passport 的扫码 / 第三方账号登录在部分环境下会拒绝内嵌窗口）。
  webLogin: {
    noteHtml: '用 CatPaw 账号完成登录（美团 passport），成功后自动加入账号列表。',
    button: '打开 CatPaw 网页登录',
    busyText: '等待 CatPaw 登录完成…',
    modes: [
      { value: 'embedded', label: '内嵌窗口（推荐）', hint: '内嵌窗口打开；完成后自动加入列表，关窗即取消等待' },
      { value: 'external', label: '系统浏览器', hint: '系统浏览器打开（复用已登录的美团账号）；完成后自动加入列表' },
    ],
  },
  manualNote: 'token 是登录态 Cookie；CatPaw 没有刷新机制，过期后需在客户端重新登录。',
  fields: [
    { key: 'token', label: 'token', rows: 3, placeholder: 'CatPaw 的 X-Passport-Token（登录态 Cookie）' },
    { key: 'uid', label: 'uid', placeholder: '必填，CatPaw 账号标识' },
    { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空则用登录名或 uid' },
  ],
  desktopNote: '读本机客户端当前登录态，每次实时读取（删掉这条记录不影响客户端登录态）。客户端重新登录后重新导入即可。',
  desktopHint: '读取 ~/.meituan-catpaw/auth.json，需已在 CatPaw 客户端登录',
}

/**
 * AutoClaw 两个地区各占一项（与 Cline 两池同一手法）：`autoclaw`（国内版，id 不变
 * —— 存量账号的落盘契约）与 `autoclaw-intl`（国际版）。两项相邻排列。
 *
 * 两地的差别：域名（后端 `autoclaw::region`，前端不体现）；桌面端导入两地都给
 * （auth.json 两地共用、没有地区标记，地区由用户在哪一项下点导入决定）；
 * **登录方式完全不同** —— 国内版只有手机验证码，国际版只有 Zai / Google OAuth。
 */
const AUTOCLAW: ProviderConfig = {
  provider: 'autoclaw',
  label: 'AutoClaw 国内版',
  manualNote: 'token 支持 enc: 前缀（Windows 上自动解密）。未填 refreshToken 无法自动续期。',
  // 国内版**唯一**的官方登录方式（它的登录页不渲染 OAuth 按钮，已核对构建产物）
  smsLogin: {
    noteHtml: '用绑定的手机号登录：点「获取验证码」后填入即可。这是国内版官方唯一的登录方式。',
  },
  fields: [
    { key: 'token', label: 'token', rows: 3, placeholder: '明文 JWT 或 auth.json 里的 enc: 加密值（自动解密）' },
    { key: 'refreshToken', label: 'refreshToken', rows: 2, optional: true, placeholder: '没有则无法自动续期' },
    { key: 'deviceId', label: 'deviceId', optional: true, placeholder: '可选，续期时带上' },
    { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空则用 userId' },
  ],
  desktopNote: '读本机客户端当前登录态，每次实时读取（删掉这条记录不影响客户端登录态）。',
  desktopHint: '读取 %APPDATA%/AutoClaw/auth.json 并解密，仅 Windows',
  // 登录态是 Electron safeStorage 密文，解密要走 DPAPI（仅 Windows），
  // 因此 macOS 上整段收起（理由见 desktopImportAvailable）
  desktopWindowsOnly: true,
}

const AUTOCLAW_INTL: ProviderConfig = {
  // AutoClaw 国际版（autoglm-api.autoglm.ai）：与国内版同一套协议与签名指纹
  // （appId/appKey 两地逐字相同，已实测），只有站点不同。
  provider: 'autoclaw-intl',
  label: 'AutoClaw 国际版',
  manualNote: '国际版与国内版账号体系独立，请填国际版的凭证。未填 refreshToken 无法自动续期。',
  // OAuth 网页登录：国际版**唯一**的登录方式（登录页只渲染 Zai / Google 两个按钮）。
  // 与另外五家的差别：授权地址前有一次强制风控验证码（阿里云滑块），
  // 必须在浏览器里跑完才能拿地址，因此点按钮后会先在本弹窗里弹滑块
  // （见 ui/autoclaw-oauth.js 的文件头）。
  oauthLogin: {
    title: '网页登录（Zai / Google）',
    noteHtml: '点按钮后先过一次滑块验证（官方风控），随后打开登录页，登录完成即自动添加账号。'
      + '<br>这是国际版官方唯一的登录方式；已在客户端登录过的，用「导入桌面端登录态」更快。',
    modes: [
      { value: 'embedded', label: '内嵌窗口（推荐）', hint: '内嵌窗口打开；关窗即取消等待（Google 账号被拒时改用系统浏览器）' },
      { value: 'external', label: '系统浏览器', hint: '系统浏览器打开（复用已登录的 Zai / Google 账号）；完成后自动加入列表' },
    ],
  },
  fields: [
    { key: 'token', label: 'token', rows: 3, placeholder: '明文 JWT（国际版账号的 access token）' },
    { key: 'refreshToken', label: 'refreshToken', rows: 2, optional: true, placeholder: '没有则无法自动续期' },
    { key: 'deviceId', label: 'deviceId', optional: true, placeholder: '可选，续期时带上' },
    { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空则用 userId' },
  ],
  // 桌面端登录态导入两个地区都给：auth.json 没有地区标记、本机判断不了，
  // 用户在哪一项下点导入就得到哪一家的账号，猜错的后果是可见的上游 401。
  desktopNote: '读本机客户端当前登录态，每次实时读取（删掉这条记录不影响客户端登录态）。用 Zai / Google 登录客户端的用户走这一条。',
  desktopHint: '读取 %APPDATA%/AutoClaw/auth.json 并解密，仅 Windows',
  desktopWindowsOnly: true,
}

/** Qoder：没有「桌面端实时登录态」可导入，因此不生成那一段 */
const QODER: ProviderConfig = {
  provider: 'qoder',
  label: 'Qoder',
  desktop: false,
  regionOptions: [
    { value: 'global', label: '国际版' },
    { value: 'cn', label: '中国版' },
  ],
  webLogin: {
    // 国际版 / 中国版都有网页登录：两站是同一套 PKCE 设备授权协议，只有站点主机不同。
    // 因此不设 region 限制 —— 地区分段切到哪一站，这段就登录哪一站。
    noteHtml: '打开官方授权页完成设备码授权，登录的是「地区」所选那一站的账号。',
    button: '打开 Qoder 网页登录',
    busyText: '等待 Qoder 授权完成…',
    modes: [
      { value: 'embedded', label: '内嵌窗口（推荐）', hint: '内嵌窗口用全新环境，多账号互不影响；关窗即取消等待' },
      { value: 'external', label: '系统浏览器', hint: '系统浏览器打开（复用已登录账号）；完成后自动加入列表' },
    ],
  },
  manualTitle: '使用个人访问令牌（PAT）',
  manualNote: '在 Qoder 账号设置 → Integrations 生成 PAT（别填 Google / GitHub 的令牌）。',
  fields: [
    { key: 'pat', label: 'Qoder PAT', rows: 3, placeholder: '粘贴 Qoder 个人访问令牌（pt-…）' },
    { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空使用账号昵称或邮箱' },
  ],
}

/**
 * Cline 是两个提供商（Cline Free / Cline Pass）。
 *
 * 为什么不是一个配置带一个「额度池」下拉：上游模型带计费通道前缀
 * （`cline-free/…` / `cline-pass/…`），池是**账号的属性**，混在一起会让
 * 「哪个池能用」在界面上看不出来、同一模型的启停规则跨池互相影响。
 * 按两家建模后各占一个分组、各有一套账号与模型清单。除 id / 标签 / 文案外同构。
 */
function clineForm(spec: { provider: string; label: string; poolNote: string }): ProviderConfig {
  const { provider, label, poolNote } = spec
  return {
    provider,
    label,
    // 桌面登录态本身不属于任何池，两家都能导入（想要两个池都用就在两家各导入一次）
    desktop: true,
    webLogin: {
      // 设备授权登录（WorkOS RFC 8628）：没有回调、没有自定义协议，
      // 就是「打开授权页 → 用户确认 → 网关轮询拿到令牌」，因此两种打开方式都可行。
      noteHtml: '打开授权页完成确认，账号自动加入 <b>' + label + '</b>' + poolNote,
      button: '打开 Cline 授权页',
      busyText: '等待 Cline 授权确认…',
      modes: [
        { value: 'embedded', label: '内嵌窗口（推荐）', hint: '内嵌窗口打开；完成后自动加入列表，关窗即取消等待' },
        { value: 'external', label: '系统浏览器', hint: '系统浏览器打开（复用已登录的 Cline 账号）；完成后自动加入列表' },
      ],
    },
    manualTitle: '填写凭证',
    manualNote: 'refreshToken 可选，填了可自动续期；两者都能从客户端登录态文件取到。'
      + `同一个 Cline 账号两个池都能用，这里归入 ${label}。`,
    fields: [
      { key: 'accessToken', label: 'accessToken', rows: 3, placeholder: '粘贴 workos:… 开头的令牌（不带前缀也会自动补上）' },
      { key: 'refreshToken', label: 'refreshToken', rows: 2, optional: true, placeholder: '可选，没有则无法自动续期' },
      { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空使用账号姓名、邮箱或令牌指纹' },
    ],
    desktopNote: '读本机客户端当前登录态，每次实时读取（删掉这条记录不影响客户端登录态）。'
      + `这一条归入 ${label}。`,
    desktopHint: '读取 ~/.cline/data/settings/providers.json，需已在 Cline 客户端登录',
  }
}

/** AtomCode：云直连，凭证来自 AtomGit OAuth，不依赖本地客户端。 */
const ATMCODE: ProviderConfig = {
  provider: 'atomcode',
  label: 'AtomCode',
  desktop: false,
  webLogin: {
    noteHtml: '打开 AtomCode 授权页并完成登录，网关会自动取回 OAuth 凭证、领取 CodingPlan 并同步模型目录。'
      + '全程不需要本地 AtomCode 客户端。',
    button: '打开 AtomCode 授权页',
    busyText: '等待 AtomCode 授权完成…',
    modes: [
      { value: 'embedded', label: '内嵌窗口（推荐）', hint: '将打开 AtomGit 授权页，确认后自动加入账号列表。关掉窗口即取消等待' },
      { value: 'external', label: '系统浏览器', hint: '将用系统默认浏览器打开 AtomGit 授权页（会复用浏览器里已登录的 AtomGit 账号）；确认后自动加入账号列表' },
    ],
  },
  manualTitle: '填写 OAuth 凭证',
  manualNote: '优先使用上方网页登录。这里仅用于恢复已有账号：需要 AtomCode OAuth 的 accessToken、refreshToken 与 userId，三者缺一不可。',
  fields: [
    { key: 'accessToken', label: 'accessToken', rows: 3, placeholder: '粘贴 AtomCode OAuth access token' },
    { key: 'refreshToken', label: 'refreshToken', rows: 2, placeholder: '粘贴 AtomCode OAuth refresh token' },
    { key: 'userId', label: 'userId', placeholder: 'AtomGit 用户 ID' },
    { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空使用 AtomGit 昵称' },
  ],
}

/** Trae SOLO 国内版：云直连，凭证来自 Trae OAuth。 */
const TRAE: ProviderConfig = {
  provider: 'trae',
  label: 'Trae',
  desktop: false,
  webLogin: {
    noteHtml: '打开 Trae 授权页并完成登录，网关会自动取回凭证并保存账号。'
      + '当前接入的是国内版 Trae SOLO，不需要本地 Trae 客户端。',
    button: '打开 Trae 授权页',
    busyText: '等待 Trae 授权完成…',
    modes: [
      { value: 'embedded', label: '内嵌窗口（推荐）', hint: '将打开 Trae 授权页，确认后自动加入账号列表。关掉窗口即取消等待' },
      { value: 'external', label: '系统浏览器', hint: '将用系统默认浏览器打开 Trae 授权页；确认后自动加入账号列表' },
    ],
  },
  manualTitle: '填写 OAuth 凭证',
  manualNote: '优先使用上方网页登录。这里仅用于恢复已有账号：需要 accessToken、refreshToken、userId、machineId、deviceId。',
  fields: [
    { key: 'accessToken', label: 'accessToken', rows: 3, placeholder: '粘贴 Trae access token' },
    { key: 'refreshToken', label: 'refreshToken', rows: 2, placeholder: '粘贴 Trae refresh token' },
    { key: 'userId', label: 'userId', placeholder: 'Trae 用户 ID' },
    { key: 'machineId', label: 'machineId', placeholder: '登录时生成的 machineId' },
    { key: 'deviceId', label: 'deviceId', placeholder: '登录时生成的 deviceId' },
    { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空使用 Trae 昵称' },
  ],
}

/**
 * Accio 两个地区（国际版 / 国内版）：同一个网关、同一套接口，只有登录站点与
 * x-package-region 头不同。地区不是账号的字段而是身份，因此输出两份配置。
 */
function accioForm(spec: { provider: string; label: string; site: string; siteNote: string }): ProviderConfig {
  const { provider, label, site, siteNote } = spec
  return {
    provider,
    label,
    // OAuth 2.0 授权码 + PKCE：回调落在本机 loopback 端口，与浏览器在哪无关，
    // 因此内嵌窗口与系统浏览器两条路都走得通（与 CatPaw / AutoClaw 国际版同一形态）
    webLogin: {
      noteHtml: `打开 Accio <b>${label}</b>的官方登录页（<code>${site}</code>）并用你的 Accio 账号登录：`
        + '登录成功后官方页面会跳回本机，网关自动用一次性授权码换取凭证并加入账号列表'
        + '（授权码只在本机传给网关，界面不显示明文 token）。',
      button: `打开 Accio ${label}登录页`,
      busyText: `等待 Accio ${label}登录完成…`,
      modes: [
        { value: 'embedded', label: '内嵌窗口（推荐）', hint: '将打开内嵌窗口；登录完成后自动加入账号列表。关掉窗口即取消等待' },
        { value: 'external', label: '系统浏览器', hint: '将用系统默认浏览器打开登录页（会复用浏览器里已登录的 Accio 账号）；完成登录后自动加入账号列表，关掉弹窗即取消等待' },
      ],
    },
    manualTitle: '填写凭证',
    manualNoteHtml: 'accessToken 是 Accio 的登录凭证（一长串不透明 token，不是 JWT）；'
      + 'refreshToken 可选，填了之后到期能自动续期。'
      + `请填写 <b>${label}</b>账号的凭证 —— ${siteNote}`
      + '（最容易拿到的办法：直接用上方的「网页登录」，不需要手工找 token。）',
    fields: [
      { key: 'accessToken', label: 'accessToken', rows: 3, placeholder: '粘贴 Accio 的 accessToken' },
      { key: 'refreshToken', label: 'refreshToken', rows: 2, optional: true, placeholder: '可选，没有则无法自动续期' },
      { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空使用账号昵称或邮箱' },
    ],
  }
}

/**
 * ZCode 两个地区（国内版 / 国际版）：同一个 zcode 平面（登录 / 领取都在
 * zcode.z.ai），只有推理站点不同，因此输出两份配置。
 */
function zcodeForm(spec: { provider: string; label: string; site: string; planNote: string }): ProviderConfig {
  const { provider, label, site, planNote } = spec
  return {
    provider,
    label,
    // 本家没有「桌面端实时登录态」可导入：凭证在它自己的加密存储里，
    // 没有 auth.json 那种稳定可读的形态（与 Accio 同一处境）
    desktop: false,
    webLogin: {
      // 授权地址由**服务端**给且不回本机：网关拿到一次性授权地址，用户在浏览器里
      // 授权后由 ZCode 服务端记录结果，网关在后台轮询取回凭证。因此两种打开方式
      // 都走得通（与 Accio 同构），但「页面最后提示无法打开 zcode:// 链接」是正常的。
      noteHtml: `打开 ZCode <b>${label}</b>的官方授权页并用你的账号登录。`
        + '授权结果由 ZCode 服务端记录，网关在后台自动取回凭证并加入账号列表。',
      button: `打开 ZCode ${label}授权页`,
      busyText: `等待 ZCode ${label}授权完成…`,
      modes: [
        { value: 'embedded', label: '内嵌窗口（推荐）', hint: '将打开内嵌窗口；授权完成后自动加入账号列表。关掉窗口即取消等待。授权页最后可能提示无法打开 zcode:// 链接，这是正常的 —— 结果已由服务端记下' },
        { value: 'external', label: '系统浏览器', hint: '将用系统默认浏览器打开授权页（会复用浏览器里已登录的 ZCode 账号）；授权完成后自动加入账号列表。页面最后可能提示无法打开 zcode:// 链接，属正常现象' },
      ],
    },
    manualTitle: '填写凭证',
    manualNoteHtml: '本家有两个**互不替代**的凭证，按你要用的功能填，至少填一个：'
      + '<b>编码套餐 API Key</b> 用于转发推理（打 <code>' + site + '</code>）；'
      + '<b>jwt</b> 用于领取套餐（打 <code>zcode.z.ai</code>，官方叫 Coding Plan JWT，'
      + '是一串三段点分的字符串）。只填 jwt 的账号能领套餐但不能转发，反之亦然。'
      + `请填写 <b>${label}</b>账号的凭证 —— ${planNote}`
      + '（最容易拿到的办法：直接用上方的「网页登录」—— 它会替你把这个 API Key 换好，两个凭证一起拿到。）',
    fields: [
      {
        key: 'accessToken',
        label: '编码套餐 API Key',
        rows: 3,
        optional: true,
        // 这里不能填 OAuth 登录态：那个串拿去转发会被上游按「OAuth 令牌」那条路
        // 校验并回 401。官方客户端与「网页登录」这条链都是先换成编码套餐 API Key 再用。
        placeholder: '用于转发；形如 apiKey.secret 两段点分（不填则这个账号不能转发）',
      },
      { key: 'jwt', label: 'Coding Plan JWT', inputKey: 'jwt', rows: 3, optional: true, placeholder: '用于领取套餐；不填则这个账号不能领取' },
      { key: 'userId', label: '用户 ID', optional: true, placeholder: '可选；用于生成账号 id 与展示名' },
      { key: 'name', label: '备注名', optional: true, placeholder: '可选，留空自动生成' },
    ],
  }
}

/** 内置家的表单块，顺序与旧 ADD_FORMS 一致（只影响 DOM 里的块顺序，不影响界面） */
export const BUILTIN_CONFIGS: ProviderConfig[] = [
  RACCOON,
  CATPAW,
  AUTOCLAW,
  AUTOCLAW_INTL,
  QODER,
  // Cline 顺序即界面上「提供商」分段的顺序：免费池在前（无门槛，更常用）
  clineForm({ provider: 'cline-free', label: 'Cline Free', poolNote: '（免费额度池，模型名带 cline-free/ 前缀）。' }),
  clineForm({ provider: 'cline-pass', label: 'Cline Pass', poolNote: '（订阅池，模型名带 cline-pass/ 前缀，需要账号有对应订阅）。' }),
  ATMCODE,
  TRAE,
  // Accio 顺序：国际版在前（默认安装的版本）
  accioForm({ provider: 'accio', label: '国际版', site: 'www.accio.com', siteNote: '国际版与国内版是**两套独立的账号**（同一账号体系的两个站点），凭证不通用。' }),
  accioForm({ provider: 'accio-cn', label: '国内版', site: 'www.accio-ai.com', siteNote: '国内版的登录站点是 www.accio-ai.com，与国际版不是同一站。' }),
  // ZCode 顺序：国内版在前（国内网络环境下更常被添加的那个，与后端注册表 PROVIDERS 的排列一致）
  zcodeForm({ provider: 'zcode', label: '国内版', site: 'open.bigmodel.cn', planNote: '国内版与**国际版是两套独立的账号与套餐**，凭证与领取的套餐都不通用。' }),
  zcodeForm({ provider: 'zcode-intl', label: '国际版', site: 'api.z.ai', planNote: '国际版的推理站点是 api.z.ai，与国内版不是同一站；套餐也各自独立。' }),
]

/** WorkBuddy 的块 id（结构特殊，单独一个组件） */
export const WORKBUDDY_PROVIDER = 'workbuddy'
/** 自定义提供商的块 id（后注册块的 provider 取值） */
export const CUSTOM_PROVIDER = 'custom'

export function configOf(providerId: string): ProviderConfig | undefined {
  return BUILTIN_CONFIGS.find(item => item.provider === providerId)
}

/* ─── 添加方式（分段项）─────────────────────────
 *
 * 文案要短（并排一行，太长会把弹窗挤到换行），详细说明留在各段自己的标题与正文里。
 * 露出哪些项由 methodsOf 按各家配置裁剪。
 *
 * `oauth` 单独一项而不并进「网页登录」：对用户来说两者都是「跳去官方页面登录」，
 * 但交互不同 —— 网页登录点一下就开窗口（等待态由 web-login.js 统一管），而
 * AutoClaw 国际版要先在本弹窗里弹一个阿里云滑块让用户拖完才能拿到授权地址。
 * 并进去会让那个引擎多出「有些家要先跑一段验证码」的分支，且两者的发起时序与
 * 按钮布局都不一样（这一项是两个变体各一个按钮）。 */
export const ADD_METHODS = [
  { id: 'oauth', label: '网页登录（Zai / Google）' },
  { id: 'sms', label: '手机验证码登录' },
  { id: 'web', label: '网页登录' },
  { id: 'manual', label: '填写凭证' },
  { id: 'desktop', label: '导入桌面端登录态' },
] as const

export type MethodId = (typeof ADD_METHODS)[number]['id']

/**
 * 壳的编译目标平台（'macos' / 'windows' / 'linux' / 'web'），由桥接脚本注入。
 *
 * 浏览器直开（没有壳）时拿不到它，退回空串 —— 此时按「不裁剪」处理：
 * 浏览器直连网关本来就用不了这些壳侧功能，多显示一个选项不会误导谁，
 * 而误裁掉一个**本来可用**的功能会让 Windows 用户莫名其妙少一项。
 */
export const platform = (): string => shared().workbuddyDesktop?.platform || ''

/**
 * 桌面端导入在这一家、这个平台是否可用。
 *
 * AutoClaw 要按平台裁掉：它读 %APPDATA%/AutoClaw/auth.json，而那个文件里的 token
 * 是 Electron safeStorage 的密文，要先过 DPAPI（CryptUnprotectData）才能解出密钥
 * —— DPAPI 只有 Windows 有。另外三家（小浣熊 ~/.box-agent、CatPaw ~/.meituan-catpaw、
 * Cline ~/.cline）读的都是 HOME 下的明文 JSON，macOS 上照样能导入，所以只裁 AutoClaw。
 */
export function desktopImportAvailable(config: ProviderConfig): boolean {
  if (config.desktop === false) return false
  // 网页端（headless 托管面板注入 platform='web'）：没有本机桌面客户端可读
  if (platform() === 'web') return false
  if (config.desktopWindowsOnly && platform() === 'macos') return false
  return true
}

/** 网页登录那一段只在某个地区露出时的判据（当前没有家用，结构上保留） */
export function methodsOf(config: ProviderConfig, region: string): MethodId[] {
  return ADD_METHODS.filter(method => {
    if (method.id === 'oauth') return Boolean(config.oauthLogin)
    if (method.id === 'sms') return Boolean(config.smsLogin)
    if (method.id === 'web') return Boolean(config.webLogin) && (!config.webLogin?.region || region === config.webLogin.region)
    if (method.id === 'desktop') return desktopImportAvailable(config)
    return true
  }).map(method => method.id)
}

/** 手填表单标题：默认「填写凭证添加」，小浣熊沿用原文案 */
export const manualTitleOf = (config: ProviderConfig): string => config.manualTitle || '填写凭证添加'
/** 说明中的行内标记优先，否则按纯文本渲染（React 的文本节点天然转义） */
export const manualNoteOf = (config: ProviderConfig): { html?: string; text?: string } =>
  (config.manualNoteHtml ? { html: config.manualNoteHtml } : { text: config.manualNote || '' })

/** 手填段主按钮文案 */
export const addButtonTextOf = (config: ProviderConfig): string =>
  config.addButton || `添加 ${config.label} 账号`

/** 字段的 DOM id（inputKey 保留小浣熊既有的 refresh-input id，请求体键仍为 refreshToken） */
export const fieldIdOf = (config: ProviderConfig, field: FieldSpec): string =>
  `${config.provider}-${field.inputKey || field.key}-input`
