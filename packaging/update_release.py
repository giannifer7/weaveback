#!/usr/bin/env python3
"""Generate PKGBUILD and flake.nix for a new release, then publish everywhere.

Version is read from [workspace.package] in Cargo.toml (the single source of
truth).  Bump it there first, then run this script.

Typical usage:

  # Edit Cargo.toml version, then tag + full publish:
  just tag

  # Tag already pushed, CI already done — just publish:
  just update-release

Requires GH_TOKEN or GITHUB_TOKEN in the environment (or gh auth login).
"""

import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import time
import urllib.error
import urllib.request
from pathlib import Path

PACKAGING   = Path(__file__).parent
REPO_ROOT   = PACKAGING.parent

MAINTAINER  = "Gianni Ferrarotti <gianni.ferrarotti@gmail.com>"
DESCRIPTION = "Bidirectional literate programming toolchain (noweb, macros, source tracing)"
HOMEPAGE    = "https://github.com/giannifer7/weaveback"
RELEASES    = f"{HOMEPAGE}/releases/download"
REPO        = "giannifer7/weaveback"
API         = "https://api.github.com"

NEEDED_ASSETS = [
    "weaveback-x86_64-linux.tar.gz",
    "weaveback-musl",
    "weaveback-macro-musl",
    "weaveback-tangle-musl",
    "weaveback-docgen-musl",
]


# ── auth ───────────────────────────────────────────────────────────────────────

def gh_token() -> str:
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if token:
        return token
    # Fall back to gh's stored credentials
    result = subprocess.run(["gh", "auth", "token"], capture_output=True, text=True)
    if result.returncode == 0 and result.stdout.strip():
        return result.stdout.strip()
    raise SystemExit(
        "No GitHub token found. Set GH_TOKEN/GITHUB_TOKEN or run 'gh auth login'."
    )


# ── GitHub API ─────────────────────────────────────────────────────────────────

def api_get(path: str, token: str) -> dict:
    req = urllib.request.Request(
        f"{API}{path}",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(req, timeout=15) as r:
        return json.loads(r.read())


def download_asset(url: str, token: str) -> bytes:
    req = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/octet-stream",
        },
    )
    with urllib.request.urlopen(req, timeout=120) as r:
        return r.read()


def wait_for_release(version: str, token: str, timeout: int = 1800, poll: int = 20) -> dict:
    """Poll the releases API until all needed assets exist; return the release data."""
    tag = f"v{version}"
    deadline = time.monotonic() + timeout
    print(f"Waiting for GitHub release {tag} assets", end="", flush=True)
    while time.monotonic() < deadline:
        try:
            data = api_get(f"/repos/{REPO}/releases/tags/{tag}", token)
            names = {a["name"] for a in data.get("assets", [])}
            if all(a in names for a in NEEDED_ASSETS):
                print(" ready.")
                return data
        except (urllib.error.URLError, TimeoutError):
            pass  # network blip or release not yet published
        print(".", end="", flush=True)
        time.sleep(poll)
    raise SystemExit(f"\nTimed out after {timeout}s waiting for release assets.")


def fetch_assets(release: dict, token: str) -> dict[str, bytes]:
    """Download needed assets from a release and return their raw bytes."""
    by_name = {a["name"]: a["url"] for a in release["assets"]}
    result = {}
    for name in NEEDED_ASSETS:
        print(f"  Downloading {name}...")
        result[name] = download_asset(by_name[name], token)
    return result


# ── hashing ────────────────────────────────────────────────────────────────────

def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_sri(data: bytes) -> str:
    return "sha256-" + base64.b64encode(hashlib.sha256(data).digest()).decode()


# ── file generators ────────────────────────────────────────────────────────────

def pkgbuild(version: str, tarball_sha256: str) -> str:
    source = f"{RELEASES}/v${{pkgver}}/weaveback-x86_64-linux.tar.gz"
    return f"""\
# Maintainer: {MAINTAINER}
#
# AUR package for weaveback — bidirectional literate programming toolchain.
# Installs weaveback (combined), weaveback-macro, and weaveback-tangle from
# the pre-built x86_64 tarball on the GitHub release.
#
# Regenerate after each release:
#   python packaging/update_release.py <version>

pkgname=weaveback-bin
pkgver={version}
pkgrel=1
pkgdesc="{DESCRIPTION}"
url="{HOMEPAGE}"
license=('0BSD' 'MIT' 'Apache-2.0')
arch=('x86_64')
provides=('weaveback')
conflicts=('weaveback' 'weaveback-git')
depends=('gcc-libs' 'glibc')
options=('!debug')
source=("weaveback-x86_64-linux.tar.gz::{source}")
sha256sums=('{tarball_sha256}')

package() {{
    install -Dm755 weaveback        -t "${{pkgdir}}/usr/bin"
    install -Dm755 weaveback-macro  -t "${{pkgdir}}/usr/bin"
    install -Dm755 weaveback-tangle -t "${{pkgdir}}/usr/bin"
    install -Dm755 weaveback-docgen -t "${{pkgdir}}/usr/bin"
}}
"""


