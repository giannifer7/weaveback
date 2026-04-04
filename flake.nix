{
  description = "Bidirectional literate programming toolchain (noweb, macros, source tracing)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      lib     = nixpkgs.lib;
      version = "0.9.2";
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
        default          = releaseBin { pname = "weaveback";         sha256 = "sha256-fzfWNQtoFmUBnBBobM4i/eOmwEojPdpF0I+QBAhJjoY="; };
        weaveback-macro  = releaseBin { pname = "weaveback-macro";   sha256 = "sha256-y7SMdZkEUExOXM1dgJ/nD7VbkmPk+RyStJN8IpTG5rU="; };
        weaveback-tangle = releaseBin { pname = "weaveback-tangle";  sha256 = "sha256-c1vqnXlg8o7lzHZFmI9PLZpwYegrXBPsWicLPQr4TfA="; };
        weaveback-docgen = releaseBin { pname = "weaveback-docgen";  sha256 = "sha256-jAMDh9SEAAl4O9fjUYP9AQOP04ucJJie+HRrJD1aDcY="; };
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
