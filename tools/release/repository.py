#!/usr/bin/env python3
"""Homebrew Channel repository for a release (after Luma's tools/repository.mjs).

    tools/release/repository.py TAG DIR

Writes into DIR, next to the release's .ipk:
  repo.json                         the Homebrew Channel feed (one package)
  com.thoxy.sourcepicker.manifest.json
  description.html                  the long description the feed links to
  largeIcon.png                     the icon the feed shows

Every URL points at this release's assets on Forgejo
(<server>/<owner>/<repo>/releases/download/<TAG>/<file>), so a feed always installs the
build it was written with. The release workflow also publishes repo.json under a fixed URL
(the generic package "latest", tools/release/publish-repo.sh) for the TV's repository list.

Env: FORGEJO_URL (https://git.example.org), FORGEJO_REPO (owner/name); in Actions these come
from github.server_url and github.repository.
"""

import hashlib
import json
import os
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from package import APP_ID, APPINFO, VERSION  # noqa: E402


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    tag, out = sys.argv[1], Path(sys.argv[2])
    if tag != f"v{VERSION}":
        print(f"tag {tag} does not match the packaged version v{VERSION}", file=sys.stderr)
        return 1
    server = os.environ.get("FORGEJO_URL", "").rstrip("/")
    repo = os.environ.get("FORGEJO_REPO", "")
    if not server or not re.fullmatch(r"[\w.-]+/[\w.-]+", repo):
        print("FORGEJO_URL and FORGEJO_REPO (owner/name) are required", file=sys.stderr)
        return 1
    source_url = f"{server}/{repo}"
    release_url = f"{source_url}/releases/download/{tag}"
    ipk_name = f"{APP_ID}_{VERSION}_arm.ipk"
    ipk = (out / ipk_name).read_bytes()

    shutil.copyfile(ROOT / "assets/app-icons/app-icon-130.png", out / "largeIcon.png")
    shutil.copyfile(ROOT / "assets/homebrew-description.html", out / "description.html")
    icon_uri = f"{release_url}/largeIcon.png"
    manifest = {
        "id": APP_ID,
        "version": VERSION,
        "type": APPINFO["type"],
        "title": APPINFO["title"],
        "appDescription": APPINFO["appDescription"],
        "iconUri": icon_uri,
        "sourceUrl": source_url,
        # The Source key takeover, the capture access and the settings folder need root.
        "rootRequired": True,
        "ipkUrl": f"{release_url}/{ipk_name}",
        "ipkHash": {"sha256": hashlib.sha256(ipk).hexdigest()},
        "ipkSize": len(ipk),
    }
    feed = {
        "paging": {"page": 1, "count": 1, "maxPage": 1, "itemsTotal": 1},
        "packages": [
            {
                "id": APP_ID,
                "title": APPINFO["title"],
                "shortDescription": APPINFO["appDescription"],
                "fullDescriptionUrl": f"{release_url}/description.html",
                "iconUri": icon_uri,
                "manifest": manifest,
            }
        ],
    }
    (out / "repo.json").write_text(json.dumps(feed, indent=2) + "\n")
    (out / f"{APP_ID}.manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Homebrew feed: {out / 'repo.json'} ({tag}, {len(ipk)} byte ipk)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
