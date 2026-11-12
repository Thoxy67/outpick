#!/bin/sh
# Shrinks Inter to what a French/English TV UI shows (Latin + Latin Extended,
# punctuation, arrows, geometric shapes such as ▲▼◀▶), ~410 KiB -> a few dozen
# KiB per weight. Needs fonttools (pyftsubset). Re-run after changing locales.
set -eu
cd "$(dirname "$0")/../assets/fonts"
for w in Regular Medium SemiBold; do
  pyftsubset "Inter-$w.ttf" \
    --unicodes='U+0020-007E,U+00A0-024F,U+02C6-02DD,U+1E00-1EFF,U+2000-206F,U+20A0-20CF,U+2100-214F,U+2190-21FF,U+2212,U+25A0-25FF' \
    --layout-features='kern' --no-hinting --desubroutinize \
    --output-file="Inter-$w.subset.ttf"
done
ls -l Inter-*.subset.ttf
