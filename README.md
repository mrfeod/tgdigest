# tgdigest

[![Build and push Docker image](https://github.com/mrfeod/tgdigest/actions/workflows/docker.yml/badge.svg)](https://github.com/mrfeod/tgdigest/actions/workflows/docker.yml)

# What's this?
It's a tool to generate a video from telegram channel's posts.

This is how the result looks like: https://ithueti.club/story

As additional artifact the tool can create top/digest page: https://ithueti.club/digest2023

# Build and Run
```sh
cargo build
```

You need to specify the configuration file as an argument and if this is your first run and there is no valid `tgdigest.session`, you have to log in.
```sh
cargo run -- -c config.json
```

After server start, basic API calls:
```text
http://127.0.0.1:8000/digest/example/ithueti/2026/3
http://127.0.0.1:8000/video/example/ithueti/2026/3?views=1
http://127.0.0.1:8000/view/ithueti/2026
```

`config.json`: file example:
```json
{
  "input_dir": "~/tgdigest/data",
  "output_dir": "./output",
  "tg_session": "./tgdigest.session",
  "tg_id": <tg_app_id>,
  "tg_hash": "<tg_app_hash>",
  "proxy_url": "socks5://host:port"
}
```

- `proxy_url` (optional): SOCKS5 proxy for Telegram connection. Supports `socks5://host:port` or `socks5://user:pass@host:port`. Omit the field to connect directly.


# Docker

Run with Docker Compose:
```sh
docker compose up --build -d
```

Or use prebuilt image from GHCR:
```sh
docker pull ghcr.io/mrfeod/tgdigest:master
TGDIGEST_IMAGE=ghcr.io/mrfeod/tgdigest:master docker compose up -d --no-build
```

If this is your first run and there is no valid `tgdigest.session`, log in using interactive mode:
```sh
docker compose run --rm tgdigest -c /app/config/config.json
```
After that, the session is stored in `./state`, and regular `docker compose up -d` works.

Default directory mounts in `docker-compose.yml`:
- `./config -> /app/config` (read-only, for secrets)
- `./data -> /app/data` (read-only, templates)
- `./output -> /app/output`
- `./state -> /app/state`

Override paths with environment variables, for example:
```sh
TGDIGEST_CONFIG_DIR=~/tgdigest-config \
TGDIGEST_DATA_DIR=~/tgdigest/data \
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

# Server Endpoints

- **GET `/userpic/<channel>`**
  - Stream channel userpic.

- **GET `/digest/<mode>/<channel>`**
- **GET `/digest/<mode>/<channel>/<year>`**
- **GET `/digest/<mode>/<channel>/<year>/<month>`**
- **GET `/digest/<mode>/<channel>/<year>/<month>/<week>`**
  - Render digest HTML page.
  - Query params (optional): `top_count`, `editor_choice`, `force`, `force_limit`
        - Only for `/<mode>/<channel>`: `from_date`, `to_date`

- **GET `/video/<mode>/<channel>`**
- **GET `/video/<mode>/<channel>/<year>`**
- **GET `/video/<mode>/<channel>/<year>/<month>`**
- **GET `/video/<mode>/<channel>/<year>/<month>/<week>`**
  - Render and return `.mp4`.
  - Query params (optional): `replies`, `reactions`, `forwards`, `views`, `top_count`, `editor_choice`, `force`
        - Only for `/<mode>/<channel>`: `from_date`, `to_date`

- **GET `/post/<channel>/<id>`**
  - Return post JSON.

- **GET `/view/<channel>/<id>`**
  - Render single post view as HTML.
  - Query params (optional): `views`, `forwards`, `reactions`, `comments`, `px_limit`, `dark`, `iframe`

- **GET `/data/<mode>/<channel>`**
  - Return digest data JSON for async templates.
  - Query params (optional): `top_count`, `editor_choice`, `from_date`, `to_date`, `force`, `force_limit`, `task_id`.
