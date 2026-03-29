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

def main():
    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    os.chdir(project_root)

    # weaveback-lsp (semantic language server bridge)
    run([
        "weaveback",
        "--dir", "crates/weaveback-lsp/",
        "--ext", "adoc",
        "--gen", "crates/",
        "--no-macros",
        "--open-delim", "<<",
        "--close-delim", ">>",
    ])

    # weaveback-core (shared constants)
    run([
        "weaveback",
        "--dir", "crates/weaveback-core/",
        "--ext", "adoc",
        "--gen", "crates/",
        "--no-macros",
        "--open-delim", "<<",
        "--close-delim", ">>",
    ])

    # weaveback-macro adocs use << >> delimiters (no self-hosting conflict).
    run([
        "weaveback",
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
        "weaveback",
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
        "weaveback",
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
        "weaveback",
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
        "weaveback",
        "--dir", "tree-sitter-weaveback/",
        "--ext", "adoc",
        "--gen", "tree-sitter-weaveback/",
        "--no-macros",
        "--open-delim", "<<",
        "--close-delim", ">>",
    ])

if __name__ == "__main__":
    main()
