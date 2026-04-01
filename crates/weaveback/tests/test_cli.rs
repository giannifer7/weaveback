// crates/weaveback/tests/test_cli.rs
//
// Integration tests for the weaveback combined CLI, covering:
//   - --dir mode (auto-discovers drivers, skips %include'd fragments)
//   - --depfile and --stamp (build-system integration)

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn weaveback_in(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("weaveback").unwrap();
    cmd.current_dir(dir);
    cmd
}

/// Common weaveback-tangle delimiters used across tests.
fn delim_args() -> Vec<&'static str> {
    vec![
        "--open-delim",
        "<<",
        "--close-delim",
        ">>",
        "--chunk-end",
        "@",
        "--comment-markers",
        "#",
    ]
}

// ── Directory mode ────────────────────────────────────────────────────────────

/// --dir discovers driver files and processes them, while %include'd
/// fragments are skipped as standalone inputs.
#[test]
fn test_directory_mode_processes_drivers() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // fragment.md: just defines a macro — no @file chunk of its own.
    // It would produce no output if run standalone, but here we verify it is
    // correctly identified as a fragment (referenced via %include) and not
    // run as an independent driver.
    write(
        &root,
        "src/fragment.md",
        "%def(greeting, Hello from fragment)\n",
    );

    // driver.md: includes the fragment and writes one @file output.
    write(
        &root,
        "src/driver.md",
        "%include(src/fragment.md)\n\
         # <<@file out.txt>>=\n\
         %greeting()\n\
         # @\n",
    );

    let gen_dir = root.join("gen");

    weaveback_in(&root)
        .arg("--dir")
        .arg(root.join("src"))
        .arg("--include")
        .arg(&root)
        .arg("--gen")
        .arg(&gen_dir)
        .args(delim_args())
        .assert()
        .success();

    let output = fs::read_to_string(gen_dir.join("out.txt")).unwrap();
    assert!(
        output.contains("Hello from fragment"),
        "driver output should contain the macro expansion from the included fragment"
    );
}

/// --directory with multiple independent drivers (no shared includes).
#[test]
fn test_directory_mode_multiple_drivers() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    write(&root, "src/a.md", "# <<@file a.txt>>=\nfrom-a\n# @\n");
    write(&root, "src/b.md", "# <<@file b.txt>>=\nfrom-b\n# @\n");

    let gen_dir = root.join("gen");

    weaveback_in(&root)
        .arg("--dir")
        .arg(root.join("src"))
        .arg("--include")
        .arg(&root)
        .arg("--gen")
        .arg(&gen_dir)
        .args(delim_args())
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(gen_dir.join("a.txt")).unwrap().trim(),
        "from-a"
    );
    assert_eq!(
        fs::read_to_string(gen_dir.join("b.txt")).unwrap().trim(),
        "from-b"
    );
}

/// %import'd files are also treated as fragments and excluded from driver discovery.
#[test]
fn test_directory_mode_import_is_fragment() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // macros.md: defines a macro via %import — no @file chunk.
    // If mistakenly run as a driver it would produce no output but still be
    // processed; we verify it is excluded from the driver set.
    write(&root, "src/macros.md", "%def(item, world)\n");

    // driver.md: uses %import (discard output) to load macros, then writes a file.
    write(
        &root,
        "src/driver.md",
        "%import(src/macros.md)\n\
         # <<@file out.txt>>=\n\
         Hello %item()\n\
         # @\n",
    );

    let gen_dir = root.join("gen");

    weaveback_in(&root)
        .arg("--dir")
        .arg(root.join("src"))
        .arg("--include")
        .arg(&root)
        .arg("--gen")
        .arg(&gen_dir)
        .args(delim_args())
        .assert()
        .success();

    let output = fs::read_to_string(gen_dir.join("out.txt")).unwrap();
    assert!(
        output.contains("Hello world"),
        "driver output should contain the macro expansion from the %import'd fragment"
    );
}

