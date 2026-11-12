#!/usr/bin/env python3
"""Release notes from the commits between two tags (ported from term's scripts/release/changelog.py).

    tools/release/changelog.py [--from TAG] [--to REV] [--title TEXT] [--repo-url URL]

--to defaults to HEAD; --from to the latest v* tag before it (all history when there is
none). Each commit becomes a section: its subject as a heading with a link to the commit, and
the paragraphs of its body as bullets ("Area: details" paragraphs get the area in bold).
Housekeeping commits (release commits, merges) are left out.

The Forgejo release workflow puts the output in the release; tools/release.sh also prepends
it to CHANGELOG.md.
"""

import argparse
import os
import re
import subprocess
import sys

SKIP = re.compile(r"^(PKGBUILD: pkgver|Release v?\d|Merge (branch|pull request|remote)|chore\(release\))", re.I)
AREA = re.compile(r"^([A-Z][\w /&+.-]{0,30}?):\s+(.+)$", re.S)
TRAILER = re.compile(r"^(Co-Authored-By|Signed-off-by|Change-Id|Reviewed-by):", re.I)


def git(*args: str) -> str:
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def try_git(*args: str) -> str | None:
    r = subprocess.run(["git", *args], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else None


def repo_url() -> str:
    """The web URL of the repository: GitHub Actions' own, else the origin remote's."""
    if os.environ.get("GITHUB_SERVER_URL") and os.environ.get("GITHUB_REPOSITORY"):
        return f"{os.environ['GITHUB_SERVER_URL']}/{os.environ['GITHUB_REPOSITORY']}"
    url = try_git("remote", "get-url", "origin") or ""
    url = re.sub(r"\.git$", "", url)
    # ssh://git@host:222/owner/repo, git@host:owner/repo, https://host/owner/repo
    m = re.match(r"^ssh://(?:[^@]+@)?([^/:]+)(?::\d+)?/(.+)$", url) or re.match(r"^(?:[^@]+@)?([^/:]+):(?!//)(.+)$", url)
    if m:
        return f"https://{m.group(1)}/{m.group(2)}"
    return url if url.startswith("http") else ""


def previous_tag(to: str) -> str | None:
    # The tag `to` itself (a release commit) is not its own predecessor.
    return try_git("describe", "--tags", "--abbrev=0", "--match", "v[0-9]*", f"{to}^")


def bullets(body: str) -> list[str]:
    out = []
    for para in re.split(r"\n\s*\n", body.strip()):
        lines = [l.strip() for l in para.strip().splitlines() if l.strip() and not TRAILER.match(l.strip())]
        if not lines:
            continue
        # A paragraph that is already a list keeps its items.
        if all(re.match(r"^[-*] ", l) for l in lines):
            out.extend(l[2:].strip() for l in lines)
            continue
        text = " ".join(re.sub(r"^[-*] ", "", l) for l in lines)
        m = AREA.match(text)
        out.append(f"**{m.group(1)}:** {m.group(2)}" if m else text)
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--from", dest="frm")
    ap.add_argument("--to", default="HEAD")
    ap.add_argument("--title", default="What's changed")
    ap.add_argument("--repo-url", default=None)
    a = ap.parse_args()

    frm = a.frm or previous_tag(a.to)
    url = (a.repo_url if a.repo_url is not None else repo_url()).rstrip("/")
    rng = f"{frm}..{a.to}" if frm else a.to
    raw = git("log", "--no-merges", "--format=%H%x1f%h%x1f%an%x1f%s%x1f%b%x1e", rng)

    commits = []
    authors: list[str] = []
    for rec in raw.split("\x1e"):
        rec = rec.strip("\n")
        if not rec:
            continue
        full, short, author, subject, body = (rec.split("\x1f") + [""] * 5)[:5]
        if SKIP.match(subject):
            continue
        commits.append((full, short, subject.strip(), body))
        if author not in authors:
            authors.append(author)

    out = [f"## {a.title}", ""]
    if not commits:
        out += ["Maintenance release (no user-facing commits).", ""]
    for full, short, subject, body in commits:
        ref = f"[`{short}`]({url}/commit/{full})" if url else f"`{short}`"
        out += [f"### {subject} · {ref}", ""]
        items = bullets(body)
        out += [f"- {b}" for b in items]
        if items:
            out.append("")
    if len(authors) > 1:
        out += [f"**Contributors:** {', '.join(authors)}", ""]
    if frm and url:
        to_name = try_git("describe", "--tags", "--exact-match", a.to) or a.to
        out += [f"**Full changelog:** [{frm}...{to_name}]({url}/compare/{frm}...{to_name})", ""]
    sys.stdout.write("\n".join(out).rstrip() + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
