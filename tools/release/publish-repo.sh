#!/usr/bin/env bash
# DEPRECATED, to be removed after one release: replaced by tools/release/publish-feed-branch.sh
# (the orphan branch "hbc", no delete + upload window, no write:package token).
#
# Publishes the Homebrew Channel feed under a fixed URL, so a TV keeps one repository entry and
# sees every new release: the Forgejo generic package <name>, version "latest".
#
#   tools/release/publish-repo.sh DIR        (DIR holds repo.json from tools/release/repository.py)
#
# URL to add in the Homebrew Channel (Settings > Repositories):
#   <FORGEJO_URL>/api/packages/<owner>/generic/<PACKAGE_NAME>/latest/repo.json
#
# Env: FORGEJO_URL, FORGEJO_REPO (owner/name), FORGEJO_TOKEN (needs write:package; the Actions
# job token may not have it, so a FORGEJO_TOKEN secret is used when set), PACKAGE_NAME
# (default: the repository name).
set -euo pipefail
DIR="${1:?usage: $0 DIR}"
: "${FORGEJO_URL:?}" "${FORGEJO_REPO:?}" "${FORGEJO_TOKEN:?}"
OWNER="${FORGEJO_REPO%%/*}"
NAME="${PACKAGE_NAME:-${FORGEJO_REPO##*/}}"
BASE="${FORGEJO_URL%/}/api/packages/$OWNER/generic/$NAME/latest"
AUTH=(-H "Authorization: token $FORGEJO_TOKEN")

# A generic package file cannot be overwritten: delete the old one first (404 the first time).
curl -sS -o /dev/null -X DELETE "${AUTH[@]}" "$BASE/repo.json" || true
curl -fsS "${AUTH[@]}" --upload-file "$DIR/repo.json" "$BASE/repo.json" >/dev/null
echo "==> Homebrew Channel repository: $BASE/repo.json"
