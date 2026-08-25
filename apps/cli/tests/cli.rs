use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use ttyinv_cli::{codes, exit};
use ttyinv_core::{MAX_SOURCE_BYTES, schema_json};
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/cases")
        .join(format!("{name}.md"))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ttyinv"))
        .args(args)
        .output()
        .expect("ttyinv process starts")
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("CLI output is UTF-8")
}

fn temp_path(label: &str) -> PathBuf {
    let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("ttyinv-cli-{label}-{}-{id}", std::process::id()))
}

#[test]
fn help_prints_usage() {
    let output = run(&["--help"]);
    assert_eq!(output.status.code(), Some(exit::SUCCESS));
    assert!(text(&output.stdout).contains("Usage:"));
    assert!(text(&output.stdout).contains("ttyinv validate"));
    assert!(output.stderr.is_empty());
}

#[test]
fn version_prints_package_version() {
    let output = run(&["--version"]);
    assert_eq!(output.status.code(), Some(exit::SUCCESS));
    assert_eq!(
        text(&output.stdout).trim(),
        format!("ttyinv {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn schema_prints_core_schema() {
    let output = run(&["schema"]);
    assert_eq!(output.status.code(), Some(exit::SUCCESS));
    assert_eq!(text(&output.stdout), schema_json());
    assert!(output.stderr.is_empty());
}

#[test]
fn schema_writes_core_schema_to_file() {
    let path = temp_path("schema");
    let output = run(&[
        "schema",
        "--output",
        path.to_str().expect("temporary path is UTF-8"),
    ]);
    assert_eq!(output.status.code(), Some(exit::SUCCESS));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(&path).expect("schema output exists"),
        schema_json()
    );
    fs::remove_file(path).expect("temporary schema output is removed");
}

#[test]
fn valid_invoice_succeeds() {
    let output = run(&[
        "validate",
        fixture("simple-valid")
            .to_str()
            .expect("fixture path is UTF-8"),
    ]);
    assert_eq!(output.status.code(), Some(exit::SUCCESS));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn invalid_invoice_returns_diagnostic_and_code_one() {
    let path = fixture("invalid-date");
    let path_text = path.to_str().expect("fixture path is UTF-8");
    let output = run(&["validate", path_text]);
    assert_eq!(output.status.code(), Some(exit::DOCUMENT_INVALID));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.contains(&format!("{path_text}:")));
    assert!(stderr.contains("error["));
}

#[test]
fn json_option_accepts_both_positions_and_preserves_order() {
    let path = fixture("invalid-date");
    let path = path.to_str().expect("fixture path is UTF-8");
    let before = run(&["validate", "--json", path]);
    let after = run(&["validate", path, "--json"]);
    assert_eq!(before.status.code(), Some(exit::DOCUMENT_INVALID));
    assert_eq!(after.status.code(), Some(exit::DOCUMENT_INVALID));
    assert_eq!(before.stdout, after.stdout);
    assert!(before.stderr.is_empty());
    assert!(after.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&before.stdout).expect("JSON output is valid");
    let object = value.as_object().expect("JSON output is an object");
    assert_eq!(object.len(), 2);
    assert!(object.contains_key("valid"));
    assert!(object.contains_key("diagnostics"));
    let diagnostic = &object["diagnostics"][0];
    assert!(diagnostic["severity"].is_string());
    assert_eq!(diagnostic["path"], path);
    let diagnostic = diagnostic.as_object().expect("diagnostic is an object");
    for optional in ["hint", "section", "section_index", "row", "column_name"] {
        assert!(!diagnostic.contains_key(optional));
    }
}

#[test]
fn missing_command_is_usage_error() {
    let output = run(&[]);
    assert_eq!(output.status.code(), Some(exit::USAGE));
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).contains("Run `ttyinv --help` for usage."));
}

#[test]
fn unwritable_stdout_returns_output_error_without_panic() {
    if !Path::new("/dev/full").exists() {
        return;
    }
    // Rust reopens closed standard descriptors as /dev/null before main runs.
    let status = Command::new("sh")
        .args(["-c", "exec 1>/dev/full; exec \"$TTYINV\" schema"])
        .env("TTYINV", env!("CARGO_BIN_EXE_ttyinv"))
        .status()
        .expect("shell process starts");
    assert_eq!(status.code(), Some(exit::OUTPUT));
}

