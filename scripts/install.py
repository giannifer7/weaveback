#!/usr/bin/env python3
"""
install.py — weaveback full-stack installer

Installs the weaveback binary and its documentation toolchain:
  - weaveback binary (from GitHub releases, or built from source)
  - Ruby + asciidoctor + rouge            (for HTML docs)
  - JDK + asciidoctor-diagram             (for PlantUML, with --diagrams)

Usage:
  python3 install.py [options]

  --diagrams          Also install JDK and asciidoctor-diagram
  --source            Build from source instead of downloading a release
  --prefix DIR        Install binary to DIR
                        default: ~/.local/bin  (Unix)
                                 %LOCALAPPDATA%\\Programs\\weaveback  (Windows)
  --version VER       Install a specific release tag (default: latest)
  --gems-only         Only (re-)install Ruby gems; skip binary and system deps
"""

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

REPO = "giannifer7/weaveback"

GEMS_BASE     = ["asciidoctor", "rouge"]
GEMS_DIAGRAMS = ["asciidoctor-diagram"]

# ── Output helpers ─────────────────────────────────────────────────────────────

def info(msg):  print(f"  {msg}")
def ok(msg):    print(f"  \u2713 {msg}")
def warn(msg):  print(f"  ! {msg}", file=sys.stderr)
def die(msg):   sys.exit(f"\nError: {msg}")


def run(cmd, *, check=True, **kwargs):
    print(f"  $ {' '.join(str(c) for c in cmd)}")
    return subprocess.run(cmd, check=check, **kwargs)


def which(name):
    return shutil.which(name) is not None


