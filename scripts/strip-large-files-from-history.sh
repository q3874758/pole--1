#!/usr/bin/env bash
# strip-large-files-from-history.sh
#
# Rewrite history to remove large / non-source artifacts that were
# committed before the matching .gitignore rules landed.
#
#   - chain/poled.exe                  (162 MB Windows binary)
#   - chain/poled                       (Linux binary, if any)
#   - chain/.tmp-poled-home/**          (poled runtime tempdir)
#   - tools/wix*.zip                    (bundled WiX toolchain)
#   - dist/click-to-run/**              (legacy packaging output)
#
# ⚠️  THIS REWRITES ALL COMMITS. After running this you MUST
#     force-push, and every collaborator must re-clone.
#
# Usage:
#   ./scripts/strip-large-files-from-history.sh --dry-run
#   ./scripts/strip-large-files-from-history.sh --apply
#
# Requirements: git >= 2.30, git-filter-repo
#   pip install git-filter-repo
#   # or
#   cargo install git-filter-repo
#
# This script is intentionally chatty — read every line before
# pressing Enter.

set -euo pipefail

readonly SCRIPT_NAME="$(basename "$0")"
readonly BACKUP_BRANCH="backup/pre-history-strip-$(date -u +%Y%m%dT%H%M%SZ)"

usage() {
  cat <<EOF
$SCRIPT_NAME — remove large artifacts from git history.

USAGE:
    $SCRIPT_NAME --dry-run
    $SCRIPT_NAME --apply

OPTIONS:
    --dry-run   Show what would be removed (no rewrite, no push)
    --apply     Perform the rewrite, create a backup branch, and exit
                before any force-push.  You push manually after review.

This script will NOT push to any remote.
EOF
}

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 64
fi

case "$1" in
  --dry-run) MODE=dry-run ;;
  --apply)   MODE=apply ;;
  -h|--help) usage; exit 0 ;;
  *) usage >&2; exit 64 ;;
esac

# ----------------------------------------------------------------------------
# 1. Pre-flight
# ----------------------------------------------------------------------------
echo "==> Pre-flight checks"

if ! command -v git-filter-repo >/dev/null 2>&1; then
  echo "ERROR: git-filter-repo is not installed." >&2
  echo "       Install with: pip install git-filter-repo" >&2
  exit 1
fi

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "ERROR: not inside a git working tree" >&2
  exit 1
fi

# Refuse to run on a dirty tree.
if ! git diff --quiet HEAD 2>/dev/null || ! git diff --cached --quiet HEAD 2>/dev/null; then
  echo "ERROR: working tree is dirty. Commit or stash first." >&2
  exit 1
fi

# Refuse to run on a protected branch without explicit override.
current_branch="$(git rev-parse --abbrev-ref HEAD)"
if [[ "$current_branch" == "main" || "$current_branch" == "master" ]]; then
  cat <<EOF
WARNING: you are on '$current_branch'.  Rewriting history of the
primary branch is destructive.  We will create a backup branch first
and you must force-push manually after reviewing the result.
EOF
fi

# ----------------------------------------------------------------------------
# 2. Show what we are about to remove
# ----------------------------------------------------------------------------
paths=(
  --path chain/poled.exe
  --path chain/poled
  --path-glob 'chain/.tmp-poled-home/**'
  --path tools/wix314-binaries.zip
  --path tools/wix.zip
  --path-glob 'tools/wix-*.zip'
  --path-glob 'dist/click-to-run/**'
  --path-glob '**/poled.exe'
  --path-glob '**/poled'
  --invert-paths
)

echo "==> Paths to remove from history"
for ((i = 0; i < ${#paths[@]}; i += 2)); do
  if [[ $((i + 1)) -lt ${#paths[@]} ]]; then
    printf "    %s %s\n" "${paths[$i]}" "${paths[$((i+1))]}"
  else
    printf "    %s\n" "${paths[$i]}"
  fi
done

# Dry-run: actually run the rewrite in-place in a scratch clone so the
# user can inspect the resulting log, but never touch the working repo.
if [[ "$MODE" == "dry-run" ]]; then
  echo
  echo "==> Dry-run: cloning to scratch dir to inspect"
  scratch="$(mktemp -d -t pole-strip.XXXXXX)"
  git clone --quiet --no-local . "$scratch/repo"
  ( cd "$scratch/repo" && git-filter-repo --force "${paths[@]}" )
  echo
  echo "==> Resulting history (top 10 commits, scratch clone):"
  ( cd "$scratch/repo" && git log --oneline -n 10 )
  echo
  echo "==> Largest 5 files remaining in the working tree of the scratch clone:"
  ( cd "$scratch/repo" && find . -type f -not -path './.git/*' -printf '%s %p\n' \
      | sort -nr | head -5 )
  rm -rf "$scratch"
  echo
  echo "Dry-run complete.  Re-run with --apply to actually rewrite history."
  exit 0
fi

# ----------------------------------------------------------------------------
# 3. Apply
# ----------------------------------------------------------------------------
echo
echo "==> Creating backup branch $BACKUP_BRANCH"
git branch "$BACKUP_BRANCH"
echo "    Backup branch created.  You can recover with:"
echo "      git checkout $BACKUP_BRANCH"

echo
echo "==> Running git-filter-repo"
git-filter-repo --force "${paths[@]}"

echo
echo "==> Post-rewrite top 10 commits:"
git log --oneline -n 10

echo
echo "==> Largest 5 files remaining in HEAD:"
git ls-files | xargs -I {} sh -c 'wc -c "{}" 2>/dev/null' \
  | sort -nr | head -5

echo
echo "Done.  Next steps (manual):"
echo "  1. Sanity-check:  cargo build, cargo test"
echo "  2. Push:          git push origin $current_branch --force"
echo "  3. Notify collaborators to re-clone:"
echo "       rm -rf pole--1"
echo "       git clone https://github.com/q3874758/pole--1.git"
echo "  4. Delete the backup branch once everyone is in sync:"
echo "       git push origin --delete $BACKUP_BRANCH"
