// crates/weaveback/tests/test_trace.rs
//
// Integration tests for `weaveback where` and `weaveback trace`.
// Macro-level tracing is always on; no --trace flag needed.
// Also includes wall-clock timing to catch regressions.

use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::Instant;
use tempfile::TempDir;

fn weaveback() -> Command {
    Command::cargo_bin("weaveback").unwrap()
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Run weaveback on a single driver file, returning the canonicalized temp directory.
fn run_weaveback(tmp: &TempDir, source: &str) -> std::path::PathBuf {
    let root = tmp.path().canonicalize().unwrap();
    write(&root, "driver.md", source);
    weaveback()
        .arg("--gen").arg(".")
        .arg("driver.md")
        .current_dir(&root)
        .assert()
        .success();
    root
}

/// Run `weaveback where <out_file> <line>` from `dir` and parse JSON output.
fn where_json(dir: &Path, out_file: &str, line: u32) -> Value {
    let out = weaveback()
        .arg("--gen").arg(".")
        .arg("where").arg(out_file).arg(line.to_string())
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "weaveback where failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("bad JSON from 'weaveback where': {e}\nstdout: {}", String::from_utf8_lossy(&out.stdout)))
}

/// Run `weaveback trace <out_file> <line> --col <col>` from `dir` and parse JSON output.
fn trace_col_json(dir: &Path, out_file: &str, line: u32, col: u32) -> Value {
    let out = weaveback()
        .arg("--gen").arg(".")
        .arg("trace").arg(out_file).arg(line.to_string())
        .arg("--col").arg(col.to_string())
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "weaveback trace --col failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("bad JSON from 'weaveback trace --col': {e}\nstdout: {}", String::from_utf8_lossy(&out.stdout)))
}

/// Run `weaveback trace <out_file> <line>` from `dir` and parse JSON output.
fn trace_json(dir: &Path, out_file: &str, line: u32) -> Value {
    let out = weaveback()
        .arg("--gen").arg(".")
        .arg("trace").arg(out_file).arg(line.to_string())
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "weaveback trace failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("bad JSON from 'weaveback trace': {e}\nstdout: {}\nstderr: {}", 
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)))
}

// ── Correctness: weaveback where ──────────────────────────────────────────────────

/// Plain literal line: `weaveback where` returns the correct chunk, file, and line.
#[test]
fn test_where_literal_line() {
    let tmp = TempDir::new().unwrap();
    let root = run_weaveback(&tmp,
        "# <[@file out.txt]>=\nHello from chunk\n# @\n",
    );

    let j = where_json(&root, "out.txt", 1);
    assert_eq!(j["out_file"], "out.txt");
    assert_eq!(j["out_line"], 1);
    assert_eq!(j["chunk"], "@file out.txt");
    assert!(j["expanded_file"].as_str().unwrap().contains("driver.md"),
        "expanded_file should reference driver.md, got: {}", j["expanded_file"]);
    assert_eq!(j["indent"], "");
}

/// Multi-line chunk: each output line maps back to its own position.
#[test]
fn test_where_multi_line_chunk() {
    let tmp = TempDir::new().unwrap();
    let root = run_weaveback(&tmp,
        "# <[@file out.txt]>=\nline one\nline two\nline three\n# @\n",
    );

    for (line_num, _expected) in [(1u32, "line one"), (2, "line two"), (3, "line three")] {
        let j = where_json(&root, "out.txt", line_num);
        assert_eq!(j["out_line"], line_num, "wrong out_line for line {line_num}");
        assert_eq!(j["chunk"], "@file out.txt", "wrong chunk for line {line_num}");
    }
}

/// Indented chunk reference: indent is preserved in the map entry.
#[test]
fn test_where_indented_chunk_ref() {
    let tmp = TempDir::new().unwrap();
    let root = run_weaveback(&tmp,
        "# <[body]>=\nreturn 0;\n# @\n\
         # <[@file main.c]>=\nint main() {\n    # <[body]>\n}\n# @\n",
    );

    // line 1 = "int main() {"   → literal, no indent
    // line 2 = "    return 0;"  → from 'body', indented with 4 spaces
    // line 3 = "}"              → literal
    let j2 = where_json(&root, "main.c", 2);
    assert_eq!(j2["chunk"], "body");
    assert_eq!(j2["indent"], "    ",
        "expected 4-space indent, got: {:?}", j2["indent"]);
}

