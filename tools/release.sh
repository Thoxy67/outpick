#!/usr/bin/env bash
# Prepares release X.Y.Z: sets the version in Cargo.toml (and Cargo.lock), prepends the notes
# since the last tag to CHANGELOG.md, commits "Release vX.Y.Z" and tags it. Nothing is pushed:
#   git push && git push origin vX.Y.Z
# then starts the Forgejo release workflow (.forgejo/workflows/release.yml).
set -euo pipefail
V="${1:?usage: tools/release.sh X.Y.Z}"
[[ "$V" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "not a version: $V" >&2; exit 1; }
cd "$(dirname "$0")/.."
[[ -z "$(git status --porcelain)" ]] || { echo "commit or stash your changes first" >&2; exit 1; }
git rev-parse -q --verify "refs/tags/v$V" >/dev/null && { echo "tag v$V exists" >&2; exit 1; }

sed -i "0,/^version = \".*\"/s//version = \"$V\"/" Cargo.toml
cargo update -p lg-c1-source --offline >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null

notes="$(tools/release/changelog.py --title "v$V — $(date +%Y-%m-%d)")"
{ printf '%s\n\n' "$notes"; [[ -f CHANGELOG.md ]] && cat CHANGELOG.md; } > CHANGELOG.md.new
mv CHANGELOG.md.new CHANGELOG.md

git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -q -m "Release v$V"
git tag -a "v$V" -m "v$V"
echo "Tagged v$V. Push with: git push && git push origin v$V"
