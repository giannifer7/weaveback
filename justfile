# justfile — weaveback workspace

# Default: list available recipes
default:
    @just --list

# ── Build ─────────────────────────────────────────────────────────────────────

# Build the whole workspace (debug)
build:
    cargo build

# Build the whole workspace (release)
release:
    cargo build --release

# ── Test ──────────────────────────────────────────────────────────────────────

# Run all tests
test:
    cargo test

# Run tests for weaveback-macro only
test-macros:
    cargo test --package weaveback-macro

# Run tests for weaveback-tangle only
test-noweb:
    cargo test --package weaveback-tangle

# ── Lint ──────────────────────────────────────────────────────────────────────

# Clippy (warnings as errors)
lint:
    cargo clippy -- -D warnings

# Format check
fmt-check:
    cargo fmt --check

# Apply formatting
fmt:
    cargo fmt

# Find code duplicates
duplicates TARGET='.':
    npx jscpd -g --ignore "test-data/**,tree-sitter-weaveback/src/*.json" {{TARGET}}

# ── Run ───────────────────────────────────────────────────────────────────────

# Run the combined weaveback tool (usage: just weaveback src/foo.md)
weaveback FILE:
    cargo run --package weaveback -- "{{FILE}}"

# Serve docs/html/ locally with live reload and inline editor (dev build)
serve *ARGS:
    cargo run --release --package weaveback -- serve {{ARGS}}

# Run weaveback-macro on a file (usage: just macros src/foo.md)
macros FILE:
    cargo run --package weaveback-macro -- "{{FILE}}"

# Run weaveback-tangle on a file (usage: just noweb src/foo.md)
noweb FILE:
    cargo run --package weaveback-tangle -- "{{FILE}}"

# ── Examples ──────────────────────────────────────────────────────────────────

# Regenerate the c_enum example
example-c-enum:
    cd examples/c_enum && cargo run --package weaveback -- status.md --gen .

# Regenerate the events fan-out example
example-events:
    cd examples/events && cargo run --package weaveback -- events.adoc --gen .

# Regenerate the nim-adoc example via meson/ninja
example-nim-adoc:
    meson setup examples/nim-adoc/build examples/nim-adoc --wipe
    ninja -C examples/nim-adoc/build

# Remove build intermediates from nim-adoc; keep gen/ and docs/html/ for commit
example-nim-adoc-clean:
    rm -rf examples/nim-adoc/build examples/nim-adoc/weaveback.db

# ── Packaging ─────────────────────────────────────────────────────────────────

# Build container stage: glibc | musl | windows | fedora
build-container TARGET:
    podman build --target {{TARGET}} -t weaveback-{{TARGET}} .

# Build container and export artifacts into dist/TARGET/
export TARGET: (build-container TARGET)
    mkdir -p dist/{{TARGET}}
    podman create --name weaveback-export-{{TARGET}} weaveback-{{TARGET}}
    podman cp weaveback-export-{{TARGET}}:/out/. dist/{{TARGET}}/
    podman rm weaveback-export-{{TARGET}}

# Build and export all targets
export-all: (export "glibc") (export "musl") (export "windows") (export "fedora")

# Build .deb locally (requires cargo-deb)
deb:
    cargo build --release --workspace
    cargo deb -p weaveback --no-build

# Build .rpm locally (requires cargo-generate-rpm)
rpm:
    cargo build --release --workspace
    cargo generate-rpm -p crates/weaveback

# Bump Cargo.toml version first, then: just tag
# Commits Cargo.lock, tags, waits for CI, writes PKGBUILD to aur-weaveback-bin/, updates flake.nix
tag:
    python packaging/update_release.py --tag

# Re-tag HEAD (same version, re-triggers CI) then publish
re-tag:
    python packaging/update_release.py --retag

# Re-run publish only — tag already pushed and CI already done
update-release:
    python packaging/update_release.py

# ── Literate programming ──────────────────────────────────────────────────────

# Tangle all .adoc literate sources under crates/ into generated Rust files
tangle:
    python3 scripts/tangle.py

# Render all .adoc files to dark-themed HTML under docs/html/ (with Rust xref)
# --special % de-escapes %% in files that use % as the macro special char
# --special ^ de-escapes ^^ in weaveback-macro adocs (which use ^ as special)
docs:
    cargo run --release --package weaveback-docgen -- --special % --special ^

# ── Clean ─────────────────────────────────────────────────────────────────────

# cargo clean + dist/
clean:
    cargo clean
    rm -rf dist/
