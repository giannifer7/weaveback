// crates/weaveback/tests/test_apply_back.rs
//
// Integration tests for the new tracing and apply-back features, modelled on
// the `config.nim.adoc` pattern from the rompla project.
//
// The source defines a `cfg_int` macro — exactly like the real-world config
// template — which expands each integer config variable into a field default
// line and a TOML-read line, emitted into named noweb chunks.
//
// Exercises:
//   - `weaveback trace --col` differentiating MacroBody / MacroArg / VarBinding
//   - `def_locations` array: db-backed %def position lookup (no regex scan)
//   - `set_locations` array: db-backed %set position lookup (no regex scan)
//   - `weaveback apply-back` propagating a gen-file edit back to a literal source line

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn weaveback() -> Command {
    Command::cargo_bin("weaveback").unwrap()
}

fn write_file(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

// ── source fixture ────────────────────────────────────────────────────────────

/// Literate source modelled on config.nim.adoc.
///
/// Line 1:  %def(cfg_int, field, toml_key, default_val, %{
/// Line 2:  # <[defaults]>=
/// Line 3:  result.%(field) = %(default_val)
/// Line 4:  # @
/// Line 5:  # <[toml reads]>=
/// Line 6:  cfg.%(field) = t.getInt("%(toml_key)", cfg.%(field))
/// Line 7:  # @
/// Line 8:  %})
/// Line 9:  (empty)
/// Line 10: %set(module_name, config)
/// Line 11: (empty)
/// Line 12: # <[@file config.nim]>=
/// Line 13: # <[defaults]>
/// Line 14: # <[toml reads]>
/// Line 15: # @
/// Line 16: (empty)
/// Line 17: # <[@file header.nim]>=
/// Line 18: // version: 1.0
/// Line 19: # module: %(module_name)
/// Line 20: # @
/// Line 21: (empty)
/// Line 22: %cfg_int(field=batchSize, toml_key=db_batch_size, default_val=300)
/// Line 23: %cfg_int(field=prefetch,  toml_key=prefetch_ahead, default_val=50)
const DRIVER: &str = r#"%def(cfg_int, field, toml_key, default_val, %{
# <[defaults]>=
result.%(field) = %(default_val)
# @
# <[toml reads]>=
cfg.%(field) = t.getInt("%(toml_key)", cfg.%(field))
# @
%})

%set(module_name, config)

# <[@file config.nim]>=
# <[defaults]>
# <[toml reads]>
# @

# <[@file header.nim]>=
// version: 1.0
# module: %(module_name)
# @

%cfg_int(field=batchSize, toml_key=db_batch_size, default_val=300)
%cfg_int(field=prefetch,  toml_key=prefetch_ahead, default_val=50)
"#;

/// Build `driver.md` from `DRIVER`, run weaveback, return the canonicalized temp root.
fn build(tmp: &TempDir) -> std::path::PathBuf {
    let root = tmp.path().canonicalize().unwrap();
    write_file(&root, "driver.md", DRIVER);
    weaveback()
        .arg("--gen").arg(".")
        .arg("driver.md")
        .current_dir(&root)
        .assert()
        .success();
    root
}

/// Run `weaveback trace <file> <line> [--col <col>]` and return parsed JSON.
fn trace_at(dir: &Path, file: &str, line: u32, col: Option<u32>) -> Value {
    let mut cmd = weaveback();
    cmd.arg("--gen").arg(".");
    cmd.arg("trace").arg(file).arg(line.to_string());
    if let Some(c) = col {
        cmd.arg("--col").arg(c.to_string());
    }
    let out = cmd.current_dir(dir).output().unwrap();
    assert!(
        out.status.success(),
        "weaveback trace failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("bad JSON: {e}\nstdout: {}", String::from_utf8_lossy(&out.stdout))
    })
}

// ── generated-file layout ─────────────────────────────────────────────────────

