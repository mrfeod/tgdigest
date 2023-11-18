# tgdigest

<pre>
> ./tgdigest.exe --help
Create digest for your telegram channel

<b>Usage: tgdigest.exe</b> [OPTIONS] <CHANNEL_NAME> [COMMAND]

Commands:
  cards  Generate cards from chosen digest posts from 1 to <TOP_COUNT>
  help   Print this message or the help of the given subcommand(s)

<b>Arguments:</b>
  <CHANNEL_NAME>  t.me/<CHANNEL_NAME>

<b>Options:</b>
  <b>-i, --input-dir</b> <INPUT_DIR>
          Directory with tgdigest.session file and html templates, default is working directory
  <b>-o, --output-dir</b> <OUTPUT_DIR>
          Directory to write all the program artifacts, default is working directory
  <b>-d, --digest</b>
          Generate digest.html
      <b>--top-count</b> <TOP_COUNT>
          Count of posts in digest [default: 3]
  <b>-e, --editor-choice-post-id</b> <EDITOR_CHOICE_POST_ID>
          The id of the post to place it in "Editor choice" block [default: -1]
  <b>-f, --from-date</b> <FROM_DATE>

  <b>-t, --to-date</b> <TO_DATE>

  <b>-h, --help</b>
          Print help
  <b>-V, --version</b>
          Print version
</pre>

# Typical usage
The next commands do:
 - Generates `digest.html` for 2021 year with 5 posts for each category and editor choice from http://t.me/ithueti/5132
 - Renders `card_*.png` for first post in each category
 - Generate `digest.mp4` video from cards
```
./tgdigest.exe ithueti --digest --top-count 5 --editor-choice-post-id 5132 --from-date '2021-01-01 00:00:01 UTC' --to-date '2021-12-31 23:59:59 UTC' cards 1 1 1 1
./make_video.sh
```
