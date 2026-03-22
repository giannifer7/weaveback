#!/usr/bin/env python3
"""Render all .adoc files under the project to dark-themed HTML.

Files containing diagram blocks (plantuml/ditaa/graphviz/mermaid) are rendered
individually so each gets its own imagesoutdir; everything else is batched into
a single asciidoctor process.  mtime-based incremental: up-to-date outputs are
skipped unless the theme changed.
"""

import os
import re
import subprocess
import sys

DIAGRAM_RE = re.compile(
    r"^\[(?:plantuml|ditaa|graphviz|mermaid|a2s|blockdiag)",
    re.MULTILINE,
)

PLANTUML_JAR_CANDIDATES = [
    "/usr/share/java/plantuml/plantuml.jar",
    os.path.expanduser("~/.local/share/plantuml/plantuml.jar"),
]

PLANTUML_NATIVE_CANDIDATES = [
    "/usr/bin/plantuml",
    "/usr/local/bin/plantuml",
    os.path.expanduser("~/.local/bin/plantuml"),
]

EXCLUDE_DIRS = {".git", "target", ".venv", "node_modules"}


def find_adoc_files(root):
    result = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in EXCLUDE_DIRS]
        for name in filenames:
            if name.endswith(".adoc"):
                result.append(os.path.join(dirpath, name))
    return sorted(result)


def has_diagram_blocks(path):
    try:
        return bool(DIAGRAM_RE.search(open(path).read()))
    except OSError:
        return False


def mtime(path):
    try:
        return os.path.getmtime(path)
    except OSError:
        return 0.0


def find_plantuml_jar():
    for c in PLANTUML_JAR_CANDIDATES:
        if os.path.isfile(c):
            return c
    return PLANTUML_JAR_CANDIDATES[0]  # best guess; asciidoctor-diagram will warn


def find_plantuml_native():
    for c in PLANTUML_NATIVE_CANDIDATES:
        if os.path.isfile(c):
            return c
    return None


def main():
    project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_dir = os.path.join(project_root, "docs", "html")
    theme_dir = os.path.join(project_root, "scripts", "asciidoc-theme")

    os.makedirs(out_dir, exist_ok=True)

    adoc_files = find_adoc_files(project_root)

    # Max mtime of any theme file so a theme edit invalidates all outputs.
    theme_mtime = 0.0
    if os.path.isdir(theme_dir):
        for dirpath, _, filenames in os.walk(theme_dir):
            for name in filenames:
                theme_mtime = max(theme_mtime, mtime(os.path.join(dirpath, name)))

    plantuml_jar = find_plantuml_jar()
    os.environ["DIAGRAM_PLANTUML_CLASSPATH"] = plantuml_jar

    plantuml_native = find_plantuml_native()

    base_args = [
        "asciidoctor",
        "-r", "asciidoctor-diagram",
        "-a", "source-highlighter=rouge",
        "-a", "rouge-css=class",
        "-a", "rouge-style=gruvbox",
        "-a", "docinfo=shared",
        "-a", f"docinfodir={theme_dir}",
        "-a", "imagesdir=.",
    ]
    if plantuml_native:
        base_args += ["-a", f"plantuml-native={plantuml_native}"]

    stale_simple = []
    stale_diagram = []

    for adoc in adoc_files:
        rel = os.path.relpath(adoc, project_root)
        out_file = os.path.join(out_dir, os.path.splitext(rel)[0] + ".html")
        os.makedirs(os.path.dirname(out_file), exist_ok=True)

        adoc_mtime = mtime(adoc)
        html_mtime = mtime(out_file)

        if os.path.isfile(out_file) and html_mtime >= adoc_mtime and html_mtime >= theme_mtime:
            continue

        if has_diagram_blocks(adoc):
            stale_diagram.append((adoc, out_file))
        else:
            stale_simple.append(adoc)

    if not stale_simple and not stale_diagram:
        print("docs: nothing to do")
        return

    # Batch: all simple stale files in one Ruby process
    if stale_simple:
        args = base_args + ["-R", project_root, "-D", out_dir] + stale_simple
        print(f"docs: rendering {len(stale_simple)} file(s) (batch)")
        result = subprocess.run(args)
        if result.returncode != 0:
            print("asciidoctor batch failed", file=sys.stderr)
            sys.exit(result.returncode)

    # Individual: diagram files need per-file imagesoutdir
    for adoc, out_file in stale_diagram:
        print(f"docs: rendering {os.path.relpath(adoc, project_root)} (diagrams)")
        args = base_args + [
            "-a", f"imagesoutdir={os.path.dirname(out_file)}",
            "-o", out_file,
            adoc,
        ]
        result = subprocess.run(args)
        if result.returncode != 0:
            print(f"asciidoctor failed: {adoc}", file=sys.stderr)
            sys.exit(result.returncode)


if __name__ == "__main__":
    main()
