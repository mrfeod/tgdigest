# syntax=docker/dockerfile:1.7
FROM rust:1-bookworm AS builder

WORKDIR /app

# Build dependencies required by some crates during compilation.
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first to improve layer caching.
COPY Cargo.toml Cargo.lock ./

COPY src ./src

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo fetch --locked

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo build --release --locked


FROM debian:bookworm-slim AS runtime

# Runtime tools/libraries:
# - chromium: HTML-to-image rendering through chromiumoxide
# - ffmpeg: frame/video composition in make_video.sh scripts
# - bash: script runner used by the service
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        bash \
        ffmpeg \
        chromium \
        chromium-sandbox \
        libasound2 \
        libatk-bridge2.0-0 \
        libgtk-3-0 \
        libnss3 \
        libx11-xcb1 \
        libgbm1 \
        fonts-dejavu-core \
        tzdata \
    && rm -rf /var/lib/apt/lists/*

ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8000

WORKDIR /app

COPY --from=builder /app/target/release/tgdigest /usr/local/bin/tgdigest

RUN mkdir -p /app/config /app/data /app/output /app/state \
    && useradd --create-home --uid 10001 appuser \
    && chown -R appuser:appuser /app

USER appuser

VOLUME ["/app/config", "/app/output", "/app/state"]

EXPOSE 8000

ENTRYPOINT ["tgdigest"]
CMD ["-c", "/app/config/config.json"]