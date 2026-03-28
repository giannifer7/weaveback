{
  description = "Bidirectional literate programming toolchain (noweb, macros, source tracing)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      pkgs    = nixpkgs.legacyPackages.x86_64-linux;
      version = "0.4.1";
      base    = "https://github.com/giannifer7/weaveback/releases/download/v${version}";
    in {
      packages.x86_64-linux.default = pkgs.stdenv.mkDerivation {
        pname   = "weaveback";
        inherit version;
        src     = pkgs.fetchurl { url = "${base}/weaveback-musl"; sha256 = "sha256-MbdMF40MNPsbyaGsoYbUJcwMBDoxVKIAzZ8XhllKs9M="; };
        dontUnpack   = true;
        installPhase = "install -Dm755 $src $out/bin/weaveback";
      };
    };
}
