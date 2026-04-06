{
  description = "Bidirectional literate programming toolchain (noweb, macros, source tracing)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      lib     = nixpkgs.lib;
      version = "0.10.0";
      base    = "https://github.com/giannifer7/weaveback/releases/download/v${version}";

      # Pre-built musl binaries are x86_64-linux only.
      # They package the CLI tools, which are a good fit for Nix consumption.
      #
      # The PyO3 extension is intentionally *not* exposed here as a pre-built
      # Nix package because it is Python-ABI- and platform-specific: for that
      # side we want wheels or a source build inside a dev shell, not a single
      # "universal musl" artifact.
      #
      # The devShell works on all common systems and includes the Python build
      # and lint tools needed for python/weaveback-agent and crates/weaveback-py.
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
        default          = releaseBin { pname = "weaveback";         sha256 = "sha256-ZhPTC6k1YBDhbH9JqwkT7P+4aOOVPY4MKS6fVET603U="; };
        weaveback-macro  = releaseBin { pname = "weaveback-macro";   sha256 = "sha256-6CL/Z8mcGL4d2NOOW0VRy+zXh1J8EX0ehUfrVdbjN3o="; };
        weaveback-tangle = releaseBin { pname = "weaveback-tangle";  sha256 = "sha256-MQjLpROiEVERJqUwqFKtaur4+D+V312uFdtUmfD2wY8="; };
        weaveback-docgen = releaseBin { pname = "weaveback-docgen";  sha256 = "sha256-3ymyGAUPiJ/9q1y6C7BIFfhk+qc3mY6NMjJY11kx5xM="; };
      };

      # Full documentation + development toolchain.
      # Usage: nix develop
      devShells = forEachDevSystem (pkgs: {
        default = pkgs.mkShell {
          buildInputs = with pkgs; [
            just         # task runner
            plantuml     # UML diagrams via --plantuml-jar (brings JDK)
            nodejs       # TypeScript bundle for the serve UI
            python3      # packaging scripts and Python project runtime
            uv           # Python package / tool runner
            maturin      # PyO3 build frontend
            ruff         # Python formatter / linter
            mypy         # Python static typing
            pylint       # Python lint baseline
            git
          ];
          shellHook = ''
            echo ""
            echo "weaveback dev shell — available recipes:"
            echo "  just tangle     regenerate source files from .adoc"
            echo "  just docs       render HTML documentation"
            echo "  just serve      live-reload server with inline editor"
            echo "  just test       run all tests"
            echo "  just py-check   build + lint + test the Python agent bridge"
            if [ -f python/weaveback-agent/pyproject.toml ]; then
              echo "  syncing python/weaveback-agent with uv..."
              if ! uv sync --project python/weaveback-agent --all-groups; then
                echo "  warning: uv sync failed; continuing with the shell environment"
              fi
            fi
            echo ""
          '';
        };
      });
    };
}
