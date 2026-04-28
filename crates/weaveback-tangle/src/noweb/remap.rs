// weaveback-tangle/src/noweb/remap.rs
// I'd Really Rather You Didn't edit this generated file.

/// Normalise a source line for content-hash matching:
/// strip leading/trailing whitespace and drop any trailing `//` comment.
fn normalise_for_hash(line: &str) -> &str {
    let trimmed = line.trim();
    // Drop inline // comment (not inside strings — good enough for heuristics).
    if let Some(pos) = trimmed.find("//") {
        trimmed[..pos].trim_end()
    } else {
        trimmed
    }
}

fn remap_noweb_entries(
    pre_lines: &[String],
    post_content: &str,
    entries: Vec<NowebMapEntry>,
) -> Vec<(u32, NowebMapEntry)> {
    use similar::{ChangeTag, TextDiff};
    use std::collections::{HashMap, HashSet};

    // Pre-normalise both sides once so all tiers can reuse the slices.
    let pre_norm: Vec<&str> = pre_lines.iter().map(|l| normalise_for_hash(l)).collect();
    let post_lines_vec: Vec<&str> = post_content.lines().collect();
    let post_norm: Vec<&str> = post_lines_vec.iter().map(|l| normalise_for_hash(l)).collect();
    let post_line_count = post_lines_vec.len();

    // --- Tier 1: diff-based exact mapping ---
    let pre_content: String = pre_lines.concat();
    let diff = TextDiff::from_lines(pre_content.as_str(), post_content);

    let mut old_to_new: Vec<Option<usize>> = vec![None; pre_lines.len()];
    let mut old_idx = 0usize;
    let mut new_idx = 0usize;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if old_idx < old_to_new.len() {
                    old_to_new[old_idx] = Some(new_idx);
                }
                old_idx += 1;
                new_idx += 1;
            }
            ChangeTag::Delete => { old_idx += 1; }
            ChangeTag::Insert => { new_idx += 1; }
        }
    }

    let mut new_to_entry: Vec<Option<NowebMapEntry>> = vec![None; post_line_count];
    for (old_i, entry) in entries.iter().enumerate() {
        if let Some(&Some(new_i)) = old_to_new.get(old_i)
            && new_i < post_line_count
        {
            new_to_entry[new_i] = Some(entry.clone()); // Confidence::Exact from expand_inner
        }
    }

    // --- Tier 2: contextual content-hash fallback ---
    // Key = (prev_norm, curr_norm, next_norm).  Three-line context prevents
    // false matches on trivial lines ({, }, etc.).
    //
    // We store *all* candidate old indices per key (Vec<usize>) so that when
    // multiple pre-formatter lines share the same context triple (e.g. two
    // identical import lines in the same chunk), we pick the *closest unused*
    // one to the new_i position rather than arbitrarily using the last.
    //
    // Chunk-aware ambiguity rejection: if the same context triple spans lines
    // from *different* chunks, the key is discarded entirely — a cross-chunk
    // false match is worse than no match.
    type CtxKey<'a> = (&'a str, &'a str, &'a str);
    let mut hash_to_old: HashMap<CtxKey<'_>, Vec<usize>> = HashMap::new();
    let mut ambiguous: HashSet<CtxKey<'_>> = HashSet::new();

    for old_i in 0..pre_norm.len() {
        let curr = pre_norm[old_i];
        if curr.len() <= 1 { continue; }
        let prev = if old_i > 0 { pre_norm[old_i - 1] } else { "" };
        let next = pre_norm.get(old_i + 1).copied().unwrap_or("");
        let key: CtxKey<'_> = (prev, curr, next);
        if ambiguous.contains(&key) { continue; }
        if let Some(existing) = hash_to_old.get(&key) {
            // If any existing candidate is from a different chunk, discard.
            let first_chunk = &entries[existing[0]].chunk_name;
            if entries[old_i].chunk_name != *first_chunk {
                hash_to_old.remove(&key);
                ambiguous.insert(key);
                continue;
            }
        }
        hash_to_old.entry(key).or_default().push(old_i);
    }

    // Pre-claim lines already placed by tier 1.
    let mut claimed: HashSet<usize> = old_to_new.iter()
        .enumerate()
        .filter_map(|(i, m)| m.map(|_| i))
        .collect();

    for new_i in 0..post_line_count {
        if new_to_entry[new_i].is_some() { continue; }
        let curr = post_norm[new_i];
        if curr.len() <= 1 { continue; }
        let prev = if new_i > 0 { post_norm[new_i - 1] } else { "" };
        let next = post_norm.get(new_i + 1).copied().unwrap_or("");
        let key: CtxKey<'_> = (prev, curr, next);
        if let Some(candidates) = hash_to_old.get(&key) {
            // Pick the unclaimed candidate whose old position is closest to new_i.
            let best = candidates.iter()
                .filter(|&&old_i| !claimed.contains(&old_i))
                .min_by_key(|&&old_i| (new_i as isize - old_i as isize).abs());
            if let Some(&old_i) = best {
                claimed.insert(old_i);
                let mut entry = entries[old_i].clone();
                entry.confidence = Confidence::HashMatch;
                new_to_entry[new_i] = Some(entry);
            }
        }
    }

    // --- Tier 3: bidirectional nearest-neighbour fill (Confidence::Inferred) ---
    // Forward pass.
    let mut last: Option<NowebMapEntry> = None;
    for slot in new_to_entry.iter_mut() {
        if slot.is_some() {
            last = slot.clone();
        } else if let Some(ref src) = last {
            let mut e = src.clone();
            e.confidence = Confidence::Inferred;
            *slot = Some(e);
        }
    }
    // Backward pass: fill remaining gaps (leading insertions).
    let mut next: Option<NowebMapEntry> = None;
    for slot in new_to_entry.iter_mut().rev() {
        if slot.is_some() {
            next = slot.clone();
        } else if let Some(ref src) = next {
            let mut e = src.clone();
            e.confidence = Confidence::Inferred;
            *slot = Some(e);
        }
    }

    new_to_entry
        .into_iter()
        .enumerate()
        .filter_map(|(i, e)| e.map(|entry| (i as u32, entry)))
        .collect()
}

