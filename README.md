# tgdigest

# What's this?
It's a tool to generate a video from telegram channel's posts.

This is how the result looks like: https://ithueti.club/story

As additional artifact the tool can create top/digest page: https://ithueti.club/digest2023

# Build
```sh
cargo build
```

# Docker

Run with Docker Compose:
```sh
docker compose up --build -d
```

If this is your first run and there is no valid `tgdigest.session`, initialize it once in interactive mode:
```sh
docker compose run --rm tgdigest -c /app/config/config.json
```
Enter phone/code/password when prompted. After that, the session is stored in `./state`, and regular `docker compose up -d` works without interactive prompts.

Default directory mounts in `docker-compose.yml`:
- `./config -> /app/config` (read-only, for secrets)
- `./data -> /app/data` (read-only, templates)
- `./output -> /app/output`
- `./state -> /app/state`

Override paths with environment variables, for example:
```sh
TGDIGEST_CONFIG_DIR=~/tgdigest-config \
TGDIGEST_DATA_DIR=~/tgdigest-private/data \
TGDIGEST_OUTPUT_DIR=$(pwd)/output \
TGDIGEST_STATE_DIR=$(pwd)/state \
docker compose up --build -d
```

Example `docker-config.json`:
```json
{
        "input_dir": "/app/data",
        "output_dir": "/app/output",
        "tg_session": "/app/state/tgdigest.session",
        "tg_id": <tg_app_id>,
        "tg_hash": "<tg_app_hash>",
        "proxy_url": "socks5://host:port"
}
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
    "tg_hash": "<tg_app_hash>",
    "proxy_url": "socks5://host:port"
}
```

- `proxy_url` (optional): SOCKS5 proxy for Telegram connection. Supports `socks5://host:port` or `socks5://user:pass@host:port`. Omit the field to connect directly.

# Caching

Post data fetched from Telegram is cached in a local SQLite database (`cache.db`, stored next to `tg_session`). The cache TTL is 24 hours. To bypass the cache and force a fresh fetch from Telegram, add `force=true` query parameter to any `/digest` or `/video` endpoint.

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
                - <force> (optional): Bypass cache and fetch fresh data from Telegram.

- **GET /digest/\<mode\>/\<channel\>**

        - Description: Retrieves a digest for the specified mode and channel.
        - Parameters:
                - <mode>: The mode of the digest.
                - <channel>: The channel name.
                - <top_count> (optional): The number of posts to include in the digest.
                - <editor_choice> (optional): Include editor's choice posts.
                - <from_date> (optional): The starting date for the digest.
                - <to_date> (optional): The ending date for the digest.
                - <force> (optional): Bypass cache and fetch fresh data from Telegram.

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
                - <force> (optional): Bypass cache and fetch fresh data from Telegram.

- **GET /digest/\<mode\>/\<channel\>/\<year\>**

        - Description: Retrieves a digest for the specified mode, channel, and year.
        - Parameters:
                - <mode>: The mode of the digest.
                - <channel>: The channel name.
                - <year>: The year.
                - <top_count> (optional): The number of posts to include in the digest.
                - <editor_choice> (optional): Include editor's choice posts.
                - <force> (optional): Bypass cache and fetch fresh data from Telegram.

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
                - <force> (optional): Bypass cache and fetch fresh data from Telegram.

- **GET /digest/\<mode\>/\<channel\>/\<year\>/\<month\>**

        - Description: Retrieves a digest for the specified mode, channel, year, and month.
        - Parameters:
                - <mode>: The mode of the digest.
                - <channel>: The channel name.
                - <year>: The year.
                - <month>: The month.
                - <top_count> (optional): The number of posts to include in the digest.
                - <editor_choice> (optional): Include editor's choice posts.
                - <force> (optional): Bypass cache and fetch fresh data from Telegram.

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
                - <force> (optional): Bypass cache and fetch fresh data from Telegram.

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
                - <force> (optional): Bypass cache and fetch fresh data from Telegram.
