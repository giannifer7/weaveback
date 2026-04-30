// weaveback-api/src/process/markdown_normalize.rs
// I'd Really Rather You Didn't edit this generated file.

pub(super) fn is_markdown_ext(expanded_ext: Option<&str>) -> bool {
    matches!(
        expanded_ext.unwrap_or_default().trim_start_matches('.'),
        "md" | "markdown"
    )
}

pub(in crate::process::markdown_normalize) fn is_asciidoc_ext(expanded_ext: Option<&str>) -> bool {
    matches!(
        expanded_ext.unwrap_or_default().trim_start_matches('.'),
        "adoc" | "asciidoc"
    )
}
mod adoc_table;
mod explicit_table;
mod markdown_table;

pub(crate) use adoc_table::normalize_adoc_tables_for_markdown;

use explicit_table::normalize_explicit_table_blocks;

pub(crate) fn normalize_expanded_document(expanded_ext: Option<&str>, expanded: &[u8]) -> String {
    let expanded = String::from_utf8_lossy(expanded);
    let expanded = normalize_explicit_table_blocks(expanded_ext, &expanded);
    if is_markdown_ext(expanded_ext) {
        normalize_adoc_tables_for_markdown(&expanded)
    } else {
        expanded
    }
}

