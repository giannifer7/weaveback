# Single-Pass Markdown Normalization

## Markdown Expanded-Document Normalization

`.wvb` sources may use AsciiDoc table blocks because the same source can
expand to AsciiDoc and Markdown.  The Markdown pass normalizes those table
blocks after macro expansion: uniform header tables become Markdown pipe
tables, while structurally richer tables fall back to HTML.

```rust
// <[process-markdown-ext]>=
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
// @
```


```rust
// <[process-adoc-table-types]>=
pub(in crate::process::markdown_normalize) fn adoc_table_col_count(attr: Option<&str>) -> Option<usize> {
    let attr = attr?;
    let cols_pos = attr.find("cols=")?;
    let after = &attr[cols_pos + "cols=".len()..];
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &after[quote.len_utf8()..];
    let end = rest.find(quote)?;
    let count = rest[..end]
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .count();
    (count > 0).then_some(count)
}

pub(in crate::process::markdown_normalize) fn adoc_table_has_header(attr: Option<&str>) -> bool {
    attr.map(|attr| {
        let compact = attr.replace(' ', "");
        compact.contains("options=\"header\"")
            || compact.contains("options='header'")
            || compact.contains("options=header")
            || compact.starts_with("[%header")
            || compact.contains(",%header")
    })
        .unwrap_or(false)
}

pub(in crate::process::markdown_normalize) fn split_adoc_cells(line: &str) -> Vec<String> {
    line.trim_start()
        .trim_start_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::process::markdown_normalize) struct AdocTable {
    pub(in crate::process::markdown_normalize) has_header: bool,
    pub(in crate::process::markdown_normalize) rows: Vec<Vec<String>>,
    pub(in crate::process::markdown_normalize) complex: bool,
}

pub(in crate::process::markdown_normalize) fn parse_adoc_table(attr: Option<&str>, body: &[&str]) -> Option<AdocTable> {
    let mut expected_cols = adoc_table_col_count(attr);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut complex = false;

    for line in body {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('|') {
            let cells = split_adoc_cells(line);
            if expected_cols.is_none() {
                expected_cols = Some(cells.len());
            }
            let cols = expected_cols?;
            if cells.is_empty() || cols == 0 {
                return None;
            }
            for cell in cells {
                row.push(cell);
                if row.len() == cols {
                    rows.push(std::mem::take(&mut row));
                }
            }
        } else if let Some(last) = row.last_mut().or_else(|| rows.last_mut().and_then(|r| r.last_mut())) {
            if !last.is_empty() {
                last.push('\n');
            }
            last.push_str(trimmed);
            complex = true;
        } else {
            return None;
        }
    }

    if !row.is_empty() {
        if let Some(cols) = expected_cols {
            row.resize(cols, String::new());
            rows.push(row);
            complex = true;
        } else {
            return None;
        }
    }

    let cols = expected_cols?;
    if rows.is_empty() || rows.iter().any(|row| row.len() != cols) {
        return None;
    }

    Some(AdocTable {
        has_header: adoc_table_has_header(attr),
        rows,
        complex,
    })
}
// @
```


