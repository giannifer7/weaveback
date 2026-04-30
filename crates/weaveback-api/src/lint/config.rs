// weaveback-api/src/lint/config.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

#[derive(serde::Deserialize)]
struct LintPassCfg {
    dir:             Option<String>,
    ext:             Option<String>,
    open_delim:      Option<String>,
    close_delim:     Option<String>,
    chunk_end:       Option<String>,
    comment_markers: Option<String>,
}

#[derive(serde::Deserialize)]
struct LintCfg {
    #[serde(rename = "pass", default)]
    passes: Vec<LintPassCfg>,
}

#[derive(Clone)]
pub(in crate::lint) struct LintSyntaxEntry {
    pub(in crate::lint) dir:    Option<PathBuf>,
    pub(in crate::lint) ext:    Option<String>,
    pub(in crate::lint) syntax: NowebSyntax,
}

pub(in crate::lint) fn load_lint_syntaxes_from(base_dir: &Path) -> Vec<LintSyntaxEntry> {
    let mut syntaxes = vec![LintSyntaxEntry {
        dir: None,
        ext: None,
        syntax: NowebSyntax::new(
            "<<",
            ">>",
            "@",
            &["#".to_string(), "//".to_string()],
        ),
    }];

    let Ok(src) = fs::read_to_string(base_dir.join("weaveback.toml")) else {
        return syntaxes;
    };
    let Ok(cfg) = toml::from_str::<LintCfg>(&src) else {
        return syntaxes;
    };

    for pass in cfg.passes {
        let open_delim      = pass.open_delim .unwrap_or_else(|| "<<".to_string());
        let close_delim     = pass.close_delim.unwrap_or_else(|| ">>".to_string());
        let chunk_end       = pass.chunk_end  .unwrap_or_else(|| "@".to_string());
        let ext             = pass.ext;
        let comment_markers = pass
            .comment_markers
            .as_deref()
            .unwrap_or("#,//")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        syntaxes.push(LintSyntaxEntry {
            dir: pass.dir.map(PathBuf::from),
            ext,
            syntax: NowebSyntax::new(&open_delim, &close_delim, &chunk_end, &comment_markers),
        });
    }

    syntaxes
}

pub(in crate::lint) fn load_lint_syntaxes() -> Vec<LintSyntaxEntry> {
    load_lint_syntaxes_from(Path::new("."))
}

pub(in crate::lint) fn lint_syntaxes_for_file<'a>(file: &Path, syntaxes: &'a [LintSyntaxEntry]) -> Vec<&'a NowebSyntax> {
    let rel = file.strip_prefix(".").unwrap_or(file);
    let file_ext = file.extension().and_then(|e| e.to_str());
    let mut matched = syntaxes
        .iter()
        .filter(|entry| {
            entry.dir.as_ref().is_some_and(|dir| rel.starts_with(dir))
                && entry.ext.as_deref().is_none_or(|ext| Some(ext) == file_ext)
        })
        .map(|entry| &entry.syntax)
        .collect::<Vec<_>>();

    if matched.is_empty() {
        matched.extend(
            syntaxes
                .iter()
                .filter(|entry| entry.dir.is_none())
                .map(|entry| &entry.syntax),
        );
    }

    matched
}

