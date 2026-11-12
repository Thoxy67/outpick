# Outpick app icon

`outpick.svg` is the source: an HDMI port whose signal rises as a violet arrow into a screen
(512×512, no external references or fonts). The PNGs are renders of it:

| File | Size | Used for |
|---|---|---|
| `outpick-80.png` | 80×80 | `icon.png` in the `.ipk` (TV app list) |
| `outpick-130.png` | 130×130 | `largeIcon.png` in the `.ipk` and the Homebrew Channel feed |
| `outpick-512.png` | 512×512 | full-size render |

Regenerate after editing the SVG:

```sh
for s in 80 130 512; do rsvg-convert -w $s -h $s outpick.svg -o outpick-$s.png; done
```

The interface icons (Lucide and Simple Icons glyphs, rasterised at build time) are in
[`../ui-icons`](../ui-icons).
