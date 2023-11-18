# tgdigest

```text
> ./tgdigest.exe --help
Create digest for your telegram channel

Usage: tgdigest.exe [OPTIONS] <CHANNEL_NAME> [COMMAND]

Commands:
  cards  Generate cards from chosen digest posts from 1 to <TOP_COUNT>
  help   Print this message or the help of the given subcommand(s)

Arguments:
  <CHANNEL_NAME>  t.me/<CHANNEL_NAME>

Options:
  -i, --input-dir <INPUT_DIR>
          Directory with tgdigest.session file and html templates, default is working directory
  -o, --output-dir <OUTPUT_DIR>
          Directory to write all the program artifacts, default is working directory
  -d, --digest
          Generate digest.html
      --top-count <TOP_COUNT>
          Count of posts in digest [default: 3]
  -e, --editor-choice-post-id <EDITOR_CHOICE_POST_ID>
          The id of the post to place it in "Editor choice" block [default: -1]
  -f, --from-date <FROM_DATE>

  -t, --to-date <TO_DATE>

  -h, --help
          Print help
  -V, --version
          Print version
```

# Typical usage
The next commands do:
 - Generates `digest.html` for 2021 year with 5 posts for each category and editor choice from http://t.me/ithueti/5132
 - Renders `card_*.png` for first post in each category
 - Generate `digest.mp4` video from cards
```
./tgdigest.exe ithueti --digest --top-count 5 --editor-choice-post-id 5132 --from-date '2021-01-01 00:00:01 UTC' --to-date '2021-12-31 23:59:59 UTC' cards 1 1 1 1
./make_video.sh
```
