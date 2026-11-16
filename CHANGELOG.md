## v0.2.1 — 2026-11-16

### Applications tile, updates from the picker, receiver volume, Liquid Glass, clearer Customize, steadier remote and previews · [`7c2c546`](https://git.thoxy.xyz/thoxy/outpick/commit/7c2c546976b910a87670b4092890d35dc280b922)

- **Applications:** an optional tile (Customize → Inputs, off by default) opens the TV's installed apps as a grid or a list, read from the TV (listLaunchPoints) each time it opens, one row per launch point with its params. Icons are the apps' own PNGs, decoded with the png crate (now in the TV build) and shrunk; the jailed app reads those it can and root copies the others into the shared folder (only files that are PNGs, into a root-owned folder, refreshed and pruned at each opening). Titles the font cannot draw fall back to the app id. Checked on the C1 and the QNED86.
- **Updates:** on opening, once a day at most, the feed of the hbc branch is read with the TV's curl (https only, certificates verified, time and size bounded; as the jailed app, through root only when it cannot reach the network). An offer is kept only if it is newer, its sha256 is 64 hex digits and its package is on this repository's releases. Customize → General has the switch and an Update row; the Update screen shows the notes of every version since the installed one (CHANGELOG.md) and OK hands the install to the Homebrew Channel, which downloads, checks the hash and installs. The first start of a new version has root refresh the key filter and boot hook. The check ran on the QNED86; the install itself is untested until a newer version is published.
- **Receiver:** volume and mute in the options (GetVolumeLevel / GetMuteStatus in their own request, MV? / MU? over telnet), changed with relative steps only (MVUP / MVDOWN twice for 1.0, MUON / MUOFF). The options open on the receiver's input in use.
- **Options:** sections of buttons laid out like the picker (the receiver's inputs, volume, sound mode chips, the device's keys, this input); the arrows follow the buttons on screen and Right at the end of a line goes on to the next.
- **Remote:** a held arrow stops at the end of a list and never opens or leaves a screen; a long Back closes while still held; the Magic Remote's click waits for the release like OK, so a long OK no longer validates first. Where a long OK means nothing, OK acts on the press. The long press time and the gap that makes an arrow a held one are set in Customize → Remote. Left on Customize's root goes back to the picker.
- **Customize:** each section's page is titled with its name and icon, a line under the rows says what the selected one does (also in the Sources editor, with column titles, and in the receiver list, which says what to check when none is found), and the Appearance and Layout pages show a miniature of the picker.
- **Themes:** Liquid Glass, translucent panels with one rim following each shape's corners and a white selection ring. The fade at a scrolling edge is no longer drawn over a see-through panel.
- **Previews:** a picture is filed under an input only if that input stayed in front from before the capture until after it, and remembered only once it has been stable (0.8 s for the input in front at opening, 2 s after a switch) and never from a frame taken after a launch was sent; the capture file is removed before each request and read only when complete and fresh; black frames replace nothing; a tile whose preview went away drops its texture. Proven by tests of the rules; no input was playing on the TVs when it was tried.
- **Keys:** the jailed app no longer reports the Source key as broken when it merely cannot see the filter's folder (the QNED86's jail).
- **README:** v0.2.1, the new features, a 2.2 MB binary and a 1.9 MB ipk.
- **Tests:** 81.

### Dependencies: resvg 0.50, lockfile to the latest compatible versions; glow stays 0.17 for egui_glow 0.37 · [`2720ec2`](https://git.thoxy.xyz/thoxy/outpick/commit/2720ec20340544afacb5696a81440e4943f93fa9)

### README: v0.2.0 is the latest release; the C1 tested with it and its Denon · [`2f884e9`](https://git.thoxy.xyz/thoxy/outpick/commit/2f884e929193e9221cf6aef9871eb5c5c496fb4a)

