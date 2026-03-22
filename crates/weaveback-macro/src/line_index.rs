// crates/weaveback-macro/src/line_index.rs

/// Converts byte offsets to 1-indexed (line, column) pairs on demand.
///
/// Construct once per source string; each lookup is O(log n) via binary search
/// over the cached newline positions.
pub struct LineIndex {
    /// Sorted byte offsets of every `\n` in the source.
    newlines: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let newlines = source
            .bytes()
            .enumerate()
            .filter_map(|(i, b)| if b == b'\n' { Some(i) } else { None })
            .collect();
        Self { newlines }
    }

    /// Returns the 1-indexed `(line, column)` for a byte offset.
    /// Column is a 1-indexed byte offset within the line.
    pub fn line_col(&self, pos: usize) -> (usize, usize) {
        // Number of newlines strictly before `pos` = the line index (0-based).
        let i = self.newlines.partition_point(|&nl| nl < pos);
        let line_start = if i == 0 { 0 } else { self.newlines[i - 1] + 1 };
        (i + 1, pos - line_start + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_line() {
        let idx = LineIndex::new("hello");
        assert_eq!(idx.line_col(0), (1, 1));
        assert_eq!(idx.line_col(4), (1, 5));
    }

    #[test]
    fn test_two_lines() {
        // "ab\ncd"  — newline at offset 2
        let idx = LineIndex::new("ab\ncd");
        assert_eq!(idx.line_col(0), (1, 1)); // 'a'
        assert_eq!(idx.line_col(1), (1, 2)); // 'b'
        assert_eq!(idx.line_col(2), (1, 3)); // '\n'
        assert_eq!(idx.line_col(3), (2, 1)); // 'c'
        assert_eq!(idx.line_col(4), (2, 2)); // 'd'
    }

    #[test]
    fn test_three_lines() {
        let idx = LineIndex::new("a\nb\nc");
        assert_eq!(idx.line_col(0), (1, 1));
        assert_eq!(idx.line_col(2), (2, 1));
        assert_eq!(idx.line_col(4), (3, 1));
    }
}
