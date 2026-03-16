# Installing azadi

## Arch Linux

```bash
paru -S azadi-bin   # or: yay -S azadi-bin
```

## Nix

```bash
nix profile install github:giannifer7/azadi
```

Or in a flake input:

```nix
inputs.azadi.url = "github:giannifer7/azadi";
environment.systemPackages = [ inputs.azadi.packages.x86_64-linux.default ];
```

## Pre-built binaries

Download from the [latest release](https://github.com/giannifer7/azadi/releases/latest):

| File | Platform | Notes |
|------|----------|-------|
| `azadi-x86_64-linux.tar.gz` | Linux x86_64 | glibc — tarball with all three binaries |
| `azadi-glibc` / `azadi-macros-glibc` / `azadi-noweb-glibc` | Linux x86_64 | glibc, individual binaries |
| `azadi-musl` / `azadi-macros-musl` / `azadi-noweb-musl` | Linux x86_64 | musl (fully static) |
| `azadi-fedora` / `azadi-macros-fedora` / `azadi-noweb-fedora` | Fedora/RHEL | |
| `*.deb` | Debian/Ubuntu | `sudo dpkg -i` |
| `*.rpm` | Fedora/RHEL | `sudo rpm -i` |
| `azadi.exe` / `azadi-macros.exe` / `azadi-noweb.exe` | Windows x86_64 | native build |
| `azadi-mingw64.exe` / ... | Windows x86_64 | MinGW cross-compiled |

### musl vs glibc

The musl builds are fully statically linked — no shared library dependencies.
Use them on any Linux distro regardless of glibc version (old RHEL/CentOS,
Alpine, containers).

The glibc builds are dynamically linked and are the better choice on standard
Debian/Ubuntu/Fedora systems where glibc's runtime is already present.

### Quick install (musl, no package manager)

```bash
curl -sL https://github.com/giannifer7/azadi/releases/latest/download/azadi-musl \
     -o /usr/local/bin/azadi && chmod +x /usr/local/bin/azadi
```

## Build from source

```bash
git clone https://github.com/giannifer7/azadi
cd azadi
cargo build --release
# binaries: target/release/azadi  target/release/azadi-macros  target/release/azadi-noweb
```
