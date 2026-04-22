// weaveback-docgen/src/d2/tests.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[test]
fn preprocess_d2_returns_none_when_no_d2_blocks() {
    let src = "= Title\n\nJust prose, no d2 blocks.\n\n----\nsome code\n----\n";
    let tmp = tempfile::TempDir::new().unwrap();
    let result = preprocess_d2(src, tmp.path(), tmp.path(), "test", 0, "dagre").unwrap();
    assert!(result.is_none(), "expected None when no d2 blocks present");
}

#[test]
fn collect_d2_blocks_finds_no_blocks_in_plain_adoc() {
    let src = "= Title\n\n----\nsome code\n----\n";
    let blocks = collect_d2_blocks(src, "test");
    assert!(blocks.is_empty());
}

#[test]
fn collect_d2_blocks_finds_d2_source_block() {
    // [source,d2] fence is recognized as a d2 block
    let src = "= Title\n\n[source,d2]\n----\na -> b\n----\n";
    let blocks = collect_d2_blocks(src, "test");
    assert_eq!(blocks.len(), 1, "expected one d2 block, got {:?}", blocks.len());
    assert!(blocks[0].2.contains("a -> b"));
}

#[test]
fn collect_d2_blocks_finds_d2_style_block() {
    let src = "= Title\n\n[d2]\n----\na -> b\n----\n";
    let blocks = collect_d2_blocks(src, "test");
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].2.contains("a -> b"));
}

#[test]
fn collect_d2_blocks_offsets_survive_include_directive() {
    let src = "include::missing.adoc[]\n\n[source,d2]\n----\na -> b\n----\n";
    let blocks = collect_d2_blocks(src, "test");
    assert_eq!(blocks.len(), 1);
    assert_eq!(&src[blocks[0].0..blocks[0].1], "[source,d2]\n----\na -> b\n----");
}

#[test]
fn collect_d2_blocks_ignores_non_d2_source_blocks() {
    let src = "[source,rust]\n----\nfn main() {}\n----\n";
    let blocks = collect_d2_blocks(src, "test");
    assert!(blocks.is_empty(), "rust source block should not be treated as d2");
}

#[test]
fn collect_d2_blocks_handles_empty_source() {
    let blocks = collect_d2_blocks("", "test");
    assert!(blocks.is_empty());
}

#[test]
fn d2_error_display_spawn() {
    let e = D2Error::Spawn(std::io::Error::other("no such binary"));
    assert!(e.to_string().contains("d2"));
}

#[test]
fn d2_error_display_exit_failure() {
    let e = D2Error::ExitFailure { code: 1, index: 0, stderr: "bad input".into() };
    assert!(e.to_string().contains("status 1"));
}

#[test]
fn d2_error_display_cache_write() {
    let e = D2Error::CacheWrite {
        path: "/tmp/x.svg".into(),
        source: std::io::Error::other("disk full"),
    };
    assert!(e.to_string().contains("/tmp/x.svg"));
}

#[test]
fn test_render_d2_mock() {
    let tmp = tempfile::TempDir::new().unwrap();
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let d2_p = bin_dir.join("d2");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&d2_p, "#!/bin/sh\nprintf '<svg>d2-svg</svg>'\n").unwrap();
        let mut perms = std::fs::metadata(&d2_p).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&d2_p, perms).unwrap();
    }

    unsafe { std::env::set_var("WEAVEBACK_D2_BIN", &d2_p); }

    #[cfg(unix)]
    {
        let res = render_d2_diagram("a -> b", 0, 0, "dagre");
        assert!(res.is_ok());
        let svg = res.unwrap();
        assert_eq!(svg, b"<svg>d2-svg</svg>");
    }

    unsafe { std::env::remove_var("WEAVEBACK_D2_BIN"); }
}

