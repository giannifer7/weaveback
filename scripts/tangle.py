#!/usr/bin/env python3
"""Tangle all .adoc literate sources under crates/ into generated Rust files."""

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

if __name__ == "__main__":
    main()
