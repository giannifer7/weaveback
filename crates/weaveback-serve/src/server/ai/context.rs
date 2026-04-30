// weaveback-serve/src/server/ai/context.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;


// ── AsciiDoc source helpers ───────────────────────────────────────────────────

/// Return the heading depth if `line` is an AsciiDoc `=`-style heading
/// (1 = `=`, 2 = `==`, …), otherwise `None`.
pub(crate) fn heading_level(line: &str) -> Option<usize> {
    let t = line.trim_end();
    if t.is_empty() { return None; }
    let count = t.bytes().take_while(|&b| b == b'=').count();
    if count > 0 && t.len() > count && t.as_bytes()[count] == b' ' {
        Some(count)
    } else {
        None
    }
}

/// Find the `(start, end)` line range (0-based, end exclusive) of the AsciiDoc
/// section that contains line `def_start`.  The section starts at the nearest
/// heading above `def_start` and ends just before the next heading at the same
/// or shallower nesting level.
pub(crate) fn section_range(lines: &[&str], def_start: usize) -> (usize, usize) {
    let mut sec_start = 0usize;
    let mut sec_level = 1usize;
    for i in (0..def_start).rev() {
        if let Some(level) = heading_level(lines[i]) {
            sec_start = i;
            sec_level = level;
            break;
        }
    }
    let sec_end = lines[def_start..]
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| heading_level(l).map(|lvl| lvl <= sec_level).unwrap_or(false))
        .map(|(i, _)| def_start + i)
        .unwrap_or(lines.len());
    (sec_start, sec_end)
}

/// Build the heading breadcrumb trail leading to `def_start`.
/// Returns titles from outermost to innermost, e.g.
/// `["Module overview", "Parsing", "Error recovery"]`.
pub(crate) fn title_chain(lines: &[&str], def_start: usize) -> Vec<String> {
    let mut chain: Vec<(usize, String)> = Vec::new();
    for line in lines.iter().take(def_start) {
        if let Some(level) = heading_level(line) {
            let title = line[level + 1..].trim().to_string();
            chain.retain(|(l, _)| *l < level);
            chain.push((level, title));
        }
    }
    chain.into_iter().map(|(_, t)| t).collect()
}

/// Extract all prose lines from `lines[start..end]`, skipping content inside
/// `----` listing-block fences and noweb chunk bodies.  The result is the
/// human-written narrative of the section — headings, paragraphs, admonitions,
/// lists — without any code.
///
/// Skipping chunk bodies here is defensive.  In well-formed literate sources,
/// chunk definitions should already live inside fenced code blocks.  If prose
/// extraction still encounters raw chunk markers in section text, that is a
/// source-structure problem and should eventually be reported by a linter
/// rather than silently normalized by every downstream consumer.
pub(crate) fn extract_prose(lines: &[&str], start: usize, end: usize) -> String {
    let end = end.min(lines.len());
    let mut in_fence = false;
    let mut in_chunk = false;
    let mut out: Vec<&str> = Vec::new();
    for l in lines.iter().take(end).skip(start) {
        let trimmed = l.trim();
        if trimmed == "----" {
            in_fence = !in_fence;
            continue;
        }
        if trimmed.starts_with("// <<") && trimmed.ends_with(">>=") {
            in_chunk = true;
            continue;
        }
        if trimmed == "// @" {
            in_chunk = false;
            continue;
        }
        if !in_fence && !in_chunk {
            out.push(l);
        }
    }
    // Trim leading/trailing blank lines.
    while out.first().map(|l| l.trim().is_empty()).unwrap_or(false) { out.remove(0); }
    while out.last().map(|l| l.trim().is_empty()).unwrap_or(false) { out.pop(); }
    out.join("\n")
}

/// Return the body text of each direct dependency of `chunk_name`.
/// Keys are chunk names; values are `{ "file": "…", "body": "…" }`.
pub fn dep_bodies(
    db: &weaveback_tangle::WeavebackDb,
    project_root: &Path,
    dep_names: &[(String, String)],
) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (dep_name, _) in dep_names {
        let defs = match db.find_chunk_defs_by_name(dep_name) {
            Ok(d) if !d.is_empty() => d,
            _ => continue,
        };
        let def = &defs[0];
        let src_path = project_root.join(&def.src_file);
        let src_text = match std::fs::read_to_string(&src_path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let src_lines: Vec<&str> = src_text.lines().collect();
        let s = def.def_start as usize;
        let e = def.def_end as usize;
        let body = if s < src_lines.len() && e <= src_lines.len() && e > 0 {
            src_lines[s..e - 1].join("\n")
        } else {
            String::new()
        };
        map.insert(dep_name.clone(), serde_json::json!({
            "file": def.src_file,
            "body": body,
        }));
    }
    map
}

/// Return recent `git log --oneline` entries for `src_file`.
pub fn git_log_for_file(project_root: &Path, src_file: &str) -> Vec<String> {
    let root = project_root.to_string_lossy();
    match std::process::Command::new("git")
        .args(["-C", &root, "log", "--follow", "-n", "5", "--oneline", "--", src_file])
        .output()
    {
        Ok(o) if o.status.success() =>
            String::from_utf8_lossy(&o.stdout).lines().map(|l| l.to_string()).collect(),
        _ => Vec::new(),
    }
}

// ── Context builder ───────────────────────────────────────────────────────────

pub(crate) fn build_chunk_context(
    project_root: &Path,
    file: &str,
    name: &str,
    nth: u32,
) -> serde_json::Value {
    let db_path = project_root.join("weaveback.db");
    let db = match weaveback_tangle::WeavebackDb::open_read_only(&db_path) {
        Ok(d) => d,
        Err(_) => return serde_json::Value::Null,
    };
    let entry = match db.get_chunk_def(file, name, nth) {
        Ok(Some(e)) => e,
        _ => return serde_json::Value::Null,
    };
    let src_path = project_root.join(file);
    let src_text = match std::fs::read_to_string(&src_path) {
        Ok(t) => t,
        Err(_) => return serde_json::Value::Null,
    };
    let src_lines: Vec<&str> = src_text.lines().collect();
    let def_start = entry.def_start as usize;
    let def_end   = entry.def_end   as usize;

    // Chunk body (lines between the open and close markers).
    let body = if def_start < src_lines.len() && def_end <= src_lines.len() && def_end > 0 {
        src_lines[def_start..def_end - 1].join("\n")
    } else {
        String::new()
    };

    // Section context: title breadcrumb + full prose of the enclosing section.
    let chain  = title_chain(&src_lines, def_start);
    let (sec_start, sec_end) = section_range(&src_lines, def_start);
    let section_prose = extract_prose(&src_lines, sec_start, sec_end);

    // Dependency graph.
    let raw_deps: Vec<(String, String)> = db.query_chunk_deps(name).unwrap_or_default();
    let dep_map = dep_bodies(&db, project_root, &raw_deps);
    let rev_deps: Vec<String> = db.query_reverse_deps(name)
        .unwrap_or_default()
        .into_iter().map(|(from, _)| from).collect();
    let output_files: Vec<String> = db.query_chunk_output_files(name).unwrap_or_default();

    // Recent git history for this source file.
    let log = git_log_for_file(project_root, file);

    serde_json::json!({
        "file":                 file,
        "name":                 name,
        "nth":                  nth,
        "body":                 body,
        "def_start":            entry.def_start,
        "def_end":              entry.def_end,
        "section_title_chain":  chain,
        "section_prose":        section_prose,
        "dependencies":         serde_json::Value::Object(dep_map),
        "reverse_dependencies": rev_deps,
        "output_files":         output_files,
        "git_log":              log,
    })
}

