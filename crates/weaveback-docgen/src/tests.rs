// weaveback-docgen/src/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn find_project_root_walks_up_to_workspace_manifest() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers = []\n").expect("workspace cargo");
    let nested = dir.path().join("crates/demo/src");
    fs::create_dir_all(&nested).expect("nested dir");

    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&nested).expect("set cwd");
    let found = find_project_root();
    std::env::set_current_dir(original).expect("restore cwd");

    assert_eq!(found, dir.path());
}

#[test]
fn read_config_uses_defaults_for_missing_or_invalid_files() {
    let dir = tempdir().expect("tempdir");

    let cfg = read_config(dir.path());
    assert!(cfg.docs.is_none());

    fs::write(
        dir.path().join("weaveback.toml"),
        "[docs]\nd2_theme = 42\nd2_layout = \"dagre\"\n",
    )
    .expect("config");
    let cfg = read_config(dir.path());
    let docs = cfg.docs.expect("docs config");
    assert_eq!(docs.d2_theme, Some(42));
    assert_eq!(docs.d2_layout.as_deref(), Some("dagre"));

    fs::write(dir.path().join("weaveback.toml"), "not = [valid").expect("bad config");
    let cfg = read_config(dir.path());
    assert!(cfg.docs.is_none());
}

#[test]
fn run_xref_cmd_reads_valid_json_from_external_command() {
    let dir = tempdir().expect("tempdir");
    let script = dir.path().join("emit-xref.sh");
    fs::write(
        &script,
        "#!/bin/sh\nprintf '{\"demo/mod\":{\"html\":\"demo.html\",\"imports\":[],\"imported_by\":[],\"symbols\":[\"Demo\"],\"lsp_links\":[]}}'\n",
    )
    .expect("script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod");
    }

    let data = run_xref_cmd(script.to_str().expect("script path"), dir.path());
    let entry = data.get("demo/mod").expect("xref entry");
    assert_eq!(entry.html, "demo.html");
    assert_eq!(entry.symbols, vec!["Demo".to_string()]);
}

#[test]
fn parse_args_from_parses_repeatable_and_path_options() {
    let raw = vec![
        "weaveback-docgen".to_string(),
        "--special".to_string(),
        "%".to_string(),
        "--special".to_string(),
        "^".to_string(),
        "--xref-cmd".to_string(),
        "emit-xref".to_string(),
        "--out-dir".to_string(),
        "docs/html".to_string(),
        "--theme-dir".to_string(),
        "scripts/theme".to_string(),
        "--plantuml-jar".to_string(),
        "/tmp/plantuml.jar".to_string(),
        "--no-xref".to_string(),
        "--ai-xref".to_string(),
    ];

    let args = parse_args_from(&raw);
    assert_eq!(args.specials, vec!['%', '^']);
    assert_eq!(args.xref_cmd.as_deref(), Some("emit-xref"));
    assert_eq!(args.out_dir.as_deref(), Some(Path::new("docs/html")));
    assert_eq!(args.theme_dir.as_deref(), Some(Path::new("scripts/theme")));
    assert_eq!(args.plantuml_jar.as_deref(), Some(Path::new("/tmp/plantuml.jar")));
    assert!(args.no_xref);
    assert!(args.ai_xref);
    assert_eq!(args.d2_theme, 200);
    assert_eq!(args.d2_layout, "elk");
}

#[test]
fn parse_args_from_ignores_invalid_special_values() {
    let raw = vec![
        "weaveback-docgen".to_string(),
        "--special".to_string(),
        "xy".to_string(),
        "--special".to_string(),
        "".to_string(),
    ];

    let args = parse_args_from(&raw);
    assert!(args.specials.is_empty());
}

