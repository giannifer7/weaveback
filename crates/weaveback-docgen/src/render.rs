use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::SystemTime;

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

fn theme_max_mtime(theme_dir: &Path) -> SystemTime {
    walkdir::WalkDir::new(theme_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| mtime(e.path()))
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH)
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
fn dedup_specials(content: &str, specials: &[char]) -> Option<String> {
    let mut out = content.to_owned();
    let mut changed = false;
    for &s in specials {
        let doubled = format!("{s}{s}");
        if out.contains(&doubled) {
            out = out.replace(&doubled, &s.to_string());
            changed = true;
        }
    }
    if changed { Some(out) } else { None }
}

/// Write preprocessed content alongside `original` as a hidden temp file.
/// Returns the temp path; the caller must delete it after asciidoctor runs.
fn write_temp_adoc(original: &Path, content: &str) -> Option<PathBuf> {
    let name = original.file_name()?.to_string_lossy();
    let temp = original.with_file_name(format!(".wbd-{name}"));
    std::fs::write(&temp, content).ok()?;
    Some(temp)
}
pub fn render_docs(project_root: &Path, theme_dir: &Path, out_dir: &Path, specials: &[char]) -> Vec<PathBuf> {
    std::fs::create_dir_all(out_dir).ok();

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
    let adoc_files = find_adoc_files(project_root);

    let mut stale_simple: Vec<PathBuf> = Vec::new();
    // Individual-render queue: (adoc, out, preprocessed_content).
    // preprocessed_content is Some(cleaned) when dedup is needed, None otherwise.
    let mut stale_individual: Vec<(PathBuf, PathBuf, Option<String>)> = Vec::new();
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

        let content = std::fs::read_to_string(adoc).unwrap_or_default();
        let preprocessed = dedup_specials(&content, specials);
        let needs_individual = has_diagram_blocks(adoc) || preprocessed.is_some();
        if needs_individual {
            stale_individual.push((adoc.clone(), out_file, preprocessed));
        } else {
            stale_simple.push(adoc.clone());
        }
    }

    if stale_simple.is_empty() && stale_individual.is_empty() {
        println!("docs: nothing to do");
        return all_html;
    }

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

    for (adoc, out_file, preprocessed) in &stale_individual {
        let label = adoc.strip_prefix(project_root).unwrap_or(adoc);
        let has_diag = has_diagram_blocks(adoc);
        println!(
            "docs: rendering {}{}",
            label.display(),
            match (has_diag, preprocessed.is_some()) {
                (true, true)  => " (diagrams + dedup)",
                (true, false) => " (diagrams)",
                (false, true) => " (dedup)",
                (false, false) => "",
            }
        );

        // Write temp file for preprocessing; use original path otherwise.
        let temp_path = preprocessed.as_deref().and_then(|c| write_temp_adoc(adoc, c));
        let render_path = temp_path.as_deref().unwrap_or(adoc);

        let mut args = base_args.clone();
        if has_diag {
            args.extend([
                "-a".into(),
                format!("imagesoutdir={}", out_file.parent().unwrap().display()),
            ]);
        }
        args.extend([
            "-o".into(),
            out_file.to_string_lossy().into_owned(),
            render_path.to_string_lossy().into_owned(),
        ]);
        let status = Command::new("asciidoctor")
            .args(&args)
            .status()
            .expect("failed to launch asciidoctor");

        // Clean up temp file whether or not asciidoctor succeeded.
        if let Some(ref tp) = temp_path {
            let _ = std::fs::remove_file(tp);
        }

        if !status.success() {
            eprintln!("asciidoctor failed: {}", adoc.display());
            std::process::exit(status.code().unwrap_or(1));
        }
    }

    all_html
}
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
                .is_some_and(|ext| ext == "adoc")
        })
        .map(|e| e.into_path())
        .collect();
    files.sort();
    files
}
