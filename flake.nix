{
  description = "weaveback — literate programming toolchain";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      pkgs    = nixpkgs.legacyPackages.x86_64-linux;
      version = "0.3.5";
      base    = "https://github.com/giannifer7/weaveback/releases/download/v${version}";
    in {
      packages.x86_64-linux.default = pkgs.stdenv.mkDerivation {
        pname   = "weaveback";
        inherit version;
        src     = pkgs.fetchurl { url = "${base}/weaveback-musl"; sha256 = "sha256-LupCYyUliFvMKGxK6rmQY3DxsaNhbCdshAkCpTAxE7o="; };
        dontUnpack   = true;
        installPhase = "install -Dm755 $src $out/bin/weaveback";
      };
    };
}
