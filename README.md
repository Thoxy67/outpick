<p align="center">
  <img src="assets/app-icon/outpick.svg" alt="Outpick" width="128">
</p>

<h1 align="center">Outpick</h1>

<p align="center">A fast, native input picker for rooted LG webOS TVs, built with Rust and egui.</p>

<p align="center">
  <a href="https://git.thoxy.xyz/thoxy/outpick">Repository</a> ·
  <a href="https://git.thoxy.xyz/thoxy/outpick/releases">Releases</a> ·
  <a href="https://git.thoxy.xyz/thoxy/outpick/issues">Issues</a> ·
  <a href="https://git.thoxy.xyz/thoxy/outpick/actions">CI</a>
</p>

<p align="center"><b>Latest release: <a href="https://git.thoxy.xyz/thoxy/outpick/releases/tag/v0.1.0">v0.1.0</a></b>
(2026-11-12), installable from the Homebrew Channel.</p>

<p align="center">
  <img src="docs/picker.png" alt="The picker over the TV picture" width="49%">
  <img src="docs/sidebar.png" alt="The docked sidebar layout" width="49%">
</p>

## Highlights

- **A real overlay**: it floats over the picture or the app in front instead of replacing it,
  and wakes the TV from the screen saver. It starts on the inputs it saw last time, then
  refreshes them.
- **Inputs that say what they are**: HDMI inputs named after the device plugged in (HDMI-CEC
  name and maker, e.g. *Denon AVC-S670H*), with **live previews** of the source being watched and
  the last picture of the others. Each device gets its own icon, from HDMI-CEC first: PlayStation,
  Xbox, Nintendo Switch, Android TV boxes, PCs, sound systems, streaming boxes.
- **CEC options per input**: power the device on or off, open its home screen, menu or info
  (only on inputs whose device answers HDMI-CEC).
