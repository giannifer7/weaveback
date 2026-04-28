// weaveback-api/src/coverage/tests_coverage/location_errors.rs
// I'd Really Rather You Didn't edit this generated file.

#[test]
fn test_parse_lcov_records_simple() {
    let lcov = "SF:file.rs\nDA:1,10\nDA:2,0\nend_of_record\nSF:other.rs\nDA:5,1\nend_of_record\n";
    let records = parse_lcov_records(lcov);
    assert_eq!(records.len(), 3);
    assert_eq!(records[0], ("file.rs".to_string(), 1, 10));
    assert_eq!(records[1], ("file.rs".to_string(), 2, 0));
    assert_eq!(records[2], ("other.rs".to_string(), 5, 1));
}

#[test]
fn test_compute_unmapped_ranges() {
    let lines = vec![
        json!({"generated_line": 1, "hit_count": 5}),
        json!({"generated_line": 2, "hit_count": 0}),
        json!({"generated_line": 3, "hit_count": 0}),
        json!({"generated_line": 5, "hit_count": 1}),
    ];
    let ranges = compute_unmapped_ranges(&lines);
    let arr = ranges.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["start"], 1);
    assert_eq!(arr[0]["end"], 3);
    assert_eq!(arr[0]["missed_count"], 2);
    assert_eq!(arr[1]["start"], 5);
    assert_eq!(arr[1]["missed_count"], 0);
}
// ── open_db ────────────────────────────────────────────────────────────

#[test]
fn open_db_errors_when_database_missing() {
    let result = open_db(std::path::Path::new("/nonexistent/weaveback.db"));
    assert!(result.is_err());
    let msg = format!("{}", result.err().unwrap());
    assert!(msg.contains("Database not found") || msg.contains("not found"), "got: {msg}");
}

// ── parse_generated_location errors ────────────────────────────────────

#[test]
fn parse_generated_location_errors_on_missing_colon() {
    let result = parse_generated_location("nocolon");
    assert!(result.is_err(), "expected Err for missing colon");
}

#[test]
fn parse_generated_location_errors_on_bad_line_number() {
    let result = parse_generated_location("file.rs:abc");
    assert!(result.is_err(), "expected Err for non-numeric line");
}

#[test]
fn parse_generated_location_errors_on_bad_col_number() {
    let result = parse_generated_location("file.rs:10:xyz");
    assert!(result.is_err(), "expected Err for non-numeric column");
}

#[test]
fn parse_generated_location_parses_file_line_col() {
    let result = parse_generated_location("src/lib.rs:42:7").unwrap();
    assert_eq!(result.0, "src/lib.rs");
    assert_eq!(result.1, 42);
    assert_eq!(result.2, 7);
}

#[test]
fn parse_generated_location_parses_file_line_only() {
    let result = parse_generated_location("src/lib.rs:42").unwrap();
    assert_eq!(result.0, "src/lib.rs");
    assert_eq!(result.1, 42);
    assert_eq!(result.2, 1); // defaults col to 1
}

// ── CoverageApiError conversions ──────────────────────────────────────────

#[test]
fn coverage_error_io_displays_message() {
    let e = CoverageApiError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
    assert!(format!("{e}").contains("gone"));
}

// ── print_coverage_summary_to_writer ──────────────────────────────────

