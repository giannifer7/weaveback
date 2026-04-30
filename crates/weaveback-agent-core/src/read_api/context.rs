// weaveback-agent-core/src/read_api/context.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use super::db::open_db;

pub(in crate::read_api) fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let count = trimmed.bytes().take_while(|&b| b == b'=').count();
    if count > 0 && trimmed.len() > count && trimmed.as_bytes()[count] == b' ' {
        Some(count)
    } else {
        None
    }
}

pub(in crate::read_api) fn section_range(lines: &[&str], def_start: usize) -> (usize, usize) {
    let mut sec_start = 0usize;
    let mut sec_level = 1usize;
    for idx in (0..def_start).rev() {
        if let Some(level) = heading_level(lines[idx]) {
            sec_start = idx;
            sec_level = level;
            break;
        }
    }

    let sec_end = lines[def_start..]
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| heading_level(line).is_some_and(|level| level <= sec_level))
        .map(|(idx, _)| def_start + idx)
        .unwrap_or(lines.len());

    (sec_start, sec_end)
}

pub(in crate::read_api) fn title_chain(lines: &[&str], def_start: usize) -> Vec<String> {
    let mut chain = Vec::new();
    for line in lines.iter().take(def_start) {
        if let Some(level) = heading_level(line) {
            let title = line[level + 1..].trim().to_string();
            chain.retain(|(existing_level, _): &(usize, String)| *existing_level < level);
            chain.push((level, title));
        }
    }
    chain.into_iter().map(|(_, title)| title).collect()
}

pub(in crate::read_api) fn extract_prose(lines: &[&str], start: usize, end: usize) -> String {
    let end = end.min(lines.len());
    let mut in_fence = false;
    let mut in_chunk = false;
    let mut out = Vec::new();

    for line in lines.iter().take(end).skip(start) {
        let trimmed = line.trim();
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
            out.push(*line);
        }
    }

    while out.first().is_some_and(|line| line.trim().is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|line| line.trim().is_empty()) {
        out.pop();
    }

    out.join("\n")
}
pub fn chunk_context(
    config: &WorkspaceConfig,
    file: &str,
    name: &str,
    nth: u32,
) -> Result<ChunkContext, String> {
    let db = open_db(config)?;
    let entry = db
        .get_chunk_def(file, name, nth)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chunk not found: {file}#{name}[{nth}]"))?;

    let src_path = config.project_root.join(file);
    let src_text = std::fs::read_to_string(&src_path)
        .map_err(|e| format!("Cannot read {}: {e}", src_path.display()))?;
    let src_lines: Vec<&str> = src_text.lines().collect();
    let def_start = entry.def_start as usize;
    let def_end = entry.def_end as usize;

    let body = if def_start < src_lines.len() && def_end <= src_lines.len() && def_end > 0 {
        src_lines[def_start..def_end - 1].join("\n")
    } else {
        String::new()
    };

    let section_breadcrumb = title_chain(&src_lines, def_start);
    let (sec_start, sec_end) = section_range(&src_lines, def_start);
    let prose = extract_prose(&src_lines, sec_start, sec_end);
    let raw_deps = db.query_chunk_deps(name).map_err(|e| e.to_string())?;
    let direct_dependencies = raw_deps.into_iter().map(|(name, _)| name).collect();
    let outputs = db.query_chunk_output_files(name).map_err(|e| e.to_string())?;

    Ok(ChunkContext {
        file: file.to_string(),
        name: name.to_string(),
        nth,
        section_breadcrumb,
        prose,
        body,
        direct_dependencies,
        outputs,
    })
}

