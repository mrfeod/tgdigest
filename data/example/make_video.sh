#!/bin/bash
set -euo pipefail

threshold_height=1750
card_seconds=5

shopt -s nullglob
cards=(card_*.png)
if [ ${#cards[@]} -eq 0 ]; then
    echo "No card_*.png files found" >&2
    exit 1
fi

inputs=()
for card in "${cards[@]}"; do
    inputs+=(-loop 1 -t "$card_seconds" -i "$card")
done

filters=()
video_labels=()
for i in "${!cards[@]}"; do
    image_height=$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of csv=p=0 "${cards[$i]}")
    if [ "${image_height:-0}" -gt "$threshold_height" ]; then
        filters+=("[$i:v]crop=in_w:1680:0:0[top$i];[$i:v]crop=in_w:70:0:in_h-70[bottom$i];[top$i][bottom$i]vstack=inputs=2[crop$i]")
        source="[crop$i]"
    else
        source="[$i:v]"
    fi

    filters+=("${source}pad=1080:1920:(ow-iw)/2:130:white,fps=30,format=yuv420p,setsar=1,setpts=PTS-STARTPTS[v$i]")
    video_labels+=("[v$i]")
done

filters+=("${video_labels[*]}concat=n=${#cards[@]}:v=1:a=0[v]")
filter_complex=$(IFS=';'; echo "${filters[*]}")

ffmpeg -y "${inputs[@]}" \
    -filter_complex "$filter_complex" \
    -map "[v]" -an \
    -r 30 -c:v libx264 -pix_fmt yuv420p \
    digest.mp4
