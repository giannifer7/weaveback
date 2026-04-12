use weaveback_macro::evaluator::{EvalConfig, Evaluator};
use weaveback_macro::macro_api::process_string;
use weaveback_tangle::db::{WeavebackDb, DbError, NowebMapEntry};
use weaveback_tangle::lookup::find_best_noweb_entry;
use weaveback_core::PathResolver;
use weaveback_lsp::LspClient;
use regex::Regex;
use similar::TextDiff;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use crate::lookup;

#[derive(Debug)]
pub enum ApplyBackError {
    Db(DbError),
    Io(std::io::Error),
    Lookup(lookup::LookupError),
}

impl From<DbError> for ApplyBackError {
    fn from(e: DbError) -> Self { ApplyBackError::Db(e) }
}
impl From<std::io::Error> for ApplyBackError {
    fn from(e: std::io::Error) -> Self { ApplyBackError::Io(e) }
}
impl From<lookup::LookupError> for ApplyBackError {
    fn from(e: lookup::LookupError) -> Self { ApplyBackError::Lookup(e) }
}

impl std::fmt::Display for ApplyBackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyBackError::Db(e)     => write!(f, "database error: {e}"),
            ApplyBackError::Io(e)     => write!(f, "I/O error: {e}"),
            ApplyBackError::Lookup(e) => write!(f, "trace lookup error: {e:?}"),
        }
    }
}

pub struct ApplyBackOptions {
    pub db_path: PathBuf,
    pub gen_dir: PathBuf,
    pub dry_run: bool,
    /// Relative paths within gen/ to process; empty = all modified files.
    pub files: Vec<String>,
    /// When present, enables two-level tracing through macro expansion.
    pub eval_config: Option<EvalConfig>,
}

/// Where a patch lands and how to apply it.
enum PatchSource {
    /// Hunk from noweb-level expanded text (no macro attribution available).
    Noweb { src_file: String, src_line: usize, len: usize },
    /// Literal text from the original literate source — safe to auto-patch.
    Literal { src_file: String, src_line: usize, len: usize },
    /// Macro body text with no variable references — safe to auto-patch.
    MacroBodyLiteral { src_file: String, src_line: usize, macro_name: String },
    /// Macro body text containing `%(...)` references.
    /// Attempt structural fix + oracle verification; report if it fails.
    MacroBodyWithVars { src_file: String, src_line: usize, macro_name: String },
    /// Argument value at a macro call site.
    /// Attempt col-based replacement + oracle verification; report if it fails.
    MacroArg {
        src_file: String,
        src_line: usize,
        src_col: u32,
        macro_name: String,
        param_name: String,
    },
    /// VarBinding or Computed — report only.
    Unpatchable { src_file: String, src_line: usize, kind_label: String },
}

impl PatchSource {
    fn src_file(&self) -> &str {
        match self {
            PatchSource::Noweb              { src_file, .. }
            | PatchSource::Literal          { src_file, .. }
            | PatchSource::MacroBodyLiteral { src_file, .. }
            | PatchSource::MacroBodyWithVars{ src_file, .. }
            | PatchSource::MacroArg         { src_file, .. }
            | PatchSource::Unpatchable      { src_file, .. } => src_file,
        }
    }
}

fn patch_source_rank(source: &PatchSource) -> i32 {
    match source {
        PatchSource::MacroArg { .. } => 50,
        PatchSource::Literal { .. } => 40,
        PatchSource::MacroBodyLiteral { .. } => 35,
        PatchSource::MacroBodyWithVars { .. } => 30,
        PatchSource::Noweb { .. } => 20,
        PatchSource::Unpatchable { .. } => 0,
    }
}

fn patch_source_location(source: &PatchSource) -> (&str, usize) {
    match source {
        PatchSource::Noweb { src_file, src_line, .. }
        | PatchSource::Literal { src_file, src_line, .. }
        | PatchSource::MacroBodyLiteral { src_file, src_line, .. }
        | PatchSource::MacroBodyWithVars { src_file, src_line, .. }
        | PatchSource::MacroArg { src_file, src_line, .. }
        | PatchSource::Unpatchable { src_file, src_line, .. } => (src_file, *src_line),
    }
}

struct Patch {
    source: PatchSource,
    /// Indent-stripped baseline gen/ text (may be multiple lines).
    old_text: String,
    /// Indent-stripped modified gen/ text (may be multiple lines).
    new_text: String,
    /// 0-indexed first line in the macro-expanded intermediate.
    expanded_line: u32,
}

struct CandidateResolution {
    line_idx: usize,
    new_line: String,
    score: i32,
}

#[derive(Clone)]
struct LspDefinitionHint {
    src_file: String,
    src_line: usize,
}

struct MacroArgSearch<'a> {
    db: &'a WeavebackDb,
    lines: &'a [String],
    hinted_line: usize,
    src_col: u32,
    old_text: &'a str,
    new_text: &'a str,
    eval_config: &'a EvalConfig,
    src_path: &'a std::path::Path,
    expanded_line: u32,
}

struct MacroBodySearch<'a> {
    db: &'a WeavebackDb,
    lines: &'a [String],
    hinted_line: usize,
    body_template: Option<&'a str>,
    old_text: &'a str,
    new_text: &'a str,
    sigil: char,
    eval_config: &'a EvalConfig,
    src_path: &'a std::path::Path,
    expanded_line: u32,
}

struct MacroCallSearch<'a> {
    lines: &'a [String],
    macro_name: &'a str,
    sigil: char,
    old_text: &'a str,
    new_text: &'a str,
    eval_config: &'a EvalConfig,
    src_path: &'a std::path::Path,
    expanded_line: u32,
}

/// Search `lines` in a ±`window` range around `center` for a unique line that
/// matches a whitespace-normalised regex derived from `needle`.
///
/// Returns the 0-indexed line index on a unique match; `None` if not found or
/// ambiguous.
fn fuzzy_find_line(lines: &[String], center: usize, needle: &str, window: usize) -> Option<usize> {
    let trimmed = needle.trim();
    if trimmed.is_empty() { return None; }

    // Build a pattern that tolerates interior whitespace changes.
    let escaped = regex::escape(trimmed);
    let parts: Vec<&str> = escaped.split(r"\ ").collect();
    let pattern = parts.join(r"\s+");
    let re = Regex::new(&format!(r"^\s*{}\s*$", pattern)).ok()?;

    let lo = center.saturating_sub(window);
    let hi = (center + window).min(lines.len().saturating_sub(1));
    let mut found: Option<usize> = None;
    for (i, line) in lines.iter().enumerate().take(hi + 1).skip(lo) {
        if re.is_match(line) {
            if found.is_some() { return None; } // ambiguous
            found = Some(i);
        }
    }
    found
}

