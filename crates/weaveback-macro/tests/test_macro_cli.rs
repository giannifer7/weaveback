// weaveback-macro/tests/test_macro_cli.rs
// I'd Really Rather You Didn't edit this generated file.

// crates/weaveback-macro/tests/test_macro_cli.rs

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// Helper function to create a file with content
fn create_test_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    let mut file = fs::File::create(&path).unwrap();
    write!(file, "{}", content).unwrap();
    path.canonicalize().unwrap()
}

// Helper to build and get command
fn cargo_weaveback_macro_cli() -> Result<escargot::CargoRun, Box<dyn std::error::Error>> {
    Ok(escargot::CargoBuild::new()
        .bin("weaveback-macro")
        .current_release()
        .current_target()
        .run()?)
}

#[test]
fn test_basic_macro_processing() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let input = create_test_file(
        &temp_path,
        "input.txt",
        r#"%def(hello, World)
Hello %hello()!"#,
    );
    assert!(input.exists(), "Input file should exist");

    let out_file = temp_path.join("output.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--output")
        .arg(&out_file)
        .arg(&input);

    let output = cmd.output()?;
    println!("Exit status: {}", output.status);
    println!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));

    assert!(output.status.success());
    assert!(out_file.exists(), "Output file should exist");

    let output_content = fs::read_to_string(&out_file)?;
    assert_eq!(output_content.trim(), "Hello World!");

    Ok(())
}

// 1) Test the help message
#[test]
fn test_cli_help() -> Result<(), Box<dyn std::error::Error>> {
    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--help");

    let output = cmd.output()?;
    assert!(
        output.status.success(),
        "Expected 'weaveback-macro --help' to succeed."
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("weaveback-macro"),
        "Help output did not mention 'weaveback-macro'"
    );
    assert!(
        stdout.contains("--output"),
        "Help output did not mention '--output'"
    );

    Ok(())
}

// 2) Test passing a non-existent input file
#[test]
fn test_missing_input_file() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let missing_input = temp_path.join("not_real.txt");
    let out_file = temp_path.join("output.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--output")
        .arg(&out_file)
        .arg(&missing_input);

    let output = cmd.output()?;
    assert!(
        !output.status.success(),
        "CLI was expected to fail on missing file."
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("(missing_input) stderr:\n{stderr}");
    assert!(
        stderr.contains("Input file does not exist"),
        "Should mention 'Input file does not exist' in error."
    );

    Ok(())
}

// 3) Test multiple input files in a single run
#[test]
fn test_multiple_inputs() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let input1 = create_test_file(
        &temp_path,
        "file1.txt",
        "%def(macro1, MACRO_ONE)\n%macro1()",
    );
    let input2 = create_test_file(
        &temp_path,
        "file2.txt",
        "%def(macro2, MACRO_TWO)\n%macro2()",
    );

    let out_file = temp_path.join("combined_output.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--output")
        .arg(&out_file)
        .arg(&input1)
        .arg(&input2);

    let output = cmd.output()?;
    assert!(output.status.success());

    let content = fs::read_to_string(&out_file)?;
    assert!(
        content.contains("MACRO_ONE"),
        "Expected 'MACRO_ONE' in combined output file."
    );
    assert!(
        content.contains("MACRO_TWO"),
        "Expected 'MACRO_TWO' in combined output file."
    );

    Ok(())
}

// 4) Test a custom sigil
#[test]
fn test_custom_sigil() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let input = create_test_file(
        &temp_path,
        "input_at.txt",
        "@def(test_macro, Hello from custom char)\n@test_macro()",
    );
    let out_file = temp_path.join("output_at.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--sigil")
        .arg("@")
        .arg("--output")
        .arg(&out_file)
        .arg(&input);

    let output = cmd.output()?;
    assert!(
        output.status.success(),
        "CLI run with custom sigil should succeed."
    );

    let content = fs::read_to_string(&out_file)?;
    assert!(
        content.contains("Hello from custom char"),
        "Expected to see expansion with '@' as the macro char."
    );

    Ok(())
}

#[test]
fn test_unicode_sigil() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let input = create_test_file(
        &temp_path,
        "input_section.txt",
        "§def(test_macro, Hello from unicode char)\n§test_macro()",
    );
    let out_file = temp_path.join("output_section.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--sigil")
        .arg("§")
        .arg("--output")
        .arg(&out_file)
        .arg(&input);

    let output = cmd.output()?;
    assert!(
        output.status.success(),
        "CLI run with unicode sigil should succeed."
    );

    let content = fs::read_to_string(&out_file)?;
    assert!(
        content.contains("Hello from unicode char"),
        "Expected to see expansion with '§' as the macro char."
    );

    Ok(())
}

// 5) Test using a colon-separated include path
#[test]
fn test_colon_separated_includes() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let includes_dir = temp_path.join("includes");
    fs::create_dir_all(&includes_dir)?;
    let _inc_file = create_test_file(&includes_dir, "my_include.txt", "From includes dir");

    let main_file = create_test_file(&temp_path, "main.txt", "%include(my_include.txt)");

    let out_file = temp_path.join("output_inc.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    let includes_str = format!(".:{}", includes_dir.to_string_lossy());

    cmd.arg("--include")
        .arg(&includes_str)
        .arg("--output")
        .arg(&out_file)
        .arg(&main_file);

    let output = cmd.output()?;
    assert!(
        output.status.success(),
        "CLI should succeed with colon-separated includes."
    );

    let content = fs::read_to_string(&out_file)?;
    assert!(
        content.contains("From includes dir"),
        "Expected the included content from includes/my_include.txt."
    );

    Ok(())
}

