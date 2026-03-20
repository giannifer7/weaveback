#!/usr/bin/env python3
# editors/neovim/install.py
#
# Installs the weaveback + asciidoc grammars and queries for Neovim (nvim-treesitter).
# Run once after cloning, and again after grammar changes.
#
# Usage:  python3 editors/neovim/install.py
#
# Prerequisites: nvim-treesitter installed as a plugin.
# After running, open Neovim and run:
#   :TSInstall weaveback asciidoc asciidoc_inline

import os
import shutil
import sys
from pathlib import Path

script_dir  = Path(__file__).resolve().parent
grammar_dir = script_dir.parent.parent          # tree-sitter-weaveback/

xdg         = os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")
nvim_cfg    = Path(xdg) / "nvim"
plugin_dir  = nvim_cfg / "after" / "plugin"

# Helix caches the asciidoc grammar source; reuse it to avoid a second download.
helix_rt    = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config")) / "helix" / "runtime"
asciidoc_src        = helix_rt / "grammars" / "sources" / "asciidoc" / "tree-sitter-asciidoc"
asciidoc_inline_src = helix_rt / "grammars" / "sources" / "asciidoc" / "tree-sitter-asciidoc_inline"

print("Installing weaveback + asciidoc grammars for Neovim...")

# ── 1. weaveback queries ──────────────────────────────────────────────────────────
weaveback_q = nvim_cfg / "queries" / "weaveback"
weaveback_q.mkdir(parents=True, exist_ok=True)
for name in ("highlights.scm", "injections.scm"):
    content = ";; extends\n" + (grammar_dir / "queries" / name).read_text()
    (weaveback_q / name).write_text(content)
print(f"Copied weaveback queries to {weaveback_q}")

# ── 2. weaveback.lua ──────────────────────────────────────────────────────────────
plugin_dir.mkdir(parents=True, exist_ok=True)
lua = (script_dir / "weaveback.lua").read_text().replace("__GRAMMAR_DIR__", str(grammar_dir))
(plugin_dir / "weaveback.lua").write_text(lua)
print(f"Installed {plugin_dir / 'weaveback.lua'}")

# ── 3. asciidoc queries ───────────────────────────────────────────────────────
# Queries are NOT bundled with nvim-treesitter for custom parsers, so we copy
# them from Helix's already-downloaded grammar source.
for lang, src in (("asciidoc", asciidoc_src), ("asciidoc_inline", asciidoc_inline_src)):
    dest = nvim_cfg / "queries" / lang
    if not (src / "queries").exists():
        print(f"WARNING: {src}/queries not found — run Helix grammar build first, or :TSInstall {lang} may fetch them")
        continue
    dest.mkdir(parents=True, exist_ok=True)
    for name in ("highlights.scm", "injections.scm"):
        qfile = src / "queries" / name
        if qfile.exists():
            (dest / name).write_text(qfile.read_text())
    print(f"Copied {lang} queries to {dest}")

# ── 4. asciidoc.lua ───────────────────────────────────────────────────────────
shutil.copy(script_dir / "asciidoc.lua", plugin_dir / "asciidoc.lua")
print(f"Installed {plugin_dir / 'asciidoc.lua'}")

# ── 5. weaveback injection into [source,weaveback] asciidoc blocks ───────────────────
asciidoc_inj = nvim_cfg / "queries" / "asciidoc" / "injections.scm"
marker       = 'injection.language "weaveback"'
weaveback_snippet = (script_dir.parent / "helix" / "asciidoc-injections.scm").read_text()
if asciidoc_inj.exists() and marker in asciidoc_inj.read_text():
    print("weaveback→asciidoc injection already present — skipping")
else:
    with asciidoc_inj.open("a") as f:
        f.write("\n" + weaveback_snippet)
    print(f"Appended weaveback injection to {asciidoc_inj}")

print()
print("Done. Open Neovim and run:")
print("  :TSInstall weaveback asciidoc asciidoc_inline")
