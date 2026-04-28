# syntax=docker/dockerfile:1.7
FROM debian:bookworm-slim

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        nginx \
        libnginx-mod-http-js \
    && rm -rf /var/lib/apt/lists/*

RUN rm -f /etc/nginx/sites-enabled/default \
    && mkdir -p /var/cache/nginx/tgdigest_view \
    && chown -R www-data:www-data /var/cache/nginx

EXPOSE 80 443

CMD ["nginx", "-g", "daemon off;"]