/// Re-evaluate `src_content` and check that line `expanded_line` of the output
/// equals `desired`.  Returns `true` on match, `false` on mismatch or error.
fn verify_candidate(
    src_content: &str,
    src_path: &std::path::Path,
    eval_config: &EvalConfig,
    expanded_line: u32,
    desired: &str,
) -> bool {
    let oracle_path = src_path.with_file_name("<oracle>");
    let mut evaluator = Evaluator::new(eval_config.clone());
    match process_string(src_content, Some(&oracle_path), &mut evaluator) {
        Ok(bytes) => {
            let s = String::from_utf8_lossy(&bytes);
            s.lines().nth(expanded_line as usize) == Some(desired)
        }
        Err(_) => false,
    }
}

/// Splice one line in `lines`, join back to a string, and return it.
fn splice_line(lines: &[String], idx: usize, new_line: &str, had_trailing_newline: bool) -> String {
    let mut out: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    out[idx] = new_line;
    let mut s = out.join("\n");
    if had_trailing_newline { s.push('\n'); }
    s
}

fn token_overlap_score(text: &str, old_text: &str, new_text: &str) -> i32 {
    fn tokens(s: &str) -> Vec<String> {
        s.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
            .filter(|t| !t.is_empty())
            .map(|t| t.to_ascii_lowercase())
            .collect()
    }

    let line_tokens = tokens(text);
    let mut score = 0i32;
    for token in tokens(old_text).into_iter().chain(tokens(new_text)) {
        if line_tokens.iter().any(|existing| existing == &token) {
            score += 3;
        }
    }
    score
}

fn differing_token_pair(old_text: &str, new_text: &str) -> Option<(String, String)> {
    fn tokens(s: &str) -> Vec<String> {
        s.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect()
    }

    let old_tokens = tokens(old_text);
    let new_tokens = tokens(new_text);
    if old_tokens.len() != new_tokens.len() {
        return None;
    }

    let diffs: Vec<(String, String)> = old_tokens.into_iter()
        .zip(new_tokens)
        .filter(|(old, new)| old != new)
        .collect();

    if diffs.len() == 1 {
        diffs.into_iter().next()
    } else {
        None
    }
}

/// For a `MacroArg` span: replace the changed portion at or after byte column `src_col`.
///
/// Primary strategy: exact match of `old_text` at `src_col` (works when `old_text` is
/// already the raw argument value).
///
/// Fallback: find the prefix where old/new expanded text first differ, then try
/// progressively shorter suffix lengths until we find an old fragment that actually
/// appears in the source from `src_col`.  This handles the common case where
/// `old_text` is the full expanded output line, not just the argument value — and
/// avoids false suffix matches when the old string is a suffix of the new one
/// (e.g. `literate` vs `illiterate`).
fn attempt_macro_arg_patch(
    lines: &[String],
    src_line: usize,
    src_col: u32,
    old_text: &str,
    new_text: &str,
) -> Option<String> {
    let line = lines.get(src_line)?;
    let col = src_col as usize;

    // Primary: exact col match.
    if col + old_text.len() <= line.len() && &line[col..col + old_text.len()] == old_text {
        let mut new_line = line.to_string();
        new_line.replace_range(col..col + old_text.len(), new_text);
        return Some(new_line);
    }

    // Fallback.
    let old_chars: Vec<char> = old_text.chars().collect();
    let new_chars: Vec<char> = new_text.chars().collect();

    // pfx: length of the common prefix between old and new.
    let pfx = old_chars.iter().zip(new_chars.iter())
        .take_while(|(a, b)| a == b).count();

    // max_sfx: upper bound on common suffix length.
    let max_sfx = old_chars.iter().rev().zip(new_chars.iter().rev())
        .take_while(|(a, b)| a == b).count();

    let search_start = col.min(line.len());
    let search_region = &line[search_start..];

    // Try increasing sfx values (longest fragment first) until we find an old_frag
    // that appears in the source.  Longest-first avoids false matches on short fragments
    // (e.g. a single "l" matching the wrong letter in the source line).
    for sfx in 0..=max_sfx {
        let end = old_chars.len().checked_sub(sfx)?;
        if pfx >= end { continue; }
        let old_frag: String = old_chars[pfx..end].iter().collect();
        if old_frag.is_empty() { continue; }

        if let Some(pos) = search_region.find(old_frag.as_str()) {
            let new_end = new_chars.len().checked_sub(sfx)?;
            if pfx > new_end { continue; }
            let new_frag: String = new_chars[pfx..new_end].iter().collect();
            let abs_pos = search_start + pos;
            let mut new_line = line.to_string();
            new_line.replace_range(abs_pos..abs_pos + old_frag.len(), &new_frag);
            return Some(new_line);
        }
    }
    None
}

/// For a `MacroBodyWithVars` span: reconstruct the body template with only the
/// literal (non-variable) parts updated.
///
/// Algorithm:
///  1. Split `body_line` into alternating literal/variable segments via `%(...)`.
///  2. Walk `old_expanded` to extract the runtime value of each variable.
///  3. Walk `new_expanded` to extract the new literal parts (variable values held fixed).
///  4. Rebuild body using original variable references and new literals.
fn attempt_macro_body_fix(
    body_line: &str,
    old_expanded: &str,
    new_expanded: &str,
    sigil: char,
) -> Option<String> {
    if old_expanded == new_expanded { return None; }

    // If the body line is exactly the expanded text, just return the new text.
    if body_line.trim() == old_expanded.trim() {
        return Some(new_expanded.to_string());
    }

    let special_esc = regex::escape(&sigil.to_string());
    let var_re = Regex::new(&format!(r"{}[(][A-Za-z_][A-Za-z0-9_]*[)]", special_esc)).ok()?;

    let mut lits: Vec<&str> = Vec::new();
    let mut var_refs: Vec<&str> = Vec::new();
    let mut pos = 0;
    for m in var_re.find_iter(body_line) {
        lits.push(&body_line[pos..m.start()]);
        var_refs.push(m.as_str());
        pos = m.end();
    }
    lits.push(&body_line[pos..]);

    if var_refs.is_empty() {
        // No variables. Just try to replace old_expanded in body_line.
        if let Some(start) = body_line.find(old_expanded) {
            let mut s = body_line.to_string();
            s.replace_range(start..start + old_expanded.len(), new_expanded);
            return Some(s);
        }
        return None;
    }

    let mut var_vals: Vec<&str> = Vec::new();
    let mut rem = old_expanded;
    for i in 0..var_refs.len() {
        rem = rem.strip_prefix(lits[i])?;
        let next_lit = lits[i + 1];
        let end = if next_lit.is_empty() && i + 1 == var_refs.len() {
            rem.len()
        } else if next_lit.is_empty() {
            return None; // adjacent variables — ambiguous
        } else {
            rem.find(next_lit)?
        };
        var_vals.push(&rem[..end]);
        rem = &rem[end..];
    }
    if !rem.starts_with(lits[var_refs.len()]) { return None; }

    let mut new_lits: Vec<String> = Vec::new();
    let mut new_rem = new_expanded;
    for var_val in &var_vals {
        let var_pos = new_rem.find(var_val)?;
        new_lits.push(new_rem[..var_pos].to_string());
        new_rem = &new_rem[var_pos + var_val.len()..];
    }
    new_lits.push(new_rem.to_string());

    let mut new_body = String::new();
    for (i, var_ref) in var_refs.iter().enumerate() {
        new_body.push_str(&new_lits[i]);
        new_body.push_str(var_ref);
    }
    new_body.push_str(&new_lits[var_refs.len()]);

    if new_body == body_line { None } else { Some(new_body) }
}

