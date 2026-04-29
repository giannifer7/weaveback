// weaveback-api/src/tangle/tests/command.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[test]
fn build_pass_cmd_sets_dir_and_gen() {
    let pass = TanglePassCfg {
        dir:             "crates/foo/".to_string(),
        output_dir:      None,
        ext:             Some("adoc".to_string()),
        no_macros:       false,
        macro_prelude:   vec![],
        expanded_ext:    None,
        expanded_adoc_dir: None,
        expanded_md_dir: None,
        macro_only:      false,
        open_delim:      None,
        close_delim:     None,
        chunk_end:       None,
        comment_markers: None,
        sigil:           None,
    };
    let cmd = build_pass_cmd(
        std::path::Path::new("weaveback"),
        &pass,
        "crates/",
        false,
    );
    let args: Vec<_> = cmd.get_args().collect();
    // --dir crates/foo/ --gen crates/ --ext adoc --no-fts
    let args_str: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
    assert!(args_str.contains(&"--dir"));
    assert!(args_str.contains(&"crates/foo/"));
    assert!(args_str.contains(&"--gen"));
    assert!(args_str.contains(&"crates/"));
    assert!(args_str.contains(&"--no-fts"));
}

#[test]
fn build_pass_cmd_adds_force_generated_flag() {
    let pass = TanglePassCfg {
        dir:             "src/".to_string(),
        output_dir:      None,
        ext:             None,
        no_macros:       true,
        macro_prelude:   vec![],
        expanded_ext:    None,
        expanded_adoc_dir: None,
        expanded_md_dir: None,
        macro_only:      false,
        open_delim:      Some("<<".to_string()),
        close_delim:     Some(">>".to_string()),
        chunk_end:       None,
        comment_markers: None,
        sigil:           None,
    };
    let cmd = build_pass_cmd(
        std::path::Path::new("weaveback"),
        &pass,
        ".",
        true,
    );
    let args: Vec<_> = cmd.get_args().collect();
    let args_str: Vec<&str> = args.iter().map(|a| a.to_str().unwrap()).collect();
    assert!(args_str.contains(&"--force-generated"));
    assert!(args_str.contains(&"--no-macros"));
    assert!(args_str.contains(&"--open-delim"));
    assert!(args_str.contains(&"<<"));
}

#[test]
fn build_pass_cmd_includes_comment_markers() {
    let pass = TanglePassCfg {
        dir:             "src/".to_string(),
        output_dir:      None,
        ext:             None,
        no_macros:       false,
        macro_prelude:   vec![],
        expanded_ext:    None,
        expanded_adoc_dir: None,
        expanded_md_dir: None,
        macro_only:      false,
        open_delim:      None,
        close_delim:     None,
        chunk_end:       None,
        comment_markers: Some("#,//".to_string()),
        sigil:           None,
    };
    let cmd = build_pass_cmd(std::path::Path::new("weaveback"), &pass, ".", false);
    let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap().to_string()).collect();
    assert!(args.windows(2).any(|w| w[0] == "--comment-markers" && w[1] == "#,//"));
}

#[test]
fn build_pass_cmd_includes_chunk_end() {
    let pass = TanglePassCfg {
        dir:             "src/".to_string(),
        output_dir:      None,
        ext:             None,
        no_macros:       false,
        macro_prelude:   vec![],
        expanded_ext:    None,
        expanded_adoc_dir: None,
        expanded_md_dir: None,
        macro_only:      false,
        open_delim:      None,
        close_delim:     None,
        chunk_end:       Some("@@".to_string()),
        comment_markers: None,
        sigil:           None,
    };
    let cmd = build_pass_cmd(std::path::Path::new("weaveback"), &pass, ".", false);
    let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap().to_string()).collect();
    assert!(args.windows(2).any(|w| w[0] == "--chunk-end" && w[1] == "@@"));
}

#[test]
fn build_pass_cmd_includes_sigil() {
    let pass = TanglePassCfg {
        dir:             "src/".to_string(),
        output_dir:      None,
        ext:             None,
        no_macros:       false,
        macro_prelude:   vec![],
        expanded_ext:    None,
        expanded_adoc_dir: None,
        expanded_md_dir: None,
        macro_only:      false,
        open_delim:      None,
        close_delim:     None,
        chunk_end:       None,
        comment_markers: None,
        sigil:           Some("^".to_string()),
    };
    let cmd = build_pass_cmd(std::path::Path::new("weaveback"), &pass, ".", false);
    let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap().to_string()).collect();
    assert!(args.windows(2).any(|w| w[0] == "--sigil" && w[1] == "^"));
}

#[test]
fn build_pass_cmd_includes_macro_prelude_fields() {
    let pass = TanglePassCfg {
        dir:             "src/".to_string(),
        output_dir:      None,
        ext:             Some("wvb".to_string()),
        no_macros:       false,
        macro_prelude:   vec!["prelude/asciidoc.wvb".to_string()],
        expanded_ext:    Some("adoc".to_string()),
        expanded_adoc_dir: Some("expanded-adoc".to_string()),
        expanded_md_dir: Some("expanded-md".to_string()),
        macro_only:      true,
        open_delim:      None,
        close_delim:     None,
        chunk_end:       None,
        comment_markers: None,
        sigil:           Some("¤".to_string()),
    };
    let cmd = build_pass_cmd(std::path::Path::new("weaveback"), &pass, ".", false);
    let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap().to_string()).collect();
    assert!(args.windows(2).any(|w| w[0] == "--macro-prelude" && w[1] == "prelude/asciidoc.wvb"));
    assert!(args.windows(2).any(|w| w[0] == "--expanded-ext" && w[1] == "adoc"));
    assert!(args.windows(2).any(|w| w[0] == "--expanded-adoc-dir" && w[1] == "expanded-adoc"));
    assert!(args.windows(2).any(|w| w[0] == "--expanded-md-dir" && w[1] == "expanded-md"));
    assert!(args.iter().any(|a| a == "--macro-only"));
}

#[test]
fn build_pass_cmd_uses_output_dir_when_set() {
    let pass = TanglePassCfg {
        dir:             "src/".to_string(),
        output_dir:      Some("out/".to_string()),
        ext:             None,
        no_macros:       false,
        macro_prelude:   vec![],
        expanded_ext:    None,
        expanded_adoc_dir: None,
        expanded_md_dir: None,
        macro_only:      false,
        open_delim:      None,
        close_delim:     None,
        chunk_end:       None,
        comment_markers: None,
        sigil:           None,
    };
    let cmd = build_pass_cmd(std::path::Path::new("weaveback"), &pass, "default/", false);
    let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap().to_string()).collect();
    // output_dir overrides default_gen
    assert!(args.windows(2).any(|w| w[0] == "--gen" && w[1] == "out/"));
    assert!(!args.windows(2).any(|w| w[0] == "--gen" && w[1] == "default/"));
}

