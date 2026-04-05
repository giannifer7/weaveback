# Containerfile — multi-stage packaging builds for weaveback
#
# Stages:
#   glibc   — Debian binary + .deb  (cargo-deb)  — includes Python/PyO3 tooling
#   musl    — Alpine static binary               — includes Python/PyO3 tooling
#   windows — MinGW cross-compiled .exe          — cross-compiles the PyO3 extension
#   fedora  — Fedora binary + .rpm               — includes Python/PyO3 tooling
#
# Usage:
#   podman build --target glibc  -t weaveback-glibc  .
#   podman build --target fedora -t weaveback-fedora .
#
# Python policy:
#   - use uv as the package/tool runner
#   - keep maturin available for crates/weaveback-py
#   - keep mypy and pylint installed for python/weaveback-agent even before
#     they are wired into CI
#   - keep the runtime stages simple; these are primarily build/dev images

# ── Rust base (Debian bookworm) ───────────────────────────────────────────────
FROM debian:bookworm-slim AS rust-base

RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates build-essential pkg-config git \
        python3 python3-dev python3-venv \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    UV_TOOL_BIN_DIR=/usr/local/bin \
    PATH=/usr/local/cargo/bin:/usr/local/bin:$PATH

RUN curl https://sh.rustup.rs -sSf \
    | sh -s -- -y --default-toolchain stable --no-modify-path
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
RUN uv tool install --python /usr/bin/python3 maturin \
    && uv tool install --python /usr/bin/python3 mypy \
    && uv tool install --python /usr/bin/python3 pylint

# ── cargo-chef planner ────────────────────────────────────────────────────────
FROM rust-base AS planner
WORKDIR /src
RUN cargo install cargo-chef
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── dependency cacher (glibc / deb) ──────────────────────────────────────────
FROM rust-base AS cacher
WORKDIR /src
RUN cargo install cargo-chef cargo-deb cargo-generate-rpm
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# ── glibc: Debian binary + .deb ──────────────────────────────────────────────
FROM cacher AS glibc
COPY . .
RUN cargo build --release --workspace
RUN cargo deb -p weaveback --no-build
RUN mkdir -p /out \
    && cp target/release/weaveback           /out/weaveback-glibc \
    && cp target/release/weaveback-macro    /out/weaveback-macro-glibc \
    && cp target/release/weaveback-tangle   /out/weaveback-tangle-glibc \
    && cp target/release/weaveback-docgen   /out/weaveback-docgen-glibc \
    && cp target/debian/*.deb               /out/ \
    && tar -czf /out/weaveback-x86_64-linux.tar.gz \
         -C target/release weaveback weaveback-macro weaveback-tangle weaveback-docgen

# ── musl: static binary (Alpine — musl-native Python for PyO3) ───────────────
FROM alpine:latest AS musl
RUN apk add --no-cache curl build-base python3 python3-dev py3-virtualenv git
ENV RUSTUP_HOME=/root/.rustup \
    CARGO_HOME=/root/.cargo \
    UV_TOOL_BIN_DIR=/usr/local/bin \
    PATH=/root/.cargo/bin:/usr/local/bin:$PATH
RUN curl https://sh.rustup.rs -sSf \
    | sh -s -- -y --default-toolchain stable --no-modify-path
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
RUN uv tool install --python /usr/bin/python3 maturin \
    && uv tool install --python /usr/bin/python3 mypy \
    && uv tool install --python /usr/bin/python3 pylint
WORKDIR /src
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl --workspace
RUN mkdir -p /out \
    && cp target/x86_64-unknown-linux-musl/release/weaveback          /out/weaveback-musl \
    && cp target/x86_64-unknown-linux-musl/release/weaveback-macro   /out/weaveback-macro-musl \
    && cp target/x86_64-unknown-linux-musl/release/weaveback-tangle  /out/weaveback-tangle-musl \
    && cp target/x86_64-unknown-linux-musl/release/weaveback-docgen  /out/weaveback-docgen-musl

# ── windows: MinGW cross-compilation (Fedora — has mingw64-python3-devel) ────
FROM fedora:latest AS windows
RUN dnf install -y \
        curl git gcc \
        mingw64-gcc \
        mingw64-python3 \
    && dnf clean all
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    PYO3_CROSS=1 \
    PYO3_CROSS_LIB_DIR=/usr/x86_64-w64-mingw32/sys-root/mingw/lib
RUN curl https://sh.rustup.rs -sSf \
    | sh -s -- -y --default-toolchain stable --no-modify-path
WORKDIR /src
COPY . .
RUN PYO3_CROSS_PYTHON_VERSION=$(ls /usr/x86_64-w64-mingw32/sys-root/mingw/include/ \
        | grep -oP '(?<=python)\d+\.\d+' | head -1) \
    && export PYO3_CROSS_PYTHON_VERSION \
    && rustup target add x86_64-pc-windows-gnu \
    && cargo build --release --target x86_64-pc-windows-gnu --workspace
RUN mkdir -p /out \
    && cp target/x86_64-pc-windows-gnu/release/weaveback.exe           /out/weaveback-mingw64.exe \
    && cp target/x86_64-pc-windows-gnu/release/weaveback-macro.exe    /out/weaveback-macro-mingw64.exe \
    && cp target/x86_64-pc-windows-gnu/release/weaveback-tangle.exe   /out/weaveback-tangle-mingw64.exe \
    && cp target/x86_64-pc-windows-gnu/release/weaveback-docgen.exe   /out/weaveback-docgen-mingw64.exe

# ── fedora: RPM ───────────────────────────────────────────────────────────────
FROM fedora:latest AS fedora
RUN dnf install -y curl gcc pkg-config git python3 python3-devel python3-virtualenv && dnf clean all
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    UV_TOOL_BIN_DIR=/usr/local/bin \
    PATH=/usr/local/cargo/bin:/usr/local/bin:$PATH
RUN curl https://sh.rustup.rs -sSf \
    | sh -s -- -y --default-toolchain stable --no-modify-path
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
RUN uv tool install --python /usr/bin/python3 maturin \
    && uv tool install --python /usr/bin/python3 mypy \
    && uv tool install --python /usr/bin/python3 pylint
RUN cargo install cargo-generate-rpm
WORKDIR /src
COPY . .
RUN cargo build --release --workspace
RUN cargo generate-rpm -p crates/weaveback
RUN mkdir -p /out \
    && cp target/release/weaveback          /out/weaveback-fedora \
    && cp target/release/weaveback-macro   /out/weaveback-macro-fedora \
    && cp target/release/weaveback-tangle  /out/weaveback-tangle-fedora \
    && cp target/release/weaveback-docgen  /out/weaveback-docgen-fedora \
    && cp target/generate-rpm/*.rpm        /out/
