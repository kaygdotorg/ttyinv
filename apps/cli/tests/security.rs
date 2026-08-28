use std::fs;
use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn public_gitleaks_policy_scans_generated_paths() {
    let policy = fs::read_to_string(repository_root().join(".gitleaks.toml")).unwrap();

    assert!(policy.contains("paths = []"));
    for generated_directory in [
        ".next",
        "artifacts",
        "node_modules",
        "dist",
        "build",
        "target",
    ] {
        assert!(!policy.contains(generated_directory));
    }
}

#[test]
fn public_secret_scan_is_full_history_and_least_privilege() {
    let workflow =
        fs::read_to_string(repository_root().join(".github/workflows/secret-scan.yml")).unwrap();

    assert!(workflow.contains("push:"));
    assert!(workflow.contains("pull_request:"));
    assert!(workflow.contains("workflow_call:"));
    assert!(workflow.contains("contents: read"));
    assert!(workflow.contains("pull-requests: read"));
    assert!(workflow.contains("fetch-depth: 0"));
    assert!(workflow.contains("actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683"));
    assert!(workflow.contains("gitleaks/gitleaks-action@e0c47f4f8be36e29cdc102c57e68cb5cbf0e8d1e",));
}

#[test]
fn public_release_and_ci_workflows_are_pinned_and_release_is_gated() {
    let root = repository_root();
    let release = fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();

    assert!(release.contains("permissions:\n  contents: read"));
    assert!(release.contains("secret-scan:\n    uses: ./.github/workflows/secret-scan.yml"));
    assert!(release.contains("needs: secret-scan"));
    assert!(release.contains("actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2",));
    assert!(release.contains(
        "dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c # stable 2026-08-05",
    ));
    assert!(release
        .contains("actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2",));
    assert!(ci.contains("permissions:\n  contents: read"));
    assert!(ci.contains("actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2",));
    assert!(ci.contains(
        "dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c # stable 2026-08-05",
    ));
    for workflow in [&release, &ci] {
        assert!(!workflow.contains("actions/checkout@v"));
        assert!(!workflow.contains("dtolnay/rust-toolchain@stable"));
        assert!(!workflow.contains("actions/upload-artifact@v"));
    }
}
