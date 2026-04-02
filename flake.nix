{
  description = "Bidirectional literate programming toolchain (noweb, macros, source tracing)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      lib     = nixpkgs.lib;
      version = "0.8.0";
      base    = "https://github.com/giannifer7/weaveback/releases/download/v${version}";

      # Pre-built musl binaries are x86_64-linux only.
      # The devShell works on all common systems.
      devSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forEachDevSystem = f: lib.genAttrs devSystems (s: f nixpkgs.legacyPackages.${s});

      linuxPkgs = nixpkgs.legacyPackages.x86_64-linux;

      releaseBin = { pname, sha256 }: linuxPkgs.stdenv.mkDerivation {
        inherit pname version;
        src        = linuxPkgs.fetchurl { url = "${base}/${pname}-musl"; inherit sha256; };
        dontUnpack = true;
        installPhase = "install -Dm755 $src $out/bin/${pname}";
      };

    in {

      packages.x86_64-linux = {
        default          = releaseBin { pname = "weaveback";         sha256 = "sha256-+yW1ftR+wjHEzQWs/faNJ6P+YQw2W8dQu09h60i9Ubo="; };
        weaveback-macro  = releaseBin { pname = "weaveback-macro";   sha256 = "sha256-E1TJDn7T/Tmlk8iBh3Ssa9ANjrUF8NUZKN+38M6Ouiw="; };
        weaveback-tangle = releaseBin { pname = "weaveback-tangle";  sha256 = "sha256-SyGek/hXOfwyoxWRirLQ6yDJxI/XnhEyTn+BpjqtJi8="; };
        weaveback-docgen = releaseBin { pname = "weaveback-docgen";  sha256 = "sha256-iPIBLb0VD9CpNc+y/wWxhybiwNfqcMnXOrPLuWA3/Ls="; };
      };

      # Full documentation + development toolchain.
      # Usage: nix develop
      devShells = forEachDevSystem (pkgs: {
        default = pkgs.mkShell {
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
        };
      });
    };
}
