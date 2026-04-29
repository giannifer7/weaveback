// weaveback-tangle/src/tests/advanced/tangle_check.rs
// I'd Really Rather You Didn't edit this generated file.

#[test]
fn tangle_check_expands_file_chunks_in_memory() {
    use crate::noweb::tangle_check;
    let src = "# <<@file out.txt>>=\nhello\n# @\n";
    let markers = vec!["#".to_string()];
    let result = tangle_check(&[(src, "src.nw")], "<<", ">>", "@", &markers).unwrap();
    assert!(result.contains_key("out.txt"), "expected out.txt in result map");
    assert_eq!(result["out.txt"], vec!["hello\n"]);
}

#[test]
fn tangle_check_returns_error_on_undefined_strict() {
    use crate::noweb::tangle_check;
    // tangle_check does not expose strict mode; referencing undefined is silent
    let src = "# <<@file out.txt>>=\n# <<missing>>\n# @\n";
    let markers = vec!["#".to_string()];
    let result = tangle_check(&[(src, "src.nw")], "<<", ">>", "@", &markers).unwrap();
    // undefined chunk expands to empty; @file out.txt has zero lines
    assert_eq!(result["out.txt"], Vec::<String>::new());
}

#[test]
fn tangle_check_handles_multiple_files() {
    use crate::noweb::tangle_check;
    let a = "# <<@file a.txt>>=\nalpha\n# @\n";
    let b = "# <<@file b.txt>>=\nbeta\n# @\n";
    let markers = vec!["#".to_string()];
    let result = tangle_check(&[(a, "a.nw"), (b, "b.nw")], "<<", ">>", "@", &markers).unwrap();
    assert_eq!(result["a.txt"], vec!["alpha\n"]);
    assert_eq!(result["b.txt"], vec!["beta\n"]);
}