/// Querying a line beyond the output returns "No mapping found" (not an error).
#[test]
fn test_where_no_mapping_for_out_of_range_line() {
    let tmp = TempDir::new().unwrap();
    let root = run_weaveback(&tmp,
        "# <[@file out.txt]>=\nOnly one line\n# @\n",
    );

    let out = weaveback()
        .arg("--gen").arg(".")
        .arg("where").arg("out.txt").arg("999")
        .current_dir(&root)
        .output()
        .unwrap();

    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("No mapping"), "expected 'No mapping found', got: {stderr}");
}

// ── Correctness: weaveback trace ──────────────────────────────────────────────────

/// Literal text maps to kind = "Literal".
#[test]
fn test_trace_literal_has_kind_literal() {
    let tmp = TempDir::new().unwrap();
    let root = run_weaveback(&tmp,
        "# <[@file out.txt]>=\nHello literal\n# @\n",
    );

    let j = trace_json(&root, "out.txt", 1);
    assert_eq!(j["kind"], "Literal",
        "expected Literal, full JSON: {j}");
    assert!(j["src_file"].as_str().unwrap().contains("driver.md"));
}

/// Macro body expansion maps to kind = "MacroBody" with the macro name.
#[test]
fn test_trace_macro_body() {
    let tmp = TempDir::new().unwrap();
    let root = run_weaveback(&tmp,
        "%def(greeting, Hello World)\n\
         # <[@file out.txt]>=\n\
         %greeting()\n\
         # @\n",
    );

    let j = trace_json(&root, "out.txt", 1);
    assert_eq!(j["kind"], "MacroBody",
        "expected MacroBody, full JSON: {j}");
    assert_eq!(j["macro_name"], "greeting");
}

/// Macro argument substitution maps to kind = "MacroArg".
#[test]
fn test_trace_macro_arg() {
    let tmp = TempDir::new().unwrap();
    let root = run_weaveback(&tmp,
        "%def(wrap, x, [x])\n\
         # <[@file out.txt]>=\n\
         %wrap(hello)\n\
         # @\n",
    );

    // The output line is "[hello]" — "hello" comes from the argument.
    // The dominant span could be MacroArg or MacroBody depending on what spans most.
    let j = trace_json(&root, "out.txt", 1);
    let kind = j["kind"].as_str().unwrap_or("");
    assert!(
        kind == "MacroArg" || kind == "MacroBody",
        "expected MacroArg or MacroBody for macro with arg substitution, got: {kind}"
    );
}

/// `weaveback trace` always returns macro-level fields (tracing is unconditional).
#[test]
fn test_trace_always_has_macro_fields() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    write(&root, "driver.md",
        "%def(msg, hi)\n\
         # <[@file out.txt]>=\n\
         %msg()\n\
         # @\n",
    );

    weaveback()
        .arg("--gen").arg(".")
        .arg("driver.md")
        .current_dir(&root)
        .assert()
        .success();

    let j = trace_json(&root, "out.txt", 1);
    // Noweb fields present
    assert!(j.get("chunk").is_some(), "chunk field missing");
    assert!(j.get("expanded_file").is_some(), "expanded_file field missing");
    // Macro fields present (tracing is always on)
    assert!(j.get("kind").is_some(),
        "kind should be present, got: {j}");
    assert_eq!(j["kind"], "MacroBody");
    assert_eq!(j["macro_name"], "msg");
}

// ── Column tracing with multi-byte characters ─────────────────────────────────

