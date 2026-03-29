import real_ladybug as lbug
import os
import shutil
from pathlib import Path

def setup_schema(conn: lbug.Connection) -> None:
    """Define the Project Knowledge Graph schema."""
    # Node Tables
    conn.execute("CREATE NODE TABLE Crate(name STRING, PRIMARY KEY (name))")
    conn.execute("CREATE NODE TABLE File(path STRING, PRIMARY KEY (path))")
    conn.execute("CREATE NODE TABLE Chunk(name STRING, PRIMARY KEY (name))")

    # Relationship Tables
    conn.execute("CREATE REL TABLE DEPENDS_ON(FROM Crate TO Crate)")
    conn.execute("CREATE REL TABLE OWNS(FROM Crate TO File)")
    conn.execute("CREATE REL TABLE DEFINES(FROM File TO Chunk)")

def populate_graph(conn: lbug.Connection, root: Path) -> None:
    """Populate the graph by scanning Weaveback workspace."""
    crates_dir = root / "crates"
    if not crates_dir.exists():
        print(f"DEBUG: crates dir not found at {crates_dir}")
        return

    # 1. Add Crates
    for crate_path in crates_dir.iterdir():
        if crate_path.is_dir():
            name = crate_path.name
            conn.execute(f"CREATE (c:Crate {{name: '{name}'}})")

            # 2. Add Files owned by Crate
            src_dir = crate_path / "src"
            if src_dir.exists():
                for file_path in src_dir.rglob("*.adoc"):
                    rel_path = file_path.relative_to(root).as_posix()
                    conn.execute(f"CREATE (f:File {{path: '{rel_path}'}})")
                    conn.execute(f"MATCH (c:Crate), (f:File) WHERE c.name = '{name}' AND f.path = '{rel_path}' CREATE (c)-[:OWNS]->(f)")

    # 3. Simple dependency modeling (hardcoded for this example)
    deps = [
        ("weaveback", "weaveback-macro"),
        ("weaveback", "weaveback-tangle"),
        ("weaveback", "weaveback-lsp"),
        ("weaveback-lsp", "weaveback-core"),
        ("weaveback-macro", "weaveback-core"),
        ("weaveback-tangle", "weaveback-core"),
    ]
    for src, dst in deps:
        conn.execute(f"MATCH (a:Crate), (b:Crate) WHERE a.name = '{src}' AND b.name = '{dst}' CREATE (a)-[:DEPENDS_ON]->(b)")

def run_queries(conn: lbug.Connection) -> None:
    """Run example Cypher queries to show the graph's power."""
    print("\n--- Query 1: Which files are owned by the 'weaveback' crate? ---")
    result = conn.execute("MATCH (c:Crate)-[:OWNS]->(f:File) WHERE c.name = 'weaveback' RETURN f.path")
    while result.has_next():
        row = result.get_next()
        print(f"  - {row[0]}")

    print("\n--- Query 2: Transitively find everything that depends on 'weaveback-core' ---")
    result = conn.execute("MATCH (a:Crate)-[:DEPENDS_ON*]->(b:Crate) WHERE b.name = 'weaveback-core' RETURN DISTINCT a.name")
    while result.has_next():
        row = result.get_next()
        print(f"  - {row[0]}")

def main() -> None:
    db_path = "weaveback_memory.lbdb"
    
    # Robust cleanup
    if os.path.exists(db_path):
        if os.path.isdir(db_path):
            shutil.rmtree(db_path)
        else:
            os.remove(db_path)

    db = lbug.Database(db_path)
    conn = lbug.Connection(db)

    setup_schema(conn)
    
    # Find project root reliably
    curr = Path(__file__).resolve()
    while curr.name != "weaveback" and curr.parent != curr:
        curr = curr.parent
    
    project_root = curr
    print(f"Populating graph from workspace: {project_root}...")
    populate_graph(conn, project_root)

    run_queries(conn)

if __name__ == "__main__":
    main()