```rust
// <[process-adoc-to-markdown]>=
pub(in crate::process::markdown_normalize) fn escape_markdown_table_cell(cell: &str) -> String {
    cell.replace('\n', " ").replace('|', "\\|").trim().to_string()
}

pub(in crate::process::markdown_normalize) fn render_markdown_table(table: &AdocTable) -> Option<String> {
    if table.complex || !table.has_header || table.rows.is_empty() {
        return None;
    }

    let cols = table.rows.first()?.len();
    let mut out = String::new();
    out.push('|');
    for cell in &table.rows[0] {
        out.push(' ');
        out.push_str(&escape_markdown_table_cell(cell));
        out.push_str(" |");
    }
    out.push('\n');
    out.push('|');
    for _ in 0..cols {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in table.rows.iter().skip(1) {
        out.push('|');
        for cell in row {
            out.push(' ');
            out.push_str(&escape_markdown_table_cell(cell));
            out.push_str(" |");
        }
        out.push('\n');
    }
    Some(out.trim_end().to_string())
}

pub(in crate::process::markdown_normalize) fn escape_html_cell(cell: &str) -> String {
    cell.chars()
        .flat_map(|ch| match ch {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect::<Vec<_>>(),
            '>' => "&gt;".chars().collect::<Vec<_>>(),
            '"' => "&quot;".chars().collect::<Vec<_>>(),
            '\'' => "&#39;".chars().collect::<Vec<_>>(),
            '\n' => "<br>\n".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

pub(in crate::process::markdown_normalize) fn render_html_table(table: &AdocTable) -> String {
    let mut out = String::from("<table>\n");
    for (idx, row) in table.rows.iter().enumerate() {
        let tag = if idx == 0 && table.has_header { "th" } else { "td" };
        out.push_str("  <tr>");
        for cell in row {
            out.push('<');
            out.push_str(tag);
            out.push('>');
            out.push_str(&escape_html_cell(cell));
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>");
    out
}

pub(in crate::process::markdown_normalize) fn render_adoc_table_for_markdown(attr: Option<&str>, body: &[&str], original: &[&str]) -> String {
    let Some(table) = parse_adoc_table(attr, body) else {
        return original.join("\n");
    };
    render_markdown_table(&table).unwrap_or_else(|| render_html_table(&table))
}

pub(crate) fn normalize_adoc_tables_for_markdown(input: &str) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut idx = 0;
    let mut in_fence: Option<&str> = None;

    while idx < lines.len() {
        let trimmed = lines[idx].trim_start();
        if trimmed.starts_with("```") {
            in_fence = if in_fence == Some("```") { None } else { Some("```") };
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }
        if trimmed.starts_with("~~~") {
            in_fence = if in_fence == Some("~~~") { None } else { Some("~~~") };
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }
        if in_fence.is_some() {
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }

        let mut attr: Option<&str> = None;
        let start_idx;
        if lines[idx].trim_start().starts_with('[')
            && lines[idx].trim_end().ends_with(']')
            && idx + 1 < lines.len()
            && lines[idx + 1].trim() == "|==="
        {
            attr = Some(lines[idx].trim());
            start_idx = idx;
            idx += 2;
        } else if lines[idx].trim() == "|===" {
            start_idx = idx;
            idx += 1;
        } else {
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }

        let body_start = idx;
        while idx < lines.len() && lines[idx].trim() != "|===" {
            idx += 1;
        }
        if idx == lines.len() {
            out.extend(lines[start_idx..].iter().map(|line| (*line).to_string()));
            break;
        }

        let original = &lines[start_idx..=idx];
        out.push(render_adoc_table_for_markdown(
            attr,
            &lines[body_start..idx],
            original,
        ));
        idx += 1;
    }

    let mut rendered = out.join("\n");
    if input.ends_with('\n') {
        rendered.push('\n');
    }
    rendered
}
// @
```


```rust
// <[process-markdown-to-adoc]>=
pub(in crate::process::markdown_normalize) fn is_markdown_table_separator_cell(cell: &str) -> bool {
    let cell = cell.trim();
    let cell = cell.trim_matches(':');
    !cell.is_empty() && cell.chars().all(|ch| ch == '-')
}

pub(in crate::process::markdown_normalize) fn split_markdown_table_row(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return None;
    }
    Some(
        trimmed
            .trim_matches('|')
            .split('|')
            .map(|cell| cell.trim().replace("\\|", "|"))
            .collect(),
    )
}

pub(in crate::process::markdown_normalize) fn parse_markdown_pipe_table(input: &str) -> Option<Vec<Vec<String>>> {
    let rows: Vec<Vec<String>> = input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(split_markdown_table_row)
        .collect::<Option<Vec<_>>>()?;
    if rows.len() < 2 {
        return None;
    }
    let cols = rows.first()?.len();
    if cols == 0 || rows.iter().any(|row| row.len() != cols) {
        return None;
    }
    if !rows[1].iter().all(|cell| is_markdown_table_separator_cell(cell)) {
        return None;
    }
    let mut out = Vec::with_capacity(rows.len() - 1);
    out.push(rows[0].clone());
    out.extend(rows.into_iter().skip(2));
    Some(out)
}

