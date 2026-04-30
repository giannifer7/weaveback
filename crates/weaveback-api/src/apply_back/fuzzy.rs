// weaveback-api/src/apply_back/fuzzy.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

/// Search `lines` in a ±`window` range around `center` for a unique line that
/// matches a whitespace-normalised regex derived from `needle`.
///
/// Returns the 0-indexed line index on a unique match; `None` if not found or
/// ambiguous.
pub(in crate::apply_back) fn fuzzy_find_line(lines: &[String], center: usize, needle: &str, window: usize) -> Option<usize> {
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

