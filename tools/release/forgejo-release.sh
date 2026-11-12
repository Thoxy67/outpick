#!/usr/bin/env bash
# Creates (or updates) the Forgejo release for TAG with NOTES.md as its text and every file
# of DIR as an asset, through the Forgejo API (after term's scripts/release/forgejo-mirror.sh).
#
#   tools/release/forgejo-release.sh TAG DIR NOTES.md [--prerelease]
#
# Env: FORGEJO_URL (https://git.example.org), FORGEJO_REPO (owner/name), FORGEJO_TOKEN (a
# token with write:repository; in Actions, the job's own token works). Assets with the same
# names are replaced, so a re-run is safe.
set -euo pipefail
TAG="${1:?usage: $0 TAG DIR NOTES.md [--prerelease]}"
DIR="${2:?}"
NOTES="${3:?}"
PRE=false
[[ "${4:-}" = "--prerelease" ]] && PRE=true
: "${FORGEJO_URL:?}" "${FORGEJO_REPO:?}" "${FORGEJO_TOKEN:?}"
API="${FORGEJO_URL%/}/api/v1/repos/$FORGEJO_REPO"
AUTH=(-H "Authorization: token $FORGEJO_TOKEN")
VERSION="${TAG#v}"
NAME="${RELEASE_NAME:-Inlet} $VERSION"

json() { python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'; }

id="$(curl -fsS "${AUTH[@]}" "$API/releases/tags/$TAG" 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])' 2>/dev/null || true)"
body="$(json <"$NOTES")"
payload="{\"tag_name\":\"$TAG\",\"name\":\"$NAME\",\"body\":$body,\"prerelease\":$PRE,\"draft\":false}"
if [[ -z "$id" ]]; then
  echo "==> creating Forgejo release $TAG"
  id="$(curl -fsS "${AUTH[@]}" -H 'Content-Type: application/json' -d "$payload" "$API/releases" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
else
  echo "==> updating Forgejo release $TAG ($id)"
  curl -fsS -X PATCH "${AUTH[@]}" -H 'Content-Type: application/json' -d "$payload" "$API/releases/$id" >/dev/null
fi

existing="$(curl -fsS "${AUTH[@]}" "$API/releases/$id/assets")"
shopt -s nullglob
for f in "$DIR"/*; do
  [[ -f "$f" ]] || continue
  name="$(basename "$f")"
  old="$(printf '%s' "$existing" | python3 -c 'import json,sys; n=sys.argv[1]; print(next((str(a["id"]) for a in json.load(sys.stdin) if a["name"]==n), ""))' "$name")"
  if [[ -n "$old" ]]; then curl -fsS -X DELETE "${AUTH[@]}" "$API/releases/$id/assets/$old" >/dev/null; fi
  echo "    $name"
  curl -fsS "${AUTH[@]}" -F "attachment=@$f" "$API/releases/$id/assets?name=$name" >/dev/null
done
echo "==> ${FORGEJO_URL%/}/$FORGEJO_REPO/releases/tag/$TAG"
