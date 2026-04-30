// weaveback-api/src/lint/run.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;
use super::rules::lint_paths;

pub fn run_lint(
    paths: Vec<PathBuf>,
    strict: bool,
    rule: Option<String>,
    json_output: bool,
) -> Result<(), String> {
    let rule_filter = match rule {
        Some(rule) => Some(rule.parse::<LintRule>()?),
        None => None,
    };

    let violations = lint_paths(&paths, rule_filter)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": violations.is_empty(),
                "count": violations.len(),
                "violations": violations,
            }))
            .unwrap()
        );
        if strict && !violations.is_empty() {
            return Err(format!("lint: {} violation(s)", violations.len()));
        }
        return Ok(());
    }
    if violations.is_empty() {
        println!("lint: no violations");
        return Ok(());
    }

    for v in &violations {
        println!("{}:{}: {}", v.file.display(), v.line, v.rule.id());
        println!("  {}", v.message);
        if let Some(hint) = &v.hint {
            println!("  hint: {hint}");
        }
    }

    let summary = format!("lint: {} violation(s)", violations.len());
    if strict {
        Err(summary)
    } else {
        eprintln!("{summary}");
        Ok(())
    }
}