pub(in crate::process::markdown_normalize) fn render_asciidoc_table_from_rows(rows: &[Vec<String>]) -> String {
    let cols = rows.first().map(Vec::len).unwrap_or_default();
    let mut out = format!(
        "[cols=\"{}\",options=\"header\"]\n|===\n",
        std::iter::repeat_n("1", cols).collect::<Vec<_>>().join(",")
    );
    for (idx, row) in rows.iter().enumerate() {
        if idx == 1 {
            out.push('\n');
        }
        out.push('|');
        for (cell_idx, cell) in row.iter().enumerate() {
            if cell_idx > 0 {
                out.push_str(" |");
            }
            out.push(' ');
            out.push_str(cell);
        }
        out.push('\n');
    }
    out.push_str("|===");
    out
}

pub(in crate::process::markdown_normalize) fn normalize_markdown_table_for_asciidoc(input: &str) -> String {
    parse_markdown_pipe_table(input)
        .map(|rows| render_asciidoc_table_from_rows(&rows))
        .unwrap_or_else(|| input.to_string())
}
// @
```


```rust
// <[process-explicit-table-blocks]>=
pub(in crate::process::markdown_normalize) fn render_explicit_table_block(expanded_ext: Option<&str>, format: &str, body: &str) -> String {
    let format = format.trim().to_ascii_lowercase();
    let body = body.trim_matches('\n');
    if is_markdown_ext(expanded_ext) {
        match format.as_str() {
            "adoc" | "asciidoc" => normalize_adoc_tables_for_markdown(body),
            "md" | "markdown" | "html" => body.to_string(),
            _ => body.to_string(),
        }
    } else if is_asciidoc_ext(expanded_ext) {
        match format.as_str() {
            "md" | "markdown" => normalize_markdown_table_for_asciidoc(body),
            "html" => format!("++++\n{body}\n++++"),
            "adoc" | "asciidoc" => body.to_string(),
            _ => body.to_string(),
        }
    } else {
        body.to_string()
    }
}

pub(in crate::process::markdown_normalize) fn normalize_explicit_table_blocks(expanded_ext: Option<&str>, input: &str) -> String {
    const TABLE_START_PREFIX: &str = concat!("<", "!-- weaveback-table:");
    const TABLE_END: &str = concat!("<", "!-- /weaveback-table -->");

    let lines: Vec<&str> = input.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut idx = 0;
    let mut in_fence: Option<&str> = None;

    while idx < lines.len() {
        let trimmed = lines[idx].trim();
        if trimmed.starts_with("```") {
            in_fence = if in_fence == Some("```") { None } else { Some("```") };
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }
        if trimmed.starts_with("~~~") {
            in_fence = if in_fence == Some("~~~") { None } else { Some("~~~") };
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }
        if trimmed == "----" {
            in_fence = if in_fence == Some("----") { None } else { Some("----") };
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }
        if trimmed == "...." {
            in_fence = if in_fence == Some("....") { None } else { Some("....") };
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }
        if in_fence.is_some() {
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }

        if !trimmed.starts_with(TABLE_START_PREFIX) || !trimmed.ends_with("-->") {
            out.push(lines[idx].to_string());
            idx += 1;
            continue;
        }

        let format = trimmed
            .trim_start_matches(TABLE_START_PREFIX)
            .trim_end_matches("-->")
            .trim();
        let body_start = idx + 1;
        idx = body_start;
        while idx < lines.len() && lines[idx].trim() != TABLE_END {
            idx += 1;
        }
        if idx == lines.len() {
            out.extend(lines[body_start - 1..].iter().map(|line| (*line).to_string()));
            break;
        }

        out.push(render_explicit_table_block(
            expanded_ext,
            format,
            &lines[body_start..idx].join("\n"),
        ));
        idx += 1;
    }

    let mut rendered = out.join("\n");
    if input.ends_with('\n') {
        rendered.push('\n');
    }
    rendered
}
// @
```


```rust
// <[process-normalize-expanded-document]>=
pub(crate) fn normalize_expanded_document(expanded_ext: Option<&str>, expanded: &[u8]) -> String {
    let expanded = String::from_utf8_lossy(expanded);
    let expanded = normalize_explicit_table_blocks(expanded_ext, &expanded);
    if is_markdown_ext(expanded_ext) {
        normalize_adoc_tables_for_markdown(&expanded)
    } else {
        expanded
    }
}
// @
```