- **Everything in one list**: Live TV, USB storage, DLNA media servers, and a Home entry that
  follows the home in use (LG, [Luma](https://github.com/ecklf/lg-luma) or Glasshouse Home).
- **Yours to shape**: row, grid, list or docked sidebar; nine positions; eight themes and eleven
  accent colours; sizes, opacity, corners, icons; rename, hide and reorder sources; French and
  English.
- **An on-screen clock** in seven styles, placed anywhere on the screen.
- **The remote's Source key opens it** (and, on remotes without one, a short press on Multikey,
  a long press staying LG's accessibility), reversible from its own settings.
- **Quick switch**: pressing Source again while the picker is open moves to the next connected
  input (unplugged ports, USB, media servers and Home are skipped). The tile shows a countdown and
  the TV switches when it ends (1 to 10 s, 3 by default); OK switches at once, an arrow cancels.
- **Small**: a 1.9 MB binary and a 1.7 MB `.ipk` (Thumb-2, size-optimised standard library,
  fonts and icons trimmed).
- **Clean to remove**: a boot hook puts LG's keys back and deletes what the app left behind once
  it is uninstalled; your settings are kept for a reinstall.

## Tested TVs

| TV | Model | webOS | Firmware | Outpick | Notes |
|---|---|---|---|---|---|
| LG OLED C1 (2021) | `OLED55C16LA` | webOS 6 (6.5.3) | 03.53.45 | 0.1.0 | The **Input** key opens Outpick. Previews need "Allow video capture" (Customize → Previews). Luma as Home (executable method). |
| LG QNED86 (2025) | `55QNED86A6A` | webOS 25 (10.3.2) | 33.31.75 | 0.1.0 | Remote without an Input key: a short press on **Multikey (···)** opens Outpick, a long press keeps LG's accessibility menu. Live previews work without any option. Luma as Home (manifest method). |

Both are rooted with the [Homebrew Channel](https://github.com/webosbrew/webos-homebrew-channel).
The binary is armv7 soft-float; the key takeover uses the compositor's JavaScript key filters, so
webOS 11 (native filters) is not supported yet.

## Install

1. In the Homebrew Channel, open **Settings → Repositories** and add:

   ```
   https://git.thoxy.xyz/thoxy/outpick/raw/branch/hbc/repo.json
   ```

2. Install **Outpick** from the list, then open it once from the TV's apps.
3. In **Customize → Remote**, turn on **Source key → this picker** (and **Multikey (···) → this
   picker** on remotes without an Input key). From then on the remote's key opens Outpick.
4. On webOS 6, turn on **Customize → Previews → Allow video capture** for the source previews.

Each new release then shows up as an update in the Homebrew Channel. The `.ipk` of every release
is also on the [releases page](https://git.thoxy.xyz/thoxy/outpick/releases) for a manual install
(`ares-install`, webOS Dev Manager, or `luna-send … appInstallService/dev/install` as root).

The picker itself runs without root. Root (a rooted TV with the Homebrew Channel) is needed for the
key takeover, the video capture access on webOS 6 and the settings-folder permissions; the feed
marks root as optional.

**Coming from "Inputs"** (`com.thoxy.sourcepicker`, the name before Outpick): install Outpick and
open it once. It takes over the Source key filter, the capture access and the settings
(`/media/developer/lg-c1-source/config.json` is read when Outpick has none yet); then uninstall
the old app.

**Before uninstalling**, give the keys back: turn them off in Customize → Remote, or run
`outpick --uninstall` as root. If the app is removed without that, a boot hook puts LG's
Source key back at the next start.

## Build

Needs the pinned nightly in `rust-toolchain.toml` and Node (for the SDK scripts):

```sh
cargo run                          # desktop preview with mock inputs
node tools/setup-webos-sdk.mjs     # once: the webOS NDK (~300 MB) and SDL-webOS into .build/
tools/build-webos.sh               # the TV binary
python3 tools/package.py           # dist/com.thoxy.outpick_<version>_arm.ipk
tools/deploy.sh 192.168.1.85       # build, install over ssh and launch on a TV
```

The preview can script itself, for checking the UI without a TV:

```sh
cargo run -- --screenshot shot.png --keys left,left,left,ok --size 1920x1080
```

## Use

- Press **Input** (or **Multikey**), move with the arrows, **OK** to switch, **Back** to close.
- Press **Input** again to step through the connected inputs: the TV switches by itself when the
  countdown on the tile ends.
- On an HDMI input with an HDMI-CEC device, **Down** (**Right** in a list) opens its options:
  power, the device's home, menu or info, rename, hide.
- **Customize** is the last tile; if you hide it, the arrow that has nothing to do in the layout
  (Up in a row, Left in a list) opens it.
- Hold **Back** to close from any screen. Everywhere, **OK** confirms and **Back** cancels.

As root on the TV:

```sh
outpick --takeover on|off|status        # the Source key opens this picker, or LG's again
outpick --capture-access on|off|status  # webOS 6: allow the video previews
outpick --uninstall                     # undo both, remove the boot hook and the previews
```

Either option installs a small boot hook, `/var/lib/webosbrew/init.d/70-outpick` (a copy,
not a link into the app). At each boot it puts the takeover back if the TV lost it, and once the
app has been uninstalled it cleans up after it by itself: our key filter out of the compositor's
chain (LG's filters left as they are), the app off the capture whitelist (other entries kept),
`/var/lib/outpick` and the previews deleted, then the hook itself. Only the settings,
`config.json`, are kept for a reinstall. `--uninstall` does the same at once, before removing the app.

## Configure

Everything is in **Customize**: its first page lists seven sections (Appearance, Layout, Inputs,
Previews, Clock, Remote, General), each with a summary of its settings; OK opens one, Back returns.
Options that cannot apply are hidden (panel opacity without the panel, the clock's details while it
is off). The Remote section has one switch per key (Source, Multikey) and the quick-switch delay;
"Reset settings" keeps the inputs and the remote keys. Inputs can be renamed, hidden, reordered and
given another icon (Customize → Inputs → Manage inputs).

The settings are kept in `/media/developer/outpick/config.json`, which survives reinstalls. Every
field is optional; a value that does not fit is ignored alone (the rest of the file is kept), and a
file that is not JSON is set aside as `config.json.bad`.

## Known limitations

- webOS 11 uses native key filters, so the key takeover is not supported there (untested otherwise).
- On the QNED (webOS 25) the Home tile names LG's home although Luma is the Home there.
- Device icons for PlayStation, Xbox, Nintendo Switch and PCs follow their HDMI-CEC names and
  makers, but no such device was plugged into the tested TVs.

## Releases

`tools/release.sh X.Y.Z` sets the version, writes `CHANGELOG.md` from the commits since the last
tag, commits and tags; `git push && git push origin vX.Y.Z` then runs
[`.forgejo/workflows/release.yml`](.forgejo/workflows/release.yml). It builds, packages, writes the
release notes and the Homebrew Channel `repo.json` automatically, publishes the Forgejo release,
and commits the feed to the orphan branch `hbc` (the repository URL above; its links point at the
tagged release). [`.forgejo/workflows/ci.yml`](.forgejo/workflows/ci.yml) runs on every push to
`main` ([runs](https://git.thoxy.xyz/thoxy/outpick/actions)): `cargo check`, clippy with warnings
as errors, the tests (on captured payloads of both TVs), the TV build with a 2.3 MB size limit,
and packaging twice to check the `.ipk` is reproducible (timestamps from `SOURCE_DATE_EPOCH`, else
the last commit). The `.ipk` of each run is kept as an artifact.

Repository settings: variable `FORGEJO_RUNNER` (runner label, default `docker`); secret
`FORGEJO_TOKEN` (`write:repository`) only if the job's own token cannot publish releases. The
job's token pushes the `hbc` branch.

## License

MIT © 2026 Thoxy. See [LICENSE](LICENSE); third-party notices are in [NOTICE](NOTICE). The webOS
Luna bridge, key decoding and libc shim are ported from [Luma](https://github.com/ecklf/lg-luma)
(MIT). The theme tokens follow PhotoCraft's (MIT OR Apache-2.0). Inter is under the SIL Open Font
License 1.1, the Lucide icons under ISC and the Simple Icons brand glyphs under CC0 (the logos
are trademarks of their owners).
