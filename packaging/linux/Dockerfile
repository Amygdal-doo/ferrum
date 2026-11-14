# Build environment for Ferrum's Linux releases (scripts/build-linux.sh).
# Debian 12 keeps glibc at 2.36, so the binaries run on most current distros.
FROM debian:bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config cmake clang lld llvm git curl ca-certificates jq file \
      libasound2-dev libfontconfig-dev libgit2-dev libglib2.0-dev libssl-dev libva-dev \
      libvulkan1 libwayland-dev libx11-xcb-dev libxkbcommon-x11-dev libzstd-dev \
      libsqlite3-dev libstdc++-12-dev dpkg-dev \
    && rm -rf /var/lib/apt/lists/*

ARG RUST_TOOLCHAIN=1.99.1
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
      --default-toolchain "$RUST_TOOLCHAIN" \
    && chmod -R a+w "$RUSTUP_HOME" "$CARGO_HOME"
