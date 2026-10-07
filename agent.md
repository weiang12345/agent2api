# Agent.MD — 发版流程

> 本文档面向维护者与 AI 代理：Agent2API（workbuddy）桌面端 + Docker 镜像的**标准发版流程**。
> 所有发布动作都由 `.github/workflows/` 下的两个工作流自动完成，人工只负责「提交、打 tag、挂 GitHub Release、验收」。

## 0. 两条人工红线（AI 代理必须停下等确认）

这两件事**必须由人先审过、明确点头**，代理才能继续执行。代理可以准备草稿、可以列依据，但不能自己拍板往下走：

1. **发版**：进入发版流程（改版本号、建发布提交、推 main / beta、打 tag、挂 Release）之前，先把版本号与「这次要发什么」讲清楚，等用户确认。**未经确认不得推任何分支或 tag** —— tag 一旦推出去就会触发两个工作流、产出公开的安装包与镜像，收不回来。
2. **提交日志**：写好的更新日志（发布提交的提交信息，同时也是 GitHub Release 正文）必须先给用户逐条过目，等明确确认后才能提交。用户可以要求改措辞、增删条目、调整分类前缀，代理按反馈改完要**再确认一次**，不能因为「改得不多」就直接提交。

两条红线同时适用：一次发版里既要有版本号的确认，也要有日志的确认；用户说「继续」之后才动手。

## 1. 合并 PR：保留原作者署名

**合并贡献者的 PR 时，提交作者（author）必须还是贡献者本人。** 这直接决定对方能不能进仓库首页的 Contributors 名单、贡献图里有没有这一笔 —— 署名是贡献者应得的东西，不该因为合并方式丢掉。

GitHub 的归属规则只有一条：**提交头里的 author 邮箱，出现在某个账号已验证的邮箱列表里，这次提交就算那个账号的**；否则记成匿名条目，不进 Contributors。补充两条口径：统计只认默认分支（本仓库 `main`），合并提交与空提交不计入。

- **用 GitHub 网页上的合并按钮**（`Merge pull request` / `Squash and merge` / `Rebase and merge`）：三种都会保留原作者为 commit 的 author；squash 时 PR 里的多位作者会一并进 co-author 名单。这是默认做法。
- **禁止「本地摘取重提」**：把对方的改动拷过来自己提交（本地 `git merge --squash` 后自己 commit、重抄一遍改动、或改了 author 再提交）都不行 —— 这样 author 是合并者，贡献者的署名在 GitHub 侧完全不可见。本仓库历史上出现过这种情况，别再来一次。
- **确实要在本地处理时**（有冲突要解、要顺带调整），必须把署名带上：单个原作者用 `git commit --author="原作者 <原作者邮箱>"`；一个提交里有多个人的工作，在提交信息末尾加 trailer `Co-authored-by: 名字 <邮箱>`（GitHub 会把 co-author 一并计入贡献者）。
- **邮箱必须能对上账号**：用对方 GitHub 账号里**已验证**的地址；对方想保密就用 GitHub 给的 `ID+用户名@users.noreply.github.com`（在对方账号的 Emails 设置里查）。内网地址、`user@机器名` 这类邮箱永远关联不上账号，提交会被记成匿名。

合并后顺手核对一次：`git log --format='%an <%ae>' -1 <合并提交>`，或在 PR 页面看 commit 旁的头像是否指向原作者。

## 2. 版本号约定

- 版本号形如 `X.Y.Z`（如 `2.7.2`），日常小版本「递增 0.01」= 末位 +1（如 `2.7.8` → `2.7.9`）。
- **递增幅度以用户要求为准**：默认用上面的「递增 0.01」；用户另有要求时（如「这次递增 0.1」）一律听用户的，按用户指定的幅度算出新版本号，不要自作主张套用默认幅度。
- **版本号要同步改 5 处**（缺一处会导致安装包与显示版本对不上）：
  1. `package.json`（根）
  2. `desktop-tauri/package.json`
  3. `desktop-tauri/src-tauri/Cargo.toml`
  4. `desktop-tauri/src-tauri/tauri.conf.json`
  5. `desktop-tauri/src-tauri/server/Cargo.toml`
- 应用内「关于」与更新检查读的是 `env!("CARGO_PKG_VERSION")`（第 5 处），改完跑一次 `cargo check` 让 `Cargo.lock` 跟着刷新。

## 3. 提交：保证工作区干净

发版前必须把工作区收干净，tag 里的代码就是发出去的代码：

```bash
git status --short        # 必须为空；有改动就先提交或清理
cargo check               # 或前端有改动时做一次构建自检
```

## 4. 发布提交：更新日志写进提交信息

**发布提交的提交信息 = 更新日志**。原因：发版收尾脚本创建 / 更新 GitHub Release 时，正文取「tag 所指提交的提交信息」（`git log -1 --format=%B`）。

- 格式沿用历史版本的编号清单，每行一条，带分类前缀：`新增：` / `优化：` / `修复：` / `更新：` / `移除：`，最后一条固定 `版本：X.Y.Z → X.Y.Z+1`；
- 内容取「上个 tag 以来的全部提交」综合整理（`git log vX.Y.Z..HEAD --oneline`），纯 CI/临时的调试提交归并成一条流程性描述即可；
- 本次没有代码改动时用空提交承载：`git commit --allow-empty -F 更新日志.txt`（历史发版两种做法都有先例）。
- 更新日志草稿与历史发布说明统一写在 `docs/release-notes/<版本号>.md`（如 `docs/release-notes/2.9.2.md`），不要在项目根目录新建 `.release-notes-*.md` —— `docs/` 整目录已在忽略列表里，新增文件不必再单独加忽略规则。

