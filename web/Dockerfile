# syntax=docker/dockerfile:1
#
# Multi-stage build for web-ferrum using cargo-chef for dependency caching.
# The Rust version matches rust-toolchain.toml (1.99.1). rust-toolchain.toml is
# deliberately NOT copied in: the base image tag pins the compiler, and copying
# it would make rustup try to download clippy/rustfmt during the build.

ARG RUST_VERSION=1.99.1

FROM rust:${RUST_VERSION}-slim-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# 1. Compute a dependency-only recipe from the manifests.
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

# 2. Build dependencies (cached until Cargo.toml/Cargo.lock change), then the app.
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src ./src
# Assets are embedded into the binary by rust-embed at compile time, and the
# usage-count SQL migrations by sqlx::migrate!.
COPY assets ./assets
COPY migrations ./migrations
RUN cargo build --release --locked --bin web-ferrum

# 3. Minimal runtime image, non-root.
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin app
COPY --from=builder /app/target/release/web-ferrum /usr/local/bin/web-ferrum
USER 10001
ENV PORT=8080 \
    RUST_LOG=info
EXPOSE 8080
CMD ["/usr/local/bin/web-ferrum"]
