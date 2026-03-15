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
import shutil
import subprocess
import time
import urllib.error
import urllib.request
from pathlib import Path

PACKAGING   = Path(__file__).parent
REPO_ROOT   = PACKAGING.parent

MAINTAINER  = "Gianni Ferrarotti <gianni.ferrarotti@gmail.com>"
DESCRIPTION = "azadi — literate programming toolchain"
HOMEPAGE    = "https://github.com/giannifer7/azadi"
RELEASES    = f"{HOMEPAGE}/releases/download"
REPO        = "giannifer7/azadi"
API         = "https://api.github.com"

NEEDED_ASSETS = ["azadi-x86_64-linux.tar.gz", "azadi-musl"]


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


def wait_for_release(version: str, token: str, timeout: int = 600, poll: int = 20) -> dict:
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
    source = f"{RELEASES}/v${{pkgver}}/azadi-x86_64-linux.tar.gz"
    return f"""\
# Maintainer: {MAINTAINER}
#
# AUR package for azadi — literate programming toolchain.
# Installs the azadi binary. The separate azadi-macros and azadi-noweb
# binaries are available in the GitHub release for advanced pipeline use.
#
# Regenerate after each release:
#   python packaging/update_release.py <version>

pkgname=azadi-bin
pkgver={version}
pkgrel=1
pkgdesc="{DESCRIPTION}"
url="{HOMEPAGE}"
license=('MIT' 'Apache-2.0')
arch=('x86_64')
provides=('azadi')
conflicts=('azadi' 'azadi-git')
depends=('gcc-libs' 'glibc')
options=('!debug')
source=("azadi-x86_64-linux.tar.gz::{source}")
sha256sums=('{tarball_sha256}')

package() {{
    install -Dm755 azadi -t "${{pkgdir}}/usr/bin"
}}
"""


def flake(version: str, sri_azadi: str) -> str:
    base = f"{RELEASES}/v${{version}}"
    return f"""\
{{
  description = "{DESCRIPTION}";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = {{ self, nixpkgs }}:
    let
      pkgs    = nixpkgs.legacyPackages.x86_64-linux;
      version = "{version}";
      base    = "{base}";
    in {{
      packages.x86_64-linux.default = pkgs.stdenv.mkDerivation {{
        pname   = "azadi";
        inherit version;
        src     = pkgs.fetchurl {{ url = "${{base}}/azadi-musl"; sha256 = "{sri_azadi}"; }};
        dontUnpack   = true;
        installPhase = "install -Dm755 $src $out/bin/azadi";
      }};
    }};
}}
"""


# ── version (read from Cargo.toml — the SSOT) ────────────────────────────────

def read_cargo_version() -> str:
    import re
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
    parser.add_argument("--dry-run", action="store_true",
                        help="Write files but skip all git/AUR steps")
    args = parser.parse_args()

    version = (args.version.lstrip("v") if args.version else read_cargo_version())
    aur_dir = REPO_ROOT.parent / "aur-azadi-bin"
    token   = gh_token()

    print(f"Releasing v{version}...")

    if args.tag:
        run(["cargo", "build"], cwd=REPO_ROOT)  # refresh Cargo.lock
        run(["git", "add", "Cargo.lock"], cwd=REPO_ROOT)
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

    tarball   = assets["azadi-x86_64-linux.tar.gz"]
    azadi_bin = assets["azadi-musl"]

    (PACKAGING / "PKGBUILD").write_text(pkgbuild(version, sha256_hex(tarball)))
    print("  Written packaging/PKGBUILD")

    (REPO_ROOT / "flake.nix").write_text(flake(version, sha256_sri(azadi_bin)))
    print("  Written flake.nix")

    if args.dry_run:
        print("\nDry run — skipping git and AUR steps.")
        return

    print("\nCommitting azadi repo...")
    run(["git", "add", "flake.nix", "packaging/PKGBUILD"], cwd=REPO_ROOT)
    run(["git", "commit", "-m", f"chore: release v{version}"], cwd=REPO_ROOT)
    run(["git", "push", "origin", "main"], cwd=REPO_ROOT)

    print("\nUpdating AUR package...")
    shutil.copy(PACKAGING / "PKGBUILD", aur_dir / "PKGBUILD")
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