/// Output line: "hello 世界!" — ASCII prefix, then two CJK chars (3 bytes each), then '!'.
/// The macro arg "世界" spans chars 7-8; the surrounding literal spans chars 1-6 and 9.
/// Verifies that --col with 1-indexed character positions correctly distinguishes spans
/// across the byte boundary between ASCII and multi-byte characters.
#[test]
fn test_trace_col_multibyte_distinguishes_spans() {
    let tmp = TempDir::new().unwrap();
    // Output: "hello 世界!"
    //  char:   123456 78   9
    //  MacroBody: h,e,l,l,o,' ' (chars 1-6) and '!' (char 9)
    //  MacroArg:  世,界 (chars 7-8)
    let root = run_weaveback(&tmp,
        "%def(greet, lang, hello %(lang)!)\n\
         # <[@file out.txt]>=\n\
         %greet(世界)\n\
         # @\n",
    );

    let j_ascii  = trace_col_json(&root, "out.txt", 1, 1); // 'h' → MacroBody
    let j_first  = trace_col_json(&root, "out.txt", 1, 7); // '世' → MacroArg
    let j_second = trace_col_json(&root, "out.txt", 1, 8); // '界' → MacroArg
    let j_suffix = trace_col_json(&root, "out.txt", 1, 9); // '!' → MacroBody

    assert_eq!(j_ascii["kind"],  "MacroBody", "char 1 ('h'): {j_ascii}");
    assert_eq!(j_first["kind"],  "MacroArg",  "char 7 ('世'): {j_first}");
    assert_eq!(j_second["kind"], "MacroArg",  "char 8 ('界'): {j_second}");
    assert_eq!(j_suffix["kind"], "MacroBody", "char 9 ('!'): {j_suffix}");

    assert_eq!(j_first["param_name"],  "lang", "param_name for '世'");
    assert_eq!(j_second["param_name"], "lang", "param_name for '界'");
}

/// Same with 2-byte characters (é = U+00E9): col picks the right span across
/// a 2-byte boundary.
#[test]
fn test_trace_col_two_byte_chars() {
    let tmp = TempDir::new().unwrap();
    // Output: "héllo résumé!"
    // %def(fmt, word, héllo %(word)!)  →  %fmt(résumé)  →  "héllo résumé!"
    //  h(1)é(2)l(3)l(4)o(5) (6)r(7)é(8)s(9)u(10)m(11)é(12)!(13)
    //  MacroBody: "héllo " (chars 1-6) and "!" (char 13)
    //  MacroArg:  "résumé" (chars 7-12)
    let root = run_weaveback(&tmp,
        "%def(fmt, word, héllo %(word)!)\n\
         # <[@file out.txt]>=\n\
         %fmt(résumé)\n\
         # @\n",
    );

    let j_body = trace_col_json(&root, "out.txt", 1, 1);  // 'h' → MacroBody
    let j_arg  = trace_col_json(&root, "out.txt", 1, 7);  // 'r' of résumé → MacroArg
    let j_e    = trace_col_json(&root, "out.txt", 1, 8);  // 'é' of résumé → MacroArg
    let j_end  = trace_col_json(&root, "out.txt", 1, 13); // '!' → MacroBody

    assert_eq!(j_body["kind"], "MacroBody", "char 1 ('h'): {j_body}");
    assert_eq!(j_arg["kind"],  "MacroArg",  "char 7 ('r'): {j_arg}");
    assert_eq!(j_e["kind"],    "MacroArg",  "char 8 ('é'): {j_e}");
    assert_eq!(j_end["kind"],  "MacroBody", "char 13 ('!'): {j_end}");
}

// ── Speed: absolute timing ────────────────────────────────────────────────────

/// Generate a source file with `n_macros` definitions and `n_lines` output lines.
fn generate_source(n_macros: usize, n_lines: usize) -> String {
    let mut s = String::new();
    for i in 0..n_macros {
        s.push_str(&format!("%def(m{i}, value_{i})\n"));
    }
    s.push_str("# <[@file output.txt]>=\n");
    for i in 0..n_lines {
        s.push_str(&format!("%m{}()\n", i % n_macros));
    }
    s.push_str("# @\n");
    s
}

