#!/usr/bin/env python3
"""Builds dist/<id>_<version>_arm.ipk from the cross-built binary.

Layout as webOS dev installs expect (an `ar` archive of debian-binary,
control.tar.gz and data.tar.gz holding usr/palm/applications/<id>/), with
SDL-webOS in lib/ next to the binary (it is linked with rpath $ORIGIN/lib).
"""
import io, json, os, struct, tarfile, time, zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APP_ID = "com.thoxy.sourcepicker"
# The version is Cargo.toml's (tools/release.sh sets it; the release workflow checks the tag).
VERSION = next(l.split('"')[1] for l in (ROOT / "Cargo.toml").read_text().splitlines() if l.startswith("version = "))
BIN = ROOT / "target/armv7-unknown-linux-gnueabi/release/source-picker"
SDL = ROOT / ".build/sdl-webos/lib/libSDL2-2.0.so.0"

APPINFO = {
    "id": APP_ID,
    "version": VERSION,
    "vendor": "thoxy",
    "type": "native",
    "main": "source-picker",
    "title": "Inputs",
    "appDescription": "A modern, customizable input picker for rooted LG webOS TVs: HDMI inputs with device names and live previews, CEC options, USB and media servers.",
    "icon": "icon.png",
    "largeIcon": "largeIcon.png",
    "iconColor": "#1A1A1C",
    "defaultWindowType": "overlay",
    "transparent": True,
    "noSplashOnLaunch": True,
    "spinnerOnLaunch": False,
    "disableBackHistoryAPI": True,
    "removable": True,
    "requiredACG": ["application.launcher", "application.query", "lunabus.query"],
}


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


def tar_gz(entries) -> bytes:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz", format=tarfile.GNU_FORMAT) as tar:
        dirs = set()
        for name, data, mode in entries:
            parts = Path(name).parts
            for i in range(1, len(parts)):
                d = "./" + "/".join(parts[:i])
                if d not in dirs:
                    dirs.add(d)
                    ti = tarfile.TarInfo(d)
                    ti.type, ti.mode, ti.mtime = tarfile.DIRTYPE, 0o755, int(time.time())
                    tar.addfile(ti)
            ti = tarfile.TarInfo("./" + name)
            ti.size, ti.mode, ti.mtime = len(data), mode, int(time.time())
            tar.addfile(ti, io.BytesIO(data))
    return buf.getvalue()


def ar(members) -> bytes:
    out = b"!<arch>\n"
    for name, data in members:
        hdr = f"{name:<16}{int(time.time()):<12}{0:<6}{0:<6}{0o100644:<8o}{len(data):<10}`\n".encode()
        out += hdr + data + (b"\n" if len(data) % 2 else b"")
    return out


def main():
    if not BIN.exists():
        raise SystemExit(f"missing {BIN}: run tools/build-webos.sh first")
    base = f"usr/palm/applications/{APP_ID}/"
    data_entries = [
        (base + "appinfo.json", json.dumps(APPINFO, indent=2).encode(), 0o644),
        # Icon 2 of assets/app-icons (HDMI port flowing into a screen).
        (base + "icon.png", (ROOT / "assets/app-icons/app-icon-80.png").read_bytes(), 0o644),
        (base + "largeIcon.png", (ROOT / "assets/app-icons/app-icon-130.png").read_bytes(), 0o644),
        (base + "source-picker", BIN.read_bytes(), 0o755),
        (base + "lib/libSDL2-2.0.so.0", SDL.read_bytes(), 0o755),
    ]
    data = tar_gz(data_entries)
    installed = sum(len(d) for _, d, _ in data_entries) // 1024
    control = (f"Package: {APP_ID}\nVersion: {VERSION}\nSection: misc\nPriority: optional\n"
               f"Architecture: arm\nInstalled-Size: {installed}\nMaintainer: thoxy\n"
               f"Description: Inputs - a customizable source picker\nwebOS-Package-Format-Version: 2\n"
               f"webOS-Packager-Version: x.y.x\n").encode()
    ctrl = tar_gz([("control", control, 0o644)])
    out = ROOT / "dist" / f"{APP_ID}_{VERSION}_arm.ipk"
    out.parent.mkdir(exist_ok=True)
    out.write_bytes(ar([("debian-binary", b"2.0\n"), ("control.tar.gz", ctrl), ("data.tar.gz", data)]))
    print(f"{out} ({out.stat().st_size // 1024} KiB)")


if __name__ == "__main__":
    main()
