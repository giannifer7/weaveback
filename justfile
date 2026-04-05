# justfile — weaveback workspace

# Prefer locally-built release binary; fall back to debug, then PATH.
_wb := if path_exists("target/release/weaveback") == "true" { \
           "./target/release/weaveback" \
       } else if path_exists("target/debug/weaveback") == "true" { \
           "./target/debug/weaveback" \
       } else { "weaveback" }

_pyproj := "python/weaveback-agent"

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

# Build the PyO3 extension in place for local Python development
py-build:
    cd {{_pyproj}} && uv run maturin develop

# Build a wheel for the Python package
py-wheel:
    cd {{_pyproj}} && uv run maturin build

# Build Linux wheels via cibuildwheel
py-wheel-ci:
    cd {{_pyproj}} && uv run --with cibuildwheel cibuildwheel --platform linux

# Build a manylinux wheel for CPython 3.14
py-wheel-manylinux:
    cd {{_pyproj}} && CIBW_BUILD='cp314-manylinux_x86_64' uv run --with cibuildwheel cibuildwheel --platform linux

# Build a musllinux wheel for CPython 3.14
py-wheel-musllinux:
    cd {{_pyproj}} && CIBW_BUILD='cp314-musllinux_x86_64' uv run --with cibuildwheel cibuildwheel --platform linux

# Sync the Python project environment
py-sync:
    cd {{_pyproj}} && uv sync

# Render the experimental option-spec sample into Rust/Python/docs/facts outputs
option-spec-demo:
    python3 scripts/option_spec/render.py --spec scripts/option_spec/specs/tangle.toml --out /tmp/weaveback-option-spec

# Run the option-spec experiment tests
option-spec-test:
    python3 -m unittest scripts/option_spec/tests/test_render.py

# Full local Python check cycle
py-check: py-build lint-python test-python

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

# Run Python tests
test-python:
    cd {{_pyproj}} && uv run pytest

# Measure Rust test coverage with cargo-llvm-cov
coverage:
    cargo llvm-cov --workspace --lcov --output-path lcov.info

# Generate an HTML Rust coverage report under coverage_report/
coverage-html:
    cargo llvm-cov --workspace --html --output-dir coverage_report

# ── Lint ──────────────────────────────────────────────────────────────────────

# Clippy (warnings as errors)
lint:
    cargo clippy -- -D warnings

# Python lint/type-check suite
lint-python:
    cd {{_pyproj}} && uv run ruff check .
    cd {{_pyproj}} && uv run pyright
    cd {{_pyproj}} && uv run --with mypy mypy src
    cd {{_pyproj}} && uv run --with pylint pylint src/weaveback_agent

# Format check
fmt-check:
    cargo fmt --check

# Apply formatting
fmt:
    cargo fmt

# Apply Python formatting
fmt-python:
    cd {{_pyproj}} && uv run ruff format .

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

# Serve with auto-rebuild: edits to .adoc or theme sources trigger tangle + docs
dev *ARGS:
    cargo run --release --package weaveback -- serve --watch {{ARGS}}

# Run weaveback-macro on a file (usage: just macros src/foo.md)
macros FILE:
    cargo run --package weaveback-macro -- "{{FILE}}"

# Run weaveback-tangle on a file (usage: just noweb src/foo.md)
noweb FILE:
    cargo run --package weaveback-tangle -- "{{FILE}}"

# ── Examples ──────────────────────────────────────────────────────────────────

# Regenerate the c_enum example
example-c-enum:
    cd examples/c_enum && cargo run --package weaveback -- status.adoc --gen .

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

# Render the examples index page to HTML (weaveback-docgen handles all .adoc)
examples-index: docs

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
tag: lint
    python packaging/update_release.py --tag

# Re-tag HEAD (same version, re-triggers CI) then publish
re-tag:
    python packaging/update_release.py --retag

# Re-run publish only — tag already pushed and CI already done
update-release:
    python packaging/update_release.py

# ── Literate programming ──────────────────────────────────────────────────────

# Tangle all .adoc literate sources from weaveback.toml
tangle:
    {{_wb}} tangle

# Install weaveback binary (+ JDK for PlantUML diagrams with --diagrams)
# Pass extra args: just install --diagrams  /  just install --source
install *ARGS:
    python3 scripts/install.py {{ARGS}}

PLANTUML_JAR := "/usr/share/java/plantuml/plantuml.jar"

# Render all .adoc files to dark-themed HTML under docs/html/ (with Rust xref)
# --sigil % de-escapes %% in files that use % as the macro sigil
# --sigil ^ de-escapes ^^ in weaveback-macro adocs (which use ^ as sigil)
docs:
    node scripts/serve-ui/build.mjs
    cargo run --release --package weaveback-docgen -- \
        --sigil % --sigil ^ \
        --plantuml-jar {{PLANTUML_JAR}}

# Generate documentation with precise LSP-based cross-references (requires rust-analyzer)
docs-ai:
    node scripts/serve-ui/build.mjs
    cargo run --release --package weaveback-docgen -- \
        --sigil % --sigil ^ \
        --plantuml-jar {{PLANTUML_JAR}} \
        --ai-xref

# Semantic language server operations (requires rust-analyzer)
# Usage: just lsp definition crates/weaveback/src/main.rs 123 45
lsp *ARGS:
    cargo run --package weaveback -- lsp {{ARGS}}

# ── Clean ─────────────────────────────────────────────────────────────────────

# cargo clean + dist/
clean:
    cargo clean
    rm -rf dist/
