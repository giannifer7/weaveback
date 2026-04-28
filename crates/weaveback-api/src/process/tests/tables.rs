// weaveback-api/src/process/tests/tables.rs
// I'd Really Rather You Didn't edit this generated file.

use super::super::{normalize_adoc_tables_for_markdown, normalize_expanded_document};

#[test]
fn normalize_adoc_table_to_markdown_pipe_table() {
    let input = concat!(
        "[cols=\"1,2\",options=\"header\"]\n",
        "|===\n",
        "| Name | Meaning\n",
        "\n",
        "| `%def` | Constant binding\n",
        "| `%redef` | Rebindable binding\n",
        "|===\n",
    );

    let out = normalize_adoc_tables_for_markdown(input);
    assert_eq!(
        out,
        concat!(
            "| Name | Meaning |\n",
            "| --- | --- |\n",
            "| `%def` | Constant binding |\n",
            "| `%redef` | Rebindable binding |\n",
        )
    );
}
#[test]
fn normalize_adoc_table_with_header_attrs_to_markdown_pipe_table() {
    let input = concat!(
        "[%header,cols=\"1,2\"]\n",
        "|===\n",
        "| Name | Meaning\n",
        "\n",
        "| `%def` | Constant binding\n",
        "| `%redef` | Rebindable binding\n",
        "|===\n",
    );

    let out = normalize_adoc_tables_for_markdown(input);
    assert_eq!(
        out,
        concat!(
            "| Name | Meaning |\n",
            "| --- | --- |\n",
            "| `%def` | Constant binding |\n",
            "| `%redef` | Rebindable binding |\n",
        )
    );
}
#[test]
fn normalize_adoc_table_handles_split_rows() {
    let input = concat!(
        "[cols=\"2,1,4\",options=\"header\"]\n",
        "|===\n",
        "| Path | Method | Description\n",
        "\n",
        "| `/__events` | GET\n",
        "| SSE stream.\n",
        "| `/__open` | GET\n",
        "| Opens an editor.\n",
        "|===\n",
    );

    let out = normalize_adoc_tables_for_markdown(input);
    assert!(out.contains("| `/__events` | GET | SSE stream. |"), "out: {out}");
    assert!(out.contains("| `/__open` | GET | Opens an editor. |"), "out: {out}");
}
#[test]
fn normalize_adoc_table_uses_html_for_complex_cells() {
    let input = concat!(
        "[cols=\"1,2\",options=\"header\"]\n",
        "|===\n",
        "| Error | Meaning\n",
        "| `UndefinedChunk`\n",
        "| A reference names a chunk that was never defined. Silently expands to\n",
        "  nothing by default.\n",
        "|===\n",
    );

    let out = normalize_adoc_tables_for_markdown(input);
    assert!(out.starts_with("<table>"), "out: {out}");
    assert!(out.contains("<br>"), "out: {out}");
    assert!(out.contains("nothing by default."), "out: {out}");
}
#[test]
fn normalize_adoc_table_skips_fenced_code_blocks() {
    let input = concat!(
        "```text\n",
        "[cols=\"1,1\",options=\"header\"]\n",
        "|===\n",
        "| A | B\n",
        "|===\n",
        "```\n",
    );

    assert_eq!(normalize_adoc_tables_for_markdown(input), input);
}
#[test]
fn explicit_adoc_table_block_renders_as_markdown_for_markdown_output() {
    let input = concat!(
        "before\n",
        "<", "!-- weaveback-table:adoc -->\n",
        "[cols=\"1,1\",options=\"header\"]\n",
        "|===\n",
        "| A | B\n",
        "| one | two\n",
        "|===\n",
        "<", "!-- /weaveback-table -->\n",
        "after\n",
    );

    let out = normalize_expanded_document(Some("md"), input.as_bytes());
    assert!(out.contains("| A | B |"), "out: {out}");
    assert!(!out.contains("weaveback-table"), "out: {out}");
    assert!(!out.contains("|==="), "out: {out}");
}
#[test]
fn explicit_markdown_table_block_renders_as_asciidoc_for_asciidoc_output() {
    let input = concat!(
        "<", "!-- weaveback-table:md -->\n",
        "| A | B |\n",
        "| --- | --- |\n",
        "| one | two |\n",
        "<", "!-- /weaveback-table -->\n",
    );

    let out = normalize_expanded_document(Some("adoc"), input.as_bytes());
    assert!(out.contains("[cols=\"1,1\",options=\"header\"]"), "out: {out}");
    assert!(out.contains("| A | B"), "out: {out}");
    assert!(out.contains("| one | two"), "out: {out}");
    assert!(!out.contains("weaveback-table"), "out: {out}");
}
#[test]
fn explicit_html_table_block_is_asciidoc_passthrough_block() {
    let input = concat!(
        "<", "!-- weaveback-table:html -->\n",
        "<table><tr><td>A</td></tr></table>\n",
        "<", "!-- /weaveback-table -->\n",
    );

    let out = normalize_expanded_document(Some("adoc"), input.as_bytes());
    assert_eq!(out.trim(), "++++\n<table><tr><td>A</td></tr></table>\n++++");
}

