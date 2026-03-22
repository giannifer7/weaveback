#!/usr/bin/env python3
"""Tangle all .adoc literate sources under crates/ into generated Rust files."""

import subprocess
import sys
import os

def main():
    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    os.chdir(project_root)

    cmd = [
        "weaveback",
        "--dir", "crates/",
        "--ext", "adoc",
        "--gen", "crates/",
        "--special", "^",
        "--open-delim", "<<",
        "--close-delim", ">>",
    ]

    print(f"Running: {' '.join(cmd)}")
    result = subprocess.run(cmd)
    if result.returncode != 0:
        sys.exit(result.returncode)

if __name__ == "__main__":
    main()
