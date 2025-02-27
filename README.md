# tgdigest

# Build
```sh
cargo build
```

# Run
You need to specify the configuration file as an argument.
```sh
cargo run -- -c config.json
```

`config.json`: file example:
```json
{
    "input_dir": "~/code/tgdigest/data",
    "output_dir": "./output",
    "tg_session": "./tgdigest.session",
    "tg_id": <tg_app_id>,
    "tg_hash": "<tg_app_hash>"
}
```

# Server Endpoints

- **GET /pic/\<channel\>**

        - Description: Retrieves an image for the specified channel.
        - Parameters:
                - <channel>: The channel name.

- **GET /video/\<mode\>/\<channel\>**

        - Description: Retrieves a video for the specified mode and channel.
        - Parameters:
                - <mode>: The mode of the video.
                - <channel>: The channel name.
                - <replies> (optional): Include replies in the response.
                - <reactions> (optional): Include reactions in the response.
                - <forwards> (optional): Include forwards in the response.
                - <views> (optional): Include views in the response.
                - <top_count> (optional): The number of videos to retrieve.
                - <editor_choice> (optional): Include editor's choice videos.
                - <from_date> (optional): The starting date for the videos.
                - <to_date> (optional): The ending date for the videos.

- **GET /digest/\<mode\>/\<channel\>**

        - Description: Retrieves a digest for the specified mode and channel.
        - Parameters:
                - <mode>: The mode of the digest.
                - <channel>: The channel name.
                - <top_count> (optional): The number of posts to include in the digest.
                - <editor_choice> (optional): Include editor's choice posts.
                - <from_date> (optional): The starting date for the digest.
                - <to_date> (optional): The ending date for the digest.

- **GET /video/\<mode\>/\<channel\>/\<year\>**

        - Description: Retrieves a video for the specified mode, channel, and year.
        - Parameters:
                - <mode>: The mode of the video.
                - <channel>: The channel name.
                - <year>: The year.
                - <replies> (optional): Include replies in the response.
                - <reactions> (optional): Include reactions in the response.
                - <forwards> (optional): Include forwards in the response.
                - <views> (optional): Include views in the response.
                - <top_count> (optional): The number of videos to retrieve.
                - <editor_choice> (optional): Include editor's choice videos.

- **GET /digest/\<mode\>/\<channel\>/\<year\>**

        - Description: Retrieves a digest for the specified mode, channel, and year.
        - Parameters:
                - <mode>: The mode of the digest.
                - <channel>: The channel name.
                - <year>: The year.
                - <top_count> (optional): The number of posts to include in the digest.
                - <editor_choice> (optional): Include editor's choice posts.

- **GET /video/\<mode\>/\<channel\>/\<year\>/\<month\>**

        - Description: Retrieves a video for the specified mode, channel, year, and month.
        - Parameters:
                - <mode>: The mode of the video.
                - <channel>: The channel name.
                - <year>: The year.
                - <month>: The month.
                - <replies> (optional): Include replies in the response.
                - <reactions> (optional): Include reactions in the response.
                - <forwards> (optional): Include forwards in the response.
                - <views> (optional): Include views in the response.
                - <top_count> (optional): The number of videos to retrieve.
                - <editor_choice> (optional): Include editor's choice videos.

- **GET /digest/\<mode\>/\<channel\>/\<year\>/\<month\>**

        - Description: Retrieves a digest for the specified mode, channel, year, and month.
        - Parameters:
                - <mode>: The mode of the digest.
                - <channel>: The channel name.
                - <year>: The year.
                - <month>: The month.
                - <top_count> (optional): The number of posts to include in the digest.
                - <editor_choice> (optional): Include editor's choice posts.

- **GET /video/\<mode\>/\<channel\>/\<year\>/\<month\>/\<week\>**

        - Description: Retrieves a video for the specified mode, channel, year, month, and week.
        - Parameters:
                - <mode>: The mode of the video.
                - <channel>: The channel name.
                - <year>: The year.
                - <month>: The month.
                - <week>: The week.
                - <replies> (optional): Include replies in the response.
                - <reactions> (optional): Include reactions in the response.
                - <forwards> (optional): Include forwards in the response.
                - <views> (optional): Include views in the response.
                - <top_count> (optional): The number of videos to retrieve.
                - <editor_choice> (optional): Include editor's choice videos.

- **GET /digest/\<mode\>/\<channel\>/\<year\>/\<month\>/\<week\>**

        - Description: Retrieves a digest for the specified mode, channel, year, month, and week.
        - Parameters:
                - <mode>: The mode of the digest.
                - <channel>: The channel name.
                - <year>: The year.
                - <month>: The month.
                - <week>: The week.
                - <top_count> (optional): The number of posts to include in the digest.
                - <editor_choice> (optional): Include editor's choice posts.
