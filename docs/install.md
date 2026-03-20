# Installing weaveback

## Arch Linux

```bash
paru -S weaveback-bin   # or: yay -S weaveback-bin
```

## Nix

```bash
nix profile install github:giannifer7/weaveback
```

Or in a flake input:

```nix
inputs.weaveback.url = "github:giannifer7/weaveback";
environment.systemPackages = [ inputs.weaveback.packages.x86_64-linux.default ];
```

## Pre-built binaries

Download from the [latest release](https://github.com/giannifer7/weaveback/releases/latest):

| File | Platform | Notes |
|------|----------|-------|
| `weaveback-x86_64-linux.tar.gz` | Linux x86_64 | glibc — tarball with all three binaries |
| `weaveback-glibc` / `weaveback-macro-glibc` / `weaveback-tangle-glibc` | Linux x86_64 | glibc, individual binaries |
| `weaveback-musl` / `weaveback-macro-musl` / `weaveback-tangle-musl` | Linux x86_64 | musl (fully static) |
| `weaveback-fedora` / `weaveback-macro-fedora` / `weaveback-tangle-fedora` | Fedora/RHEL | |
| `*.deb` | Debian/Ubuntu | `sudo dpkg -i` |
| `*.rpm` | Fedora/RHEL | `sudo rpm -i` |
| `weaveback.exe` / `weaveback-macro.exe` / `weaveback-tangle.exe` | Windows x86_64 | native build |
| `weaveback-mingw64.exe` / ... | Windows x86_64 | MinGW cross-compiled |

### musl vs glibc

The musl builds are fully statically linked — no shared library dependencies.
Use them on any Linux distro regardless of glibc version (old RHEL/CentOS,
Alpine, containers).

The glibc builds are dynamically linked and are the better choice on standard
Debian/Ubuntu/Fedora systems where glibc's runtime is already present.

### Quick install (musl, no package manager)

```bash
curl -sL https://github.com/giannifer7/weaveback/releases/latest/download/weaveback-musl \
     -o /usr/local/bin/weaveback && chmod +x /usr/local/bin/weaveback
```

## Build from source

```bash
git clone https://github.com/giannifer7/weaveback
cd weaveback
cargo build --release
# binaries: target/release/weaveback  target/release/weaveback-macro  target/release/weaveback-tangle
```