/// After building, config.nim must have the four expanded lines in the right order:
/// two defaults then two TOML-read lines, one per %cfg_int call.
#[test]
fn test_config_nim_layout() {
    let tmp = TempDir::new().unwrap();
    let root = build(&tmp);

    let content = fs::read_to_string(root.join("config.nim")).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 4, "config.nim should have 4 lines: {lines:?}");
    assert_eq!(lines[0], "result.batchSize = 300");
    assert_eq!(lines[1], "result.prefetch = 50");
    assert_eq!(lines[2], r#"cfg.batchSize = t.getInt("db_batch_size", cfg.batchSize)"#);
    assert_eq!(lines[3], r#"cfg.prefetch = t.getInt("prefetch_ahead", cfg.prefetch)"#);

    let hdr = fs::read_to_string(root.join("header.nim")).unwrap();
    let hlines: Vec<&str> = hdr.lines().collect();
    assert_eq!(hlines.len(), 2, "header.nim should have 2 lines: {hlines:?}");
    assert_eq!(hlines[0], "// version: 1.0");
    assert_eq!(hlines[1], "# module: config");
}

// ── weaveback trace --col: MacroBody with def_locations ──────────────────────────

/// Col 0 of `result.batchSize = 300` hits the literal `r` of `result.`,
/// which is a MacroBody span.  The `def_locations` array must be populated
/// and point to the `%def(cfg_int, ...)` call in driver.md (line 1).
#[test]
fn test_trace_macro_body_with_def_locations() {
    let tmp = TempDir::new().unwrap();
    let root = build(&tmp);

    // col 0 → `r` of `result.` — literal text in the cfg_int body
    let j = trace_at(&root, "config.nim", 1, Some(0));
    assert_eq!(j["kind"], "MacroBody",
        "expected MacroBody at col 0, full trace: {j}");
    assert_eq!(j["macro_name"], "cfg_int");

    let locs = j["def_locations"].as_array()
        .unwrap_or_else(|| panic!("def_locations must be a JSON array; got: {j}"));
    assert!(!locs.is_empty(), "def_locations must not be empty; got: {j}");

    let loc = &locs[0];
    assert!(
        loc["file"].as_str().unwrap_or("").contains("driver.md"),
        "def_locations[0].file should reference driver.md: {loc}"
    );
    // %def(cfg_int, ...) is on the very first line of driver.md
    assert_eq!(loc["line"], 1,
        "def_locations[0].line should be 1 (%def is on line 1): {loc}");
}

// ── weaveback trace --col: MacroArg(field) ───────────────────────────────────────

/// `result.batchSize = 300`:  `result.` is 7 chars, so col 8 is the first
/// character of `batchSize` — a MacroArg bound to the `field` parameter.
#[test]
fn test_trace_macro_arg_field_col() {
    let tmp = TempDir::new().unwrap();
    let root = build(&tmp);

    //  r e s u l t .  b ...
    //  1 2 3 4 5 6 7  8    (1-indexed character positions)
    let j = trace_at(&root, "config.nim", 1, Some(8));
    assert_eq!(j["kind"], "MacroArg",
        "expected MacroArg at col 8 (batchSize), full trace: {j}");
    assert_eq!(j["macro_name"], "cfg_int");
    assert_eq!(j["param_name"], "field");
}

// ── weaveback trace --col: MacroArg(default_val) ─────────────────────────────────

/// `result.batchSize = 300`:  `result.batchSize = ` is 19 chars, so col 20
/// is the first character of `300` — a MacroArg bound to `default_val`.
#[test]
fn test_trace_macro_arg_default_val_col() {
    let tmp = TempDir::new().unwrap();
    let root = build(&tmp);

    //  r e s u l t . b a t c h S i z e   =   3  ...
    //  1 2 3 4 5 6 7 8 9 ...           17 18 19 20   (1-indexed)
    let j = trace_at(&root, "config.nim", 1, Some(20));
    assert_eq!(j["kind"], "MacroArg",
        "expected MacroArg at col 20 (300), full trace: {j}");
    assert_eq!(j["macro_name"], "cfg_int");
    assert_eq!(j["param_name"], "default_val");
}

// ── weaveback trace --col: VarBinding with set_locations ─────────────────────────

/// `# module: config`:  `# module: ` is 10 chars, so col 11 is the first
/// character of `config` — a VarBinding from `%(module_name)`.
/// The `set_locations` array must point to the `%set(module_name, config)`
/// call in driver.md.
#[test]
fn test_trace_var_binding_with_set_locations() {
    let tmp = TempDir::new().unwrap();
    let root = build(&tmp);

    //  #   m o d u l e :    c  ...
    //  1 2 3 4 5 6 7 8 9 10 11     (1-indexed character positions)
    let j = trace_at(&root, "header.nim", 2, Some(11));
    assert_eq!(j["kind"], "VarBinding",
        "expected VarBinding at col 10 (config), full trace: {j}");
    assert_eq!(j["var_name"], "module_name");

    let locs = j["set_locations"].as_array()
        .unwrap_or_else(|| panic!("set_locations must be a JSON array; got: {j}"));
    assert!(!locs.is_empty(), "set_locations must not be empty; got: {j}");

    let loc = &locs[0];
    assert!(
        loc["file"].as_str().unwrap_or("").contains("driver.md"),
        "set_locations[0].file should reference driver.md: {loc}"
    );
    // %set(module_name, config) is on line 10 of driver.md (after the %def block)
    assert_eq!(loc["line"], 10,
        "set_locations[0].line should be 10 (%set is on line 10): {loc}");
}

// ── col disambiguation: same line, different tokens ──────────────────────────

/// Verify that col=1 and col=8 on the same output line return different kinds,
/// confirming sub-line granularity works end-to-end (all 1-indexed char positions).
#[test]
fn test_trace_col_distinguishes_literal_from_arg() {
    let tmp = TempDir::new().unwrap();
    let root = build(&tmp);

    let at_1  = trace_at(&root, "config.nim", 1, Some(1));
    let at_8  = trace_at(&root, "config.nim", 1, Some(8));
    let at_20 = trace_at(&root, "config.nim", 1, Some(20));

    assert_eq!(at_1["kind"],  "MacroBody", "col 1: {at_1}");
    assert_eq!(at_8["kind"],  "MacroArg",  "col 8: {at_8}");
    assert_eq!(at_20["kind"], "MacroArg",  "col 20: {at_20}");

    // Both MacroArg spans come from different params of the same macro call
    assert_eq!(at_8["param_name"],  "field",       "col 8 param: {at_8}");
    assert_eq!(at_20["param_name"], "default_val", "col 20 param: {at_20}");
}

// ── weaveback apply-back: literal line ───────────────────────────────────────────

/// Simulate an IDE edit: change `// version: 1.0` to `// version: 2.0` in
/// the generated `header.nim`.  `weaveback apply-back` must propagate the edit
/// back to driver.md — the literal source line in the `@file header.nim` chunk.
#[test]
fn test_apply_back_literal_line() {
    let tmp = TempDir::new().unwrap();
    let root = build(&tmp);

    // Verify initial state
    let initial = fs::read_to_string(root.join("driver.md")).unwrap();
    assert!(initial.contains("// version: 1.0"),
        "driver.md should contain '// version: 1.0' before apply-back");

    // Simulate IDE edit: update the version comment in the generated file
    let hdr_path = root.join("header.nim");
    let original = fs::read_to_string(&hdr_path).unwrap();
    let patched_hdr = original.replace("// version: 1.0", "// version: 2.0");
    assert_ne!(original, patched_hdr, "replacement should have changed something");
    fs::write(&hdr_path, &patched_hdr).unwrap();

    // Run apply-back
    let out = weaveback()
        .arg("--gen").arg(".")
        .arg("apply-back")
        .current_dir(&root)
        .output()
        .unwrap();

    // Print for diagnosis; apply-back exits 0 regardless of patch count
    eprintln!("stdout: {}", String::from_utf8_lossy(&out.stdout));
    eprintln!("stderr: {}", String::from_utf8_lossy(&out.stderr));

    let after = fs::read_to_string(root.join("driver.md")).unwrap();
    assert!(
        after.contains("// version: 2.0"),
        "driver.md should contain '// version: 2.0' after apply-back:\n{after}"
    );
    assert!(
        !after.contains("// version: 1.0"),
        "driver.md should no longer contain '// version: 1.0' after apply-back:\n{after}"
    );
}

// ── weaveback apply-back: multi-line insertion ──────────────────────────────────

/// Simulate an IDE edit: insert a new line between existing lines.
/// `weaveback apply-back` must attribute this insertion to the preceding line's
/// chunk and propagate it back to driver.md.
#[test]
fn test_apply_back_multi_line_insertion() {
    let tmp = TempDir::new().unwrap();
    let root = build(&tmp);

    // Simulate IDE edit: insert a new comment after the version line
    let hdr_path = root.join("header.nim");
    let content = fs::read_to_string(&hdr_path).unwrap();
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    lines.insert(1, "# added via dogfooding".to_string());
    fs::write(&hdr_path, lines.join("\n") + "\n").unwrap();

    // Run apply-back
    let out = weaveback()
        .arg("--gen").arg(".")
        .arg("apply-back")
        .current_dir(&root)
        .output()
        .unwrap();

    assert!(out.status.success(), "apply-back failed: {}", String::from_utf8_lossy(&out.stderr));

    let after = fs::read_to_string(root.join("driver.md")).unwrap();
    assert!(
        after.contains("# added via dogfooding"),
        "driver.md should contain the inserted line after apply-back:\n{after}"
    );
}

#[test]
fn test_apply_back_macro_body_after_source_shift() {
    let tmp = TempDir::new().unwrap();
    let root = build(&tmp);

    let driver_path = root.join("driver.md");
    let driver_before = fs::read_to_string(&driver_path).unwrap();
    fs::write(&driver_path, format!("# unrelated shift\n{driver_before}")).unwrap();

    let cfg_path = root.join("config.nim");
    let original = fs::read_to_string(&cfg_path).unwrap();
    let patched = original.replace("result.batchSize = 300", "settings.batchSize = 300");
    fs::write(&cfg_path, patched).unwrap();

    let out = weaveback()
        .arg("--gen").arg(".")
        .arg("apply-back")
        .current_dir(&root)
        .output()
        .unwrap();

    assert!(out.status.success(), "apply-back failed: {}", String::from_utf8_lossy(&out.stderr));

    let after = fs::read_to_string(&driver_path).unwrap();
    assert!(
        after.contains("settings.%(field) = %(default_val)"),
        "driver.md should contain updated macro body after shifted-source apply-back:\n{after}"
    );
}
