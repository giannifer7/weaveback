{
  description = "azadi — literate programming toolchain";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      pkgs    = nixpkgs.legacyPackages.x86_64-linux;
      version = "0.3.0";
      base    = "https://github.com/giannifer7/azadi/releases/download/v${version}";
    in {
      packages.x86_64-linux.default = pkgs.stdenv.mkDerivation {
        pname   = "azadi";
        inherit version;
        src     = pkgs.fetchurl { url = "${base}/azadi-musl"; sha256 = "sha256-NR8Yl88BF+8f4JPAuYiU/rWNWEKDX+X5VBey8hR1yO0="; };
        dontUnpack   = true;
        installPhase = "install -Dm755 $src $out/bin/azadi";
      };
    };
}
