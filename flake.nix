{
  description = "Bidirectional literate programming toolchain (noweb, macros, source tracing)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      pkgs    = nixpkgs.legacyPackages.x86_64-linux;
      version = "0.3.7";
      base    = "https://github.com/giannifer7/weaveback/releases/download/v${version}";
    in {
      packages.x86_64-linux.default = pkgs.stdenv.mkDerivation {
        pname   = "weaveback";
        inherit version;
        src     = pkgs.fetchurl { url = "${base}/weaveback-musl"; sha256 = "sha256-DWNrpx/IzcRRV2vi3TmHOQqdIyVM9lg26WiwmrZZ3aY="; };
        dontUnpack   = true;
        installPhase = "install -Dm755 $src $out/bin/weaveback";
      };
    };
}
