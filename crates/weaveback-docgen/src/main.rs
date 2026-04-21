mod adoc_scan;
mod d2;
mod inject;
mod literate_index;
mod plantuml;
mod render;
mod xref;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use xref::XrefEntry;

fn find_project_root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("cannot determine cwd");
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = std::fs::read_to_string(&cargo_toml).unwrap_or_default();
            if content.contains("[workspace]") {
                return dir;
            }
        }
        if !dir.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap()
}
const HELP: &str = "\
Usage: weaveback-docgen [OPTIONS]

Renders all .adoc files to HTML, post-processes them with chunk IDs and
a literate-source index, and (for Rust workspaces) injects cross-reference
data linking modules by their import graph.

Options:
  --out-dir   <path>   Output directory for rendered HTML
                       (default: <project-root>/docs/html)
  --theme-dir <path>   Directory containing docinfo.html / docinfo-footer.html
                       (default: <project-root>/scripts/asciidoc-theme)
  --special   <char>   De-duplicate doubled occurrences of CHAR before
                       passing .adoc files to the acdc renderer (repeatable)
  --xref-cmd  <cmd>    External command to supply cross-reference data.
                       Receives the project root as its first argument and
                       must print a JSON object matching HashMap<key, XrefEntry>
                       to stdout.  Replaces the built-in Rust scanner.
  --no-xref            Skip cross-reference analysis entirely.
  --ai-xref            Use LSP (rust-analyzer) to build precise cross-references.
  --plantuml-jar <path>  Path to plantuml.jar; renders [plantuml] blocks directly
                         (SVGs cached by BLAKE3).
  --help               Print this message and exit.

Cross-reference notes:
  The built-in scanner is Rust-specific: it parses .rs files with syn and
  expects workspace members under <project-root>/crates/.  Workspaces that
  place members elsewhere (root-level, packages/, libs/, ...) will not get
  automatic xref -- use --xref-cmd to supply data from an external tool, or
  --no-xref to skip it.
";
struct Args {
    specials: Vec<char>,
    xref_cmd: Option<String>,
    no_xref: bool,
    ai_xref: bool,
    out_dir: Option<PathBuf>,
    theme_dir: Option<PathBuf>,
    plantuml_jar: Option<PathBuf>,
    d2_theme: u32,
    d2_layout: String,
}

#[derive(serde::Deserialize, Default)]
struct DocsConfig {
    d2_theme: Option<u32>,
    d2_layout: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct WeavebackConfig {
    docs: Option<DocsConfig>,
}

fn read_config(root: &Path) -> WeavebackConfig {
    let path = root.join("weaveback.toml");
    if let Ok(content) = std::fs::read_to_string(&path) {
        toml::from_str(&content).unwrap_or_default()
    } else {
        WeavebackConfig::default()
    }
}

fn parse_args_from(raw: &[String]) -> Args {
    let mut specials = Vec::new();
    let mut xref_cmd = None;
    let mut no_xref = false;
    let mut ai_xref = false;
    let mut out_dir = None;
    let mut theme_dir = None;
    let mut plantuml_jar = None;
    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "--special" => {
                if let Some(s) = raw.get(i + 1) {
                    let mut chars = s.chars();
                    if let (Some(c), None) = (chars.next(), chars.next()) {
                        specials.push(c);
                    }
                    i += 2;
                    continue;
                }
            }
            "--xref-cmd" => {
                if let Some(cmd) = raw.get(i + 1) {
                    xref_cmd = Some(cmd.clone());
                    i += 2;
                    continue;
                }
            }
            "--out-dir" => {
                if let Some(p) = raw.get(i + 1) {
                    out_dir = Some(PathBuf::from(p));
                    i += 2;
                    continue;
                }
            }
            "--theme-dir" => {
                if let Some(p) = raw.get(i + 1) {
                    theme_dir = Some(PathBuf::from(p));
                    i += 2;
                    continue;
                }
            }
            "--plantuml-jar" => {
                if let Some(p) = raw.get(i + 1) {
                    plantuml_jar = Some(PathBuf::from(p));
                    i += 2;
                    continue;
                }
            }
            "--no-xref" => {
                no_xref = true;
            }
            "--ai-xref" => {
                ai_xref = true;
            }
            _ => {}
        }
        i += 1;
    }
    Args {
        specials,
        xref_cmd,
        no_xref,
        ai_xref,
        out_dir,
        theme_dir,
        plantuml_jar,
        d2_theme: 200,
        d2_layout: "elk".to_string(),
    }
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    parse_args_from(&raw)
}
fn run_xref_cmd(cmd: &str, project_root: &Path) -> HashMap<String, XrefEntry> {
    let output = Command::new(cmd)
        .arg(project_root)
        .output()
        .unwrap_or_else(|e| {
            eprintln!("xref-cmd: failed to run '{cmd}': {e}");
            std::process::exit(1);
        });
    if !output.status.success() {
        let code = output.status.code().unwrap_or(1);
        eprintln!("xref-cmd: '{cmd}' exited with status {code}");
        std::process::exit(code);
    }
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        eprintln!("xref-cmd: failed to parse JSON from '{cmd}': {e}");
        std::process::exit(1);
    })
}
fn main() {
    let root = find_project_root();
    let config = read_config(&root);
    let docs_cfg = config.docs.unwrap_or_default();

    let mut args = parse_args();
    if let Some(theme) = docs_cfg.d2_theme {
        args.d2_theme = theme;
    }
    if let Some(layout) = docs_cfg.d2_layout {
        args.d2_layout = layout;
    }

    let out_dir = args.out_dir.clone().unwrap_or_else(|| root.join("docs").join("html"));
    let theme_dir = args.theme_dir.clone().unwrap_or_else(|| root.join("scripts").join("asciidoc-theme"));

    let all_html = render::render_docs(
        &root,
        &theme_dir,
        &out_dir,
        &args.specials,
        args.plantuml_jar.as_deref(),
        args.d2_theme,
        &args.d2_layout,
    );
    let existing_html: std::collections::HashSet<String> = all_html
        .iter()
        .filter_map(|p| p.strip_prefix(&out_dir).ok())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .collect();

    let crates_dir = root.join("crates");

    let (xref_data, adoc_map) = if args.no_xref {
        (HashMap::new(), HashMap::new())
    } else if let Some(ref cmd) = args.xref_cmd {
        println!("xref: running '{cmd}'...");
        let data = run_xref_cmd(cmd, &root);
        println!("xref: {} entries", data.len());
        (data, HashMap::new())
    } else if crates_dir.exists() {
        println!("xref: analysing crates...");
        let data = xref::build_xref(&root, args.ai_xref);
        let adoc_map = xref::scan_adoc_file_declarations(&root, &crates_dir);
        println!("xref: {} modules indexed, {} adoc overrides", data.len(), adoc_map.len());
        (data, adoc_map)
    } else {
        (HashMap::new(), HashMap::new())
    };

    let xref_json_path = out_dir.join("xref.json");
    match serde_json::to_string_pretty(&xref_data) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&xref_json_path, &json) {
                eprintln!("xref: could not write {}: {}", xref_json_path.display(), e);
            } else {
                println!("xref: wrote {}", xref_json_path.display());
            }
        }
        Err(e) => eprintln!("xref: serialisation error: {}", e),
    }

    inject::rewrite_adoc_links(&out_dir);
    inject::inject_xref(&out_dir, &xref_data, &existing_html, &adoc_map);
    literate_index::generate_and_inject_all(&out_dir);
    inject::inject_chunk_ids(&out_dir);
}
#[cfg(test)]
mod tests {
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
}
