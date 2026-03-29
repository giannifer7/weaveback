use lbug::{Database, Connection, Value, SystemConfig};
use weaveback_tangle::db::WeavebackDb;
use anyhow::{Result, Context};
use std::path::Path;
use std::fs;

fn main() -> Result<()> {
    let db_path = "weaveback.db";
    let graph_path = "weaveback_graph.lbdb";

    if !Path::new(db_path).exists() {
        anyhow::bail!("Relational database '{}' not found. Run weaveback first.", db_path);
    }

    // 1. Cleanup previous graph run
    if Path::new(graph_path).exists() {
        if Path::new(graph_path).is_dir() {
            fs::remove_dir_all(graph_path).context("Failed to cleanup old graph dir")?;
        } else {
            fs::remove_file(graph_path).context("Failed to cleanup old graph file")?;
        }
    }

    // 2. Initialize LadybugDB
    let db = Database::new(graph_path, SystemConfig::default())?;
    let conn = Connection::new(&db)?;

    println!("Creating Knowledge Graph schema...");
    let mut stmt = conn.prepare("CREATE NODE TABLE File(path STRING, PRIMARY KEY (path))")?;
    conn.execute(&mut stmt, Vec::new())?;
    
    let mut stmt = conn.prepare("CREATE NODE TABLE Chunk(name STRING, PRIMARY KEY (name))")?;
    conn.execute(&mut stmt, Vec::new())?;
    
    let mut stmt = conn.prepare("CREATE REL TABLE DEFINES(FROM File TO Chunk)")?;
    conn.execute(&mut stmt, Vec::new())?;
    
    let mut stmt = conn.prepare("CREATE REL TABLE REFERENCES(FROM Chunk TO Chunk, src_file STRING)")?;
    conn.execute(&mut stmt, Vec::new())?;

    // 3. Open Weaveback SQLite DB
    let wb_db = WeavebackDb::open_read_only(db_path)?;

    // 4. Populate Files and Chunks
    println!("Migrating data from SQLite to Graph...");
    let chunk_defs = wb_db.list_all_chunk_defs()?;
    for entry in chunk_defs {
        let _ = conn.execute(&mut conn.prepare(&format!("CREATE (f:File {{path: '{}'}})", entry.src_file))?, Vec::new());
        let _ = conn.execute(&mut conn.prepare(&format!("CREATE (c:Chunk {{name: '{}'}})", entry.chunk_name))?, Vec::new());
        
        let mut stmt = conn.prepare(&format!(
            "MATCH (f:File), (c:Chunk) WHERE f.path = '{}' AND c.name = '{}' CREATE (f)-[:DEFINES]->(c)",
            entry.src_file, entry.chunk_name
        ))?;
        let _ = conn.execute(&mut stmt, Vec::new());
    }

    // 5. Populate Dependencies
    let deps = wb_db.list_all_chunk_deps()?;
    for (from, to, file) in deps {
        let mut stmt = conn.prepare(&format!(
            "MATCH (a:Chunk), (b:Chunk) WHERE a.name = '{}' AND b.name = '{}' CREATE (a)-[:REFERENCES {{src_file: '{}'}}]->(b)",
            from, to, file
        ))?;
        let _ = conn.execute(&mut stmt, Vec::new());
    }

    // 6. Run a more general query
    println!("\n--- Graph Query: Find all chunks defined in 'crates/weaveback/src/weaveback.adoc' and their transitive dependencies ---");
    let mut stmt = conn.prepare(
        "MATCH (f:File)-[:DEFINES]->(a:Chunk)-[:REFERENCES*1..3]->(b:Chunk) \
         WHERE f.path = 'crates/weaveback/src/weaveback.adoc' \
         RETURN DISTINCT a.name, b.name"
    )?;
    let mut result = conn.execute(&mut stmt, Vec::new())?;

    while let Some(row) = result.next() {
        if let (Value::String(start), Value::String(end)) = (&row[0], &row[1]) {
            println!("  - {} -> {}", start, end);
        }
    }

    Ok(())
}