fn candidate_line_indices(
    lines: &[String],
    hinted: usize,
    anchor_text: Option<&str>,
    old_text: &str,
) -> Vec<usize> {
    let mut indices = Vec::new();
    let mut push_unique = |idx: usize| {
        if idx < lines.len() && !indices.contains(&idx) {
            indices.push(idx);
        }
    };

    push_unique(hinted);

    if let Some(anchor) = anchor_text
        && let Some(idx) = fuzzy_find_line(lines, hinted, anchor, 40)
    {
        push_unique(idx);
    }
    if let Some(idx) = fuzzy_find_line(lines, hinted, old_text, 40) {
        push_unique(idx);
    }

    let lo = hinted.saturating_sub(6);
    let hi = (hinted + 6).min(lines.len().saturating_sub(1));
    for idx in lo..=hi {
        push_unique(idx);
    }

    indices
}

fn rank_candidate(
    hinted: usize,
    idx: usize,
    current_line: &str,
    old_text: &str,
    new_text: &str,
    context_bonus: i32,
) -> i32 {
    let distance_penalty = hinted.abs_diff(idx) as i32 * 2;
    let mut score = 100 - distance_penalty + context_bonus;
    score += token_overlap_score(current_line, old_text, new_text);
    if current_line.contains(old_text) {
        score += 12;
    }
    score
}

fn choose_best_candidate(
    mut candidates: Vec<CandidateResolution>,
) -> Option<CandidateResolution> {
    candidates.sort_by(|left, right| {
        right.score.cmp(&left.score)
            .then_with(|| left.line_idx.cmp(&right.line_idx))
    });
    let best = candidates.first()?;
    if candidates.get(1).is_some_and(|next| next.score == best.score && next.line_idx != best.line_idx) {
        None
    } else {
        Some(candidates.remove(0))
    }
}

fn chunk_context_bonus(
    db: &WeavebackDb,
    src_file: &str,
    hinted_line_0: usize,
    idx: usize,
) -> i32 {
    let Ok(defs) = db.query_chunk_defs_overlapping(src_file, hinted_line_0 as u32 + 1, hinted_line_0 as u32 + 1) else {
        return 0;
    };
    if defs.iter().any(|def| {
        let lo = def.def_start.saturating_sub(1) as usize;
        let hi = def.def_end.saturating_sub(1) as usize;
        idx >= lo && idx <= hi
    }) {
        20
    } else {
        0
    }
}

fn resolve_noweb_entry(
    db: &WeavebackDb,
    out_file: &str,
    out_line_0: u32,
    resolver: &PathResolver,
) -> Result<Option<NowebMapEntry>, ApplyBackError> {
    if let Some(entry) =
        find_best_noweb_entry(db, out_file, out_line_0, resolver).map_err(ApplyBackError::Db)?
    {
        return Ok(Some(entry));
    }

    let resolved = resolver.resolve_gen(out_file);
    find_best_noweb_entry(db, resolved.to_string_lossy().as_ref(), out_line_0, resolver)
        .map_err(ApplyBackError::Db)
}

fn search_macro_arg_candidate(request: MacroArgSearch<'_>) -> Option<CandidateResolution> {
    let candidate_indices = candidate_line_indices(
        request.lines,
        request.hinted_line,
        None,
        request.old_text,
    );
    let mut candidates = Vec::new();

    for idx in candidate_indices {
        let Some(new_line) = attempt_macro_arg_patch(
            request.lines,
            idx,
            request.src_col,
            request.old_text,
            request.new_text,
        ) else {
            continue;
        };
        let candidate_src = splice_line(request.lines, idx, &new_line, true);
        if !verify_candidate(
            &candidate_src,
            request.src_path,
            request.eval_config,
            request.expanded_line,
            request.new_text,
        ) {
            continue;
        }
        candidates.push(CandidateResolution {
            line_idx: idx,
            new_line,
            score: rank_candidate(
                request.hinted_line,
                idx,
                &request.lines[idx],
                request.old_text,
                request.new_text,
                chunk_context_bonus(
                    request.db,
                    &request.src_path.to_string_lossy(),
                    request.hinted_line,
                    idx,
                ),
            ),
        });
    }

    choose_best_candidate(candidates)
}

fn search_macro_body_candidate(request: MacroBodySearch<'_>) -> Option<CandidateResolution> {
    let anchor = request.body_template.unwrap_or(request.old_text);
    let candidate_indices = candidate_line_indices(
        request.lines,
        request.hinted_line,
        Some(anchor),
        request.old_text,
    );
    let mut candidates = Vec::new();

    for idx in candidate_indices {
        let template = request.body_template.unwrap_or(request.lines.get(idx)?.as_str());
        let Some(new_line) = attempt_macro_body_fix(
            template,
            request.old_text,
            request.new_text,
            request.sigil,
        ) else {
            continue;
        };
        let candidate_src = splice_line(request.lines, idx, &new_line, true);
        if !verify_candidate(
            &candidate_src,
            request.src_path,
            request.eval_config,
            request.expanded_line,
            request.new_text,
        ) {
            continue;
        }
        candidates.push(CandidateResolution {
            line_idx: idx,
            new_line,
            score: rank_candidate(
                request.hinted_line,
                idx,
                &request.lines[idx],
                request.old_text,
                request.new_text,
                chunk_context_bonus(
                    request.db,
                    &request.src_path.to_string_lossy(),
                    request.hinted_line,
                    idx,
                ),
            ),
        });
    }

    choose_best_candidate(candidates)
}

fn search_macro_call_candidate(request: MacroCallSearch<'_>) -> Option<CandidateResolution> {
    let needle = format!("{}{}(", request.sigil, request.macro_name);
    let mut candidates = Vec::new();
    let token_pair = differing_token_pair(request.old_text, request.new_text);

    for (idx, line) in request.lines.iter().enumerate() {
        if !line.contains(&needle) {
            continue;
        }
        if let Some(new_line) = attempt_macro_arg_patch(
            request.lines,
            idx,
            0,
            request.old_text,
            request.new_text,
        ) {
            let candidate_src = splice_line(request.lines, idx, &new_line, true);
            if verify_candidate(
                &candidate_src,
                request.src_path,
                request.eval_config,
                request.expanded_line,
                request.new_text,
            ) {
                candidates.push(CandidateResolution {
                    line_idx: idx,
                    new_line,
                    score: 80 + token_overlap_score(line, request.old_text, request.new_text),
                });
            }
        }

        if let Some((ref old_token, ref new_token)) = token_pair {
            for (pos, _) in line.match_indices(old_token) {
                let before_ok = pos == 0 || !line[..pos].chars().last().is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
                let after_pos = pos + old_token.len();
                let after_ok = after_pos == line.len() || !line[after_pos..].chars().next().is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
                if !(before_ok && after_ok) {
                    continue;
                }

                let mut token_line = line.clone();
                token_line.replace_range(pos..after_pos, new_token);
                let candidate_src = splice_line(request.lines, idx, &token_line, true);
                if !verify_candidate(
                    &candidate_src,
                    request.src_path,
                    request.eval_config,
                    request.expanded_line,
                    request.new_text,
                ) {
                    continue;
                }
                candidates.push(CandidateResolution {
                    line_idx: idx,
                    new_line: token_line,
                    score: 95 + token_overlap_score(line, request.old_text, request.new_text),
                });
            }
        }
    }

    choose_best_candidate(candidates)
}

