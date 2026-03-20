#!/usr/bin/env python3
# editors/helix/install.py
#
# Installs the weaveback grammar and queries for Helix.
# Run once after cloning, and again after grammar changes.
#
# Usage:  python3 editors/helix/install.py

import os
import shutil
import subprocess
import sys
from pathlib import Path

script_dir = Path(__file__).resolve().parent
grammar_dir = script_dir.parent.parent          # tree-sitter-weaveback/

xdg = os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")
helix_rt    = Path(xdg) / "helix" / "runtime"
queries_dir = helix_rt / "queries" / "weaveback"
lang_conf   = Path(xdg) / "helix" / "languages.toml"

print("Installing weaveback grammar for Helix...")

# 1. Append language + grammar config if not already present
# Substitute __GRAMMAR_DIR__ with the actual path of this checkout.
template = (script_dir / "languages.toml").read_text()
snippet = template.replace("__GRAMMAR_DIR__", str(grammar_dir))
if lang_conf.exists() and 'name = "weaveback"' in lang_conf.read_text():
    print(f"weaveback already present in {lang_conf} — skipping")
else:
    lang_conf.parent.mkdir(parents=True, exist_ok=True)
    with lang_conf.open("a") as f:
        f.write("\n" + snippet)
    print(f"Appended weaveback config to {lang_conf}")

# 2. Copy weaveback query files into Helix runtime
queries_dir.mkdir(parents=True, exist_ok=True)
for name in ("highlights.scm", "injections.scm"):
    shutil.copy(grammar_dir / "queries" / name, queries_dir / name)
print(f"Copied queries to {queries_dir}")

# 3. Append weaveback injection into AsciiDoc files (once)
asciidoc_inj = helix_rt / "queries" / "asciidoc" / "injections.scm"
marker = 'injection.language "weaveback"'
asciidoc_snippet = (script_dir / "asciidoc-injections.scm").read_text()
if asciidoc_inj.exists() and marker in asciidoc_inj.read_text():
    print(f"weaveback→asciidoc injection already present in {asciidoc_inj} — skipping")
else:
    asciidoc_inj.parent.mkdir(parents=True, exist_ok=True)
    with asciidoc_inj.open("a") as f:
        f.write("\n" + asciidoc_snippet)
    print(f"Appended weaveback injection to {asciidoc_inj}")

# 4. Build the grammar
# Helix is installed as 'hx' on most systems, 'helix' on Arch Linux.
hx_cmd = shutil.which("hx") or shutil.which("helix")
if not hx_cmd:
    print("ERROR: neither 'hx' nor 'helix' found in PATH", file=sys.stderr)
    sys.exit(1)
result = subprocess.run([hx_cmd, "--grammar", "build"], capture_output=False)
if result.returncode != 0:
    # Non-zero often means *other* grammars failed; weaveback may have built fine.
    print("WARNING: some grammars failed to build (see above). "
          "If 'weaveback' is listed under 'built now', it succeeded.", file=sys.stderr)

print("Done.  Open a .weaveback file in Helix to verify highlighting.")
