# Inputs (lg-c1-source)

A modern, customizable native input picker for rooted LG webOS TVs (tested on a C1, webOS 6,
and a QNED86, webOS 25), written in Rust with egui. It replaces LG's Source menu as an overlay.

- HDMI inputs named after the device plugged in (HDMI-CEC), with live previews of the source being
  watched and the last picture of the others; CEC options per input (Down, or Right in a list).
- USB storage, DLNA media servers, Live TV and a Home entry (follows Luma or Glasshouse Home).
- Row, grid, list or docked sidebar; nine positions; eight themes and accent colours; French and English.
- Optional takeover of the Source key (and the Multikey button: short press opens the picker,
  a long press keeps LG's accessibility), reversible from Customize.

## Build

```sh
cargo run                                   # desktop preview with mock inputs
cargo run -- --screenshot shot.png --keys left,left,left,ok --size 1920x1080
node tools/setup-webos-sdk.mjs              # once: pinned webOS NDK (~300 MB) in .build/
tools/build-webos.sh                        # TV binary: armv7 soft-float, Thumb-2, ~2.1 MB
python3 tools/package.py                    # dist/com.thoxy.sourcepicker_<version>_arm.ipk
tools/deploy.sh 192.168.1.85                # build, install on a TV over ssh, launch
```

The toolchain is the nightly pinned in `rust-toolchain.toml` (the TV build uses `-Z build-std`
and `-C panic=immediate-abort`).

## On the TV (root)

```sh
source-picker --takeover on|off|status        # Source key -> this picker
source-picker --capture-access on|off|status  # webOS 6: allow the video previews
```

Settings live in `/media/developer/lg-c1-source/config.json`.

## Releases (Forgejo)

`tools/release.sh X.Y.Z` sets the version, prepends the notes since the last tag to
`CHANGELOG.md`, commits and tags; then `git push && git push origin vX.Y.Z`.

The tag runs `.forgejo/workflows/release.yml`. It builds and packages the app, generates the notes
from the commits (`tools/release/changelog.py`), and publishes a Forgejo release with the `.ipk`,
`repo.json`, the manifest, the description, the icon and `SHA256SUMS`. It then updates the
Homebrew Channel feed at a fixed URL:

```
https://<forgejo>/api/packages/<owner>/generic/lg-c1-source/latest/repo.json
```

Add that URL once in the Homebrew Channel's repositories; every release then shows up as an update.

Repository settings: variable `FORGEJO_RUNNER` (runner label, default `docker`); secret
`FORGEJO_TOKEN` with `write:repository` and `write:package` if the job's own token cannot publish
packages.
