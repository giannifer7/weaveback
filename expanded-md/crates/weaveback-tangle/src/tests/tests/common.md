# Test Common Fixtures





```rust
// <[@file weaveback-tangle/src/tests/common.rs]>=
// weaveback-tangle/src/tests/common.rs
// I'd Really Rather You Didn't edit this generated file.

// src/tests/common.rs
use crate::*;
use std::fs;
use tempfile::TempDir;

pub(crate) struct TestSetup {
    pub _temp_dir: TempDir,
    pub clip: Clip,
}

impl TestSetup {
    pub fn new(comment_markers: &[&str]) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let gen_path = temp_dir.path().join("gen");
        fs::create_dir_all(&gen_path).unwrap();
        let safe_writer = SafeFileWriter::new(gen_path).unwrap();

        let comment_markers = comment_markers
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        let clip = Clip::new(safe_writer, "<<", ">>", "@", &comment_markers);

        TestSetup {
            _temp_dir: temp_dir,
            clip,
        }
    }
}

pub(crate) const BASIC_CHUNK: &str = r#"
# <<test>>=
Hello
# @
"#;

pub(crate) const TWO_CHUNKS: &str = r#"
# <<chunk1>>=
First chunk
# @
# <<chunk2>>=
Second chunk
# @
"#;

pub(crate) const NESTED_CHUNKS: &str = r#"
# <<outer>>=
Before
# <<inner>>
After
# @
# <<inner>>=
Nested content
# @
"#;

pub(crate) const INDENTED_CHUNK: &str = r#"
# <<main>>=
    # <<indented>>
# @
# <<indented>>=
some code
# @
"#;

pub(crate) const PYTHON_CODE: &str = r#"
# <<code>>=
def example():
    # <<body>>
# @
# <<body>>=
print('hello')
# @
"#;

pub(crate) const MULTI_COMMENT_CHUNKS: &str = r#"
# <<python_chunk>>=
def hello():
    print("Hello")
# @

// <<rust_chunk>>=
fn main() {
    println!("Hello");
}
// @
"#;

pub(crate) const FILE_CHUNKS: &str = r#"
# <<@file output.txt>>=
content
# @
# <<other>>=
other content
# @
"#;

pub(crate) const SEQUENTIAL_CHUNKS: &str = r#"
# <<main>>=
# <<part1>>
# <<part2>>
# @
# <<part1>>=
First part
# @
# <<part2>>=
Second part
# @
"#;

pub(crate) const EMPTY_CHUNK: &str = r#"
# <<empty>>=
# @
"#;

// @@
```