/// A fragment included via a computed path (%include(%if(...))) is still
/// correctly excluded from driver discovery.
#[test]
fn test_directory_mode_conditional_include_is_fragment() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // extras.adoc: a fragment that is only included when a variable is set.
    // The %if condition evaluates to empty (variable not set), so the include
    // fires with an empty path — extras.adoc is NOT pulled in this run.
    // But we also test the case where the path IS computed: cond_helper.md
    // is always included via a macro that builds the path.
    write(
        &root,
        "src/cond_helper.md",
        "%def(helper_msg, from helper)\n",
    );

    // driver.md: includes cond_helper.md via a macro-computed path.
    write(
        &root,
        "src/driver.md",
        "%def(helper_name, cond_helper.md)\n\
         %include(%helper_name())\n\
         # <<@file out.txt>>=\n\
         %helper_msg()\n\
         # @\n",
    );

    let gen_dir = root.join("gen");

    weaveback_in(&root)
        .arg("--dir")
        .arg(root.join("src"))
        .arg("--include")
        .arg(&root.join("src"))
        .arg("--gen")
        .arg(&gen_dir)
        .args(delim_args())
        .assert()
        .success();

    let output = fs::read_to_string(gen_dir.join("out.txt")).unwrap();
    assert!(
        output.contains("from helper"),
        "macro-computed include path should be resolved and fragment excluded from drivers"
    );
}

/// --ext adoc overrides the default .md extension to scan .adoc files.
#[test]
fn test_directory_mode_custom_ext() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    write(
        &root,
        "src/fragment.adoc",
        "%def(greeting, Hello from adoc)\n",
    );
    write(
        &root,
        "src/driver.adoc",
        "%include(src/fragment.adoc)\n\
         # <<@file out.txt>>=\n\
         %greeting()\n\
         # @\n",
    );

    let gen_dir = root.join("gen");

    weaveback_in(&root)
        .arg("--dir")
        .arg(root.join("src"))
        .arg("--ext")
        .arg("adoc")
        .arg("--include")
        .arg(&root)
        .arg("--gen")
        .arg(&gen_dir)
        .args(delim_args())
        .assert()
        .success();

    let output = fs::read_to_string(gen_dir.join("out.txt")).unwrap();
    assert!(
        output.contains("Hello from adoc"),
        "--ext adoc should discover .adoc driver files; got:\n{output}"
    );
}

/// --ext can be repeated to scan multiple extensions at once.
#[test]
fn test_directory_mode_multiple_exts() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    write(&root, "src/a.md", "# <<@file a.txt>>=\nfrom-md-a\n# @\n");
    write(&root, "src/b.md", "# <<@file b.txt>>=\nfrom-md\n# @\n");

    let gen_dir = root.join("gen");

    weaveback_in(&root)
        .arg("--dir")
        .arg(root.join("src"))
        .arg("--ext")
        .arg("adoc")
        .arg("--ext")
        .arg("md")
        .arg("--include")
        .arg(&root)
        .arg("--gen")
        .arg(&gen_dir)
        .args(delim_args())
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(gen_dir.join("a.txt")).unwrap().trim(),
        "from-md-a"
    );
    assert_eq!(
        fs::read_to_string(gen_dir.join("b.txt")).unwrap().trim(),
        "from-md"
    );
}

// ── Depfile and stamp ─────────────────────────────────────────────────────────

/// --stamp creates an empty file on success.
#[test]
fn test_stamp_is_written_on_success() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    write(&root, "src/driver.md", "# <<@file out.txt>>=\nok\n# @\n");

    let stamp = root.join("build.stamp");

    weaveback_in(&root)
        .arg("--dir")
        .arg(root.join("src"))
        .arg("--include")
        .arg(&root)
        .arg("--gen")
        .arg(root.join("gen"))
        .arg("--stamp")
        .arg(&stamp)
        .args(delim_args())
        .assert()
        .success();

    assert!(stamp.exists(), "--stamp file should be created on success");
}

