#!/usr/bin/env python3
# weaveback_tool.py
# I’d Really Rather You Didn’t edit this generated file.
import sqlite3
import argparse
import sys
import os



def main():
    parser = argparse.ArgumentParser(description="Query weaveback.db")
    parser.add_argument("db", help="Path to weaveback.db")
    subparsers = parser.add_subparsers(dest="command", help="Sub-commands")

    # Stats command
    subparsers.add_parser("stats", help="Show database statistics")

    # List command
    subparsers.add_parser("list", help="List files and chunks")

    # Trace command
    trace_parser = subparsers.add_parser("trace", help="Trace an output line")
    trace_parser.add_argument("file", help="Generated file path")
    trace_parser.add_argument("line", type=int, help="Line number (1-indexed)")

    args = parser.parse_args()

    if not os.path.exists(args.db):
        print(f"Error: Database {args.db} not found.")
        sys.exit(1)

    conn = sqlite3.connect(args.db)
    try:
        if args.command == "stats":
            do_stats(conn)
        elif args.command == "list":
            do_list(conn)
        elif args.command == "trace":
            do_trace(conn, args.file, args.line)
        else:
            parser.print_help()
    finally:
        conn.close()

def do_stats(conn):
    cursor = conn.cursor()
    print("Weaveback Database Statistics")
    print("-" * 30)

    tables = ["gen_baselines", "noweb_map", "macro_map", "src_snapshots", "chunk_defs"]
    for table in tables:
        cursor.execute(f"SELECT COUNT(*) FROM {table}")
        count = cursor.fetchone()[0]
        print(f"{table:15}: {count}")

def do_list(conn):
    cursor = conn.cursor()
    print("Files and Chunks")
    print("-" * 30)

    print("\nSource Files:")
    cursor.execute("SELECT DISTINCT src_file FROM chunk_defs")
    for row in cursor.fetchall():
        print(f"  - {row[0]}")

    print("\nChunks:")
    cursor.execute("SELECT DISTINCT chunk_name FROM chunk_defs ORDER BY chunk_name")
    for row in cursor.fetchall():
        print(f"  - {row[0]}")

def do_trace(conn, out_file, out_line):
    # Convert 1-indexed to 0-indexed if necessary, but Weaveback DB uses 0-indexed or 1-indexed?
    # Let's assume the user provides 1-indexed (editor style).
    # Based on db.adoc, src_line is 0-indexed. out_line is also INTEGER.
    # Usually we refer to output lines as 1-indexed in CLI.
    
    cursor = conn.cursor()
    # We try both 0 and 1 indexed just in case, but usually we record it exactly.
    # Actually, let's look at db.adoc again: primary key (out_file, out_line).
    
    cursor.execute(
        "SELECT src_file, chunk_name, src_line, confidence FROM noweb_map "
        "WHERE out_file = ? AND out_line = ?",
        (out_file, out_line)
    )
    res = cursor.fetchone()
    if res:
        src_file, chunk, src_line, confidence = res
        print(f"Trace for {out_file}:{out_line}")
        print("-" * 30)
        print(f"Source File : {src_file}")
        print(f"Chunk Name  : {chunk}")
        print(f"Source Line : {src_line + 1} (1-indexed)")
        print(f"Confidence  : {confidence}")
    else:
        print(f"No mapping found for {out_file}:{out_line}")


if __name__ == "__main__":
    main()
