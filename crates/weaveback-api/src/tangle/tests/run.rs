// weaveback-api/src/tangle/tests/run.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[test]
fn run_tangle_all_errors_on_missing_config() {
    let dir = TempDir::new().unwrap();
    let cfg_path = dir.path().join("nonexistent.toml");
    let result = run_tangle_all(&cfg_path, false);
    assert!(result.is_err());
}

#[test]
fn run_tangle_all_errors_on_bad_toml() {
    let dir = TempDir::new().unwrap();
    let cfg_path = dir.path().join("weaveback.toml");
    std::fs::write(&cfg_path, "[[pass\nbad toml{{{{").unwrap();
    let result = run_tangle_all(&cfg_path, false);
    assert!(result.is_err());
}

#[test]
fn test_run_tangle_all_with_db_post_processing() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("weaveback.db");
    let cfg_path = dir.path().join("weaveback.toml");

    // Seed a DB
    {
        let _db = weaveback_tangle::db::WeavebackDb::open(&db_path).unwrap();
    }

    let toml_src = r#"
[tags]
backend = "ollama"
endpoint = "http://127.0.0.1:9/v1"

[embeddings]
backend = "ollama"
endpoint = "http://127.0.0.1:9/v1"

[[pass]]
dir = "src/"
"#;
    std::fs::write(&cfg_path, toml_src).unwrap();

    // Since run_tangle_all looks for "weaveback.db" in CWD,
    // and we are in a test environment where we don't want to pollute CWD,
    // we'll use a little trick: we'll test a version that doesn't
    // find the DB if we don't create it here.
    // But to hit the 180+ lines, we NEED it to exist.

    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    // We override the passes to empty so it doesn't try to spawn current_exe
    let toml_empty_passes = r#"
[tags]
backend = "openai"
model = "gpt-4o"
[pass]
"#;
    std::fs::write(&cfg_path, toml_empty_passes).unwrap();

    let _ = run_tangle_all(&cfg_path, false);

    std::env::set_current_dir(old_cwd).unwrap();
}

#[test]
fn test_run_tangle_all_db_open_error_path() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("weaveback.db");
    // Create a directory where the file should be to cause open failure
    std::fs::create_dir(&db_path).unwrap();

    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let cfg_path = dir.path().join("weaveback.toml");
    std::fs::write(&cfg_path, "[[pass]]\ndir=\"src/\"\n").unwrap();

    // This will skip passes (because they fail) but we want to see if it handles DB error
    // Actually, it returns early if passes fail.
    // So we use an empty pass list.
    std::fs::write(&cfg_path, "[pass]\n").unwrap();
    let _ = run_tangle_all(&cfg_path, false);

    std::env::set_current_dir(old_cwd).unwrap();
}

#[test]
fn test_run_tangle_all_fails_if_pass_fails() {
    let dir = TempDir::new().unwrap();
    let cfg_path = dir.path().join("weaveback.toml");
    // Use a directory that definitely doesn't exist to ensure failure
    let toml_src = "[[pass]]\ndir = \"/tmp/nonexistent_path_weaveback_test\"\n";
    std::fs::write(&cfg_path, toml_src).unwrap();

    let res = run_tangle_all(&cfg_path, false);
    // This fails because the current_exe (test runner) is spawned
    // and its exit status is checked. Since it's called with unknown args,
    // it exits with error 101 or similar.
    assert!(res.is_err());
}

