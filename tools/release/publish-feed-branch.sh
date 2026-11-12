#!/usr/bin/env bash
# Publishes the Homebrew Channel feed under a fixed URL as the orphan branch "hbc", holding only
# repo.json, description.html and largeIcon.png, updated by a normal commit and push. A TV keeps
# one repository entry and sees every new release; the file is never missing in between (no
# delete + upload), and the job token's contents:write is enough.
#
#   tools/release/publish-feed-branch.sh TAG DIR [REMOTE]
#
# DIR holds the files from tools/release/repository.py; their URLs keep pointing at the TAG's
# release assets. REMOTE defaults to "origin" (any git URL or path works, e.g. a local bare
# repository for a dry run). BRANCH (default hbc) overrides the branch name, NO_PUSH=1 commits
# without pushing.
#
# URL to add in the Homebrew Channel (Settings > Repositories):
#   <FORGEJO_URL>/<owner>/<repo>/raw/branch/hbc/repo.json
set -euo pipefail
TAG="${1:?usage: $0 TAG DIR [REMOTE]}"
DIR="$(realpath "${2:?usage: $0 TAG DIR [REMOTE]}")"
REMOTE="${3:-origin}"
BRANCH="${BRANCH:-hbc}"
FILES=(repo.json description.html largeIcon.png)
for f in "${FILES[@]}"; do [[ -f "$DIR/$f" ]] || { echo "missing $DIR/$f" >&2; exit 1; }; done

cd "$(git rev-parse --show-toplevel)"
WT="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/hbc-feed.XXXXXX")"
cleanup() { git worktree remove --force "$WT" >/dev/null 2>&1 || rm -rf "$WT"; git worktree prune; }
trap cleanup EXIT

# Start from the remote branch when it exists, else as a new orphan branch with no history.
if git fetch -q "$REMOTE" "refs/heads/$BRANCH" 2>/dev/null; then
  git worktree add -q --detach "$WT" FETCH_HEAD
else
  git worktree add -q --detach "$WT"
  git -C "$WT" checkout -q --orphan "hbc-feed-$$"
  git -C "$WT" rm -rfq . >/dev/null 2>&1 || true
fi
# Only the feed lives there.
git -C "$WT" ls-files -z | (cd "$WT" && xargs -0r git rm -q --)
for f in "${FILES[@]}"; do cp "$DIR/$f" "$WT/$f"; done
git -C "$WT" add -- "${FILES[@]}"
if git -C "$WT" rev-parse -q --verify HEAD >/dev/null && git -C "$WT" diff --cached --quiet; then
  echo "==> feed already at $TAG, nothing to commit"
else
  git -C "$WT" -c user.name="${GIT_AUTHOR_NAME:-Forgejo Actions}" -c user.email="${GIT_AUTHOR_EMAIL:-actions@noreply.invalid}" \
    commit -q -m "feed $TAG"
fi
if [[ "${NO_PUSH:-0}" = 1 ]]; then
  echo "==> NO_PUSH: feed commit $(git -C "$WT" rev-parse --short HEAD) not pushed"
else
  git -C "$WT" push -q "$REMOTE" "HEAD:refs/heads/$BRANCH"
  echo "==> pushed $BRANCH ($(git -C "$WT" rev-parse --short HEAD)) to $REMOTE"
fi
