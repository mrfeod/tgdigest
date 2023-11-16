#!/bin/bash

# Cut the middle of long images
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

# Expand frames to 1080x1920
ffmpeg -y -i crop_%2d.png -vf "pad=1080:1920:(ow-iw)/2:130:white" frame_%d.png
# Render cards to video (11 sec per card)
ffmpeg -y -framerate 1/11 -i frame_%d.png -vf "fps=30" -c:v libx264 -pix_fmt yuv420p output.mp4
# Add an audio with fades
ffmpeg -y -i output.mp4 -i digest_audio.mp3 -filter_complex "[1:a]afade=type=out:duration=2:start_time=53[a]" -map 0:v -map "[a]" -c:v copy -shortest digest_body.mp4
# Add an ending part
ffmpeg -y -i digest_body.mp4 -i digest_ending.mp4 -filter_complex "[0:v] [0:a] [1:v] [1:a] concat=n=2:v=1:a=1 [v] [a]" -map "[v]" -map "[a]" digest.mp4
