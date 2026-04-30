// weaveback-serve/src/server/source_watcher.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub(crate) fn find_docgen_bin() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("weaveback-docgen");
        if sibling.exists() { return sibling; }
    }
    PathBuf::from("weaveback-docgen")
}

fn find_plantuml_jar() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PLANTUML_JAR") {
        let jar = PathBuf::from(path);
        if jar.exists() {
            return Some(jar);
        }
    }

    let default = PathBuf::from("/usr/share/java/plantuml/plantuml.jar");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}

fn run_rebuild(project_root: &Path, tangle: bool, theme: bool) {
    if tangle {
        eprintln!("wb-serve --watch: tangle...");
        let exe = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::from("weaveback"));
        let ok = std::process::Command::new(&exe)
            .arg("tangle")
            .current_dir(project_root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok { eprintln!("wb-serve --watch: tangle failed"); return; }
    }
    if theme {
        eprintln!("wb-serve --watch: theme...");
        let ok = std::process::Command::new("node")
            .arg(project_root.join("scripts").join("serve-ui").join("build.mjs"))
            .current_dir(project_root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok { eprintln!("wb-serve --watch: theme build failed"); return; }
    }
    eprintln!("wb-serve --watch: docs...");
    let mut cmd = std::process::Command::new(find_docgen_bin());
    cmd.args(["--sigil", "%", "--sigil", "^"])
        .current_dir(project_root);
    if let Some(jar) = find_plantuml_jar() {
        cmd.arg("--plantuml-jar").arg(jar);
    }
    let _ = cmd.status();
}

pub(in crate::server) fn spawn_source_watcher(project_root: PathBuf) {
    use std::time::Duration;
    thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => { eprintln!("wb-serve: source watcher error: {e}"); return; }
        };
        if let Err(e) = watcher.watch(&project_root, RecursiveMode::Recursive) {
            eprintln!("wb-serve: source watch error: {e}");
            return;
        }
        let docs_html  = project_root.join("docs").join("html");
        let target_dir = project_root.join("target");
        let theme_src  = project_root.join("scripts").join("serve-ui").join("src");
        while let Ok(first) = rx.recv() {
            let mut need_tangle = false;
            let mut need_theme  = false;
            if let Ok(event) = first {
                for p in &event.paths {
                    if p.starts_with(&docs_html) || p.starts_with(&target_dir) { continue; }
                    if p.extension().is_some_and(|e| e == "adoc") { need_tangle = true; }
                    if p.starts_with(&theme_src) { need_theme = true; }
                }
            }
            while let Ok(Ok(event)) = rx.recv_timeout(Duration::from_millis(500)) {
                for p in &event.paths {
                    if p.starts_with(&docs_html) || p.starts_with(&target_dir) { continue; }
                    if p.extension().is_some_and(|e| e == "adoc") { need_tangle = true; }
                    if p.starts_with(&theme_src) { need_theme = true; }
                }
            }
            if need_tangle || need_theme {
                run_rebuild(&project_root, need_tangle, need_theme);
            }
        }
        drop(watcher);
    });
}

