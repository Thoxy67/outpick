#!/bin/sh
# Shrinks Inter to the characters a French/English TV UI shows, ~410 KiB -> ~30 KiB per
# weight. Needs fonttools (pyftsubset). Re-run after adding a character to the locales or
# the code (tools/subset-fonts.sh --check lists the ones not covered):
#   U+0020-007E ASCII, U+00A0-017F Latin-1 and Latin Extended-A (accents, × ·, device names),
#   U+2010-2027 dashes, quotes, … and •, U+2030-203A ‰ ′ ″ ‹ ›, U+20AC €, U+2190-2193 arrows,
#   U+2212 minus, U+25B2 U+25B6 U+25BC U+25C0 the ▲ ▶ ▼ ◀ hints, U+25FB ◻ (egui's
#   replacement glyph for anything else).
# Two weights: Regular and SemiBold (theme::medium() draws with SemiBold).
set -eu
cd "$(dirname "$0")/.."
UNICODES='U+0020-007E,U+00A0-017F,U+2010-2027,U+2030-203A,U+20AC,U+2190-2193,U+2212,U+25B2,U+25B6,U+25BC,U+25C0,U+25FB'
if [ "${1:-}" = --check ]; then
  # Every non-ASCII character of the locales and the sources must be in UNICODES.
  find locales src -name '*.lang' -o -name '*.rs' | xargs cat | python3 -c '
import sys
ranges = []
for part in sys.argv[1].split(","):
    a, _, b = part[2:].partition("-")
    ranges.append((int(a, 16), int(b or a, 16)))
missing = sorted({c for c in sys.stdin.read() if ord(c) > 0x7E and not any(a <= ord(c) <= b for a, b in ranges)})
for c in missing:
    print(f"not in the font subset: U+{ord(c):04X} {c}")
sys.exit(1 if missing else 0)' "$UNICODES"
  exit
fi
cd assets/fonts
for w in Regular SemiBold; do
  pyftsubset "Inter-$w.ttf" --unicodes="$UNICODES" \
    --layout-features='kern' --no-hinting --desubroutinize \
    --output-file="Inter-$w.subset.ttf"
done
ls -l Inter-*.subset.ttf