def flake(version: str, sri: dict) -> str:
    base = f"{RELEASES}/v${{version}}"
    return f"""\
{{
  description = "{DESCRIPTION}";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = {{ self, nixpkgs }}:
    let
      lib     = nixpkgs.lib;
      version = "{version}";
      base    = "{base}";

      # Pre-built musl binaries are x86_64-linux only.
      # The devShell works on all common systems.
      devSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forEachDevSystem = f: lib.genAttrs devSystems (s: f nixpkgs.legacyPackages.${{s}});

      linuxPkgs = nixpkgs.legacyPackages.x86_64-linux;

      releaseBin = {{ pname, sha256 }}: linuxPkgs.stdenv.mkDerivation {{
        inherit pname version;
        src        = linuxPkgs.fetchurl {{ url = "${{base}}/${{pname}}-musl"; inherit sha256; }};
        dontUnpack = true;
        installPhase = "install -Dm755 $src $out/bin/${{pname}}";
      }};

    in {{

      packages.x86_64-linux = {{
        default          = releaseBin {{ pname = "weaveback";         sha256 = "{sri['weaveback-musl']}"; }};
        weaveback-macro  = releaseBin {{ pname = "weaveback-macro";   sha256 = "{sri['weaveback-macro-musl']}"; }};
        weaveback-tangle = releaseBin {{ pname = "weaveback-tangle";  sha256 = "{sri['weaveback-tangle-musl']}"; }};
        weaveback-docgen = releaseBin {{ pname = "weaveback-docgen";  sha256 = "{sri['weaveback-docgen-musl']}"; }};
      }};

      # Full documentation + development toolchain.
      # Usage: nix develop
      devShells = forEachDevSystem (pkgs: {{
        default = pkgs.mkShell {{
          buildInputs = with pkgs; [
            just         # task runner
            plantuml     # UML diagrams via --plantuml-jar (brings JDK)
            nodejs       # TypeScript bundle for the serve UI
            python3      # scripts/install.py, packaging scripts
            git
          ];
          shellHook = ''
            echo ""
            echo "weaveback dev shell — available recipes:"
            echo "  just tangle     regenerate source files from .adoc"
            echo "  just docs       render HTML documentation"
            echo "  just serve      live-reload server with inline editor"
            echo "  just test       run all tests"
            echo ""
          '';
        }};
      }});
    }};
}}
"""


# ── version (read from Cargo.toml — the SSOT) ────────────────────────────────

def read_cargo_version() -> str:
    text = (REPO_ROOT / "Cargo.toml").read_text()
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not m:
        raise SystemExit("Could not read version from Cargo.toml")
    return m.group(1)


# ── subprocess helpers (git, makepkg only) ─────────────────────────────────────

def run(args: list, cwd: Path) -> None:
    subprocess.run(args, cwd=cwd, check=True)


# ── main ───────────────────────────────────────────────────────────────────────

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("version", nargs="?",
                        help="Release version (default: read from Cargo.toml)")
    parser.add_argument("--tag", action="store_true",
                        help="Commit Cargo.lock, push the git tag, then wait for CI")
    parser.add_argument("--retag", action="store_true",
                        help="Delete existing tag, then do --tag (re-triggers CI)")
    parser.add_argument("--dry-run", action="store_true",
                        help="Write files but skip all git/AUR steps")
    args = parser.parse_args()

    version = (args.version.lstrip("v") if args.version else read_cargo_version())
    aur_dir = REPO_ROOT.parent / "aur-weaveback-bin"
    token   = gh_token()

    print(f"Releasing v{version}...")

    if args.retag:
        tag = f"v{version}"
        subprocess.run(["git", "push", "--delete", "origin", tag], cwd=REPO_ROOT)
        subprocess.run(["git", "tag", "-d", tag], cwd=REPO_ROOT)
        args.tag = True

    if args.tag:
        run(["cargo", "build"], cwd=REPO_ROOT)  # refresh Cargo.lock
        run(["git", "add", "Cargo.toml", "Cargo.lock"], cwd=REPO_ROOT)
        # commit only if there's something staged
        result = subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=REPO_ROOT)
        if result.returncode != 0:
            run(["git", "commit", "-m", f"chore: release v{version}"], cwd=REPO_ROOT)
            run(["git", "push", "origin", "main"], cwd=REPO_ROOT)
        print(f"Tagging v{version}...")
        run(["git", "tag", "-a", f"v{version}", "-m", f"v{version}"], cwd=REPO_ROOT)
        run(["git", "push", "origin", f"v{version}"], cwd=REPO_ROOT)

    release = wait_for_release(version, token)
    assets  = fetch_assets(release, token)

    tarball = assets["weaveback-x86_64-linux.tar.gz"]
    sri     = {name: sha256_sri(data) for name, data in assets.items() if name != "weaveback-x86_64-linux.tar.gz"}

    (aur_dir / "PKGBUILD").write_text(pkgbuild(version, sha256_hex(tarball)))
    print("  Written aur-weaveback-bin/PKGBUILD")

    (REPO_ROOT / "flake.nix").write_text(flake(version, sri))
    print("  Written flake.nix")

    if args.dry_run:
        print("\nDry run — skipping git and AUR steps.")
        return

    print("\nCommitting weaveback repo...")
    run(["git", "add", "flake.nix"], cwd=REPO_ROOT)
    has_changes = subprocess.run(
        ["git", "diff", "--cached", "--quiet"], cwd=REPO_ROOT
    ).returncode != 0
    if has_changes:
        run(["git", "commit", "-m", f"chore: release v{version}"], cwd=REPO_ROOT)
        run(["git", "push", "origin", "main"], cwd=REPO_ROOT)
    else:
        print("  Nothing changed — skipping commit and push.")

    print("\nUpdating AUR package...")
    srcinfo = subprocess.run(
        ["makepkg", "--printsrcinfo"],
        cwd=aur_dir, check=True, capture_output=True, text=True,
    ).stdout
    (aur_dir / ".SRCINFO").write_text(srcinfo)
    run(["git", "add", "PKGBUILD", ".SRCINFO"], cwd=aur_dir)
    run(["git", "commit", "-m", f"Release {version}"], cwd=aur_dir)
    run(["git", "push"], cwd=aur_dir)

    print(f"\nDone. Released v{version}.")


if __name__ == "__main__":
    main()
