#!/usr/bin/env bash
#
# Release Doit GPUI: bump the version, generate the changelog, tag and push.
#
# Usage:
#   ./scripts/release.sh                 # bump patch  (0.1.0 -> 0.1.1)
#   ./scripts/release.sh minor           # bump minor  (0.1.0 -> 0.2.0)
#   ./scripts/release.sh major           # bump major  (0.1.0 -> 1.0.0)
#   ./scripts/release.sh 1.2.3           # set an explicit version
#   ./scripts/release.sh --dry-run       # preview everything, change nothing
#
# What it does:
#   1. bump the version in Cargo.toml / Cargo.lock
#   2. generate the CHANGELOG section from commits since the last tag
#      (added / fixed / improved / docs / other buckets)
#   3. cargo check as a quick sanity gate
#   4. commit Cargo.toml + Cargo.lock + CHANGELOG.md, tag vX.Y.Z, push main+tags
#
# Pushing the tag triggers .github/workflows/release.yml, which builds the
# macOS universal (Apple Silicon + Intel), Windows x64 and Linux x64 binaries
# and publishes the GitHub Release with this changelog.

set -euo pipefail

cd "$(dirname "$0")/.."

DRY_RUN=0
BUMP="patch"
for arg in "$@"; do
  if [ "$arg" = "--dry-run" ] || [ "$arg" = "-n" ]; then
    DRY_RUN=1
  else
    BUMP="$arg"
  fi
done

# ── resolve the next version ────────────────────────────────────────────────
cur_version="$(sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -n1)"
if ! [[ "$cur_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: cannot parse version from Cargo.toml: '$cur_version'" >&2
  exit 1
fi
IFS='.' read -r cmj cmn cpt <<<"$cur_version"

case "$BUMP" in
  major) nmj=$((cmj + 1)); nmn=0; npt=0 ;;
  minor) nmj=$cmj; nmn=$((cmn + 1)); npt=0 ;;
  patch) nmj=$cmj; nmn=$cmn; npt=$((cpt + 1)) ;;
  *)
    if [[ "$BUMP" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      IFS='.' read -r nmj nmn npt <<<"$BUMP"
    else
      echo "usage: $0 [--dry-run] [major|minor|patch|X.Y.Z]" >&2
      exit 1
    fi
    ;;
esac
next_version="$nmj.$nmn.$npt"

if [ "$cur_version" = "$next_version" ]; then
  echo "notice: version unchanged ($cur_version); will tag it as-is"
fi

# ── changelog inputs ─────────────────────────────────────────────────────────
last_tag="$(git describe --tags --abbrev=0 2>/dev/null || true)"
if [ -n "$last_tag" ]; then
  range="$last_tag..HEAD"
else
  range=""
fi
today="$(date +%F)"

build_section() {
  local added="" fixed="" improved="" docs="" other=""
  while IFS= read -r line; do
    [ -z "$line" ] && continue
    case "$line" in
      feat:*|feat\(*\):*|新增:*|新增*|增加:*|支持:*|添加:*)
        added="${added}\n- $line" ;;
      fix:*|fix\(*\):*|bugfix:*|bugfix\(*\):*|修复:*|修复*|解决:*|修正:*)
        fixed="${fixed}\n- $line" ;;
      refactor:*|refactor\(*\):*|perf:*|perf\(*\):*|优化:*|重构:*|调整:*)
        improved="${improved}\n- $line" ;;
      docs:*|docs\(*\):*|文档:*)
        docs="${docs}\n- $line" ;;
      chore:*|chore\(*\):*|build:*|ci:*|test:*|test\(*\):*|release:*)
        ;;  # housekeeping, not user-facing -> skip
      *)
        other="${other}\n- $line" ;;
    esac
  done <<<"$(git log --pretty=format:'%s' $range)"

  printf '## [v%s] - %s\n\n' "$next_version" "$today"
  if [ -n "$added" ];    then printf '### 新增\n%b\n\n' "$added";    fi
  if [ -n "$fixed" ];    then printf '### 修复\n%b\n\n' "$fixed";    fi
  if [ -n "$improved" ]; then printf '### 优化\n%b\n\n' "$improved"; fi
  if [ -n "$docs" ];     then printf '### 文档\n%b\n\n' "$docs";     fi
  if [ -n "$other" ];    then printf '### 其他\n%b\n\n' "$other";    fi
}

if [ "$DRY_RUN" = "1" ]; then
  echo "当前版本 : $cur_version"
  echo "下个版本 : v$next_version"
  echo "变更区间 : ${range:-<全部历史>}"
  echo "──────────── 变更日志预览 ────────────"
  build_section
  echo "──────────────────────────────────────"
  echo "（dry-run：未修改任何文件）"
  exit 0
fi

# ── working tree must be clean ───────────────────────────────────────────────
if [ -n "$(git status --porcelain)" ]; then
  echo "error: working tree is dirty; commit or stash your changes first" >&2
  exit 1
fi

# ── bump Cargo.toml / Cargo.lock ─────────────────────────────────────────────
sed -i.bak "s/^version = \"$cur_version\"$/version = \"$next_version\"/" Cargo.toml
if ! grep -q "version = \"$next_version\"" Cargo.toml; then
  rm -f Cargo.toml.bak
  echo "error: failed to bump the version in Cargo.toml" >&2
  exit 1
fi
rm -f Cargo.toml.bak

if [ -f Cargo.lock ]; then
  perl -0pi -e "s/(name = \"doit-gpui\"\nversion = \")[^\"]+(\")/\${1}${next_version}\${2}/" Cargo.lock
fi

echo "→ 版本提升: $cur_version -> $next_version"

# sanity gate
echo "→ cargo check ..."
if ! cargo check 1>/dev/null 2>&1; then
  echo "error: cargo check failed after the version bump" >&2
  exit 1
fi

# ── update CHANGELOG.md (prepend the new section) ────────────────────────────
section="$(mktemp)"
build_section > "$section"

if [ ! -f CHANGELOG.md ]; then
  printf '# Changelog\n\n所有显著变更都会记录在此文件中。\n' > CHANGELOG.md
fi
tmp="$(mktemp)"
{
  head -n1 CHANGELOG.md
  printf '\n'
  cat "$section"
  printf '\n'
  tail -n +2 CHANGELOG.md
} > "$tmp"
mv "$tmp" CHANGELOG.md
rm -f "$section"
echo "→ CHANGELOG.md 已更新"

# ── commit, tag, push ────────────────────────────────────────────────────────
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -q -m "chore: release v${next_version}"
git tag -a "v${next_version}" -m "v${next_version}"
git push origin main --tags

echo ""
echo "✓ 已发布 v${next_version}（main + tag 已推送）"
echo "  多平台构建与 Release：https://github.com/nabaonan/doit-gpui/actions"
echo "  下载地址：https://github.com/nabaonan/doit-gpui/releases"