#[allow(clippy::too_many_arguments)]
fn resolve_patch_source(
    rel_path: &str,
    out_line_0: u32,
    col: u32,
    db: &WeavebackDb,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
    nw_src_file: &str,
    nw_src_line: u32,
    snapshot: Option<&[u8]>,
    sigil: char,
    len: usize,
) -> Result<PatchSource, ApplyBackError> {
    let trace = lookup::perform_trace(
        rel_path,
        out_line_0 + 1,
        col,
        db,
        resolver,
        eval_config.clone(),
    )?
    .or_else(|| {
        let resolved = resolver.resolve_gen(rel_path);
        let resolved = resolved.to_string_lossy().into_owned();
        if resolved == rel_path {
            None
        } else {
            lookup::perform_trace(
                &resolved,
                out_line_0 + 1,
                col,
                db,
                resolver,
                eval_config.clone(),
            )
            .ok()
            .flatten()
        }
    });

    let Some(json) = trace else {
        return Ok(PatchSource::Noweb {
            src_file: nw_src_file.to_string(),
            src_line: nw_src_line as usize,
            len,
        });
    };

    let obj = json.as_object().unwrap();

    let (src_file, src_line_0) = match (obj.get("src_file"), obj.get("src_line")) {
        (Some(sf), Some(sl)) => {
            let sf = sf.as_str().unwrap_or(nw_src_file).to_string();
            let sl = sl.as_u64().unwrap_or(nw_src_line as u64 + 1) as usize - 1;
            (sf, sl)
        }
        _ => return Ok(PatchSource::Noweb {
            src_file: nw_src_file.to_string(),
            src_line: nw_src_line as usize,
            len,
        }),
    };

    let kind = obj.get("kind").and_then(|k| k.as_str()).unwrap_or("Literal");

    match kind {
        "Literal" => Ok(PatchSource::Literal { src_file, src_line: src_line_0, len }),

        "MacroBody" => {
            let macro_name = obj.get("macro_name")
                .and_then(|v| v.as_str()).unwrap_or("?").to_string();
            let snap_line = snapshot.and_then(|bytes| {
                let s = String::from_utf8_lossy(bytes);
                s.lines().nth(src_line_0).map(|l| l.to_string())
            });
            let has_vars = snap_line.as_deref()
                .is_none_or(|l| l.contains(sigil));

            if has_vars {
                Ok(PatchSource::MacroBodyWithVars { src_file, src_line: src_line_0, macro_name })
            } else {
                Ok(PatchSource::MacroBodyLiteral { src_file, src_line: src_line_0, macro_name })
            }
        }

        "MacroArg" => {
            let macro_name = obj.get("macro_name").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            let param_name = obj.get("param_name").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            let src_col    = obj.get("src_col")   .and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            Ok(PatchSource::MacroArg { src_file, src_line: src_line_0, src_col, macro_name, param_name })
        }

        other => Ok(PatchSource::Unpatchable {
            src_file,
            src_line: src_line_0,
            kind_label: other.to_string(),
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_best_patch_source(
    rel_path: &str,
    out_line_0: u32,
    old_text: &str,
    new_text: &str,
    indent_chars: u32,
    db: &WeavebackDb,
    resolver: &PathResolver,
    eval_config: &EvalConfig,
    nw_src_file: &str,
    nw_src_line: u32,
    snapshot: Option<&[u8]>,
    sigil: char,
    len: usize,
    lsp_hint: Option<&LspDefinitionHint>,
) -> Result<PatchSource, ApplyBackError> {
    let first_diff = old_text.chars().zip(new_text.chars())
        .position(|(a, b)| a != b)
        .unwrap_or(0) as u32;
    let old_len = old_text.chars().count() as u32;
    let new_len = new_text.chars().count() as u32;
    let last_changed = old_len.max(new_len).saturating_sub(1);
    let start = first_diff.min(last_changed);
    let end = (first_diff + 6).min(last_changed);

    let mut cols = Vec::new();
    for rel_col in start..=end {
        let col = indent_chars + rel_col + 1;
        if !cols.contains(&col) {
            cols.push(col);
        }
    }
    if cols.is_empty() {
        cols.push(indent_chars + 1);
    }

    let mut best = None;
    let mut best_rank = i32::MIN;
    for col in cols {
        let candidate = resolve_patch_source(
            rel_path,
            out_line_0,
            col,
            db,
            resolver,
            eval_config,
            nw_src_file,
            nw_src_line,
            snapshot,
            sigil,
            len,
        )?;
        let (candidate_file, candidate_line) = patch_source_location(&candidate);
        let mut rank = patch_source_rank(&candidate);
        if matches!(candidate, PatchSource::Literal { .. } | PatchSource::Noweb { .. })
            && candidate_file == nw_src_file
            && candidate_line == nw_src_line as usize
        {
            rank -= 20;
        }
        if let Some(hint) = lsp_hint
            && candidate_file == hint.src_file
        {
            rank += 15;
            if candidate_line.abs_diff(hint.src_line) <= 2 {
                rank += 20;
            }
        }
        if rank > best_rank {
            best_rank = rank;
            best = Some(candidate);
        }
    }

    best.ok_or_else(|| ApplyBackError::Io(std::io::Error::other("no patch source candidates found")))
}

fn lsp_definition_hint(
    rel_path: &str,
    out_line_0: u32,
    col_1: u32,
    resolver: &PathResolver,
    db: &WeavebackDb,
    eval_config: &EvalConfig,
    lsp_clients: &mut HashMap<String, LspClient>,
) -> Option<LspDefinitionHint> {
    let ext = std::path::Path::new(rel_path).extension()?.to_str()?;
    let client = if let Some(client) = lsp_clients.get_mut(ext) {
        client
    } else {
        let (lsp_cmd, lsp_lang) = weaveback_lsp::get_lsp_config(ext)?;
        let project_root = std::env::current_dir().ok()?;
        let mut client = LspClient::spawn(&lsp_cmd, &[], &project_root, lsp_lang).ok()?;
        client.initialize(&project_root).ok()?;
        lsp_clients.insert(ext.to_string(), client);
        lsp_clients.get_mut(ext)?
    };

    let out_path = resolver.resolve_gen(rel_path);
    client.did_open(&out_path).ok()?;
    let loc = client.goto_definition(&out_path, out_line_0, col_1.saturating_sub(1)).ok()??;
    let target_path = loc.uri.to_file_path().ok()?;
    let target_line = loc.range.start.line + 1;
    let target_col = loc.range.start.character + 1;
    let traced = lookup::perform_trace(
        &target_path.to_string_lossy(),
        target_line,
        target_col,
        db,
        resolver,
        eval_config.clone(),
    ).ok()??;
    let obj = traced.as_object()?;
    Some(LspDefinitionHint {
        src_file: obj.get("src_file")?.as_str()?.to_string(),
        src_line: obj.get("src_line")?.as_u64()? as usize - 1,
    })
}

/// Try to apply one line replacement to `lines` at `src_line`.
/// Falls back to fuzzy search in a ±15-line window.
#[allow(clippy::too_many_arguments)]
fn do_patch(
    src_file: &str,
    src_line: usize,
    old_len: usize,
    old_text: &str,
    new_text: &str,
    lines: &mut Vec<String>,
    dry_run: bool,
    skipped: &mut usize,
    applied: &mut usize,
    conflicts: &mut usize,
    label_suffix: Option<&str>,
    out: &mut dyn Write,
) {
    let label = if old_len <= 1 {
        match label_suffix {
            Some(s) => format!("{}:{} ({})", src_file, src_line + 1, s),
            None    => format!("{}:{}", src_file, src_line + 1),
        }
    } else {
        match label_suffix {
            Some(s) => format!("{}:{}-{} ({})", src_file, src_line + 1, src_line + old_len, s),
            None    => format!("{}:{}-{}", src_file, src_line + 1, src_line + old_len),
        }
    };

    // Check if the range matches the old text.
    let matches_old = if src_line + old_len <= lines.len() {
        let current_hunk = lines[src_line..src_line + old_len].join("\n");
        current_hunk == old_text
    } else {
        false
    };

    if matches_old {
        if dry_run {
            let _ = writeln!(out, "  [dry-run] {}: replaced", label);
        } else {
            let new_lines: Vec<String> = new_text.lines().map(|l| l.to_string()).collect();
            lines.splice(src_line..src_line + old_len, new_lines);
            let _ = writeln!(out, "  {}: patched", label);
        }
        *applied += 1;
    } else {
        // Check if already applied.
        let new_lines_count = new_text.lines().count();
        let matches_new = if src_line + new_lines_count <= lines.len() {
            let current_hunk = lines[src_line..src_line + new_lines_count].join("\n");
            current_hunk == new_text
        } else {
            false
        };

        if matches_new {
            let _ = writeln!(out, "  {}: already applied", label);
        } else {
            if old_len == 1 && let Some(idx) = fuzzy_find_line(lines, src_line, old_text, 15) {
                if dry_run {
                    let _ = writeln!(out, "  [dry-run] {}: replaced (fuzzy match at line {})", label, idx + 1);
                } else {
                    let new_lines: Vec<String> = new_text.lines().map(|l| l.to_string()).collect();
                    lines.splice(idx..idx + 1, new_lines);
                    let _ = writeln!(out, "  {}: patched (fuzzy match at line {})", label, idx + 1);
                }
                *applied += 1;
                return;
            }

            let _ = writeln!(out, "  CONFLICT {}: source does not match expected text", label);
            let _ = writeln!(out, "    expected: {:?}", old_text);
            if src_line < lines.len() {
                let actual = if old_len == 1 { lines[src_line].clone() } else { lines[src_line.. (src_line + old_len).min(lines.len())].join("\n") };
                let _ = writeln!(out, "    actual:   {:?}", actual);
            }
            *conflicts += 1;
            *skipped += 1;
        }
    }
}

struct FilePatchContext<'a> {
    db: &'a WeavebackDb,
    src_file: &'a str,
    src_root: &'a std::path::Path,
    patches: &'a [Patch],
    dry_run: bool,
    eval_config: Option<EvalConfig>,
    snapshot: Option<&'a [u8]>,
    sigil: char,
}

fn apply_patches_to_file(
    ctx: FilePatchContext,
    skipped: &mut usize,
    out: &mut dyn Write,
) -> Result<(), ApplyBackError> {
    let src_path = {
        let p = std::path::Path::new(ctx.src_file);
        if p.is_absolute() || p.exists() { p.to_path_buf() } else { ctx.src_root.join(p) }
    };
    let content = std::fs::read_to_string(&src_path)?;
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let had_trailing_newline = content.ends_with('\n');

    let mut applied = 0;
    let mut conflicts = 0;

    for patch in ctx.patches {
        match &patch.source {
            PatchSource::Unpatchable { src_line, kind_label, .. } => {
                let _ = writeln!(out, "  SKIP {}:{}: {} — cannot auto-patch", ctx.src_file, src_line + 1, kind_label);
                *skipped += 1;
            }

            PatchSource::Noweb { src_line, len, .. }
            | PatchSource::Literal { src_line, len, .. } => {
                do_patch(ctx.src_file, *src_line, *len, &patch.old_text, &patch.new_text,
                         &mut lines, ctx.dry_run, skipped, &mut applied, &mut conflicts, None, out);
            }

            PatchSource::MacroBodyLiteral { src_line, macro_name, .. } => {
                do_patch(ctx.src_file, *src_line, 1, &patch.old_text, &patch.new_text,
                         &mut lines, ctx.dry_run, skipped, &mut applied, &mut conflicts,
                         Some(&format!("macro body `{}`", macro_name)), out);
            }

            PatchSource::MacroBodyWithVars { src_line, macro_name, .. } => {
                let label = format!("{}:{} (macro body `{}`)", ctx.src_file, src_line + 1, macro_name);

                let body_template = ctx.snapshot
                    .and_then(|b| String::from_utf8_lossy(b).lines().nth(*src_line).map(|l| l.to_string()));

                match ctx.eval_config.clone() {
                    Some(ec) => {
                        let hint = *src_line;
                        if let Some(candidate) = search_macro_body_candidate(MacroBodySearch {
                            db: ctx.db,
                            lines: &lines,
                            hinted_line: hint,
                            body_template: body_template.as_deref(),
                            old_text: &patch.old_text,
                            new_text: &patch.new_text,
                            sigil: ctx.sigil,
                            eval_config: &ec,
                            src_path: &src_path,
                            expanded_line: patch.expanded_line,
                        }) {
                            if ctx.dry_run {
                                let _ = writeln!(out, "  [dry-run] {}: {:?} → {:?}", label, lines[candidate.line_idx], candidate.new_line);
                            } else {
                                lines[candidate.line_idx] = candidate.new_line;
                                let _ = writeln!(out, "  {}: patched (body search+oracle)", label);
                            }
                            applied += 1;
                        } else if let Some(candidate) = search_macro_call_candidate(MacroCallSearch {
                            lines: &lines,
                            macro_name,
                            sigil: ctx.sigil,
                            old_text: &patch.old_text,
                            new_text: &patch.new_text,
                            eval_config: &ec,
                            src_path: &src_path,
                            expanded_line: patch.expanded_line,
                        }) {
                            if ctx.dry_run {
                                let _ = writeln!(out, "  [dry-run] {}: {:?} → {:?}", label, lines[candidate.line_idx], candidate.new_line);
                            } else {
                                lines[candidate.line_idx] = candidate.new_line;
                                let _ = writeln!(out, "  {}: patched (macro-call search+oracle)", label);
                            }
                            applied += 1;
                        } else {
                            let _ = writeln!(out, "  MANUAL {}: no verified body candidate found — edit manually\n    desired output: {:?}", label, patch.new_text);
                            *skipped += 1;
                        }
                    }
                    None => {
                        let _ = writeln!(out, "  MANUAL {}: contains variables — edit manually\n    desired output: {:?}", label, patch.new_text);
                        *skipped += 1;
                    }
                }
            }

            PatchSource::MacroArg { src_line, src_col, macro_name, param_name, .. } => {
                let label = format!("{}:{} (arg `{}` of `{}`)", ctx.src_file, src_line + 1, param_name, macro_name);

                let candidate = attempt_macro_arg_patch(&lines, *src_line, *src_col, &patch.old_text, &patch.new_text);

                match ctx.eval_config.clone() {
                    Some(ec) => {
                        if let Some(candidate) = search_macro_arg_candidate(MacroArgSearch {
                            db: ctx.db,
                            lines: &lines,
                            hinted_line: *src_line,
                            src_col: *src_col,
                            old_text: &patch.old_text,
                            new_text: &patch.new_text,
                            eval_config: &ec,
                            src_path: &src_path,
                            expanded_line: patch.expanded_line,
                        }) {
                            if ctx.dry_run {
                                let _ = writeln!(out, "  [dry-run] {}: {:?} → {:?}", label, lines[candidate.line_idx], candidate.new_line);
                            } else {
                                lines[candidate.line_idx] = candidate.new_line;
                                let _ = writeln!(out, "  {}: patched (arg search+oracle)", label);
                            }
                            applied += 1;
                        } else if let Some(candidate) = search_macro_call_candidate(MacroCallSearch {
                            lines: &lines,
                            macro_name,
                            sigil: ctx.sigil,
                            old_text: &patch.old_text,
                            new_text: &patch.new_text,
                            eval_config: &ec,
                            src_path: &src_path,
                            expanded_line: patch.expanded_line,
                        }) {
                            if ctx.dry_run {
                                let _ = writeln!(out, "  [dry-run] {}: {:?} → {:?}", label, lines[candidate.line_idx], candidate.new_line);
                            } else {
                                lines[candidate.line_idx] = candidate.new_line;
                                let _ = writeln!(out, "  {}: patched (macro-call search+oracle)", label);
                            }
                            applied += 1;
                        } else {
                            let _ = writeln!(out, "  MANUAL {}: no verified arg candidate found — edit manually\n    desired output: {:?}\n    at col {}", label, patch.new_text, src_col);
                            *skipped += 1;
                        }
                    }
                    None if candidate.is_none() => {
                        let _ = writeln!(out, "  MANUAL {}: could not locate arg value at col {} — edit manually\n    desired output: {:?}", label, src_col, patch.new_text);
                        *skipped += 1;
                    }
                    None => {
                        let _ = writeln!(out, "  MANUAL {}: no eval config for verification — edit manually\n    desired output: {:?}", label, patch.new_text);
                        *skipped += 1;
                    }
                }
            }
        }
    }

    if !ctx.dry_run && applied > 0 {
        let mut content_out = lines.join("\n");
        if had_trailing_newline { content_out.push('\n'); }
        std::fs::write(&src_path, content_out)?;
    }

    if conflicts > 0 {
        let _ = writeln!(out, "  {} conflict(s) in {}", conflicts, ctx.src_file);
    }

    Ok(())
}

fn strip_indent<'a>(line: &'a str, indent: &str) -> &'a str {
    line.strip_prefix(indent).unwrap_or(line)
}

pub fn run_apply_back(opts: ApplyBackOptions, out: &mut dyn Write) -> Result<(), ApplyBackError> {
    if !opts.db_path.exists() {
        let _ = writeln!(out,
            "Database not found at {}. Run weaveback on your source files first.",
            opts.db_path.display()
        );
        return Ok(());
    }

    let db = WeavebackDb::open(&opts.db_path)?;

    // If gen_dir is the default "gen" and that directory doesn't exist, fall back
    // to the gen_dir stored in the database from the last tangle run.
    let gen_dir = {
        let default_gen = std::path::PathBuf::from("gen");
        if opts.gen_dir == default_gen && !default_gen.exists() {
            db.get_run_config("gen_dir")?
                .map(std::path::PathBuf::from)
                .unwrap_or(opts.gen_dir)
        } else {
            opts.gen_dir
        }
    };

    let project_root = opts.db_path
        .canonicalize()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let resolver = PathResolver::new(project_root.clone(), gen_dir.clone());

    let baselines: Vec<(String, Vec<u8>)> = if opts.files.is_empty() {
        db.list_baselines()?
    } else {
        opts.files
            .iter()
            .filter_map(|f| db.get_baseline(f).ok().flatten().map(|b| (f.clone(), b)))
            .collect()
    };

    let sigil = opts.eval_config.as_ref().map_or('%', |ec| ec.sigil);

    // Snapshot cache: driver path → bytes.  Populated lazily.
    let mut snapshot_cache: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    let mut lsp_clients: HashMap<String, LspClient> = HashMap::new();

    let mut any_changed = false;

    for (rel_path, baseline_bytes) in &baselines {
        let gen_path = gen_dir.join(rel_path);
        let current_bytes = match std::fs::read(&gen_path) {
            Ok(b) => b,
            Err(_) => {
                let _ = writeln!(out, "  skip {}: file not found in gen/", rel_path);
                continue;
            }
        };

        if current_bytes == *baseline_bytes { continue; }

        any_changed = true;
        let _ = writeln!(out, "Processing {}", rel_path);

        let baseline_str = String::from_utf8_lossy(baseline_bytes);
        let current_str  = String::from_utf8_lossy(&current_bytes);
        let baseline_lines: Vec<&str> = baseline_str.lines().collect();
        let current_lines:  Vec<&str> = current_str.lines().collect();

        let mut src_patches: HashMap<String, Vec<Patch>> = HashMap::new();
        let mut skipped = 0usize;

        let diff = TextDiff::from_lines(baseline_str.as_ref(), current_str.as_ref());
        for op in diff.ops() {
            match op {
                similar::DiffOp::Equal { .. } => {}

                similar::DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                    let old_lines = &baseline_lines[*old_index..*old_index + *old_len];
                    let new_lines = &current_lines[*new_index..*new_index + *new_len];

                    // If it's a 1-for-1 replacement, we try to go two levels deep (macros).
                    if old_len == new_len && *old_len == 1 {
                        let out_line_0 = *old_index as u32;
                        let old_line = old_lines[0];
                        let new_line = new_lines[0];

                        match resolve_noweb_entry(&db, rel_path, out_line_0, &resolver)? {
                            None => {
                                let _ = writeln!(out, "  skip line {}: no source map entry", out_line_0 + 1);
                                skipped += 1;
                            }
                            Some(entry) => {
                                let old_text = strip_indent(old_line, &entry.indent).to_string();
                                let new_text = strip_indent(new_line, &entry.indent).to_string();

                                let snap = snapshot_cache
                                    .entry(entry.src_file.clone())
                                    .or_insert_with(|| {
                                        db.get_src_snapshot(&entry.src_file).ok().flatten()
                                    })
                                    .as_deref();

                                // Retrieve the config used for this source file to get the correct sigil.
                                let mut file_eval_config = opts.eval_config.clone();
                                let mut file_special_char = sigil;
                                if let Ok(Some(cfg)) = weaveback_tangle::lookup::find_best_source_config(&db, &entry.src_file) {
                                    if file_eval_config.is_none() {
                                        file_eval_config = Some(EvalConfig::default());
                                    }
                                    if let Some(ec) = &mut file_eval_config {
                                        ec.sigil = cfg.sigil;
                                    }
                                    file_special_char = cfg.sigil;
                                }

                                let source = if let Some(ec) = &file_eval_config {
                                    let lsp_hint = lsp_definition_hint(
                                        rel_path,
                                        out_line_0,
                                        entry.indent.chars().count() as u32 + 1,
                                        &resolver,
                                        &db,
                                        ec,
                                        &mut lsp_clients,
                                    );
                                    resolve_best_patch_source(
                                        rel_path,
                                        out_line_0,
                                        &old_text,
                                        &new_text,
                                        entry.indent.chars().count() as u32,
                                        &db, &resolver, ec,
                                        &entry.src_file, entry.src_line,
                                        snap, file_special_char, 1,
                                        lsp_hint.as_ref(),
                                    )?
                                } else {
                                    PatchSource::Noweb {
                                        src_file: entry.src_file.clone(),
                                        src_line: entry.src_line as usize,
                                        len: 1,
                                    }
                                };

                                let file_key = source.src_file().to_string();
                                src_patches
                                    .entry(file_key)
                                    .or_default()
                                    .push(Patch {
                                        source,
                                        old_text,
                                        new_text,
                                        expanded_line: entry.src_line,
                                    });
                            }
                        }
                        continue;
                    }

                    // For multi-line or size-changing Replace, we only support Noweb-level patching for now.
                    // Check if the entire hunk maps to a continuous region in one source file.
                    let mut hunk_entries = Vec::new();
                    for i in 0..*old_len {
                        hunk_entries.push(resolve_noweb_entry(&db, rel_path, (*old_index + i) as u32, &resolver)?);
                    }

                    if hunk_entries.iter().all(|e| e.is_some()) {
                        let entries: Vec<_> = hunk_entries.into_iter().flatten().collect();
                        let first = &entries[0];
                        if entries.iter().all(|e| e.src_file == first.src_file && e.indent == first.indent)
                            && entries.windows(2).all(|w| w[1].src_line == w[0].src_line + 1)
                        {
                            let old_text = old_lines.iter().map(|l| strip_indent(l, &first.indent)).collect::<Vec<_>>().join("\n");
                            let new_text = new_lines.iter().map(|l| strip_indent(l, &first.indent)).collect::<Vec<_>>().join("\n");

                            src_patches
                                .entry(first.src_file.clone())
                                .or_default()
                                .push(Patch {
                                    source: PatchSource::Noweb {
                                        src_file: first.src_file.clone(),
                                        src_line: first.src_line as usize,
                                        len: *old_len,
                                    },
                                    old_text,
                                    new_text,
                                    expanded_line: first.src_line,
                                });
                            continue;
                        }
                    }

                    let _ = writeln!(out,
                        "  skip lines {}-{}: complex size-changing hunk ({} → {} lines) — edit literate source manually",
                        old_index + 1, old_index + old_len, old_len, new_len,
                    );
                    skipped += old_len;
                }

                similar::DiffOp::Delete { old_index, old_len, .. } => {
                    let mut hunk_entries = Vec::new();
                    for i in 0..*old_len {
                        hunk_entries.push(resolve_noweb_entry(&db, rel_path, (*old_index + i) as u32, &resolver)?);
                    }

                    if hunk_entries.iter().all(|e| e.is_some()) {
                        let entries: Vec<_> = hunk_entries.into_iter().flatten().collect();
                        let first = &entries[0];
                        if entries.iter().all(|e| e.src_file == first.src_file && e.indent == first.indent)
                            && entries.windows(2).all(|w| w[1].src_line == w[0].src_line + 1)
                        {
                            let old_text = baseline_lines[*old_index..*old_index + *old_len]
                                .iter().map(|l| strip_indent(l, &first.indent)).collect::<Vec<_>>().join("\n");

                            src_patches
                                .entry(first.src_file.clone())
                                .or_default()
                                .push(Patch {
                                    source: PatchSource::Noweb {
                                        src_file: first.src_file.clone(),
                                        src_line: first.src_line as usize,
                                        len: *old_len,
                                    },
                                    old_text,
                                    new_text: "".to_string(),
                                    expanded_line: first.src_line,
                                });
                            continue;
                        }
                    }

                    let _ = writeln!(out,
                        "  skip lines {}-{}: {} deleted line(s) — remove from literate source manually",
                        old_index + 1, old_index + old_len, old_len,
                    );
                    skipped += old_len;
                }

                similar::DiffOp::Insert { old_index, new_index, new_len, .. } => {
                    let mut is_after = true;
                    let target_entry = if *old_index > 0 {
                        resolve_noweb_entry(&db, rel_path, (*old_index - 1) as u32, &resolver)?
                    } else {
                        is_after = false;
                        resolve_noweb_entry(&db, rel_path, *old_index as u32, &resolver)?
                    };

                    if let Some(entry) = target_entry {
                        let new_text = current_lines[*new_index..*new_index + *new_len]
                            .iter().map(|l| strip_indent(l, &entry.indent)).collect::<Vec<_>>().join("\n");

                        let src_line = if is_after { entry.src_line as usize + 1 } else { entry.src_line as usize };

                        src_patches
                            .entry(entry.src_file.clone())
                            .or_default()
                            .push(Patch {
                                source: PatchSource::Noweb {
                                    src_file: entry.src_file.clone(),
                                    src_line,
                                    len: 0,
                                },
                                old_text: "".to_string(),
                                new_text: format!("{}\n", new_text),
                                expanded_line: entry.src_line,
                            });
                    } else {
                        let _ = writeln!(out,
                            "  skip {} inserted line(s) at gen/ line {} — add to literate source manually",
                            new_len, old_index + 1,
                        );
                        skipped += new_len;
                    }
                }
            }
        }

        // Apply collected patches to each source file.
        for (src_file, patches) in &src_patches {
            let snap = snapshot_cache
                .entry(src_file.clone())
                .or_insert_with(|| {
                    db.get_src_snapshot(src_file).ok().flatten()
                })
                .as_deref();

            // Retrieve the config used for this source file to get the correct sigil.
            let mut file_eval_config = opts.eval_config.clone();
            let mut file_special_char = sigil;
            if let Ok(Some(cfg)) = weaveback_tangle::lookup::find_best_source_config(&db, src_file) {
                if file_eval_config.is_none() {
                    file_eval_config = Some(EvalConfig::default());
                }
                if let Some(ec) = &mut file_eval_config {
                    ec.sigil = cfg.sigil;
                }
                file_special_char = cfg.sigil;
            }

            apply_patches_to_file(
                FilePatchContext {
                    db: &db,
                    src_file,
                    src_root: &project_root,
                    patches,
                    dry_run: opts.dry_run,
                    eval_config: file_eval_config,
                    snapshot: snap,
                    sigil: file_special_char,
                },
                &mut skipped,
                out,
            )?;
        }

        if opts.dry_run {
            let _ = writeln!(out, "  [dry-run] would update baseline for {}", rel_path);
        } else if skipped == 0 {
            db.set_baseline(rel_path, &current_bytes)?;
            let _ = writeln!(out, "  baseline updated for {}", rel_path);
        } else {
            let _ = writeln!(out,
                "  baseline NOT updated for {} ({} line(s) could not be applied)",
                rel_path, skipped,
            );
        }
    }

    if !any_changed {
        let _ = writeln!(out, "No modified gen/ files found.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(str::to_string).collect()
    }

    // ── fuzzy_find_line ────────────────────────────────────────────────────

    #[test]
    fn fuzzy_find_line_finds_unique_match() {
        let ls = lines("foo\nbar baz\nqux");
        assert_eq!(fuzzy_find_line(&ls, 1, "bar baz", 5), Some(1));
    }

    #[test]
    fn fuzzy_find_line_returns_none_when_ambiguous() {
        let ls = lines("foo\nfoo\nfoo");
        assert_eq!(fuzzy_find_line(&ls, 1, "foo", 5), None);
    }

    #[test]
    fn fuzzy_find_line_returns_none_outside_window() {
        let ls = lines("match\nother\nother\nother\nother\nother\nother\nother\nother\nother");
        // center=9, window=0 — "match" is at index 0, distance 9 > window 0
        assert_eq!(fuzzy_find_line(&ls, 9, "match", 0), None);
    }

    #[test]
    fn fuzzy_find_line_tolerates_leading_whitespace() {
        // The pattern is anchored with ^\s* and \s*$, so leading/trailing
        // spaces in the source line are ignored.
        let ls = lines("   bar baz   ");
        assert_eq!(fuzzy_find_line(&ls, 0, "bar baz", 0), Some(0));
    }

    // ── splice_line ────────────────────────────────────────────────────────

    #[test]
    fn splice_line_replaces_indexed_line() {
        let ls = lines("aaa\nbbb\nccc");
        let result = splice_line(&ls, 1, "BBB", false);
        assert_eq!(result, "aaa\nBBB\nccc");
    }

    #[test]
    fn splice_line_preserves_trailing_newline() {
        let ls = lines("x\ny");
        let result = splice_line(&ls, 0, "X", true);
        assert!(result.ends_with('\n'));
    }

    // ── token_overlap_score ────────────────────────────────────────────────

    #[test]
    fn token_overlap_score_counts_shared_tokens() {
        // "hello world" shares "hello" with old and "world" with new
        let score = token_overlap_score("hello world", "hello foo", "world bar");
        assert!(score > 0, "expected positive score, got {score}");
    }

    #[test]
    fn token_overlap_score_zero_when_no_overlap() {
        let score = token_overlap_score("abc", "xyz", "uvw");
        assert_eq!(score, 0);
    }

    // ── differing_token_pair ───────────────────────────────────────────────

    #[test]
    fn differing_token_pair_single_diff_returns_pair() {
        let result = differing_token_pair("foo bar", "foo baz");
        assert_eq!(result, Some(("bar".to_string(), "baz".to_string())));
    }

    #[test]
    fn differing_token_pair_returns_none_when_multiple_diffs() {
        let result = differing_token_pair("foo bar", "qux baz");
        assert_eq!(result, None);
    }

    #[test]
    fn differing_token_pair_returns_none_when_token_counts_differ() {
        let result = differing_token_pair("foo", "foo bar");
        assert_eq!(result, None);
    }

    // ── attempt_macro_arg_patch ────────────────────────────────────────────

    #[test]
    fn attempt_macro_arg_patch_exact_col_replaces() {
        let ls = lines("    let x = old_val;");
        // old_text "old_val" starts at byte 12
        let result = attempt_macro_arg_patch(&ls, 0, 12, "old_val", "new_val");
        assert_eq!(result, Some("    let x = new_val;".to_string()));
    }

    #[test]
    fn attempt_macro_arg_patch_returns_none_when_not_found() {
        let ls = lines("irrelevant line");
        let result = attempt_macro_arg_patch(&ls, 0, 0, "missing", "replacement");
        assert_eq!(result, None);
    }

    // ── attempt_macro_body_fix ─────────────────────────────────────────────

    #[test]
    fn attempt_macro_body_fix_no_vars_replaces_literal() {
        // Body has no %(…) variables; old_expanded matches body_line
        let result = attempt_macro_body_fix("hello world", "hello world", "hello Rust", '%');
        assert_eq!(result, Some("hello Rust".to_string()));
    }

    #[test]
    fn attempt_macro_body_fix_returns_none_when_same() {
        let result = attempt_macro_body_fix("foo", "foo", "foo", '%');
        assert_eq!(result, None);
    }

    // ── ApplyBackError Display ─────────────────────────────────────────────

    #[test]
    fn apply_back_error_display_io_variant() {
        let e = ApplyBackError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
        let s = format!("{e}");
        assert!(s.contains("I/O error"), "got: {s}");
    }

    // ── patch_source_rank ──────────────────────────────────────────────────

    #[test]
    fn patch_source_rank_macro_arg_outranks_literal() {
        let arg = PatchSource::MacroArg {
            src_file: "f".into(), src_line: 1, src_col: 0,
            macro_name: "m".into(), param_name: "p".into(),
        };
        let lit = PatchSource::Literal { src_file: "f".into(), src_line: 1, len: 1 };
        assert!(patch_source_rank(&arg) > patch_source_rank(&lit));
    }

    #[test]
    fn patch_source_rank_unpatchable_is_lowest() {
        let unp = PatchSource::Unpatchable { src_file: "f".into(), src_line: 1, kind_label: "x".into() };
        let nw = PatchSource::Noweb { src_file: "f".into(), src_line: 1, len: 1 };
        assert!(patch_source_rank(&unp) < patch_source_rank(&nw));
    }
}
