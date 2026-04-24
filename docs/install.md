---
title: |-
  Installing weaveback
toc: left
---
# Installing weaveback

weaveback is distributed as a set of focused binaries:

* `wb-tangle`
* `wb-query`
* `wb-serve`
* `wb-mcp`
* `weaveback-macro`
* `weaveback-tangle`
* `weaveback-docgen`

HTML documentation rendering is built in via `weaveback-docgen`, which uses
the Rust `acdc` AsciiDoc parser directly — no Ruby or external AsciiDoc
toolchain required.

Diagram support requires:
* the `d2` CLI
* a JDK and a `plantuml.jar` (pass `--plantuml-jar <path>` to
  `weaveback-docgen`)

The installer script installs the split binaries and, optionally, the diagram
toolchain.

## Installer script (recommended)

Python 3 is required. It is available on every supported platform.

```sh
# Clone or download the repo, then:
python3 scripts/install.py
```


With diagram support:

```sh
python3 scripts/install.py --diagrams
```


Build from source instead of downloading release binaries:

```sh
python3 scripts/install.py --source
```


Full options:

```text
--diagrams          Install D2 and a JDK (for PlantUML via --plantuml-jar)
--source            Build from source (requires cargo / Rust toolchain)
--prefix DIR        Install binary to DIR
                      default: ~/.local/bin          (Linux/macOS)
                               %LOCALAPPDATA%\Programs\weaveback  (Windows)
--version VER       Pin to a specific release, e.g. v0.4.1
```


The script detects your package manager (paru/yay, apt, dnf, brew, winget,
choco, scoop), downloads the right release assets from GitHub, and
(with `--diagrams`) installs D2 and a JDK for diagram rendering.

On Arch Linux it installs the package via `paru -S weaveback-bin` instead of
downloading release assets.

## Manual installation

### Arch Linux

```sh
paru -S weaveback-bin       # or: yay -S weaveback-bin
# with diagrams:
sudo pacman -S d2 jdk-openjdk
```


### Debian / Ubuntu

```sh
python3 scripts/install.py
# with diagrams:
sudo apt install default-jdk-headless
curl -fsSL https://d2lang.com/install.sh | sh
```


### Fedora / RHEL

```sh
python3 scripts/install.py
# with diagrams:
sudo dnf install java-latest-openjdk-headless
curl -fsSL https://d2lang.com/install.sh | sh
```


### macOS

No pre-built macOS binary is provided yet. Build from source:

```sh
cargo build --release --workspace
# with diagrams:
brew install openjdk d2
```


### Windows

Use the installer script, which downloads the current Windows `.exe` set and
updates the user `%PATH%`:

```powershell
python scripts/install.py
```


```cmd
rem with diagrams:
winget install Microsoft.OpenJDK.21
winget install Terrastruct.D2
```


### Any Linux (musl, no package manager)

Quick one-liner for the public split binaries:

```sh
base=https://github.com/giannifer7/weaveback/releases/latest/download
for bin in wb-tangle wb-query wb-serve wb-mcp; do
  curl -sL "$base/$bin-musl" -o "$HOME/.local/bin/$bin"
  chmod +x "$HOME/.local/bin/$bin"
done
```


## Pre-built binary reference

| File | Platform | Notes |
| --- | --- | --- |
| `weaveback-x86_64-linux.tar.gz` | Linux x86_64 | glibc tarball with all split binaries |
| `wb-tangle-musl`, `wb-query-musl`, `wb-serve-musl`, `wb-mcp-musl` | Linux x86_64 | public musl binaries |
| `weaveback-macro-musl`, `weaveback-tangle-musl`, `weaveback-docgen-musl` | Linux x86_64 | lower-level musl tools |
| `wb-tangle-mingw64.exe`, `wb-query-mingw64.exe`, `wb-serve-mingw64.exe`, `wb-mcp-mingw64.exe` | Windows x86_64 | public Windows binaries |
| `weaveback-macro-mingw64.exe`, `weaveback-tangle-mingw64.exe`, `weaveback-docgen-mingw64.exe` | Windows x86_64 | lower-level Windows tools |

The musl builds are fully statically linked — use them on old distros,
Alpine, or containers where glibc version is uncertain.

## Build from source

```sh
git clone https://github.com/giannifer7/weaveback
cd weaveback
cargo build --release --workspace
# binaries: target/release/wb-tangle, wb-query, wb-serve, wb-mcp, ...
```


## Nix

```sh
nix profile install github:giannifer7/weaveback
```


Or in a flake:

```nix
inputs.weaveback.url = "github:giannifer7/weaveback";
environment.systemPackages = [ inputs.weaveback.packages.x86_64-linux.default ];
```

