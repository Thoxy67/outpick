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