#[test]
fn json_unwritable_stdout_returns_output_error_without_stderr() {
    if !Path::new("/dev/full").exists() {
        return;
    }
    let input = fixture("simple-valid");
    let output = Command::new("sh")
        .args([
            "-c",
            "exec 1>/dev/full; exec \"$TTYINV\" validate --json \"$INPUT\"",
        ])
        .env("TTYINV", env!("CARGO_BIN_EXE_ttyinv"))
        .env("INPUT", input)
        .output()
        .expect("shell process starts");
    assert_eq!(output.status.code(), Some(exit::OUTPUT));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
#[test]
fn missing_input_returns_code_three() {
    let path = temp_path("missing-input");
    let _ = fs::remove_file(&path);
    let output = run(&["validate", path.to_str().expect("temporary path is UTF-8")]);
    assert_eq!(output.status.code(), Some(exit::INPUT));
    assert!(output.stdout.is_empty());
    assert!(
        text(&output.stderr).contains(&format!("error[{}]: cannot read input", codes::INPUT001))
    );
}

#[test]
fn invalid_utf8_returns_code_three() {
    let path = temp_path("invalid-utf8");
    fs::write(&path, [0xff, 0xfe, 0xfd]).expect("invalid UTF-8 fixture is written");
    let output = run(&["validate", path.to_str().expect("temporary path is UTF-8")]);
    assert_eq!(output.status.code(), Some(exit::INPUT));
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).contains(&format!(
        "error[{}]: input is not valid UTF-8",
        codes::INPUT001
    )));

    fs::remove_file(path).expect("invalid UTF-8 fixture is removed");
}

#[test]
fn warning_only_document_succeeds_and_prints_warning() {
    let path = fixture("missing-markdown");
    let output = run(&["validate", path.to_str().expect("fixture path is UTF-8")]);
    assert_eq!(output.status.code(), Some(exit::SUCCESS));
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).contains("warning["));

    let json = run(&[
        "validate",
        "--json",
        path.to_str().expect("fixture path is UTF-8"),
    ]);
    assert_eq!(json.status.code(), Some(exit::SUCCESS));
    assert!(json.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&json.stdout).expect("JSON output is valid");
    assert_eq!(value["valid"], true);
    assert!(
        value["diagnostics"]
            .as_array()
            .expect("diagnostics is an array")
            .iter()
            .all(|diagnostic| diagnostic["severity"] == "warning")
    );
}

#[test]
fn human_diagnostics_render_section_title_separately() {
    let output = run(&[
        "validate",
        fixture("malformed-table")
            .to_str()
            .expect("fixture path is UTF-8"),
    ]);
    assert_eq!(output.status.code(), Some(exit::DOCUMENT_INVALID));
    let stderr = text(&output.stderr);
    assert!(stderr.contains("section: Services"));
    assert!(!stderr.contains("message: Services"));
}

#[test]
fn json_input_errors_are_single_object_with_empty_stderr() {
    let path = temp_path("missing-json-input");
    let _ = fs::remove_file(&path);
    let output = run(&[
        "validate",
        "--json",
        path.to_str().expect("temporary path is UTF-8"),
    ]);
    assert_eq!(output.status.code(), Some(exit::INPUT));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON output is valid");
    assert_eq!(value["valid"], false);
    assert_eq!(value["diagnostics"].as_array().expect("array").len(), 1);
}

#[test]
fn empty_input_path_is_usage_error() {
    let output = run(&["validate", ""]);
    assert_eq!(output.status.code(), Some(exit::USAGE));
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).contains("validate requires an input file"));

    let json = run(&["validate", "--json", ""]);
    assert_eq!(json.status.code(), Some(exit::USAGE));
    assert!(json.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&json.stdout).expect("JSON output is valid");
    assert_eq!(value["valid"], false);
    assert_eq!(value["diagnostics"][0]["code"], codes::USAGE001);
}

#[test]
fn internal_exit_code_is_reserved() {
    assert_eq!(exit::INTERNAL, 70);
    assert_ne!(run(&["validate", ""]).status.code(), Some(exit::INTERNAL));
}

#[test]
fn oversized_input_returns_code_three() {
    let path = temp_path("oversized-input");
    let bytes = vec![b'a'; MAX_SOURCE_BYTES + 1];
    fs::write(&path, bytes).expect("oversized fixture is written");
    let output = run(&["validate", path.to_str().expect("temporary path is UTF-8")]);
    assert_eq!(output.status.code(), Some(exit::INPUT));
    assert!(text(&output.stderr).contains(&format!("error[{}]", codes::INPUT002)));
    fs::remove_file(path).expect("oversized fixture is removed");
}