def fetch_json(url):
    req = urllib.request.Request(url, headers={"User-Agent": "weaveback-installer/1"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read())


def download(url, dest: Path):
    info(f"Downloading {url}")
    req = urllib.request.Request(url, headers={"User-Agent": "weaveback-installer/1"})
    with urllib.request.urlopen(req, timeout=120) as r, open(dest, "wb") as f:
        total = int(r.headers.get("Content-Length", 0))
        received = 0
        while chunk := r.read(65536):
            f.write(chunk)
            received += len(chunk)
            if total:
                pct = received * 100 // total
                print(f"\r  {pct:3d}%", end="", flush=True)
    print()
    ok(f"Saved {dest.name} ({received // 1024} KB)")


# ── Platform detection ─────────────────────────────────────────────────────────

def detect_platform():
    system  = platform.system()    # 'Linux', 'Darwin', 'Windows'
    machine = platform.machine()   # 'x86_64', 'aarch64', 'AMD64', ...
    arch    = "x86_64" if machine in ("x86_64", "AMD64") else machine.lower()

    pkg_manager = None
    distro_id   = ""
    distro_like = ""

    if system == "Linux":
        try:
            with open("/etc/os-release") as f:
                rel = {}
                for line in f:
                    k, _, v = line.strip().partition("=")
                    rel[k] = v.strip('"')
            distro_id   = rel.get("ID", "").lower()
            distro_like = rel.get("ID_LIKE", "").lower()
        except FileNotFoundError:
            pass

        for pm in ("paru", "yay", "pacman", "apt-get", "dnf", "zypper", "apk"):
            if which(pm):
                pkg_manager = "apt" if pm == "apt-get" else pm
                break

    elif system == "Darwin":
        if which("brew"):
            pkg_manager = "brew"

    elif system == "Windows":
        for pm in ("winget", "choco", "scoop"):
            if which(pm):
                pkg_manager = pm
                break

    return {
        "system":      system,
        "arch":        arch,
        "pkg_manager": pkg_manager,
        "distro_id":   distro_id,
        "distro_like": distro_like,
    }


# ── System dependency tables ───────────────────────────────────────────────────

_RUBY = {
    "paru":   ["ruby"],
    "yay":    ["ruby"],
    "pacman": ["ruby"],
    "apt":    ["ruby-full"],
    "dnf":    ["ruby", "ruby-devel"],
    "zypper": ["ruby"],
    "brew":   ["ruby"],
    "winget": ["RubyInstallerTeam.Ruby.3.3"],
    "choco":  ["ruby"],
    "scoop":  ["ruby"],
}

_JDK = {
    "paru":   ["jdk-openjdk"],
    "yay":    ["jdk-openjdk"],
    "pacman": ["jdk-openjdk"],
    "apt":    ["default-jdk-headless"],
    "dnf":    ["java-latest-openjdk-headless"],
    "zypper": ["java-21-openjdk-headless"],
    "brew":   ["openjdk"],
    "winget": ["Microsoft.OpenJDK.21"],
    "choco":  ["openjdk"],
    "scoop":  ["openjdk"],
}


def _pkg_install(pm, packages):
    if pm in ("paru", "yay"):
        run([pm, "-S", "--needed", "--noconfirm"] + packages)
    elif pm == "pacman":
        run(["sudo", "pacman", "-S", "--needed", "--noconfirm"] + packages)
    elif pm == "apt":
        run(["sudo", "apt-get", "install", "-y"] + packages)
    elif pm == "dnf":
        run(["sudo", "dnf", "install", "-y"] + packages)
    elif pm == "zypper":
        run(["sudo", "zypper", "install", "-y"] + packages)
    elif pm == "brew":
        run(["brew", "install"] + packages)
    elif pm == "winget":
        for pkg in packages:
            run(["winget", "install", "--accept-source-agreements",
                 "--accept-package-agreements", "-e", "--id", pkg])
    elif pm == "choco":
        run(["choco", "install", "-y"] + packages)
    elif pm == "scoop":
        run(["scoop", "install"] + packages)
    else:
        die(f"Unhandled package manager: {pm}")


def install_system_deps(pf, diagrams):
    print("\n\u2500\u2500 System packages \u2500\u2500")
    pm = pf["pkg_manager"]
    if not pm:
        warn("No supported package manager found \u2014 install Ruby (and JDK) manually.")
        return

    if which("ruby") or which("ruby3"):
        ok("Ruby already installed")
    else:
        pkgs = _RUBY.get(pm)
        if pkgs:
            _pkg_install(pm, pkgs)
        else:
            warn(f"Don\u2019t know how to install Ruby via {pm} \u2014 install manually.")

    if diagrams:
        if which("java"):
            ok("JDK already installed")
        else:
            pkgs = _JDK.get(pm)
            if pkgs:
                _pkg_install(pm, pkgs)
            else:
                warn(f"Don\u2019t know how to install JDK via {pm} \u2014 install manually.")


# ── Binary installation ────────────────────────────────────────────────────────

def _asset_name(pf):
    """Return the GitHub release asset name for this platform, or None."""
    system, arch  = pf["system"], pf["arch"]
    distro_id     = pf["distro_id"]
    distro_like   = pf["distro_like"]

    if system == "Windows":
        return "weaveback.exe"

    if system == "Darwin":
        return None   # no macOS binaries yet

    if system == "Linux" and arch == "x86_64":
        fedora_ids = {"fedora", "rhel", "centos", "rocky", "almalinux"}
        if distro_id in fedora_ids or any(f in distro_like for f in ("fedora", "rhel")):
            return "weaveback-fedora"
        if distro_id in ("debian", "ubuntu", "linuxmint", "pop") \
                or "debian" in distro_like or "ubuntu" in distro_like:
            # prefer .deb — look for it in assets by suffix
            return ".deb"
        return "weaveback-glibc"

    return None


def _get_release(version):
    url = f"https://api.github.com/repos/{REPO}/releases/" + \
          ("latest" if version is None else f"tags/{version}")
    try:
        return fetch_json(url)
    except Exception as exc:
        die(f"Could not fetch release info from GitHub: {exc}")


def install_binary_from_release(pf, prefix: Path, version=None):
    print("\n\u2500\u2500 weaveback binary \u2500\u2500")

    # Arch: prefer AUR
    if pf["distro_id"] in ("arch", "manjaro", "endeavouros", "garuda") \
            and pf["pkg_manager"] in ("paru", "yay"):
        pm = pf["pkg_manager"]
        info(f"Arch Linux \u2014 installing via AUR ({pm} -S weaveback-bin)")
        run([pm, "-S", "--needed", "--noconfirm", "weaveback-bin"])
        return

    want = _asset_name(pf)
    if want is None:
        if pf["system"] == "Darwin":
            warn("No macOS binary in releases \u2014 build from source:")
            warn("  cargo install --git https://github.com/giannifer7/weaveback weaveback")
        else:
            warn(f"No pre-built binary for {pf['system']}/{pf['arch']} \u2014 use --source.")
        return

    release = _get_release(version)
    tag     = release["tag_name"]
    assets  = {a["name"]: a["browser_download_url"] for a in release.get("assets", [])}

    # .deb match by suffix
    if want == ".deb":
        deb_assets = [n for n in assets if n.endswith(".deb")]
        if not deb_assets:
            warn(f"No .deb asset found in release {tag} \u2014 falling back to glibc binary")
            want = "weaveback-glibc"
        else:
            want = deb_assets[0]

    if want not in assets:
        die(f"Expected asset \u2018{want}\u2019 not found in release {tag}.\n"
            f"Available: {sorted(assets)}")

    url = assets[want]
    prefix.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as tmp:
        dest = Path(tmp) / want
        download(url, dest)

        if want.endswith(".deb"):
            run(["sudo", "apt-get", "install", "-y", str(dest)])

        elif want == "weaveback-fedora":
            run(["sudo", "dnf", "install", "-y", str(dest)])

        elif want == "weaveback.exe":
            target = prefix / "weaveback.exe"
            shutil.copy2(dest, target)
            ok(f"Installed to {target}")
            _windows_add_to_path(prefix)

        else:   # plain binary (glibc / musl)
            target = prefix / "weaveback"
            shutil.copy2(dest, target)
            target.chmod(0o755)
            ok(f"Installed to {target}")
            _unix_path_hint(prefix)


def install_from_source(prefix: Path):
    print("\n\u2500\u2500 Building from source \u2500\u2500")
    if not which("cargo"):
        die("cargo not found \u2014 install Rust from https://rustup.rs")
    run(["cargo", "build", "--release", "--package", "weaveback"])
    src = Path("target/release") / ("weaveback.exe" if platform.system() == "Windows" else "weaveback")
    if not src.exists():
        die(f"Build succeeded but binary not found at {src}")
    prefix.mkdir(parents=True, exist_ok=True)
    target = prefix / src.name
    shutil.copy2(src, target)
    if platform.system() != "Windows":
        target.chmod(0o755)
        _unix_path_hint(prefix)
    else:
        _windows_add_to_path(prefix)
    ok(f"Installed to {target}")


def _unix_path_hint(prefix: Path):
    try:
        in_path = any(
            Path(p).resolve() == prefix.resolve()
            for p in os.environ.get("PATH", "").split(":")
            if p
        )
    except Exception:
        in_path = False
    if not in_path:
        warn(f"{prefix} is not in PATH.")
        warn(f"  Add to your shell profile:  export PATH=\"$PATH:{prefix}\"")


def _windows_add_to_path(prefix: Path):
    try:
        import winreg  # type: ignore[import-untyped]
        key = winreg.OpenKey(
            winreg.HKEY_CURRENT_USER, "Environment",
            access=winreg.KEY_READ | winreg.KEY_WRITE,
        )
        try:
            current, _ = winreg.QueryValueEx(key, "Path")
        except FileNotFoundError:
            current = ""
        s = str(prefix)
        if s.lower() not in current.lower():
            winreg.SetValueEx(key, "Path", 0, winreg.REG_EXPAND_SZ,
                              f"{current};{s}" if current else s)
            ok(f"Added {prefix} to user PATH \u2014 restart your terminal")
        else:
            ok(f"{prefix} already in PATH")
        winreg.CloseKey(key)
    except Exception as exc:
        warn(f"Could not update PATH automatically: {exc}")
        warn(f"  Add manually: {prefix}")


# ── Ruby gems ─────────────────────────────────────────────────────────────────

def install_gems(diagrams):
    print("\n\u2500\u2500 Ruby gems \u2500\u2500")
    gem = shutil.which("gem") or shutil.which("gem3")
    if not gem:
        die("gem not found \u2014 Ruby installation may be incomplete or not in PATH")
    gems = GEMS_BASE + (GEMS_DIAGRAMS if diagrams else [])
    cmd  = [gem, "install"] + gems
    if platform.system() != "Windows":
        cmd.append("--user-install")
    run(cmd)
    ok(f"Installed: {', '.join(gems)}")
    if platform.system() != "Windows":
        # gem --user-install puts binaries in ~/.local/share/gem/ruby/*/bin or similar
        result = subprocess.run([gem, "env", "bindir"], capture_output=True, text=True)
        gem_bin = result.stdout.strip()
        if gem_bin and not which("asciidoctor"):
            warn(f"Add gem bindir to PATH:  export PATH=\"$PATH:{gem_bin}\"")


# ── Verify ────────────────────────────────────────────────────────────────────

def verify():
    print("\n\u2500\u2500 Verification \u2500\u2500")
    checks = [
        ("weaveback",  ["weaveback", "--version"]),
        ("asciidoctor", ["asciidoctor", "--version"]),
    ]
    all_ok = True
    for name, cmd in checks:
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
            if r.returncode == 0:
                ok(f"{name}: {r.stdout.splitlines()[0][:72]}")
            else:
                warn(f"{name}: exited with code {r.returncode}")
                all_ok = False
        except FileNotFoundError:
            warn(f"{name}: not found in PATH")
            all_ok = False
    return all_ok


# ── Main ──────────────────────────────────────────────────────────────────────

def default_prefix():
    if platform.system() == "Windows":
        base = os.environ.get("LOCALAPPDATA", str(Path.home() / "AppData" / "Local"))
        return Path(base) / "Programs" / "weaveback"
    return Path.home() / ".local" / "bin"


def main():
    ap = argparse.ArgumentParser(
        description="Install weaveback and its documentation toolchain.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""\
Examples:
  python3 install.py                   # core install (binary + asciidoctor + rouge)
  python3 install.py --diagrams        # also install JDK + asciidoctor-diagram
  python3 install.py --source          # build from source (needs cargo)
  python3 install.py --gems-only       # (re-)install Ruby gems only
  python3 install.py --version v0.4.1  # pin to a specific release
""",
    )
    ap.add_argument("--diagrams",  action="store_true",
                    help="Install JDK and asciidoctor-diagram (PlantUML support)")
    ap.add_argument("--source",    action="store_true",
                    help="Build weaveback from source (requires Rust/cargo)")
    ap.add_argument("--prefix",    type=Path, default=None, metavar="DIR",
                    help="Directory to install the weaveback binary")
    ap.add_argument("--version",   default=None, metavar="VER",
                    help="Release tag to install, e.g. v0.4.1 (default: latest)")
    ap.add_argument("--gems-only", action="store_true",
                    help="Only (re-)install Ruby gems; skip binary and system deps")
    args = ap.parse_args()

    prefix = args.prefix or default_prefix()
    pf     = detect_platform()

    print(f"Platform : {pf['system']} {pf['arch']}"
          + (f"  ({pf['distro_id']})" if pf["distro_id"] else ""))
    print(f"Pkg mgr  : {pf['pkg_manager'] or 'none detected'}")
    print(f"Prefix   : {prefix}")
    print(f"Diagrams : {'yes' if args.diagrams else 'no'}")

    if not args.gems_only:
        install_system_deps(pf, args.diagrams)

        if args.source:
            install_from_source(prefix)
        else:
            install_binary_from_release(pf, prefix, version=args.version)

    install_gems(args.diagrams)

    all_good = verify()
    print()
    if all_good:
        print("Installation complete.")
    else:
        print("Installation finished with warnings \u2014 check PATH messages above.")


if __name__ == "__main__":
    main()
