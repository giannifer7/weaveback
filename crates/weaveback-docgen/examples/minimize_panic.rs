fn panics(s: &str) -> bool {
    std::panic::catch_unwind(|| asciidoc_parser::Parser::default().parse(s)).is_err()
}

fn check(desc: &str, src: &str) {
    let p = panics(src);
    println!("[{}] {desc}", if p { "PANIC" } else { "ok   " });
    if p {
        println!("       ---\n{src}\n       ---\n");
    }
}

fn main() {
    println!("=== Bug 1: final minimization ===\n");
    let cell_text = "| `clean_node` returns `None` for `LineComment` and `BlockComment` nodes;\ncallers in `clean_node`'s own recursion skip `None` results.  The root";
    check(
        "confirmed base",
        &format!("[cols=\"1,3\",options=\"header\"]\n|===\n| A | B\n\n| foo\n{cell_text}"),
    );
    // Can the cell text be simpler?
    check(
        "simpler: multi-line cell with backtick",
        "[cols=\"1,3\"]\n|===\n| A | B\n\n| foo\n| `x` bar\nnext line",
    );
    check(
        "no cols, multi-line cell with backtick",
        "|===\n| A | B\n\n| foo\n| `x` bar\nnext line",
    );
    check(
        "cols=1,3 + single-row multi-line cell",
        "[cols=\"1,3\"]\n|===\n| A | B\n\n| `x` bar\nnext line",
    );
    check(
        "cols=1,3 + multi-line cell without backtick",
        "[cols=\"1,3\"]\n|===\n| A | B\n\n| foo\n| bar baz\nnext line",
    );
    // Is it the semicolon?
    check(
        "cols=1,3 + cell ending with semicolon + continuation",
        "[cols=\"1,3\"]\n|===\n| A | B\n\n| foo\n| `x` bar;\nnext line",
    );

    println!("\n=== Bug 2: final minimization ===\n");
    check(
        "confirmed: backtick URI with <port>",
        "`http://127.0.0.1:<port>/`",
    );
    // Is it the URI scheme?
    check("ftp:// with <port>", "`ftp://host:<port>/`");
    check("any scheme with angle bracket", "`foo://bar:<x>/`");
    check("no scheme, just angle bracket in backtick", "`bar:<x>`");
    check("URI without angle bracket", "`http://127.0.0.1:8080/`");
    // Is the slash after > needed?
    check("URI scheme + colon + angle bracket", "`http://host:<x>`");
    check(
        "scheme + angle bracket no colon before",
        "`http://host/<x>`",
    );
    // Minimal: just ://  + angle bracket pattern?
    check("`a://b:<c>`", "`a://b:<c>`");
    check("`a://b<c>`", "`a://b<c>`");
    check("`a://<b>`", "`a://<b>`");
    check("`a://<b>c`", "`a://<b>c`");
}