**Full changelog:** [v0.2.0...HEAD](https://git.thoxy.xyz/thoxy/outpick/compare/v0.2.0...HEAD)

## v0.2.0 — 2026-11-13

### README: network receivers, long OK for the options, the Network receiver section · [`95f3782`](https://git.thoxy.xyz/thoxy/outpick/commit/95f37829e7a4aaacb5b8c9673e63c5c1912d2237)

### Denon / Marantz receivers over the network; options on a long OK in every layout · [`f1d0bf2`](https://git.thoxy.xyz/thoxy/outpick/commit/f1d0bf2168fc02faee349106975c15f12c3e85c2)

- **Receiver:** the Denon or Marantz behind an HDMI input is found on the LAN (SSDP, the addresses of the receivers the TV already knows as media servers, and a sweep of port 8080, which is what answers inside the app jail). With several, the one whose model matches the input's HDMI-CEC name is taken, then the one whose power matches CEC, then the last one used; two identical receivers both on are left for the user to pick.
- **Options:** the receiver's input lists its inputs under the names set on it, video ones and TV Audio (which keeps the TV's picture), its sound mode changed with Left/Right (Movie, Music, Game, Direct, Pure Direct, Stereo, Auto, with the receiver's own name for the mode), its setup menu driven from the TV remote (arrows, OK, Back; hold Back to close), and, with several receivers found, which one is this input's. Choosing an input shows the receiver's HDMI input and turns the receiver on first if needed. The tile and the header show the input the receiver is on.
- **Customize:** a Network receiver section turns it on or off, picks the receiver (Automatic, one of those found, or an IP address typed in) and the protocol: Automatic (HTTP, else telnet), HTTP or telnet. HTTP is the default as any number of clients share it, while most receivers take a single telnet client (often Home Assistant). The last receiver and its inputs are kept in avr.json next to the settings, so they show at once.
- **Protocol:** AppCommand.xml takes at most 5 commands per request (more get <error>1</error>), so state and input names are two requests; commands passed through formiPhoneAppDirect.xml have their spaces encoded (MNMEN%20ON; a raw space gets HTTP 400). Telnet exchanges are short connections; a list cut by the deadline is treated as unknown.
- **Worker:** searches only where a receiver is likely (a Denon/Marantz input, one known on the network or used before, or a search asked for), backs off when none answers and does not retry on its own when the choice is ambiguous. Settings sent in a burst are coalesced, a protocol change keeps following the same receiver, a receiver change drops everything of the old one, and a receiver gone for five polls is searched for again (DHCP). Selecting an input gives up as soon as the receiver does not answer, so the app never lingers on exit.
- **Keys:** a long OK (600 ms) opens any tile's options in every layout (row, grid, list, sidebar), which also fixes the grid where Down only worked from the last row; Down / Right still open them where they did. OK is sent on release, and the options keep their selection on the same action when rows come and go.
- **Text fields:** the LG keyboard's Enter validates at once, and closing the keyboard with Back leaves the field instead of raising it again; an invalid address says so. /tmp/outpick.keys turns on a key event log in /tmp/outpick-keys.log for debugging remotes.
- **Tests:** 46, with real answers of an AVC-S660H and AVC-S670H (MAC addresses replaced): device info, both status answers, telnet answers, sound mode groups, the receiver choice, addresses, chunked bodies, the receivers among the TV's media servers. Audited by Claude Fable 5.2.

### README: v0.1.0 release, install steps, use, known limitations, CI checks · [`275f2c0`](https://git.thoxy.xyz/thoxy/outpick/commit/275f2c0a933b7a2d8d9ae83f2fd3548035da5e79)

### README screenshots: the current interface (row and sidebar) · [`1775e13`](https://git.thoxy.xyz/thoxy/outpick/commit/1775e13c171f48839bfd7db6a193bc4c7c593df5)

### Icons: keep only the Outpick app icon (assets/app-icon), interface glyphs in assets/ui-icons · [`c81254d`](https://git.thoxy.xyz/thoxy/outpick/commit/c81254d3688400329b287bfe92296ddda09031ff)

**Full changelog:** [v0.1.0...HEAD](https://git.thoxy.xyz/thoxy/outpick/compare/v0.1.0...HEAD)

## v0.1.0 — 2026-11-12

### Pointer hovers only after moving 12 pt from where it was first seen; a long Back (600 ms) closes from anywhere · [`aabfa69`](https://git.thoxy.xyz/thoxy/outpick/commit/aabfa694403423d25b416d8adddaae8ed1318ef6)

### Picker: empty/error state with a way to Customize; the arrow that leads nowhere opens Customize (open_customize helper) · [`3532935`](https://git.thoxy.xyz/thoxy/outpick/commit/35329353fad5322414bafa9b8a9144317c709953)

### Move LG's JSON parsing out of the TV-only code, tested on real payloads · [`2f81f05`](https://git.thoxy.xyz/thoxy/outpick/commit/2f81f05f8d317b75676ae2aaf51fb489f57080b9)

- parse_inputs, parse_storage and foreground_of move unchanged from src/webos/tv.rs to the ungated src/parse.rs, so they build and are tested on the host. foreground_of takes the app id to skip and now prefers the _WEBOS_WINDOW_TYPE_CARD entry (an overlay like the screen saver can be listed first), else the first that is not ours.
- Fixtures, read-only captures: getAllInputStatus of the C1 and the QNED, the C1's listDevices (LAN addresses replaced with 192.0.2.10) and the QNED's foreground app info.

### CI: check, clippy -D warnings, TV binary size limit, .ipk artifact · [`27d79d6`](https://git.thoxy.xyz/thoxy/outpick/commit/27d79d66f3c809da4fbd8be3cc14c54b6b32baef)

- rust-toolchain.toml adds clippy. The headless test build (--no-default-features) allows dead code: nothing runs in it but tests.

### Locales: drop the unused hint.save and rename.help · [`1df27ff`](https://git.thoxy.xyz/thoxy/outpick/commit/1df27ff9df765651eac06648e95ff2a579bcaa6d)

### Tests: device names, brand hints, BMP decoding, locale placeholders · [`10af1de`](https://git.thoxy.xyz/thoxy/outpick/commit/10af1de9ee6566565ff6716ecf80d6baa09a0568)

### widgets: cycle_num for numeric rows, tests for the pure helpers · [`2b82f03`](https://git.thoxy.xyz/thoxy/outpick/commit/2b82f03bf2f1389c0fee3329a9e9d6ffe64535a8)

- cycle_num steps from where a hand-set value falls in the list instead of jumping to the first entry. Not wired yet (the settings rows).

### config: lenient load, kept bad file, synced save, version 1 · [`73afa6d`](https://git.thoxy.xyz/thoxy/outpick/commit/73afa6d76cb7154c4e1815ebedd56542838a90ec)

- A bad value or a JSON typo no longer discards the whole file (which the next save then overwrote): each key falls back to its default alone, clock key by key, order/sources entry by entry. A file that is not a JSON object is copied to config.json.bad before defaults are used.
- Saving fsyncs the temp file before the rename, writes "version": 1 and drops source overrides that change nothing and empty/repeated order ids. clamp() snaps preview_fps to 1, 2 or 4. The show_customize_tile comment no longer promises a Source-key-held shortcut that does not exist.
- **Tests:** both legacy files (C1, QNED), bad enums/types/elements, syntax errors, clamp, pruning, save/load round trip through OUTPICK_CONFIG.

### Clock placing: help bar moves to the bottom under a high clock; held arrows take 3 % steps · [`8aee62d`](https://git.thoxy.xyz/thoxy/outpick/commit/8aee62d9bb7122c4c550ed676c9b83a2ad6f4d7e)

### Customize: two levels (seven sections with summaries), honest remote switches, clearer names · [`1f187b3`](https://git.thoxy.xyz/thoxy/outpick/commit/1f187b332bb3c1b734914a78620ebf6a10da8e08)

- Root page lists Appearance, Layout, Inputs, Previews, Clock, Remote, General with a live summary; OK/Right opens a section, Back/Left returns to it. Rows that cannot apply are hidden. Source key and Multikey are one switch each, setting their own flag and the key filter; Give back to LG is gone. Reset settings keeps inputs, language and remote keys. Size from 80 %, clock size in fifths above 160 %, gauge chevrons dim at their ends, no Yes/No next to switches.

### Without the panel: header and hint labels shadowed like the clock, solid key caps · [`68f0d13`](https://git.thoxy.xyz/thoxy/outpick/commit/68f0d13023969b1d53689b8bffb2564989701062)

### Position picker: the sidebar offers left or right only; the panel glyph takes the real panel's size · [`06da6f9`](https://git.thoxy.xyz/thoxy/outpick/commit/06da6f95a3b2a96a7a38e8300611dd1e11de001c)

### Sources editor: Back while moving a row restores the order; icons chosen in a grid picker · [`3bb4eba`](https://git.thoxy.xyz/thoxy/outpick/commit/3bb4eba2888a2d8bb87cddeb98e7032bd46a32a2)

### Quick switch: cycles plugged HDMI and Live TV only, counts from the last Source press, says where and when in the header, new footer · [`a225f47`](https://git.thoxy.xyz/thoxy/outpick/commit/a225f47703a075d9da76664de52f9008af849f25)

### List layouts: rows span the panel's width · [`48b7850`](https://git.thoxy.xyz/thoxy/outpick/commit/48b7850d0902072b449927859b427205ee960b18)

### Overflowing picker: edges with hidden tiles fade into the panel, with a chevron sideways · [`71cd1ce`](https://git.thoxy.xyz/thoxy/outpick/commit/71cd1ce8258e5d9c3f08d1580de56fb1fb2ae2b9)

### Tiles: light text over previews, fixed icon/name anchors, balanced two-line names, 15 pt badge and key caps · [`ad88b6b`](https://git.thoxy.xyz/thoxy/outpick/commit/ad88b6b49d4a0fadce71c394e58f631335df6de8)

### CEC options only for inputs with HDMI-CEC; power row names on/off; facts omit unknowns · [`d927d0b`](https://git.thoxy.xyz/thoxy/outpick/commit/d927d0bcedfae31bbfbc0b54225178937871a735)

- Source gains cec (a SIMPLINK sub-entry exists) and cec_power (SIMPLINK cecPower, webOS 25). The options screen lists CEC rows only with CEC, drops Switch on the input already shown, and its footer reads Choose / Confirm / Back.

### Rename to Outpick (com.thoxy.outpick): app, binary, paths, key filter, links to git.thoxy.xyz/thoxy/outpick; migrate the key filter, capture access and settings of the old com.thoxy.sourcepicker · [`67b8013`](https://git.thoxy.xyz/thoxy/outpick/commit/67b80132cfc92c49481cc495b3d2b10df69c7457)

### Zero warnings: TV-only items marked, mock logs launches and CEC keys, drop unused luna::call · [`46ae75d`](https://git.thoxy.xyz/thoxy/outpick/commit/46ae75d127150d1b7b534822ff5be046fe170a77)

### Post-merge: 4 KiB page alignment, sticky config dir, relocatable sdl2.pc, whole-word PC makers · [`ae854a5`](https://git.thoxy.xyz/thoxy/outpick/commit/ae854a5604e6986048e32a5e7e16d4b05512cc43)

### Merge wp-a: startup order, state cache, previews in /tmp, clean exit · [`7d5ae93`](https://git.thoxy.xyz/thoxy/outpick/commit/7d5ae9331b8b39a162dfe44265b3f8ce11656c95)

### Split src/app.rs into src/app/ modules · [`fc82884`](https://git.thoxy.xyz/thoxy/outpick/commit/fc8288448e6f72df12b6ab1fc5527e09ea04f458)

- **Pure move:** one file per screen (picker, settings, sources/rename, CEC options, style/position/clock pickers), entries, and shared widgets, as several impl App blocks.
- Dedupes, with no change in behaviour or pixels: one layout_glyph() for the style picker and the Customize strip position_anchor() and layout_shape() tables, used by panel_glyph_rect(), which position_picture() now calls selected_row() for the selected row of Customize and CEC options settings_key() and toggle_takeover() take the frame's TvState instead of locking and cloning the backend state again the picker's visible entries are built once per frame in ui() and passed down (rebuilt only after a key changed the config)

### README: --uninstall and the boot hook's cleanup after an uninstall · [`16cc07f`](https://git.thoxy.xyz/thoxy/outpick/commit/16cc07f60978774ea6ce5e21d830a1c60c85443d)

### Root side: filter out of the shared folder, keys in configd, boot hook, clean uninstall · [`2f7384c`](https://git.thoxy.xyz/thoxy/outpick/commit/2f7384c6b555cdf014cbcb9c8e5aeafbd3fe4d64)

- The key filter lives in /var/lib/lg-c1-source (0755 root), no longer in the world-writable shared folder; an old chain entry is moved there (one reload) and the old file deleted, also from the app at launch. The shared folder is sticky (1777); --fix-permissions <uid> hands the app a root-owned config.json and never follows links. Source and Multikey are separate: config sourceKeyOpens (default true) beside multikeyOpens; the filter is installed iff either is on and ignores the Source keys when that one is off. --takeover on|off keeps working from the saved flags. The filter text is constant; the keys are the configd value com.thoxy.sourcepicker.keys, followed by the filter from init(). Changing Multikey writes only that value: the chain is written only on install, uninstall or a new filter text. Boot hook /var/lib/webosbrew/init.d/70-lg-c1-source (a copy): runs --boot while the app is there (re-asserts the takeover after a configd reset), cleans up and deletes itself once the app is gone, keeping config.json. Removed when both options are off. New --uninstall. Capture off takes only this app out of forceCaptureClient and deletes the file only if the rest equals the system one; no more .bak (a stale one equal to the system file is deleted); tmp+rename writes. access() also says whether the list is enforced (webOS 6). detect() reads the chain, the native chain and Glasshouse's keys in one call. The filter falls back to LG's picker when the app manager has not answered a launch within 1.5 s. Pure logic in src/rootside.rs, tested on the host.

### README, Homebrew description: hbc feed URL, root optional, uninstall, clock, quick switch · [`6ec41fa`](https://git.thoxy.xyz/thoxy/outpick/commit/6ec41fad4515338e571e84f3070b9733129e60ff)

- Install points at the hbc branch feed and says what needs root; how to give the keys back before uninstalling; Configure lists Clock; Highlights get the clock (seven styles), quick switch, eleven accent colours and the current binary size.

### build.rs: 96 px icon masks (embedded 80 352 -> 55 738 bytes) · [`efd97e4`](https://git.thoxy.xyz/thoxy/outpick/commit/efd97e482cc37c97ad38210443ae2d57b3149eec)

- The largest icon is drawn at about 97 px (scale 1.5 on a 1080p TV), so 128 px masks were downscaled anyway. Binary 1 968 060 -> 1 902 524 bytes.

### build-webos.sh: std with optimize_for_size (1 968 060 bytes, -65 KB) · [`1761343`](https://git.thoxy.xyz/thoxy/outpick/commit/1761343b7ce7ab2b83319a9b1ef56c4102f50d49)

### Fonts: subset to the characters used, drop Inter Medium · [`3767e6d`](https://git.thoxy.xyz/thoxy/outpick/commit/3767e6dc5a6618358dd328132a6ac86c09329da7)

- subset-fonts.sh keeps ASCII, Latin-1 + Extended-A, the punctuation, arrows and triangles the UI draws (--check lists any character of the locales or the sources left out). medium() draws with SemiBold. Binary 2 164 668 -> 2 033 596 bytes.

### Packaging: reproducible ipk, hbc feed branch, optional root · [`167e3bd`](https://git.thoxy.xyz/thoxy/outpick/commit/167e3bdba6fc6a26f35f45857d56d16cf25c251b)

- package.py: timestamps from SOURCE_DATE_EPOCH or the last commit, gzip mtime 0, sorted root-owned members, packager version 1.0.0; adds usr/palm/packages/<id>/packageinfo.json; appinfo gets resolution 1920x1080 and handlesRelaunch false; prints the ipk path first. publish-feed-branch.sh: the Homebrew Channel feed as the orphan branch hbc (repo.json, description.html, largeIcon.png), a normal push instead of deleting and re-uploading a generic package. publish-repo.sh is kept one more release, marked for removal. repository.py: rootRequired "optional" (the picker runs without root). release.yml: cargo metadata --locked check, feed branch step. ci.yml: package twice and compare the sha256. deploy.sh: --no-launch, .build/tv.env (tools/tv.env.example), installs the ipk package.py wrote.

### Device icons from HDMI-CEC first: PlayStation, Xbox, Nintendo Switch, Android TV boxes, PC · [`5f358bb`](https://git.thoxy.xyz/thoxy/outpick/commit/5f358bb9f56846beca0da628f5b800db43b7acdd)

### Quick switch delay setting (1-10 s, default 3); preview screenshot delay · [`d853ad6`](https://git.thoxy.xyz/thoxy/outpick/commit/d853ad665fc22b348cceb81c673d1b7ee2d940fd)

### Quick switch always on with the Source key only (3 s), no setting · [`161f1de`](https://git.thoxy.xyz/thoxy/outpick/commit/161f1de4c9418c0ebcc48d545af6b988214d856b)

### Quick switch: repeated Source presses cycle inputs and switch after a countdown · [`142011b`](https://git.thoxy.xyz/thoxy/outpick/commit/142011b56f11efbefb9c72309f3bc0b43e032f82)

### On-screen clock (7 styles, placement screen) and option to hide the panel background · [`0fc4428`](https://git.thoxy.xyz/thoxy/outpick/commit/0fc44281994aa4c9cb70f28f31b34c3cfd5e12e5)

### Pickers and rename: OK confirms, Back cancels · [`c395229`](https://git.thoxy.xyz/thoxy/outpick/commit/c395229353a1331bd9ad0752437b7316f2a8b88e)

### Style and position pickers: layouts as cards, positions on a screen moved with the arrows · [`5375d81`](https://git.thoxy.xyz/thoxy/outpick/commit/5375d8196c4b2a56a3ae81a470b1562b9d1ba1fe)

### List: the selected row pushes its neighbours apart, animated with the selection · [`64dd750`](https://git.thoxy.xyz/thoxy/outpick/commit/64dd7504cba3b964671bbbd65888aef3897078ae)

### Key hints never leave the panel: the list widens for them, a narrow panel drops the arrow hints then shrinks the text · [`28308a8`](https://git.thoxy.xyz/thoxy/outpick/commit/28308a88d31507ca101dd4ddaefc8ddf00161b10)

### README in term's style with the tested TVs, screenshots, LICENSE and NOTICE · [`fd09598`](https://git.thoxy.xyz/thoxy/outpick/commit/fd0959832082e146d308b91170eec5dcbfbae1f9)

### CI and releases on Forgejo: pinned nightly, TV build, generated notes, Homebrew Channel repo.json · [`f17cbd5`](https://git.thoxy.xyz/thoxy/outpick/commit/f17cbd52fb120046a165275527ad2ac80291d353)

- **CI:** host tests, the size-optimised TV build and its ipk on every push to main.
- **Release:** a vX.Y.Z tag checks the version, builds, packages, generates the notes since the previous tag and publishes the Forgejo release with the ipk, repo.json, the manifest, the description, the icon and SHA256SUMS; repo.json is also published under a fixed URL (generic package "latest") for the TVs' Homebrew Channel.
- **Tools:** tools/release.sh, tools/release/changelog.py (from term), forgejo-release.sh, repository.py (after Luma's), publish-repo.sh.

### Multikey: short press opens the picker, long press keeps LG accessibility; stored previews on home; Luma manifest detection · [`4ef4385`](https://git.thoxy.xyz/thoxy/outpick/commit/4ef4385d8cc227c8b90a1ec003ee05df14b46d3f)

### Source previews (live for the input in front, last seen otherwise); webOS 6 capture access option · [`7c6616f`](https://git.thoxy.xyz/thoxy/outpick/commit/7c6616fa254a5484b9e4b0cba7293a7be98a704c)

### Size: build-time icon masks, Inter subset, no default fonts, build-std + immediate-abort + Thumb-2 (5.8 MB -> 2.1 MB); app icon 2 · [`de1c420`](https://git.thoxy.xyz/thoxy/outpick/commit/de1c420a77cc00301bd6d14bde8ef439ca817476)

### Customize: layout and position shown as pictures · [`49e82b8`](https://git.thoxy.xyz/thoxy/outpick/commit/49e82b8cf790b2f7b85dd66c2a5365049cb129ae)

### Customize: sliders, switches, real buttons, per-row key hints · [`ef07137`](https://git.thoxy.xyz/thoxy/outpick/commit/ef071372bca4199c6ae81e2c95e6d77f9b22873d)

### lg-c1-source: native egui input picker for LG webOS (C1 + QNED), CEC options, takeover · [`a9a605a`](https://git.thoxy.xyz/thoxy/outpick/commit/a9a605a465ffbafd5742e55409982fde0f3bb251)

