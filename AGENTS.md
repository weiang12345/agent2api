# Agent2API 项目记忆

## 1. 语言与仓库

- 与用户沟通使用简体中文。
- 本仓库是公开 fork：`weiang12345/agent2api`。
- 上游仓库：`aimod-cc/agent2api`。
  - `upstream` remote 指向上游。
  - `origin` remote 指向本 fork。
- 默认软件更新仓库必须保持为 `weiang12345/agent2api`，对应
  `desktop-tauri/src-tauri/server/src/server/core/update/version.rs` 里的 `DEFAULT_REPO`。
- 仓库保持 public。若改回 private：
  - GitHub Release API 匿名访问会 404。
  - 安装包下载也会失败。
  - 必须配置 `WORKBUDDY_GITHUB_TOKEN` 或 `GITHUB_TOKEN`，否则更新功能不可用。

## 2. 分支与同步

- 发布基线是 `main`。
- 不要在 `chore/sync-upstream` 分支上提交代码；同步工作流会定期重建它。
- 上游同步流程：
  1. 从 `upstream/main` 拉取最新代码。
  2. 合并到本地 `main` 或单独分支。
  3. 冲突时优先保留本 fork 的定制改动，尤其是：
     - `DEFAULT_REPO`
     - CatPaw JSON 深度上限
     - 更新逻辑
  4. 合并后必须重新跑测试和构建。

- 上游 `2.4.6` 起，内容脱敏已改为出站指纹脱敏：
  - 配置键：`sanitizeBlacklistFingerprints`
  - 入口：`/api/sanitize`
  - 核心实现：`desktop-tauri/src-tauri/server/src/server/core/sanitize.rs`
  - 旧词表、远程同步和独立脱敏页已删除，不要再按旧方案恢复。

## 3. 构建与测试

本 fork 只构建和发布 Windows 版本，不构建 macOS 安装包。

在仓库根目录执行：

```powershell
npm --prefix desktop-tauri ci
npm run tauri:build
```

Rust 侧验证命令：

```powershell
cd desktop-tauri/src-tauri
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --lib
```

Windows 本机构建时，Cargo 需要清空代理并离线运行，避免代理环境导致依赖解析失败：

```powershell
$env:HTTP_PROXY=''
$env:HTTPS_PROXY=''
$env:ALL_PROXY=''
$env:NO_PROXY='*'
$env:CARGO_NET_OFFLINE='true'
```

如果 Git 访问 GitHub 需要 HTTPS 代理，可单独设置：

```powershell
$env:HTTP_PROXY='http://127.0.0.1:7890'
$env:HTTPS_PROXY='http://127.0.0.1:7890'
```

## 4. 发布流程

当前 `.github/workflows/build.yml` 只构建 Windows 并上传 artifact，不自动创建 GitHub Release；
实际发布仍按本节手动执行。

发布前必须完成：

1. `main` 分支干净并与 `origin/main` 同步。
2. `cargo check --locked --all-targets` 通过。
3. `cargo test --locked --lib` 通过。
4. 本地 Windows Tauri 构建成功，产物路径：
   - Windows：`target/release/bundle/nsis/*.exe`

发布步骤：

1. 在 GitHub 上创建 Release。
2. Release tag 命名：
   - 上游版本未变、只是本 fork 修复：`v{上游版本}-fork.{N}`，例如 `v2.4.5-fork.1`。
   - 想让客户端真正检测到新版本：必须把应用版本号整体提升到更高数字，再使用对应 tag。
3. 将构建出的 Windows 安装包上传为 Release asset。
4. Release 说明使用简体中文，写明：
   - 同步的上游版本
   - 本 fork 的改动
   - 更新仓库地址

## 5. 版本与更新逻辑

- 更新比较逻辑只读取版本号前三段数字。
- `v2.4.5-fork.1` 会被视为与 `2.4.5` 相同，不会提示“有新版本”。
- 因此：
  - 只做 fork 修补时，`-fork.N` 可以继续沿用。
  - 需要用户更新时，必须提升 `package.json`、`desktop-tauri/package.json`、
    `desktop-tauri/src-tauri/Cargo.toml`、`desktop-tauri/tauri.conf.json` 里的版本号。
- 更新检查默认访问：

