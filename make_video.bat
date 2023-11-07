ffmpeg -i card_%%2d.png -vf "pad=1080:1920:(ow-iw)/2:130:white" frame_%%d.png
ffmpeg -framerate 1/11 -i frame_%%d.png -vf "fps=30" -c:v libx264 -pix_fmt yuv420p output.mp4
ffmpeg -i output.mp4 -i digest_audio.mp3 -filter_complex "[1:a]afade=type=out:duration=2:start_time=53[a]" -map 0:v -map "[a]" -c:v copy -shortest digest_body.mp4
ffmpeg -i digest_body.mp4 -i digest_ending.mp4 -filter_complex "[0:v] [0:a] [1:v] [1:a] concat=n=2:v=1:a=1 [v] [a]" -map "[v]" -map "[a]" digest.mp4