/// Time a single weaveback run; returns elapsed milliseconds.
fn time_run(source: &str) -> u128 {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    fs::write(root.join("driver.md"), source).unwrap();

    let start = Instant::now();
    let mut cmd = std::process::Command::new(cargo_bin("weaveback"));
    cmd.arg("--gen").arg(".");
    cmd.arg("driver.md").current_dir(&root);
    let status = cmd.status().unwrap();
    let elapsed = start.elapsed().as_millis();
    assert!(status.success());
    elapsed
}

/// Medium workload: 50 macros × 500 output lines.
#[test]
fn test_trace_speed_medium() {
    let source = generate_source(50, 500);

    let _ = time_run(&source); // warm-up

    let n = 3usize;
    let avg: u128 = (0..n).map(|_| time_run(&source)).sum::<u128>() / n as u128;

    eprintln!("\n── Trace speed (medium: 50 macros × 500 lines) ──");
    eprintln!("  avg : {avg} ms  (avg of {n})");
    eprintln!("─────────────────────────────────────────────────");

    assert!(avg < 2000, "medium workload took {avg} ms, expected < 2000 ms");
}

/// Large workload: 200 macros × 2 000 output lines.
#[test]
fn test_trace_speed_large() {
    let source = generate_source(200, 2000);

    let _ = time_run(&source); // warm-up
    let elapsed = time_run(&source);

    eprintln!("\n── Trace speed (large: 200 macros × 2000 lines) ─");
    eprintln!("  elapsed : {elapsed} ms");
    eprintln!("─────────────────────────────────────────────────");

    assert!(elapsed < 5000, "large workload took {elapsed} ms, expected < 5000 ms");
}

// ── Path Normalization and Config Recording ──────────────────────────────────

/// Verifies that `weaveback trace` correctly normalizes paths with common prefixes.
#[test]
fn test_trace_path_normalization() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    
    // Create a "crates" subdirectory to mimic the workspace layout.
    let crates_dir = root.join("crates");
    fs::create_dir_all(&crates_dir).unwrap();
    
    write(&crates_dir, "foo.md", "# <[@file out.txt]>=\nHello path normalization\n# @\n");
    
    weaveback()
        .arg("--dir").arg("crates")
        .arg("--gen").arg("gen")
        .arg("--ext").arg("md")
        .current_dir(&root)
        .assert()
        .success();

    // The DB stores "out.txt". We query with "gen/out.txt" and "crates/gen/out.txt".
    let j1 = trace_json(&root, "gen/out.txt", 1);
    assert_eq!(j1["out_file"], "gen/out.txt");
    assert_eq!(j1["chunk"], "@file out.txt");

    // Even with a deeper prefix, it should find it.
    let out = weaveback()
        .arg("--gen").arg("gen")
        .arg("trace").arg("crates/gen/out.txt").arg("1")
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(out.status.success(), "trace failed for prefixed path: {}", String::from_utf8_lossy(&out.stderr));
}

/// Verifies that the special character is correctly recorded and used by apply-back.
#[test]
fn test_apply_back_config_recording() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    
    // Use a non-standard special char '^'
    write(&root, "source.md", "^def(val, hello)\n# <[@file out.txt]>=\n^val()\n# @\n");
    
    weaveback()
        .arg("--special").arg("^")
        .arg("--gen").arg("gen")
        .arg("source.md")
        .current_dir(&root)
        .assert()
        .success();

    // Modify the generated file.
    write(&root, "gen/out.txt", "world\n");

    // Run apply-back. It should use the stored '^' config to verify the fix.
    // Without config recording, it would default to '%' and fail to verify the macro call.
    let out = weaveback()
        .arg("--gen").arg("gen")
        .arg("apply-back")
        .current_dir(&root)
        .output()
        .unwrap();
    
    if !out.status.success() {
        panic!("apply-back failed: {}\nstdout: {}\nstderr: {}", 
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr));
    }
    println!("apply-back stdout: {}", String::from_utf8_lossy(&out.stdout));
    println!("apply-back stderr: {}", String::from_utf8_lossy(&out.stderr));

    let patched = fs::read_to_string(root.join("source.md")).unwrap();
    assert!(patched.contains("^def(val, world)"), "apply-back failed to patch with correct special char: {}", patched);
}
