// weaveback-api/src/coverage/lcov/parse.rs
// I'd Really Rather You Didn't edit this generated file.

pub fn parse_lcov_records(text: &str) -> Vec<(String, u32, u64)> {
    let mut current_file: Option<String> = None;
    let mut out = Vec::new();

    for line in text.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            current_file = Some(path.to_string());
            continue;
        }
        if line == "end_of_record" {
            current_file = None;
            continue;
        }
        let Some(rest) = line.strip_prefix("DA:") else {
            continue;
        };
        let Some(file) = current_file.as_ref() else {
            continue;
        };
        let mut parts = rest.split(',');
        let Some(line_no) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Some(hit_count) = parts.next().and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        out.push((file.clone(), line_no, hit_count));
    }

    out
}

