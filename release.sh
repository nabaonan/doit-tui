#!/usr/bin/env bash
# 发版脚本：自动推算下一个版本号 → 同步版本号（Cargo.toml / Cargo.lock）→ 提交 → 打 git tag
# → 推送到 GitHub，触发 release.yml 三平台构建并发布 Release。
#
# 用法：
#   ./release.sh                   补丁版 +0.0.1：v0.1.0 → v0.1.1（默认）
#   ./release.sh patch             同上面，显式指定补丁版
#   ./release.sh minor             次版本 +0.1.0：v0.1.1 → v0.2.0
#   ./release.sh major             主版本 +1.0.0：v0.2.0 → v1.0.0
#   ./release.sh 1.2.3             直接指定完整版本号 → v1.2.3（跳过自增）
#   ./release.sh minor --dry-run   只打印将打的版本，不做任何操作
#   ./release.sh --help            显示用法
#
# 注意事项：
#   - 必须在仓库根目录（doit-tui/）运行，且远端必须能 push（SSH 或 HTTPS 凭据）。
#   - 默认要求工作区干净（有未提交改动会中止），想无视就用 --force。
#   - 版本号需三处一致：Cargo.toml [package] + Cargo.lock [[package]] + git tag。

set -euo pipefail

usage() {
    sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
}

[[ $# -gt 0 && "$1" == "--help" || $# -gt 0 && "$1" == "-h" ]] && usage

BUMP="patch"
DRY=""
FORCE=""
EXPLICIT=""
ARGS=()
for a in "$@"; do
    case "$a" in
        patch|minor|major) BUMP="$a" ;;
        --dry-run) DRY=1 ;;
        --force|-f) FORCE=1 ;;
        --help|-h) usage ;;
        v[0-9]*.[0-9]*.[0-9]*|[0-9]*.[0-9]*.[0-9]*) EXPLICIT="$a" ;;
        *) ARGS+=("$a") ;;
    esac
done

if (( ${#ARGS[@]} > 0 )); then
    echo "❌ 无法识别的参数：${ARGS[*]}"
    usage
fi

# 0. 前置：必须仓库根目录
if [[ ! -f Cargo.toml ]]; then
    echo "❌ 请在 doit-tui/ 仓库根目录运行（找不到 Cargo.toml）"
    exit 1
fi

# 1. 解析当前最新 tag；没有任何 tag 时从 v0.0.0 开始
CURRENT="$(git describe --tags --abbrev=0 2>/dev/null || echo v0.0.0)"
if [[ "$CURRENT" =~ ^v([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    MAJOR="${BASH_REMATCH[1]}"
    MINOR="${BASH_REMATCH[2]}"
    PATCH="${BASH_REMATCH[3]}"
else
    echo "❌ 最近一个 tag（${CURRENT}）不是 vX.Y.Z 格式，无法自动自增"
    echo "   可手动指定：./release.sh 1.0.0"
    exit 1
fi

# 2. 计算新版本号
if [[ -n "$EXPLICIT" ]]; then
    NEW="${EXPLICIT#v}"
elif [[ "$BUMP" == "patch" ]]; then
    PATCH=$((PATCH + 1)); NEW="${MAJOR}.${MINOR}.${PATCH}"
elif [[ "$BUMP" == "minor" ]]; then
    MINOR=$((MINOR + 1)); NEW="${MAJOR}.${MINOR}.0"
else
    MAJOR=$((MAJOR + 1)); NEW="${MAJOR}.0.0"
fi

TAG="v${NEW}"

echo "📦 当前版本：${CURRENT}"
if [[ -n "$EXPLICIT" ]]; then
    echo "  下一版本：${TAG}（手动指定）"
else
    echo "  下一版本：${TAG}（${BUMP}）"
fi
if [[ -n "$DRY" ]]; then
    echo "  （dry-run，未做任何操作）"
    exit 0
fi

# 3. 前置检查：工作区干净
if [[ -z "$FORCE" ]]; then
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "❌ 工作区有未提交的改动："
        git status --short
        echo "   先提交或 stash，或用 --force 强制发版。"
        exit 1
    fi
fi

if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null 2>&1; then
    echo "❌ 本地已存在 tag ${TAG}，删掉或用其它版本号："
    echo "   git tag -d ${TAG}"
    exit 1
fi

if git ls-remote --exit-code --tags origin "$TAG" >/dev/null 2>&1; then
    echo "❌ 远端 origin 已存在 tag ${TAG}，换一个版本号。"
    exit 1
fi

# 4. 同步版本号到 Cargo.toml / Cargo.lock（锁文件同名 [[package]] name = "doit-tui"）
echo "✏️  同步版本号 ${NEW} 到 Cargo.toml / Cargo.lock ……"
python3 - "$NEW" <<'PY'
import re, sys
ver = sys.argv[1]

# Cargo.toml [package] version（只改第一个）
p = 'Cargo.toml'
s = open(p, encoding='utf-8').read()
s2 = re.sub(r'(?m)^version = "[^"]+"$', f'version = "{ver}"', s, count=1)
if s2 == s:
    raise SystemExit('Cargo.toml 未匹配到 version 行')
open(p, 'w', encoding='utf-8').write(s2)
print('   Cargo.toml ->', ver)

# Cargo.lock [[package]] name = "doit-tui" 的 version（跳过原 version 行防 TOML duplicate key）
p = 'Cargo.lock'
s = open(p, encoding='utf-8').read()
lines = s.split('\n')
out, done, skip = [], False, False
for i, ln in enumerate(lines):
    if skip:
        skip = False
        continue
    if not done and ln == 'name = "doit-tui"' and i + 1 < len(lines) and lines[i + 1].startswith('version'):
        out.append(ln)
        out.append(f'version = "{ver}"')
        done = True
        skip = True
        continue
    out.append(ln)
open(p, 'w', encoding='utf-8').write('\n'.join(out))
if not done:
    print('   ⚠️  Cargo.lock 未找到 doit-tui 包条目（构建时会自动更新）')
else:
    print('   Cargo.lock ->', ver)
PY

# 5. 提交 + 打 tag + 推送
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to ${NEW}"

echo "🚀 创建并推送 ${TAG} ……"
git tag -a "$TAG" -m "release: $TAG"
git push origin "$TAG"

echo ""
echo "✅ 已推送 ${TAG}，GitHub Actions 会自动构建并发布 Release。"
echo "   查看进度：https://github.com/nabaonan/doit-tui/actions"
