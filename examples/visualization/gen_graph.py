#!/usr/bin/env python3
# gen_graph.py
# I’d Really Rather You Didn’t edit this generated file.
import sqlite3
import json
import sys
import os


def main():
    if len(sys.argv) < 2:
        print("Usage: gen_graph.py <weaveback.db>")
        sys.exit(1)

    db_path = sys.argv[1]
    if not os.path.exists(db_path):
        print(f"Error: {db_path} not found.")
        sys.exit(1)

    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()

    # --- collect chunks ---
    cursor.execute("SELECT DISTINCT from_chunk FROM chunk_deps")
    nodes_set = {row[0] for row in cursor.fetchall()}

    cursor.execute("SELECT DISTINCT to_chunk FROM chunk_deps")
    for row in cursor.fetchall():
        nodes_set.add(row[0])

    nodes = []
    chunk_to_file = {}

    for name in sorted(nodes_set):
        cursor.execute("""
            SELECT src_file, nth
            FROM chunk_defs
            WHERE chunk_name = ?
            ORDER BY nth ASC
            LIMIT 1
        """, (name,))
        row = cursor.fetchone()

        node_type = "file" if name.startswith("@file") else "chunk"

        if row:
            src_file, nth = row
            nodes.append({
                "id": f"chunk:{name}",
                "label": name,
                "type": node_type,
                "src_file": src_file,
                "nth": nth
            })
        else:
            nodes.append({
                "id": f"chunk:{name}",
                "label": name,
                "type": node_type
            })

    # --- links ---
    cursor.execute("SELECT from_chunk, to_chunk FROM chunk_deps")
    links = []

    for src, dst in cursor.fetchall():
        links.append({
            "source": f"chunk:{src}",
            "target": f"chunk:{dst}",
            "type": "chunk_dep"
        })

    graph = {"nodes": nodes, "links": links}

    with open("graph.json", "w") as f:
        json.dump(graph, f, indent=2)

    print(f"Graph: {len(nodes)} nodes, {len(links)} edges")

    conn.close()


if __name__ == "__main__":
    main()
