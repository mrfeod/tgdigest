#!/bin/bash
set -euo pipefail

card_seconds=5

shopt -s nullglob
cards=(card_*.png)
if [ ${#cards[@]} -eq 0 ]; then
    echo "No card_*.png files found" >&2
    exit 1
fi

filelist=$(mktemp /tmp/ffmpeg_cards_XXXXXX.txt)
trap 'rm -f "$filelist"' EXIT
for card in "${cards[@]}"; do
    printf "file '%s/%s'\n" "$PWD" "$card" >> "$filelist"
    printf "duration %s\n" "$card_seconds" >> "$filelist"
done

ffmpeg -y \
    -f concat -safe 0 -i "$filelist" \
    -vsync vfr \
    -c:v libx264 -preset ultrafast -pix_fmt yuv420p \
    digest.mp4