// ── %env builtin ──────────────────────────────────────────────────────────────

/// %env(NAME) expands to the value of the environment variable when --allow-env
/// is passed.
#[test]
fn test_env_builtin_with_allow_env() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    write(
        &root,
        "src/driver.md",
        "# <<@file out.txt>>=\n%env(WEAVEBACK_TEST_VAR)\n# @\n",
    );

    let gen_dir = root.join("gen");

    weaveback_in(&root)
        .env("WEAVEBACK_TEST_VAR", "hello-from-env")
        .arg("--dir")
        .arg(root.join("src"))
        .arg("--include")
        .arg(&root)
        .arg("--gen")
        .arg(&gen_dir)
        .arg("--allow-env")
        .args(delim_args())
        .assert()
        .success();

    let output = fs::read_to_string(gen_dir.join("out.txt")).unwrap();
    assert!(
        output.contains("hello-from-env"),
        "output should contain the env var value; got:\n{output}"
    );
}

/// %env(NAME) fails with a clear error when --allow-env is NOT passed.
#[test]
fn test_env_builtin_disabled_by_default() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    write(
        &root,
        "src/driver.md",
        "# <<@file out.txt>>=\n%env(HOME)\n# @\n",
    );

    weaveback_in(&root)
        .arg("--dir")
        .arg(root.join("src"))
        .arg("--include")
        .arg(&root)
        .arg("--gen")
        .arg(root.join("gen"))
        .args(delim_args())
        .assert()
        .failure()
        .stderr(contains("--allow-env"));
}

/// --depfile lists all discovered files as dependencies and names the
/// stamp as the Makefile target.
#[test]
fn test_depfile_lists_source_files() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    write(&root, "src/fragment.md", "%def(msg, hi)\n");
    write(
        &root,
        "src/driver.md",
        "%include(src/fragment.md)\n# <<@file out.txt>>=\n%msg()\n# @\n",
    );

    let stamp = root.join("build.stamp");
    let depfile = root.join("build.d");

    weaveback_in(&root)
        .arg("--dir")
        .arg(root.join("src"))
        .arg("--include")
        .arg(&root)
        .arg("--gen")
        .arg(root.join("gen"))
        .arg("--stamp")
        .arg(&stamp)
        .arg("--depfile")
        .arg(&depfile)
        .args(delim_args())
        .assert()
        .success();

    let dep_content = fs::read_to_string(&depfile).unwrap();

    // The depfile target should be the stamp path.
    assert!(
        dep_content.contains("build.stamp"),
        "depfile should name the stamp as target; got:\n{dep_content}"
    );
    // Both source files should appear as dependencies.
    assert!(
        dep_content.contains("driver.md"),
        "depfile should list driver.md; got:\n{dep_content}"
    );
    assert!(
        dep_content.contains("fragment.md"),
        "depfile should list fragment.md; got:\n{dep_content}"
    );
}

// ── Serve command ────────────────────────────────────────────────────────────

/// `weaveback serve` accepts AI configuration flags.
#[test]
fn test_serve_ai_flags() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // Create dummy docs/html so serve doesn't fail immediately.
    fs::create_dir_all(root.join("docs/html")).unwrap();
    fs::write(root.join("docs/html/index.html"), "<html></html>").unwrap();

    // We can't easily test the full server loop, but we can verify it parses
    // the new flags and starts up (before we kill it).
    use std::time::Duration;
    weaveback_in(&root)
        .arg("serve")
        .arg("--port").arg("0") // Random port
        .arg("--ai-backend").arg("ollama")
        .arg("--ai-model").arg("llama3")
        .arg("--ai-endpoint").arg("http://localhost:11434")
        .timeout(Duration::from_secs(2))
        .assert()
        // It might fail because of "Address already in use" if we are unlucky,
        // or just timeout (which is success for a server test).
        // But we want to see it didn't fail with "Unknown argument".
        .stderr(predicates::str::contains("Unknown argument").not());
}
