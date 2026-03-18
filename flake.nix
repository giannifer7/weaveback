{
  description = "azadi — literate programming toolchain";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      pkgs    = nixpkgs.legacyPackages.x86_64-linux;
      version = "0.3.1";
      base    = "https://github.com/giannifer7/azadi/releases/download/v${version}";
    in {
      packages.x86_64-linux.default = pkgs.stdenv.mkDerivation {
        pname   = "azadi";
        inherit version;
        src     = pkgs.fetchurl { url = "${base}/azadi-musl"; sha256 = "sha256-2vF/4xTWt6dIkyM5cws1huhpoCKhZ+1pFl3C5soXhCc="; };
        dontUnpack   = true;
        installPhase = "install -Dm755 $src $out/bin/azadi";
      };
    };
}
