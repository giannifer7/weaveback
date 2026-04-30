// weaveback-api/src/process/markdown_normalize/markdown_table.rs
// I'd Really Rather You Didn't edit this generated file.

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

