# Azadi

Azadi is a literate-programming toolchain. Write your source code inside
Markdown (or any text file), expand macros, extract named chunks, and let
the tool write the real files.

```bash
azadi source.md --gen src
```

Under the hood two passes run in sequence:

1. **azadi-macros** — expands `%macro(...)` calls
2. **azadi-noweb** — extracts `<[@file ...]>` chunks and writes them to disk

Both passes run in-process; no intermediate files or subprocesses.

---

## Install

**Arch Linux:** `paru -S azadi-bin`

**Nix:** `nix profile install github:giannifer7/azadi`

**Quick (musl, any Linux):**
```bash
curl -sL https://github.com/giannifer7/azadi/releases/latest/download/azadi-musl \
     -o /usr/local/bin/azadi && chmod +x /usr/local/bin/azadi
```

→ [Full installation guide](docs/install.md) (all platforms, binaries, musl vs glibc)

---

## Quick start

```bash
cd examples/c_enum
azadi status.md --gen .
```

This expands macros in `status.md`, extracts all `@file` chunks, and writes
them under the current directory.

To see what the macro expander produced before noweb runs:

```bash
azadi status.md --gen . --dump-expanded 2>expanded.txt
```

---

## Documentation

| | |
|---|---|
| [CLI reference](docs/cli.md) | All flags for `azadi`, `azadi-macros`, `azadi-noweb`; directory mode; build-system integration |
| [Macro language](docs/macros.md) | `%def`, calling conventions, `%if`, `%rhaidef`, `%pydef`, X macro pattern |
| [Chunk syntax](docs/noweb.md) | `@file`, named chunks, `@replace`, `@reversed` |
| [Source tracing](docs/tracing.md) | `azadi where`/`trace`, MCP server for IDE/agent integration |
| [Installation](docs/install.md) | All platforms, package managers, pre-built binaries |

---

## License

MIT OR Apache-2.0
