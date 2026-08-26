use std::fs;
use std::process::Command;

const SOURCE: &str = include_str!("../../../examples/simple.md");
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ttyinv"))
        .args(args)
        .output()
        .unwrap()
}
fn temp(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("ttyinv-v2-{name}-{}", std::process::id()));
    fs::write(&path, SOURCE).unwrap();
    path
}
#[test]
fn validate_schema_and_sections_json() {
    let path = temp("validate");
    let out = run(&["validate", path.to_str().unwrap(), "--json"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"valid\":true"));
    let schema = run(&["schema"]);
    assert!(schema.status.success());
    assert!(String::from_utf8_lossy(&schema.stdout).contains("ttyinv/v2"));
    let sections = run(&["sections", path.to_str().unwrap(), "--json"]);
    assert!(sections.status.success());
    assert!(String::from_utf8_lossy(&sections.stdout).contains("ordinary_sections"));
    let _ = fs::remove_file(path);
}
#[test]
fn edits_atomic_stdout_check_and_one_based() {
    let path = temp("edit");
    let original = fs::read_to_string(&path).unwrap();
    let check = run(&[
        "edit",
        "set-gap",
        path.to_str().unwrap(),
        "--section",
        "1",
        "--gap",
        "roomy",
        "--check",
    ]);
    assert!(!check.status.success());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    let stdout = run(&[
        "edit",
        "set-gap",
        path.to_str().unwrap(),
        "--section",
        "1",
        "--gap",
        "roomy",
        "--stdout",
    ]);
    assert!(stdout.status.success());
    assert!(String::from_utf8_lossy(&stdout.stdout).contains("gap-before roomy"));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    let atomic = run(&[
        "edit",
        "set-gap",
        path.to_str().unwrap(),
        "--section",
        "1",
        "--gap",
        "roomy",
    ]);
    assert!(atomic.status.success());
    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains("gap-before roomy"));
    let _ = fs::remove_file(path);
}
#[test]
fn usage_and_bounds_fail() {
    assert!(!run(&["unknown"]).status.success());
    let path = temp("bounds");
    let out = run(&[
        "edit",
        "move-section",
        path.to_str().unwrap(),
        "--from",
        "99",
        "--to",
        "1",
        "--json",
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("EDIT002"));
    let _ = fs::remove_file(path);
}

#[test]
fn missing_and_zero_indices_are_usage_errors() {
    let path = temp("usage-index");
    for args in [
        vec!["edit", "move-section", path.to_str().unwrap(), "--to", "2"],
        vec![
            "edit",
            "move-section",
            path.to_str().unwrap(),
            "--from",
            "0",
            "--to",
            "1",
        ],
        vec!["edit", "set-gap", path.to_str().unwrap(), "--gap", "tight"],
        vec![
            "edit",
            "set-gap",
            path.to_str().unwrap(),
            "--section",
            "0",
            "--gap",
            "tight",
        ],
    ] {
        assert_eq!(run(&args).status.code(), Some(2));
        assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);
    }
    let _ = fs::remove_file(path);
}
