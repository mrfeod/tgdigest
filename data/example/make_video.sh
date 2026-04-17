#!/bin/bash

threshold_height=1750
for input_image in card_*.png; do
    if [ ! -e "$input_image" ]; then
        continue
    fi

    output_image=$(echo "$input_image" | sed 's/card/crop/')
    image_height=$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of csv=p=0 "$input_image")

    if [ "$image_height" -gt "$threshold_height" ]; then
        ffmpeg -y -i "$input_image" -filter_complex "[0:v]crop=in_w:1680:0:0[top]; [0:v]crop=in_w:70:0:in_h-70[bottom]; [top][bottom]vstack=inputs=2[out]" -map "[out]" "$output_image"
    else
        cp "$input_image" "$output_image"
    fi
done

ffmpeg -y -i crop_%2d.png -vf "pad=1080:1920:(ow-iw)/2:130:white" frame_%d.png
ffmpeg -y -framerate 1/5 -i frame_%d.png -vf "fps=30" -c:v libx264 -pix_fmt yuv420p digest.mp4
