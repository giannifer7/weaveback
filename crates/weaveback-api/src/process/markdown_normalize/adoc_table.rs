// weaveback-api/src/process/markdown_normalize/adoc_table.rs
// I'd Really Rather You Didn't edit this generated file.

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

