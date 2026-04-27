# tgdigest

[![Build and push Docker image](https://github.com/mrfeod/tgdigest/actions/workflows/docker.yml/badge.svg)](https://github.com/mrfeod/tgdigest/actions/workflows/docker.yml)

# What's this?
It's a tool to generate a video from telegram channel's posts.

[This is how the result looks like.](https://tgd.ithueti.club/?tab=2)

As additional artifact the tool can create [statistics](https://tgd.ithueti.club/?tab=0) or [digest](https://tgd.ithueti.club/?tab=1) page.

# Build and Run
```sh
cargo build
cargo run -- -c config.json
```
If this is your first run and there is no valid `tgdigest.session`, you have to log in with your Telegram account.

`config.json`: file example:
```json
{
  "input_dir": "~/tgdigest/data",
  "output_dir": "./output",
  "tg_session": "./tgdigest.session",
  "tg_id": <tg_app_id>,
  "tg_hash": "<tg_app_hash>",
  "cache_limit_mb": 1024,
  "public_base_url": "https://digest.example.com",
  "proxy_url": "socks5://host:port"
}
```

- `tg_*`: create your Telegram App credentials https://my.telegram.org/apps.
- `public_base_url` (optional): public base URL used in digest meta tags such as `canonical`, `og:url`, `og:image`, `twitter:image`.
- `proxy_url` (optional): SOCKS5 proxy for Telegram connection. Supports `socks5://host:port` or `socks5://user:pass@host:port`. Omit the field to connect directly.

After server start, basic API calls:
- **Digest:** http://127.0.0.1:8000/digest/example/ithueti/2026/3
- **Video:** http://127.0.0.1:8000/video/example/ithueti/2026/3?views=1
- **View:** http://127.0.0.1:8000/view/ithueti/2026

# Docker

Run with Docker Compose:
```sh
docker compose up --build -d
```

Or use prebuilt image from GHCR:
```sh
export TGDIGEST_IMAGE=ghcr.io/mrfeod/tgdigest:master
docker pull $TGDIGEST_IMAGE
docker compose up -d --no-build
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
  "cache_limit_mb": 1024,
  "public_base_url": "https://digest.example.com",
  "proxy_url": "socks5://host:port"
}
```

# Server Endpoints

Parameters without specified data type are flags, i.e. `comments`, `dark`, `force`, etc.
Example: ?dark` → `dark=true`

Ranking params belong to the range `[1, top_count]`.
Example: ?views=1` → use the best by views.

Timestamps (`utc_ts_sec`) are Unix timestamps in seconds (UTC).

Path param `mode`: directory name inside [`./data`](./data)
Example: /digest/example/ithueti` → uses templates from `./data/example`.

- **GET `/userpic/<channel>`** → `image/png`
  - Stream channel userpic.
  - Example: https://localhost:8000/userpic/ithueti

- **GET `/digest/<mode>/<channel>`** → `text/html`
- **GET `/digest/<mode>/<channel>/<year>`**
- **GET `/digest/<mode>/<channel>/<year>/<month>`**
- **GET `/digest/<mode>/<channel>/<year>/<month>/<week>`**
  - Render digest HTML page.
  - Query params (optional): `top_count=<int>`, `editor_choice=<int:post_id>`, `force_limit`, `force`
  - Only for `/<mode>/<channel>`: `from_date=<utc_ts_sec>`, `to_date=<utc_ts_sec>`
  - Example: https://localhost:8000/digest/example/ithueti?top_count=10&editor_choice=2026

- **GET `/video/<mode>/<channel>`** → `video/mp4`
- **GET `/video/<mode>/<channel>/<year>`**
- **GET `/video/<mode>/<channel>/<year>/<month>`**
- **GET `/video/<mode>/<channel>/<year>/<month>/<week>`**
  - Render and return `.mp4`.
  - Query params (optional):
    `top_count=<int>`, `replies=<int:[1, top_count]>`, `reactions=<int:[1, top_count]>`, `forwards=<int:[1, top_count]>`, `views=<int:[1, top_count]>`, `editor_choice=<int:post_id>`, `force`
  - Only for `/<mode>/<channel>`: `from_date=<utc_ts_sec>`, `to_date=<utc_ts_sec>`
  - Example: https://localhost:8000/video/example/ithueti?top_count=5&views=1&replies=1

- **GET `/post/<channel>/<id>`** → `application/json`
  - Return post JSON.
  - Example: https://localhost:8000/post/ithueti/2026`

- **GET `/view/<channel>/<id>`** → `text/html`
  - Render single post view as HTML.
  - Query params (optional): `views`, `forwards`, `reactions`, `comments`, `dark`, `iframe`, `px_limit=<int>`
  - Example: https://localhost:8000/view/ithueti/2026?dark&iframe&px_limit=900

- **GET `/data/<mode>/<channel>`** → `application/json`
  - Return digest data JSON for async templates.
  - Query params (optional):
    `top_count=<int>`, `editor_choice=<int:post_id>`, `from_date=<utc_ts_sec>`, `to_date=<utc_ts_sec>`, `force_limit=<int>`, `force`, `task_id=<int>`
  - Example: https://localhost:8000/data/example/ithueti?top_count=10&from_date=1700000000&to_date=1705000000
