# Apply Back Heuristics

Macro-argument, macro-body, and macro-call candidate search heuristics.

## Heuristic patch candidates

`attempt_macro_arg_patch` replaces the argument value at a specific byte column
in the source line.  It verifies the expected old value is actually there before
replacing.

`attempt_macro_body_fix` reconstructs a macro body template given the old and
new expanded outputs.  It decomposes the body into alternating literal/variable
segments, extracts runtime variable values from the old expansion, then derives
new literals from the new expansion while keeping variable references fixed.
Returns `None` when the structure is ambiguous (e.g., adjacent variables with
no literal separator).

```rust
// <[applyback-heuristics]>=
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
///  1. Split `body_line` into alternating literal/variable segments via `%%(...)`.
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
// @
```

