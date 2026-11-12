#!/usr/bin/env python3
"""Builds dist/<id>_<version>_arm.ipk from the cross-built binary.

Layout as webOS dev installs expect (an `ar` archive of debian-binary,
control.tar.gz and data.tar.gz holding usr/palm/applications/<id>/), with
SDL-webOS in lib/ next to the binary (it is linked with rpath $ORIGIN/lib), and
usr/palm/packages/<id>/packageinfo.json as ares-package writes it.

Reproducible: every timestamp is SOURCE_DATE_EPOCH, else the last commit's time,
the gzip headers carry no time or name, members are sorted and owned by root, so
the same tree packages to the same bytes (CI packages twice and compares).
"""
import gzip, io, json, os, struct, subprocess, tarfile, zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APP_ID = "com.thoxy.outpick"
# The version is Cargo.toml's (tools/release.sh sets it; the release workflow checks the tag).
VERSION = next(l.split('"')[1] for l in (ROOT / "Cargo.toml").read_text().splitlines() if l.startswith("version = "))
BIN = ROOT / "target/armv7-unknown-linux-gnueabi/release/outpick"
SDL = ROOT / ".build/sdl-webos/lib/libSDL2-2.0.so.0"

APPINFO = {
    "id": APP_ID,
    "version": VERSION,
    "vendor": "thoxy",
    "type": "native",
    "main": "outpick",
    "title": "Outpick",
    "appDescription": "A modern, customizable input picker for rooted LG webOS TVs: HDMI inputs with device names and live previews, CEC options, USB and media servers.",
    "icon": "icon.png",
    "largeIcon": "largeIcon.png",
    "iconColor": "#1A1A1C",
    "defaultWindowType": "overlay",
    "resolution": "1920x1080",
    "transparent": True,
    "noSplashOnLaunch": True,
    "spinnerOnLaunch": False,
    "handlesRelaunch": False,
    "disableBackHistoryAPI": True,
    "removable": True,
    "requiredACG": ["application.launcher", "application.query", "lunabus.query"],
}


def source_date_epoch() -> int:
    """SOURCE_DATE_EPOCH, else the last commit's time, else 0 (not a git checkout)."""
    if os.environ.get("SOURCE_DATE_EPOCH"):
        return int(os.environ["SOURCE_DATE_EPOCH"])
    try:
        out = subprocess.run(["git", "-C", str(ROOT), "log", "-1", "--format=%ct"],
                             capture_output=True, text=True, check=True).stdout.strip()
        return int(out) if out else 0
    except (OSError, subprocess.CalledProcessError, ValueError):
        return 0


def png(size: int) -> bytes:
    """A rounded violet tile with three white bars (an input list), no PIL needed."""
    bg, fg, accent = (26, 26, 28), (236, 236, 240), (139, 124, 246)
    r = size * 0.22
    rows = []
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            # rounded-rect mask
            cx = min(max(x, r), size - 1 - r)
            cy = min(max(y, r), size - 1 - r)
            inside = (x - cx) ** 2 + (y - cy) ** 2 <= r * r
            c, a = bg, 255 if inside else 0
            fx, fy = x / size, y / size
            for i, by in enumerate((0.30, 0.47, 0.64)):
                if 0.24 <= fx <= 0.76 and by <= fy <= by + 0.09:
                    c = accent if i == 0 else fg
            row += bytes((*c, a))
        rows.append(bytes(row))
    raw = b"".join(rows)

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def tar_gz(entries, mtime: int) -> bytes:
    """A gzipped tar of (name, data, mode), its parent directories added, sorted, root-owned."""
    members = {}
    for name, data, mode in entries:
        parts = Path(name).parts
        for i in range(1, len(parts)):
            members.setdefault("./" + "/".join(parts[:i]), (None, 0o755))
        members["./" + name] = (data, mode)
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.GNU_FORMAT) as tar:
        for name in sorted(members):
            data, mode = members[name]
            ti = tarfile.TarInfo(name)
            ti.mode, ti.mtime, ti.uid, ti.gid, ti.uname, ti.gname = mode, mtime, 0, 0, "root", "root"
            if data is None:
                ti.type = tarfile.DIRTYPE
                tar.addfile(ti)
            else:
                ti.size = len(data)
                tar.addfile(ti, io.BytesIO(data))
    out = io.BytesIO()
    # mtime=0 and no file name: the gzip header is the same on every run.
    with gzip.GzipFile(filename="", fileobj=out, mode="wb", compresslevel=9, mtime=0) as gz:
        gz.write(raw.getvalue())
    return out.getvalue()


def ar(members, mtime: int) -> bytes:
    out = b"!<arch>\n"
    for name, data in members:
        hdr = f"{name:<16}{mtime:<12}{0:<6}{0:<6}{0o100644:<8o}{len(data):<10}`\n".encode()
        out += hdr + data + (b"\n" if len(data) % 2 else b"")
    return out


def main():
    if not BIN.exists():
        raise SystemExit(f"missing {BIN}: run tools/build-webos.sh first")
    mtime = source_date_epoch()
    base = f"usr/palm/applications/{APP_ID}/"
    # What ares-package adds: the installer reads it to know the package's apps.
    packageinfo = {"id": APP_ID, "version": VERSION, "app": APP_ID}
    data_entries = [
        (base + "appinfo.json", json.dumps(APPINFO, indent=2).encode(), 0o644),
        # The app icon, assets/app-icon/outpick.svg (HDMI port flowing into a screen).
        (base + "icon.png", (ROOT / "assets/app-icon/outpick-80.png").read_bytes(), 0o644),
        (base + "largeIcon.png", (ROOT / "assets/app-icon/outpick-130.png").read_bytes(), 0o644),
        (base + "outpick", BIN.read_bytes(), 0o755),
        (base + "lib/libSDL2-2.0.so.0", SDL.read_bytes(), 0o755),
        (f"usr/palm/packages/{APP_ID}/packageinfo.json", json.dumps(packageinfo, indent=2).encode(), 0o644),
    ]
    data = tar_gz(data_entries, mtime)
    installed = sum(len(d) for _, d, _ in data_entries) // 1024
    control = (f"Package: {APP_ID}\nVersion: {VERSION}\nSection: misc\nPriority: optional\n"
               f"Architecture: arm\nInstalled-Size: {installed}\nMaintainer: thoxy\n"
               f"Description: Outpick - a customizable source picker\nwebOS-Package-Format-Version: 2\n"
               f"webOS-Packager-Version: 1.0.0\n").encode()
    ctrl = tar_gz([("control", control, 0o644)], mtime)
    out = ROOT / "dist" / f"{APP_ID}_{VERSION}_arm.ipk"
    out.parent.mkdir(exist_ok=True)
    out.write_bytes(ar([("debian-binary", b"2.0\n"), ("control.tar.gz", ctrl), ("data.tar.gz", data)], mtime))
    # deploy.sh installs the path on the first line: keep it alone there.
    print(out)
    print(f"  {out.stat().st_size} bytes, installed {installed} KiB, mtime {mtime}")


if __name__ == "__main__":
    main()
