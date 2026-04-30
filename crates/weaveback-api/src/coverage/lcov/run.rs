// weaveback-api/src/coverage/lcov/run.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub fn run_coverage(
    summary_only: bool,
    top_sources: usize,
    top_sections: usize,
    explain_unattributed: bool,
    lcov_file: PathBuf,
    db_path: PathBuf,
    gen_dir: PathBuf,
) -> Result<(), CoverageApiError> {
    let text = std::fs::read_to_string(&lcov_file).map_err(CoverageApiError::Io)?;
    let records = parse_lcov_records(&text);
    let db = open_db(&db_path)?;
    let project_root = std::env::current_dir().unwrap_or_default();
    let resolver = PathResolver::new(project_root.clone(), gen_dir);
    let summary = build_coverage_summary(&records, &db, &project_root, &resolver);
    if summary_only {
        print_coverage_summary_to_writer(&summary, top_sources, top_sections, explain_unattributed, &mut std::io::stdout())
            .map_err(CoverageApiError::Io)?;
    } else {
        let value = build_coverage_summary_view(&summary, top_sources, top_sections);
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
    }
    Ok(())
}