// 7) Test forcing a custom --pathsep
#[test]
fn test_custom_pathsep_includes() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let includes_dir = temp_path.join("my_includes");
    fs::create_dir_all(&includes_dir)?;
    create_test_file(&includes_dir, "m_incl.txt", "Inside custom pathsep dir");

    let main_file = create_test_file(&temp_path, "custom_sep_main.txt", "%include(m_incl.txt)");

    let out_file = temp_path.join("output_sep.txt");
    let includes_str = format!(".|{}", includes_dir.display());

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--include")
        .arg(&includes_str)
        .arg("--pathsep")
        .arg("|")
        .arg("--output")
        .arg(&out_file)
        .arg(&main_file);

    let output = cmd.output()?;
    assert!(output.status.success());

    let content = fs::read_to_string(&out_file)?;
    assert!(
        content.contains("Inside custom pathsep dir"),
        "Expected custom pathsep to locate includes dir."
    );

    Ok(())
}

// 8) Test that the CLI can handle a large input file (smoke test)
#[test]
fn test_large_input() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let mut big_content = String::new();
    big_content.push_str("%def(say, HELLO)\n");
    for _ in 0..10_000 {
        big_content.push_str("%say()");
        big_content.push('\n');
    }

    let big_file = create_test_file(&temp_path, "big_file.txt", &big_content);
    let out_file = temp_path.join("output_big.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--output")
        .arg(&out_file)
        .arg(&big_file);

    let output = cmd.output()?;
    assert!(
        output.status.success(),
        "CLI should handle a large input file."
    );

    let out_content = fs::read_to_string(&out_file)?;
    let line_count = out_content.matches("HELLO").count();
    assert_eq!(
        line_count, 10_000,
        "Expected 10,000 expansions of HELLO in the large output."
    );

    Ok(())
}

#[test]
fn test_undefined_variable_is_strict_by_default_cli() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let input = create_test_file(&temp_path, "strict_vars.txt", "before%(missing)after");
    let out_file = temp_path.join("strict_vars_out.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--output")
        .arg(&out_file)
        .arg(&input);

    let output = cmd.output()?;
    assert!(!output.status.success(), "undefined variable should fail by default");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Undefined variable: missing"),
        "expected undefined-variable error, got: {stderr}"
    );

    Ok(())
}

#[test]
fn test_unbound_params_are_strict_by_default_cli() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let input = create_test_file(
        &temp_path,
        "strict_params.txt",
        "%def(greet, name, msg, Hello %(name)%(msg)!)\n%greet(Alice)\n",
    );
    let out_file = temp_path.join("strict_params_out.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--output")
        .arg(&out_file)
        .arg(&input);

    let output = cmd.output()?;
    assert!(!output.status.success(), "missing args should fail by default");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unbound parameter 'msg' in macro 'greet'"),
        "expected unbound-parameter error, got: {stderr}"
    );

    Ok(())
}


#[test]
fn test_define_cli() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let input = create_test_file(&temp_path, "define.txt", "before%(name)after");
    let out_file = temp_path.join("define_out.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("-D")
        .arg("name=value")
        .arg("--output")
        .arg(&out_file)
        .arg(&input);

    let output = cmd.output()?;
    assert!(output.status.success(), "define should seed a top-level variable");

    let body = fs::read_to_string(&out_file)?;
    assert_eq!(body, "beforevalueafter");

    Ok(())
}

#[test]
fn test_env_prefix_cli() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let input = create_test_file(&temp_path, "env_prefix.txt", "%env(DEMO)");
    let out_file = temp_path.join("env_prefix_out.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.env("WB_DEMO", "scoped")
        .arg("--allow-env")
        .arg("--env-prefix")
        .arg("WB_")
        .arg("--output")
        .arg(&out_file)
        .arg(&input);

    let output = cmd.output()?;
    assert!(output.status.success(), "env-prefix should map to prefixed environment variables");

    let body = fs::read_to_string(&out_file)?;
    assert_eq!(body, "scoped");

    Ok(())
}

#[test]
fn test_recursion_limit_cli() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let temp_path = temp.path().canonicalize()?;

    let input = create_test_file(
        &temp_path,
        "recursion_limit.txt",
        "%def(loop, %loop())\n%loop()",
    );
    let out_file = temp_path.join("recursion_limit_out.txt");

    let run = cargo_weaveback_macro_cli()?;
    let mut cmd = run.command();
    cmd.arg("--recursion-limit")
        .arg("4")
        .arg("--output")
        .arg(&out_file)
        .arg(&input);

    let output = cmd.output()?;
    assert!(
        !output.status.success(),
        "CLI run with a self-recursive macro should fail once the configured recursion limit is reached."
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("maximum recursion depth (4) exceeded"),
        "expected configured recursion limit in stderr, got: {stderr}"
    );

    Ok(())
}

