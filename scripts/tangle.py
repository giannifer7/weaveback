#!/usr/bin/env python3
"""Tangle all .adoc literate sources into their generated output files."""

import subprocess
import sys
import os

def run(cmd):
    print(f"Running: {' '.join(cmd)}")
    result = subprocess.run(cmd)
    if result.returncode != 0:
        sys.exit(result.returncode)

def find_weaveback(project_root):
    """Prefer the locally-built binary over whatever is in PATH."""
    for candidate in ["target/release/weaveback", "target/debug/weaveback"]:
        path = os.path.join(project_root, candidate)
        if os.path.isfile(path) and os.access(path, os.X_OK):
            return path
    return "weaveback"  # fall back to PATH

def main():
    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    os.chdir(project_root)
    wb = find_weaveback(project_root)

    # weaveback-lsp (semantic language server bridge)
    run([
        wb,
        "--dir", "crates/weaveback-lsp/",
        "--ext", "adoc",
        "--gen", "crates/",
        "--no-macros",
        "--open-delim", "<<",
        "--close-delim", ">>",
    ])

    # weaveback-core (shared constants)
    run([
        wb,
        "--dir", "crates/weaveback-core/",
        "--ext", "adoc",
        "--gen", "crates/",
        "--no-macros",
        "--open-delim", "<<",
        "--close-delim", ">>",
    ])

    # weaveback-macro adocs use << >> delimiters (no self-hosting conflict).
    run([
        wb,
        "--dir", "crates/weaveback-macro/",
        "--ext", "adoc",
        "--gen", "crates/",
        "--special", "^",
        "--open-delim", "<<",
        "--close-delim", ">>",
    ])

    # weaveback-tangle adocs use <[ ]> delimiters and // comment marker only.
    # No macros are used; --no-macros avoids any collision with literal % or ^
    # in the embedded Rust source.
    run([
        wb,
        "--dir", "crates/weaveback-tangle/",
        "--ext", "adoc",
        "--gen", "crates/",
        "--no-macros",
        "--open-delim", "<[",
        "--close-delim", "]>",
        "--comment-markers", "//",
        "--chunk-end", "@@",
    ])

    # weaveback-docgen adocs use the same <[ ]> / // / @@ conventions as tangle.
    run([
        wb,
        "--dir", "crates/weaveback-docgen/",
        "--ext", "adoc",
        "--gen", "crates/",
        "--no-macros",
        "--open-delim", "<[",
        "--close-delim", "]>",
        "--comment-markers", "//",
        "--chunk-end", "@@",
    ])

    # weaveback (combined) adocs use << >> delimiters and no macros.
    run([
        wb,
        "--dir", "crates/weaveback/",
        "--ext", "adoc",
        "--gen", "crates/",
        "--no-macros",
        "--open-delim", "<<",
        "--close-delim", ">>",
    ])

    # tree-sitter-weaveback adocs use << >> delimiters, // comment marker, no macros.
    # Generates grammar.js, query .scm files, and editor integration files.
    run([
        wb,
        "--dir", "tree-sitter-weaveback/",
        "--ext", "adoc",
        "--gen", "tree-sitter-weaveback/",
        "--no-macros",
        "--open-delim", "<<",
        "--close-delim", ">>",
    ])

if __name__ == "__main__":
    main()
