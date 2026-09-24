#!/usr/bin/env bash
# 发版收尾脚本：v* tag 的 build 工作流跑完后，挂载 Windows GitHub Release。
#
#   bash scripts/release.sh vX.Y.Z            # 自动找该 tag 的成功 build run
#   bash scripts/release.sh vX.Y.Z <run-id>   # 或显式指定 run
#
# 做三件事：
#   1. 下载 build 工作流的 windows-nsis artifact 到 dist/；
#   2. 用 tag 所指提交的提交信息创建 / 更新 GitHub Release 并挂附件；
#   3. 打印验收提示。
#
# 幂等可重跑：附件 `--clobber` 覆盖上传。

set -euo pipefail

TAG="${1:?用法: scripts/release.sh vX.Y.Z [build-run-id]}"
RUN_ID="${2:-}"

case "$TAG" in
  v*) ;;
  *) echo "错误: '$TAG' 不是 v* 形式的 tag" >&2; exit 1 ;;
esac

command -v gh   >/dev/null || { echo "错误: 未安装 gh CLI" >&2; exit 1; }
command -v node >/dev/null || { echo "错误: 未安装 node" >&2; exit 1; }

if [ -z "$RUN_ID" ]; then
  echo "→ 查找 $TAG 触发的 build 工作流…"
  # 不按 run 整体 conclusion 过滤：只验 Windows artifact 是否存在。
  for id in $(gh run list --workflow=build --limit 30 --json databaseId,headBranch \
      | node -e 'let s="";process.stdin.on("data",d=>s+=d);process.stdin.on("end",()=>{JSON.parse(s).filter(r=>r.headBranch===process.argv[1]).forEach(r=>process.stdout.write(r.databaseId+"\n"))})' "$TAG"); do
    if gh api "repos/$(gh repo view --json nameWithOwner -q .nameWithOwner)/actions/runs/$id/artifacts" \
         --jq 'any(.artifacts[].name; . == "windows-nsis")' >/dev/null 2>&1; then
      RUN_ID=$id
      break
    fi
  done
  [ -n "$RUN_ID" ] || {
    echo "错误: 没找到 $TAG 的可用 build run（需 windows-nsis artifact）—— 先确认 CI 跑完（gh run list），或手动传 run-id" >&2
    exit 1
  }
fi
echo "→ build run: $RUN_ID"

rm -rf dist
gh run download "$RUN_ID" -D dist
EXE=$(ls dist/windows-nsis/*.exe)
echo "→ 已下载 $(basename "$EXE")"

NOTES_FILE=$(mktemp)
trap 'rm -f "$NOTES_FILE"' EXIT
git log -1 --format=%B "$TAG" > "$NOTES_FILE"

echo "→ GitHub Release"
if gh release view "$TAG" >/dev/null 2>&1; then
  gh release upload "$TAG" --clobber "$EXE"
  echo "  ↳ 已存在，附件覆盖上传"
else
  gh release create "$TAG" --title "$TAG" --notes-file "$NOTES_FILE" "$EXE"
fi

echo
echo "发版收尾完成。核对："
echo "  [1] GitHub Release:  gh release view $TAG"
echo "  [2] 安装包「关于」页版本号与 $TAG 一致"
