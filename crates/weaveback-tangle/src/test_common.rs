use super::*;
use std::fs;
use tempfile::TempDir;

pub struct TestSetup {
    pub _temp_dir: TempDir,
    pub clip: Clip,
}

impl TestSetup {
    pub fn new(comment_markers: &[&str]) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let gen_path = temp_dir.path().join("gen");
        let private_path = temp_dir.path().join("private");
        fs::create_dir_all(&gen_path).unwrap();
        fs::create_dir_all(&private_path).unwrap();
        let safe_writer = SafeFileWriter::new(gen_path, private_path);

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
pub const BASIC_CHUNK: &str = r#"
# <<test>>=
Hello
# @
"#;

pub const TWO_CHUNKS: &str = r#"
# <<chunk1>>=
First chunk
# @
# <<chunk2>>=
Second chunk
# @
"#;

pub const NESTED_CHUNKS: &str = r#"
# <<outer>>=
Before
# <<inner>>
After
# @
# <<inner>>=
Nested content
# @
"#;
pub const INDENTED_CHUNK: &str = r#"
# <<main>>=
    # <<indented>>
# @
# <<indented>>=
some code
# @
"#;

pub const PYTHON_CODE: &str = r#"
# <<code>>=
def example():
    # <<body>>
# @
# <<body>>=
print('hello')
# @
"#;

pub const SEQUENTIAL_CHUNKS: &str = r#"
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

pub const EMPTY_CHUNK: &str = r#"
# <<empty>>=
# @
"#;
pub const FILE_CHUNKS: &str = r#"
# <<@file output.txt>>=
content
# @
# <<other>>=
other content
# @
"#;

pub const TWO_FILES: &str = r#"
# <<@file file1.txt>>=
Content 1
# @
# <<@file file2.txt>>=
Content 2
# @
"#;
pub const MULTI_COMMENT_CHUNKS: &str = r#"
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
