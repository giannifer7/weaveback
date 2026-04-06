// Run with: rustc bisect_panic.rs -L target/debug/deps --edition 2024 \
//   $(cargo build 2>&1 >/dev/null; find target/debug/deps -name 'libasciidoc_parser*.rlib' | head -1 | xargs -I{} echo --extern asciidoc_parser={})
// Simpler: just use `cargo script` or run as a test.
//
// Usage: cargo run --example bisect_panic -- <file>

fn try_parse(s: &str) -> bool {
    std::panic::catch_unwind(|| asciidoc_parser::Parser::default().parse(s)).is_err()
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: bisect_panic <file>");
    let content = std::fs::read_to_string(&path).expect("cannot read file");
    let lines: Vec<&str> = content.lines().collect();
    let n = lines.len();

    println!("File: {path}  ({n} lines)");

    if !try_parse(&content) {
        println!("No panic detected in full file — nothing to bisect.");
        return;
    }
    println!("Full file panics. Bisecting...\n");

    // Find first line that, when included, causes a panic (prefix bisect).
    let mut lo = 1usize;
    let mut hi = n;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let candidate = lines[..mid].join("\n");
        if try_parse(&candidate) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let prefix_end = lo;
    println!("Panic first appears with prefix of {prefix_end} lines.");
    println!("Trigger line ({prefix_end}): {:?}", lines[prefix_end - 1]);

    // Now try to shrink further: remove lines from the start of the prefix.
    let trigger_prefix = lines[..prefix_end].join("\n");
    let mut start = 0usize;
    let mut end = prefix_end;
    while start < end.saturating_sub(1) {
        let mid = (start + end) / 2;
        let candidate = lines[mid..prefix_end].join("\n");
        if try_parse(&candidate) {
            end = mid;
        } else {
            start = mid + 1;
        }
    }

    let minimal = lines[start..prefix_end].join("\n");
    println!(
        "\nMinimal reproducer ({} lines, lines {}-{} of original):",
        prefix_end - start,
        start + 1,
        prefix_end
    );
    println!("---");
    println!("{minimal}");
    println!("---");

    // Verify
    if try_parse(&minimal) {
        println!("\n✓ Confirmed: this snippet alone triggers the panic.");
    } else {
        println!("\n⚠ Snippet alone does not panic — context may be needed.");
        println!("Full prefix (lines 1-{prefix_end}) triggers it:");
        println!("---");
        println!("{trigger_prefix}");
        println!("---");
    }
}
