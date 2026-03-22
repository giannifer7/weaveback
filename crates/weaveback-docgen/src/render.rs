use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::SystemTime;

// ── Helpers ───────────────────────────────────────────────────────────────────

const EXCLUDE_DIRS: &[&str] = &["target", ".git", "node_modules", ".venv"];

fn diagram_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^\[(?:plantuml|ditaa|graphviz|mermaid|a2s|blockdiag)").unwrap()
    })
}

fn has_diagram_blocks(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|s| diagram_re().is_match(&s))
        .unwrap_or(false)
}

fn mtime(path: &Path) -> SystemTime {
    path.metadata()
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn find_plantuml_jar() -> String {
    let candidates = [
        "/usr/share/java/plantuml/plantuml.jar".to_string(),
        format!(
            "{}/.local/share/plantuml/plantuml.jar",
            std::env::var("HOME").unwrap_or_default()
        ),
    ];
    candidates
        .iter()
        .find(|c| Path::new(c).is_file())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone())
}

fn find_plantuml_native() -> Option<String> {
    let candidates = [
        "/usr/bin/plantuml".to_string(),
        "/usr/local/bin/plantuml".to_string(),
        format!(
            "{}/.local/bin/plantuml",
            std::env::var("HOME").unwrap_or_default()
        ),
    ];
    candidates.into_iter().find(|c| Path::new(c).is_file())
}

fn theme_max_mtime(theme_dir: &Path) -> SystemTime {
    walkdir::WalkDir::new(theme_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| mtime(e.path()))
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Render all stale .adoc files to HTML, mirroring the gen_docs.py logic.
/// Returns the list of output HTML files (both fresh and newly rendered).
pub fn render_docs(project_root: &Path, theme_dir: &Path, out_dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(out_dir).ok();

    let plantuml_jar = find_plantuml_jar();
    unsafe { std::env::set_var("DIAGRAM_PLANTUML_CLASSPATH", &plantuml_jar) };
    let plantuml_native = find_plantuml_native();

    let mut base_args: Vec<String> = vec![
        "-r".into(),
        "asciidoctor-diagram".into(),
        "-a".into(),
        "source-highlighter=rouge".into(),
        "-a".into(),
        "rouge-css=class".into(),
        "-a".into(),
        "rouge-style=gruvbox".into(),
        "-a".into(),
        "docinfo=shared".into(),
        "-a".into(),
        format!("docinfodir={}", theme_dir.display()),
        "-a".into(),
        "imagesdir=.".into(),
    ];
    if let Some(native) = plantuml_native {
        base_args.push("-a".into());
        base_args.push(format!("plantuml-native={}", native));
    }

    let theme_mtime = theme_max_mtime(theme_dir);

    // Collect adoc files
    let adoc_files = find_adoc_files(project_root);

    let mut stale_simple: Vec<PathBuf> = Vec::new();
    let mut stale_diagram: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut all_html: Vec<PathBuf> = Vec::new();

    for adoc in &adoc_files {
        let rel = adoc.strip_prefix(project_root).unwrap_or(adoc);
        let out_file = out_dir.join(rel).with_extension("html");
        std::fs::create_dir_all(out_file.parent().unwrap()).ok();

        let adoc_mtime = mtime(adoc);
        let html_mtime = mtime(&out_file);

        all_html.push(out_file.clone());

        if out_file.exists() && html_mtime >= adoc_mtime && html_mtime >= theme_mtime {
            continue;
        }

        if has_diagram_blocks(adoc) {
            stale_diagram.push((adoc.clone(), out_file));
        } else {
            stale_simple.push(adoc.clone());
        }
    }

    if stale_simple.is_empty() && stale_diagram.is_empty() {
        println!("docs: nothing to do");
        return all_html;
    }

    // Batch render simple files
    if !stale_simple.is_empty() {
        println!("docs: rendering {} file(s) (batch)", stale_simple.len());
        let mut args = base_args.clone();
        args.extend([
            "-R".into(),
            project_root.to_string_lossy().into_owned(),
            "-D".into(),
            out_dir.to_string_lossy().into_owned(),
        ]);
        for f in &stale_simple {
            args.push(f.to_string_lossy().into_owned());
        }
        let status = Command::new("asciidoctor")
            .args(&args)
            .status()
            .expect("failed to launch asciidoctor");
        if !status.success() {
            eprintln!("asciidoctor batch failed");
            std::process::exit(status.code().unwrap_or(1));
        }
    }

    // Individual render for diagram files
    for (adoc, out_file) in &stale_diagram {
        println!(
            "docs: rendering {} (diagrams)",
            adoc.strip_prefix(project_root)
                .unwrap_or(adoc)
                .display()
        );
        let mut args = base_args.clone();
        args.extend([
            "-a".into(),
            format!(
                "imagesoutdir={}",
                out_file.parent().unwrap().display()
            ),
            "-o".into(),
            out_file.to_string_lossy().into_owned(),
            adoc.to_string_lossy().into_owned(),
        ]);
        let status = Command::new("asciidoctor")
            .args(&args)
            .status()
            .expect("failed to launch asciidoctor");
        if !status.success() {
            eprintln!("asciidoctor failed: {}", adoc.display());
            std::process::exit(status.code().unwrap_or(1));
        }
    }

    all_html
}

// ── File discovery ────────────────────────────────────────────────────────────

fn find_adoc_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                !EXCLUDE_DIRS.iter().any(|ex| name == *ex)
            } else {
                true
            }
        })
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "adoc")
        })
        .map(|e| e.into_path())
        .collect();
    files.sort();
    files
}
