# Containerfile — multi-stage packaging builds for weaveback
#
# Stages:
#   glibc   — Debian binaries + tarball          — includes Python/PyO3 + docs tooling
#   musl    — Alpine static CLI binaries         — Python wheels built separately
#   windows — MinGW cross-compiled CLI .exe      — Python wheels built separately
#   fedora  — Fedora binaries                    — includes Python/PyO3 + docs tooling
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
#   - build weaveback-py wheels separately via maturin/cibuildwheel rather than
#     forcing the generic musl/MinGW CLI release stages to compile a cdylib

# ── Rust base (Debian bookworm) ───────────────────────────────────────────────
FROM debian:bookworm-slim AS rust-base

RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates build-essential pkg-config git graphviz \
        python3 python3-dev python3-venv \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    UV_INSTALL_DIR=/usr/local/bin \
    UV_TOOL_BIN_DIR=/usr/local/bin \
    PATH=/usr/local/cargo/bin:/usr/local/bin:$PATH

RUN curl https://sh.rustup.rs -sSf \
    | sh -s -- -y --default-toolchain stable --no-modify-path
RUN cargo install cargo-llvm-cov
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
RUN curl -fsSL https://d2lang.com/install.sh | sh -s -- --prefix /usr/local
RUN uv tool install --python /usr/bin/python3 maturin \
    && uv tool install --python /usr/bin/python3 mypy \
    && uv tool install --python /usr/bin/python3 pylint

# ── cargo-chef planner ────────────────────────────────────────────────────────
FROM rust-base AS planner
WORKDIR /src
RUN cargo install cargo-chef
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── dependency cacher ────────────────────────────────────────────────────────
FROM rust-base AS cacher
WORKDIR /src
RUN cargo install cargo-chef cargo-llvm-cov
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# ── glibc: Linux binaries + tarball ──────────────────────────────────────────
FROM cacher AS glibc
COPY . .
RUN cargo build --release --workspace
RUN mkdir -p /out \
    && cp target/release/weaveback-macro    /out/weaveback-macro-glibc \
    && cp target/release/weaveback-tangle   /out/weaveback-tangle-glibc \
    && cp target/release/weaveback-docgen   /out/weaveback-docgen-glibc \
    && cp target/release/wb-tangle          /out/wb-tangle-glibc \
    && cp target/release/wb-query           /out/wb-query-glibc \
    && cp target/release/wb-serve           /out/wb-serve-glibc \
    && cp target/release/wb-mcp             /out/wb-mcp-glibc \
    && tar -czf /out/weaveback-x86_64-linux.tar.gz \
         -C target/release weaveback-macro weaveback-tangle weaveback-docgen wb-tangle wb-query wb-serve wb-mcp

# ── musl: static CLI binaries (Alpine) ────────────────────────────────────────
FROM alpine:latest AS musl
RUN apk add --no-cache curl build-base python3 python3-dev py3-virtualenv git graphviz
ENV RUSTUP_HOME=/root/.rustup \
    CARGO_HOME=/root/.cargo \
    UV_INSTALL_DIR=/usr/local/bin \
    UV_TOOL_BIN_DIR=/usr/local/bin \
    PATH=/root/.cargo/bin:/usr/local/bin:$PATH
RUN curl https://sh.rustup.rs -sSf \
    | sh -s -- -y --default-toolchain stable --no-modify-path
RUN cargo install cargo-llvm-cov
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
RUN curl -fsSL https://d2lang.com/install.sh | sh -s -- --prefix /usr/local
RUN uv tool install --python /usr/bin/python3 maturin \
    && uv tool install --python /usr/bin/python3 mypy \
    && uv tool install --python /usr/bin/python3 pylint
WORKDIR /src
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl \
        -p weaveback-macro \
        -p weaveback-tangle \
        -p weaveback-docgen \
        -p wb-tangle \
        -p wb-query \
        -p wb-serve \
        -p wb-mcp
RUN mkdir -p /out \
    && cp target/x86_64-unknown-linux-musl/release/weaveback-macro   /out/weaveback-macro-musl \
    && cp target/x86_64-unknown-linux-musl/release/weaveback-tangle  /out/weaveback-tangle-musl \
    && cp target/x86_64-unknown-linux-musl/release/weaveback-docgen  /out/weaveback-docgen-musl \
    && cp target/x86_64-unknown-linux-musl/release/wb-tangle         /out/wb-tangle-musl \
    && cp target/x86_64-unknown-linux-musl/release/wb-query          /out/wb-query-musl \
    && cp target/x86_64-unknown-linux-musl/release/wb-serve          /out/wb-serve-musl \
    && cp target/x86_64-unknown-linux-musl/release/wb-mcp            /out/wb-mcp-musl

# ── windows: MinGW cross-compilation for CLI binaries ────────────────────────
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
RUN rustup target add x86_64-pc-windows-gnu \
    && cargo build --release --target x86_64-pc-windows-gnu \
        -p weaveback-macro \
        -p weaveback-tangle \
        -p weaveback-docgen \
        -p wb-tangle \
        -p wb-query \
        -p wb-serve \
        -p wb-mcp
RUN mkdir -p /out \
    && cp target/x86_64-pc-windows-gnu/release/weaveback-macro.exe    /out/weaveback-macro-mingw64.exe \
    && cp target/x86_64-pc-windows-gnu/release/weaveback-tangle.exe   /out/weaveback-tangle-mingw64.exe \
    && cp target/x86_64-pc-windows-gnu/release/weaveback-docgen.exe   /out/weaveback-docgen-mingw64.exe \
    && cp target/x86_64-pc-windows-gnu/release/wb-tangle.exe          /out/wb-tangle-mingw64.exe \
    && cp target/x86_64-pc-windows-gnu/release/wb-query.exe           /out/wb-query-mingw64.exe \
    && cp target/x86_64-pc-windows-gnu/release/wb-serve.exe           /out/wb-serve-mingw64.exe \
    && cp target/x86_64-pc-windows-gnu/release/wb-mcp.exe             /out/wb-mcp-mingw64.exe

# ── fedora: Linux binaries ───────────────────────────────────────────────────
FROM fedora:latest AS fedora
RUN dnf install -y curl gcc pkg-config git graphviz python3 python3-devel python3-virtualenv && dnf clean all
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    UV_INSTALL_DIR=/usr/local/bin \
    UV_TOOL_BIN_DIR=/usr/local/bin \
    PATH=/usr/local/cargo/bin:/usr/local/bin:$PATH
RUN curl https://sh.rustup.rs -sSf \
    | sh -s -- -y --default-toolchain stable --no-modify-path
RUN cargo install cargo-llvm-cov
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
RUN curl -fsSL https://d2lang.com/install.sh | sh -s -- --prefix /usr/local
RUN uv tool install --python /usr/bin/python3 maturin \
    && uv tool install --python /usr/bin/python3 mypy \
    && uv tool install --python /usr/bin/python3 pylint
WORKDIR /src
COPY . .
RUN cargo build --release --workspace
RUN mkdir -p /out \
    && cp target/release/weaveback-macro   /out/weaveback-macro-fedora \
    && cp target/release/weaveback-tangle  /out/weaveback-tangle-fedora \
    && cp target/release/weaveback-docgen  /out/weaveback-docgen-fedora \
    && cp target/release/wb-tangle         /out/wb-tangle-fedora \
    && cp target/release/wb-query          /out/wb-query-fedora \
    && cp target/release/wb-serve          /out/wb-serve-fedora \
    && cp target/release/wb-mcp            /out/wb-mcp-fedora