## 5. 打 tag 并推送：一条 tag 触发全部构建

```bash
git push origin main
git tag vX.Y.Z
git push origin vX.Y.Z
```

推 `v*` tag 会**同时触发两个工作流**（`.github/workflows/`）：

| 工作流 | 产出 | 说明 |
|---|---|---|
| `build.yml`（build） | Windows NSIS 安装包 + macOS universal dmg | `macos` / `windows` 两个 job 构建并上传 artifact（macOS 包**只能在 CI 构建**，无法从 Windows 交叉编译）；安装包只挂 artifact，GitHub Release 由本地脚本挂载（见第 6 节） |
| `docker.yml`（docker） | Docker Hub `aimodcc/agent2api:<版本>` + `:latest`（amd64 / arm64 双架构） | 手动 `workflow_dispatch` 触发时只出 `:dev` 测试 tag，不碰正式 tag |

跟踪进度（手动跑 gh 前要先设代理，见第 8 节）：

```bash
gh run list --limit 4          # 确认工作流都已触发
gh run watch <run-id> --exit-status
```

## 6. 发版收尾：挂 GitHub Release（本地脚本一条命令）

GitHub Release **不由 CI 发布**（Release 本来就不会自动创建），由本地脚本
一步挂载：

```bash
bash scripts/release.sh vX.Y.Z            # 自动找该 tag 的成功 build run
bash scripts/release.sh vX.Y.Z <run-id>   # 或显式指定 run
```

脚本做三件事：下载 build 的两个安装包 artifact 到 dist/ → 按 tag 提交信息
（= 更新日志，见第 4 节）创建 / 更新 GitHub Release 并挂附件 → 打印验收
提示。幂等可重跑（`--clobber` 覆盖附件）。

## 7. 验收清单（三处核对）

- [ ] GitHub Release：`gh release view vX.Y.Z`（手动跑 gh 前先设代理，见第 8 节）—— 正文日志齐全，exe / dmg 两个附件都在；
- [ ] Docker Hub：`aimodcc/agent2api` 的 Tags 页出现 `<版本>` 与 `latest`，Pushed 时间一致；
- [ ] 安装包「关于」页版本号与 tag 一致。

## 8. 已知坑与排查

- **GHCR 新包默认私有**：若以后镜像改推 GHCR，首次推送后需到包设置手动改 Public（当前推的是 Docker Hub，无此问题）。
- **gh 不读 git 的代理配置，跑 gh 前要先设代理环境变量**：`gh` 是 Go 程序，不读 `~/.gitconfig` 里的 `http(s).proxy`，也不读 Windows 系统代理（WinINET），只认 `HTTPS_PROXY` / `HTTP_PROXY` 环境变量。本机 GitHub 直连时通时不通，手动执行 gh 命令前先设：

  ```bash
  export HTTPS_PROXY=$(git config --get-urlmatch http.proxy https://github.com || git config --get https.proxy)
  ```

  只设 `HTTPS_PROXY` 就够（gh 的请求全是 HTTPS，实测不读 `HTTP_PROXY`）。第 6 节的 `scripts/release.sh` 已内置这一步（从 git 配置读取后透传给 gh），用脚本发版无需手动设。代理没开时 gh 会立刻报 `proxyconnect tcp: ... connection refused` 而不回退直连 —— 与 `git push` 的表现一致。

## 9. fork 维护备忘（weiang12345/agent2api）

本节是 fork 相对上游的差异记录。第 5–7 节描述的是上游发版流程，fork 实际按本节执行。

- **本机不编译，发版全走 GitHub Actions 云端构建**（2026-10-07 起）：本机 `target/`（约 26GB）与 `desktop-tauri/src-tauri/target`（约 3GB）构建产物已全部删除，后续发版只推 `v*` tag 让云端构建，`scripts/release.sh` 收尾挂 Release。只有本地调试需要时才重新 `cargo check`（依赖重编约 2-3 分钟），平时不要在本机跑构建占磁盘。
- **CI 与上游的差异**：`build.yml` 只保留 `windows` job（不构建 macOS dmg，上游第 5 节表格里的 macOS 产物 fork 没有）；`docker.yml` 只允许手动 `workflow_dispatch`（发版 tag 不推 Docker Hub 镜像）；`release.sh` 只下载 `windows-nsis` 一个 artifact。
- **合并上游时必须保留的 fork 功能**：Trae 签到、CatPaw `tools` 嵌套过深修复（ Responses 入参 anyOf 展平）、ZCode 活动套餐通道，以及上面两条 CI 差异。
- **版本号格式**：fork 发版用 `X.Y.Z-fork.N`，`X.Y.Z` 跟随上游当前版本，fork 序号从 1 递增（如上游 2.9.4 → `2.9.4-fork.1`）。改完版本号用 `cargo update -p agent2api-server -p workbuddy-proxy-desktop` 刷新 Cargo.lock，不要在本机跑 `cargo check` 重编 22GB 依赖。
