use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rsomics-tsv-filter"))
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Every `*.out` was frozen from a real `csvtk filter` v0.37.0 run (commands in
/// the README Origin section) and is checked in, so the test shells out only to
/// our own binary. Arg lists match csvtk verbatim except `-t`→`--tabs`, whose
/// short collides with the shared CommonFlags `-t/--threads`.
fn assert_golden(args: &[&str], input: &str, golden: &str) {
    let dir = golden_dir();
    let out = bin()
        .args(args)
        .arg(dir.join(input))
        .output()
        .expect("rsomics-tsv-filter failed to run");
    assert!(
        out.status.success(),
        "exit {}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let expected = std::fs::read(dir.join(golden)).expect("golden file missing");
    assert_eq!(
        out.stdout,
        expected,
        "mismatch for {golden}\nours:\n{}\nexpected:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&expected)
    );
}

fn assert_fails(args: &[&str], input: &str) {
    let out = bin()
        .args(args)
        .arg(golden_dir().join(input))
        .output()
        .expect("run failed");
    assert!(
        !out.status.success(),
        "expected non-zero exit for {args:?} on {input}"
    );
}

#[test]
fn numeric_gt_by_name() {
    assert_golden(&["-f", "age>26"], "people.csv", "age_gt.out");
}

#[test]
fn numeric_gt_by_index() {
    assert_golden(&["-f", "2>26"], "people.csv", "idx_gt.out");
}

#[test]
fn ge_operator() {
    assert_golden(&["-f", "score>=88"], "people.csv", "score_ge.out");
}

#[test]
fn ne_operator() {
    assert_golden(&["-f", "age!=30"], "people.csv", "age_ne.out");
}

#[test]
fn le_operator() {
    assert_golden(&["-f", "age<=30"], "people.csv", "age_ldeq.out");
}

#[test]
fn eq_operator_single_equals() {
    assert_golden(&["-f", "age=30"], "people.csv", "age_eq.out");
}

// `<>` is csvtk's alias for `!=`.
#[test]
fn ltgt_operator_alias() {
    assert_golden(&["-f", "age<>30"], "people.csv", "age_ltgt.out");
}

// Range field `2-3` with the default all-must-satisfy rule.
#[test]
fn range_fields_all() {
    assert_golden(&["-f", "2-3>20"], "people.csv", "range_all.out");
}

#[test]
fn any_of_two_fields() {
    assert_golden(
        &["--any", "-f", "age,score>90"],
        "people.csv",
        "any_score.out",
    );
}

// A non-numeric selected value rejects every data row (only the header prints).
#[test]
fn non_numeric_field_drops_rows() {
    assert_golden(&["-f", "name>0"], "people.csv", "nonnum.out");
}

// -n prepends the `row` column (1-based input data-row indices, preserved).
#[test]
fn line_number_column() {
    assert_golden(&["-n", "-f", "age>26"], "people.csv", "linenum.out");
}

#[test]
fn delete_header() {
    assert_golden(&["-U", "-f", "age>26"], "people.csv", "delhdr.out");
}

#[test]
fn no_header_index() {
    assert_golden(&["-H", "-f", "2>26"], "nohdr.csv", "nohdr.out");
}

#[test]
fn no_header_line_number() {
    assert_golden(&["-H", "-n", "-f", "2>26"], "nohdr.csv", "nohdr_ln.out");
}

#[test]
fn tab_delimited() {
    assert_golden(&["--tabs", "-f", "age>26"], "people.tsv", "tsv_gt.out");
}

#[test]
fn explicit_tab_delimiters() {
    assert_golden(
        &["-d", "\t", "-D", "\t", "-f", "age>26"],
        "people.tsv",
        "tsv_dD.out",
    );
}

// Passing rows carry fields that need Go-exact re-quoting on output (comma,
// doubled quote, leading space).
#[test]
fn quoted_fields_roundtrip() {
    assert_golden(&["-f", "val>100"], "quoted.csv", "quoted.out");
}

// -F glob expands to c1,c2 (header order); default all-must-satisfy.
#[test]
fn fuzzy_fields_all() {
    assert_golden(&["-F", "-f", "c*>8"], "cols.csv", "fuzzy_cols.out");
}

#[test]
fn fuzzy_fields_any() {
    assert_golden(&["-F", "--any", "-f", "c*>25"], "cols.csv", "fuzzy_any.out");
}

// A comment line before the header does not consume a row number.
#[test]
fn comment_line_and_line_number() {
    assert_golden(&["-n", "-f", "age>26"], "cmt.csv", "cmt_ln.out");
}

#[test]
fn ragged_row_skipped_with_ignore_illegal() {
    assert_golden(&["-I", "-f", "1>0"], "ragged.csv", "ragged_ign.out");
}

#[test]
fn bare_quote_accepted_with_lazy_quotes() {
    assert_golden(&["-l", "-f", "1>0"], "barequote.csv", "barequote_lz.out");
}

// Fail-loud paths (no golden; just a non-zero exit).
#[test]
fn ragged_row_fails_loud_by_default() {
    assert_fails(&["-f", "1>0"], "ragged.csv");
}

#[test]
fn bare_quote_fails_loud_by_default() {
    assert_fails(&["-f", "1>0"], "barequote.csv");
}

#[test]
fn missing_filter_fails() {
    assert_fails(&[], "people.csv");
}

#[test]
fn double_equals_is_invalid_expression() {
    assert_fails(&["-f", "age==30"], "people.csv");
}

#[test]
fn no_operator_is_invalid_filter() {
    assert_fails(&["-f", "age"], "people.csv");
}

#[test]
fn comma_in_threshold_fails() {
    assert_fails(&["-f", "age>1,0"], "people.csv");
}

// A value that matches reDigitals but is not a valid float (`1.2.3`) is a fatal
// parse error in csvtk; we reproduce the loud failure.
#[test]
fn unparseable_numeric_value_fails() {
    assert_fails(&["-f", "val>1"], "baddigit.csv");
}

#[test]
fn crlf_in_quoted_field_normalized() {
    // Go encoding/csv converts every \r\n to \n, including inside a quoted
    // multi-line field; normalize_crlf keeps us byte-exact on CRLF input.
    assert_golden(&["-f", "age>26"], "crlf.csv", "crlf.out");
}

#[test]
fn bare_cr_kept_as_field_content() {
    // Go's csv reader ends records only on \n, so a bare \r (not \r\n) is field
    // content; go_reader_builder pins Terminator::Any(b'\n') to match — the csv
    // crate's default would split the record on the lone \r.
    assert_golden(&["-f", "n>0"], "barecr.csv", "barecr.out");
}

#[test]
fn trailing_cr_before_eof_dropped() {
    // Go drops a single trailing \r when the last line has no \n; normalize_crlf
    // (csvio 0.3.1) mirrors that so an unterminated final \r isn't kept as content.
    assert_golden(&["-f", "n>0"], "trailcr.csv", "trailcr.out");
}

#[test]
fn mid_stream_error_emits_no_partial_output() {
    // csvtk's buffered writer never flushes on a mid-stream fatal (Go's os.Exit
    // skips the deferred Flush), so a ragged row leaves stdout empty even though
    // earlier rows passed the filter. Ours stages output and discards on error.
    let out = bin()
        .args(["-f", "v>0"])
        .arg(golden_dir().join("miderr.csv"))
        .output()
        .expect("run failed");
    assert!(!out.status.success(), "expected non-zero exit");
    assert!(
        out.stdout.is_empty(),
        "expected empty stdout, got: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