```text
https://api.github.com/repos/weiang12345/agent2api/releases/latest
```

## 6. 发布后验证

每次发布后必须验证：

1. 匿名访问 Release API 返回 200，并包含正确 tag 和资产。
2. 匿名下载安装包成功。
3. 安装包 SHA256 与本地构建产物一致。
4. GitHub Actions 的 `ci` 与 Windows `build` workflow 通过。

示例：

```powershell
$response = Invoke-RestMethod https://api.github.com/repos/weiang12345/agent2api/releases/latest
$response.tag_name
$response.assets[0].name
$response.assets[0].size
```

## 7. 禁止事项

- 不要提交 `target/`、`node_modules/`、安装包或临时构建产物。
- 不要修改上游仓库 `aimod-cc/agent2api`。
- 不要把 `DEFAULT_REPO` 改回上游地址。
- 不要在未跑测试的情况下发布 Release。
- 不要假设私有仓库可以直接匿名访问 GitHub API。

## 8. 当前部署基线

- 当前 Release：`v2.7.7-fork.1`
- 当前发布基线提交：`498dbac`
- 当前更新仓库：`weiang12345/agent2api`
- 当前构建产物：`Agent2API_2.7.7_x64-setup.exe`
- 当前 SHA256：

```text
B9B70B270F108CF6B068CB1E7AB763BAA683F8878517F4C3BC85892C95A955A3
```

## 9. AtomCode 云直连与上游合并

- AtomCode 是本 fork 的专属 provider，上游没有对应实现。
- 同步上游时必须保留：
  - `desktop-tauri/src-tauri/server/src/server/core/providers/atomcode/`
  - `desktop-tauri/src-tauri/server/src/server/core/account_store/atomcode_accounts.rs`
  - `desktop-tauri/ui/add-atomcode.js`
  - `ProviderKind::AtmCode`
  - `adapter_for(ProviderKind::AtmCode)`
- 上游若重构 provider 契约，优先把 AtomCode 模块迁移到新契约，不要直接删除。
- 合并冲突时，公共层改动尽量保持“新增分支”形态，避免重排上游代码。
- AtomCode 走云直连：
  - OAuth：`https://acs.atomgit.com`
  - CodingPlan：`https://api.gitcode.com/api/v5`
  - 模型网关：`https://llm-api.atomgit.com/v1`
- AtomCode 请求签名使用社区维护的 `atomcode-signing-v1`，上游协议变化时优先参考
  `Atom2Api` / `atomgit-opencode-bridge` 的更新。

## 10. Trae 云直连与上游合并

- Trae 是本 fork 的专属 provider，上游没有对应实现。
- 同步上游时必须保留：
  - `desktop-tauri/src-tauri/server/src/server/core/providers/trae/`
  - `desktop-tauri/src-tauri/server/src/server/core/account_store/trae_accounts.rs`
  - `desktop-tauri/ui/add-trae.js`
  - `ProviderKind::Trae`
  - `adapter_for(ProviderKind::Trae)`
- 上游若重构 provider 契约，优先把 Trae 模块迁移到新契约，不要直接删除。
- 当前只接入国内版 Trae SOLO，不做国际版和双区域自动路由。
- Trae 走云直连：
  - 对话：`https://trae-api-cn.mchost.guru`
  - OAuth：`https://api.trae.com.cn`
  - 额度 / 签到：`https://api.trae.cn`
- Trae 协议参考 `wangqi233/trae2api` 与 `JeffHu0912/trae2api`，上游变化时优先
  对照这两个项目的协议更新。

## 11. 上游接口自测纪律

- 修改签到、登录、余额、模型目录或任何上游协议前，必须先用真实账号凭证
  手工调用一次目标接口。
- 手工自测时只输出：
  - HTTP 状态码
  - 业务码
  - 响应 JSON
  - 必要的非敏感请求头名
- 禁止把 token / refreshToken / cookie / Authorization 值输出到终端或日志。
- 若手工请求失败，先对比参考项目的请求头、请求体和设备标识，再改代码。
- 修完必须补一个可复现测试或至少一次真实接口验证。
- Trae 签到尤其要使用：
  - 派生设备 ID
  - 极简请求头
  - 9074 后轮换代数
