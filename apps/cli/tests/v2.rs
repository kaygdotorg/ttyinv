use std::fs;
use std::io::Write;
use std::process::Command;

const SOURCE: &str = include_str!("../../../examples/simple.md");
const ONE_PIXEL_PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x04\x00\x00\x00\xb5\x1c\x0c\x02\x00\x00\x00\x0bIDATx\xdacd\xf8\x0f\x00\x01\x05\x01\x01'\x18\xe3f\x00\x00\x00\x00IEND\xaeB`\x82";
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ttyinv"))
        .args(args)
        .output()
        .unwrap()
}
fn temp(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("ttyinv-v2-{name}-{}.md", std::process::id()));
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

#[test]
fn set_scalar_supports_all_output_modes() {
    let path = temp("scalar");
    let original = fs::read_to_string(&path).unwrap();

    let check = run(&[
        "edit",
        "set-scalar",
        path.to_str().unwrap(),
        "--path",
        "title",
        "--value",
        "Changed",
        "--check",
    ]);
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    let stdout = run(&[
        "edit",
        "set-scalar",
        path.to_str().unwrap(),
        "--path",
        "title",
        "--value",
        "Changed",
        "--stdout",
    ]);
    assert!(stdout.status.success());
    assert!(String::from_utf8_lossy(&stdout.stdout).contains("# Changed"));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    let json = run(&[
        "edit",
        "set-scalar",
        path.to_str().unwrap(),
        "--path",
        "title",
        "--value",
        "Changed",
        "--json",
    ]);
    assert!(json.status.success());
    assert!(String::from_utf8_lossy(&json.stdout).contains("\"source\""));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    let atomic = run(&[
        "edit",
        "set-scalar",
        path.to_str().unwrap(),
        "--path",
        "title",
        "--value",
        "Changed",
    ]);
    assert!(atomic.status.success());
    assert!(fs::read_to_string(&path).unwrap().contains("# Changed"));
    let _ = fs::remove_file(path);
}

#[test]
fn convert_round_trips_structured_formats_and_validates_by_extension() {
    let source = temp("convert-source");
    let json_path = source.with_extension("json");
    let yaml_path = source.with_extension("yaml");

    let json = run(&[
        "convert",
        source.to_str().unwrap(),
        "--to",
        "json",
        "--output",
        json_path.to_str().unwrap(),
    ]);
    assert!(json.status.success());
    let valid_json = run(&["validate", json_path.to_str().unwrap(), "--json"]);
    assert!(valid_json.status.success());
    assert!(String::from_utf8_lossy(&valid_json.stdout).contains("\"valid\":true"));

    let yaml = run(&[
        "convert",
        source.to_str().unwrap(),
        "--to",
        "yaml",
        "--output",
        yaml_path.to_str().unwrap(),
    ]);
    assert!(yaml.status.success());
    let valid_yaml = run(&["validate", yaml_path.to_str().unwrap()]);
    assert!(valid_yaml.status.success());

    let canonical = run(&["convert", json_path.to_str().unwrap(), "--to", "markdown"]);
    assert!(canonical.status.success());
    assert!(!canonical.stdout.is_empty());

    let _ = fs::remove_file(source);
    let _ = fs::remove_file(json_path);
    let _ = fs::remove_file(yaml_path);
}

#[test]
fn adapters_require_known_input_or_explicit_from_and_help_is_available() {
    let unknown = std::env::temp_dir().join(format!("ttyinv-v2-unknown-{}", std::process::id()));
    fs::write(&unknown, SOURCE).unwrap();
    let validate = run(&["validate", unknown.to_str().unwrap()]);
    assert_eq!(validate.status.code(), Some(2));
    let explicit = run(&["validate", unknown.to_str().unwrap(), "--from", "markdown"]);
    assert!(explicit.status.success());

    let help = run(&["convert", "--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--to markdown|json|yaml"));

    let _ = fs::remove_file(unknown);
}
#[test]
fn inspect_modes_sections_shape_and_edit_input_contract() {
    let root = std::env::temp_dir().join(format!("ttyinv-v2-contract-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();

    let extensionless = root.join("invoice");
    fs::write(&extensionless, SOURCE).unwrap();
    let edited = run(&[
        "edit",
        "set-gap",
        extensionless.to_str().unwrap(),
        "--section",
        "1",
        "--gap",
        "roomy",
    ]);
    assert!(edited.status.success(), "{edited:?}");
    assert!(fs::read_to_string(&extensionless)
        .unwrap()
        .contains("gap-before roomy"));

    let json_path = root.join("invoice.json");
    let converted = run(&[
        "convert",
        extensionless.to_str().unwrap(),
        "--to",
        "json",
        "--output",
        json_path.to_str().unwrap(),
    ]);
    assert!(converted.status.success(), "{converted:?}");
    let original_json = fs::read_to_string(&json_path).unwrap();
    let refused = run(&[
        "edit",
        "set-gap",
        json_path.to_str().unwrap(),
        "--section",
        "1",
        "--gap",
        "tight",
    ]);
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("EDIT005"));
    assert_eq!(fs::read_to_string(&json_path).unwrap(), original_json);

    let sections = run(&["sections", extensionless.to_str().unwrap()]);
    assert!(sections.status.success());
    assert!(String::from_utf8_lossy(&sections.stdout)
        .lines()
        .any(|line| line.contains('\t') && line.contains("Contract fees")));
    let sections_json = run(&["sections", extensionless.to_str().unwrap(), "--json"]);
    assert!(sections_json.status.success());
    let manifest: serde_json::Value = serde_json::from_slice(&sections_json.stdout).unwrap();
    assert!(manifest["fixed_blocks"].is_array());
    assert!(manifest["ordinary_sections"].is_array());
    assert_eq!(manifest["ordinary_sections"][0]["title"], "Contract fees");

    for mode in ["structure", "summary", "manifest"] {
        let output = run(&[
            "inspect",
            extensionless.to_str().unwrap(),
            "--mode",
            mode,
            "--json",
        ]);
        assert!(output.status.success(), "inspect {mode}: {output:?}");
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["mode"], mode);
        assert!(!value[mode].is_null());
    }

    let malformed = root.join("bad.json");
    fs::write(&malformed, "{").unwrap();
    let malformed_output = run(&["validate", malformed.to_str().unwrap()]);
    assert_eq!(malformed_output.status.code(), Some(3));
    let invalid = root.join("invalid.json");
    fs::write(&invalid, "{}").unwrap();
    let invalid_output = run(&["validate", invalid.to_str().unwrap()]);
    assert_eq!(invalid_output.status.code(), Some(1));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn render_writes_requested_formats_and_rejects_ambiguous_output() {
    let path = temp("render");
    for (format, magic) in [
        ("html", b"<!doctype html>".as_slice()),
        ("pdf", b"%PDF-".as_slice()),
        ("png", b"\x89PNG\r\n\x1a\n".as_slice()),
    ] {
        let out_path = path.with_extension(format);
        let out = run(&[
            "render",
            path.to_str().unwrap(),
            "--format",
            format,
            "--output",
            out_path.to_str().unwrap(),
        ]);
        assert!(
            out.status.success(),
            "render {format}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(fs::read(&out_path).unwrap().starts_with(magic));
        let _ = fs::remove_file(out_path);
    }
    let bad = run(&[
        "render",
        path.to_str().unwrap(),
        "--format",
        "html",
        "--stdout",
        "--output",
        "ignored",
    ]);
    assert_eq!(bad.status.code(), Some(2));
    let _ = fs::remove_file(path);
}
#[test]
fn render_defaults_to_a_safe_file_and_requires_force_for_replacement() {
    let path = temp("render-default");
    let default_output = path.with_extension("html");
    let first = run(&["render", path.to_str().unwrap(), "--format", "html"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(first.stdout.is_empty());
    assert!(fs::read(&default_output)
        .unwrap()
        .starts_with(b"<!doctype html>"));

    let refused = run(&["render", path.to_str().unwrap(), "--format", "html"]);
    assert_eq!(refused.status.code(), Some(4));
    let forced = run(&[
        "render",
        path.to_str().unwrap(),
        "--format",
        "html",
        "--force",
    ]);
    assert!(
        forced.status.success(),
        "{}",
        String::from_utf8_lossy(&forced.stderr)
    );
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(default_output);
}

#[test]
#[cfg(unix)]
fn render_refuses_symlink_output_without_force() {
    use std::os::unix::fs::symlink;
    let path = temp("render-symlink");
    let target = path.with_extension("target");
    let output = path.with_extension("html");
    fs::write(&target, b"do not overwrite").unwrap();
    symlink(&target, &output).unwrap();
    let refused = run(&[
        "render",
        path.to_str().unwrap(),
        "--format",
        "html",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert_eq!(refused.status.code(), Some(4));
    assert_eq!(fs::read(&target).unwrap(), b"do not overwrite");
    let _ = fs::remove_file(&output);
    let _ = fs::remove_file(&target);
    let _ = fs::remove_file(path);
}

#[test]
fn render_reports_png_scale_reductions_on_stderr() {
    let root = std::env::temp_dir().join(format!(
        "ttyinv-v2-png-scale-warning-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let input = root.join("invoice.md");
    let output = root.join("invoice.png");
    fs::write(
        &input,
        include_str!("../../../render-compat/10-multi-page-500.md"),
    )
    .unwrap();
    let rendered = run(&[
        "render",
        input.to_str().unwrap(),
        "--format",
        "png",
        "--png-scale",
        "2",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let stderr = String::from_utf8_lossy(&rendered.stderr);
    assert!(stderr.contains("warning[PNG_SCALE_REDUCED]"));
    assert!(stderr.contains("requested scale 2"));
    assert!(stderr.contains("actual scale 1"));
    let _ = fs::remove_dir_all(root);
}

#[test]
#[cfg(target_os = "linux")]
fn render_rejects_fifo_assets_without_blocking() {
    let root = std::env::temp_dir().join(format!(
        "ttyinv-v2-assets-fifo-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let input = root.join("invoice.md");
    let output = root.join("invoice.html");
    let fifo = root.join("logo.png");
    assert!(Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    let source = SOURCE.replace(
        "## From\n\n- Name: Northstar Studio",
        "## From\n\n![Logo](logo.png)\n\n- Name: Northstar Studio",
    );
    fs::write(&input, source).unwrap();
    let rendered = Command::new("timeout")
        .arg("5s")
        .arg(env!("CARGO_BIN_EXE_ttyinv"))
        .args([
            "render",
            input.to_str().unwrap(),
            "--format",
            "html",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(rendered.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&rendered.stderr).contains("not a regular file"));
    let _ = fs::remove_dir_all(root);
}

#[test]
#[cfg(target_os = "linux")]
fn render_rejects_directory_assets_as_nonregular_files() {
    let root = std::env::temp_dir().join(format!(
        "ttyinv-v2-assets-directory-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let input = root.join("invoice.md");
    let output = root.join("invoice.html");
    fs::create_dir(root.join("logo.png")).unwrap();
    let source = SOURCE.replace(
        "## From\n\n- Name: Northstar Studio",
        "## From\n\n![Logo](logo.png)\n\n- Name: Northstar Studio",
    );
    fs::write(&input, source).unwrap();
    let rendered = run(&[
        "render",
        input.to_str().unwrap(),
        "--format",
        "html",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert_eq!(rendered.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&rendered.stderr).contains("not a regular file"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn render_resolves_local_images_relative_to_input_and_bounds_assets() {
    let root = std::env::temp_dir().join(format!("ttyinv-v2-assets-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let input = root.join("invoice.md");
    let output = root.join("invoice.html");
    let image = root.join("logo.png");
    let source = SOURCE.replace(
        "## From\n\n- Name: Northstar Studio",
        "## From\n\n![Logo](logo.png)\n\n- Name: Northstar Studio",
    );
    fs::write(&input, source).unwrap();
    fs::write(
        &image,
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x04\x00\x00\x00\xb5\x1c\x0c\x02\x00\x00\x00\x0bIDATx\xdacd\xf8\x0f\x00\x01\x05\x01\x01'\x18\xe3f\x00\x00\x00\x00IEND\xaeB`\x82",
    )
    .unwrap();
    let rendered = run(&[
        "render",
        input.to_str().unwrap(),
        "--format",
        "html",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    assert!(String::from_utf8_lossy(&fs::read(&output).unwrap()).contains("data:image/png;base64,"));
    let _ = fs::remove_dir_all(root);
}

#[test]
#[cfg(target_os = "linux")]
fn render_reads_local_assets_from_explicit_descriptor_base() {
    let root = std::env::temp_dir().join(format!(
        "ttyinv-v2-assets-descriptor-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let assets = root.join("assets");
    let input_dir = root.join("input");
    fs::create_dir_all(&assets).unwrap();
    fs::create_dir_all(&input_dir).unwrap();
    let input = input_dir.join("invoice.md");
    let output = root.join("invoice.html");
    let source = SOURCE.replace(
        "## From\n\n- Name: Northstar Studio",
        "## From\n\n![Logo](logo.png)\n\n- Name: Northstar Studio",
    );
    fs::write(&input, source).unwrap();
    fs::write(assets.join("logo.png"), ONE_PIXEL_PNG).unwrap();
    let rendered = run(&[
        "render",
        input.to_str().unwrap(),
        "--asset-base",
        assets.to_str().unwrap(),
        "--format",
        "html",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(
        rendered.status.success(),
        "{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    assert!(String::from_utf8_lossy(&fs::read(&output).unwrap()).contains("data:image/png;base64,"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn render_rejects_local_asset_paths_outside_the_trusted_base() {
    let root = std::env::temp_dir().join(format!(
        "ttyinv-v2-assets-escape-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let assets = root.join("assets");
    fs::create_dir_all(&assets).unwrap();
    let outside = root.join("private.png");
    fs::write(&outside, ONE_PIXEL_PNG).unwrap();
    let input = assets.join("invoice.md");
    let output = assets.join("invoice.html");

    for source_path in [
        "../private.png".to_owned(),
        outside.to_str().unwrap().to_owned(),
    ] {
        let source = SOURCE.replace(
            "## From\n\n- Name: Northstar Studio",
            &format!("## From\n\n![Private]({source_path})\n\n- Name: Northstar Studio"),
        );
        fs::write(&input, source).unwrap();
        let rendered = run(&[
            "render",
            input.to_str().unwrap(),
            "--format",
            "html",
            "--output",
            output.to_str().unwrap(),
        ]);
        assert_eq!(rendered.status.code(), Some(5));
        assert!(!output.exists());
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
#[cfg(unix)]
fn render_rejects_symlinked_local_assets_that_escape_the_base() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!(
        "ttyinv-v2-assets-symlink-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let assets = root.join("assets");
    fs::create_dir_all(&assets).unwrap();
    let outside = root.join("private.png");
    fs::write(&outside, ONE_PIXEL_PNG).unwrap();
    symlink(&outside, assets.join("logo.png")).unwrap();
    let input = assets.join("invoice.md");
    let output = assets.join("invoice.html");
    let source = SOURCE.replace(
        "## From\n\n- Name: Northstar Studio",
        "## From\n\n![Private](logo.png)\n\n- Name: Northstar Studio",
    );
    fs::write(&input, source).unwrap();
    let rendered = run(&[
        "render",
        input.to_str().unwrap(),
        "--format",
        "html",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert_eq!(rendered.status.code(), Some(5));
    assert!(!output.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
#[cfg(unix)]
fn render_rejects_symlinked_output_ancestor_before_rendering() {
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!(
        "ttyinv-v2-output-ancestor-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let real_parent = root.join("real");
    let redirected_parent = root.join("redirected");
    fs::create_dir_all(&real_parent).unwrap();
    fs::create_dir_all(&redirected_parent).unwrap();
    let input = real_parent.join("invoice.md");
    fs::write(&input, SOURCE).unwrap();
    let output = real_parent.join("link").join("invoice.html");
    symlink(&redirected_parent, real_parent.join("link")).unwrap();
    let rendered = run(&[
        "render",
        input.to_str().unwrap(),
        "--format",
        "html",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert_eq!(rendered.status.code(), Some(4));
    assert!(!redirected_parent.join("invoice.html").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn render_stdin_rejects_relative_images_without_an_asset_base() {
    let root = std::env::temp_dir().join(format!("ttyinv-v2-stdin-assets-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let input = root.join("invoice.md");
    let source = SOURCE.replace(
        "## From\n\n- Name: Northstar Studio",
        "## From\n\n![Logo](logo.png)\n\n- Name: Northstar Studio",
    );
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_ttyinv"))
        .args([
            "render", "-", "--from", "markdown", "--format", "html", "--stdout",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--asset-base"));
    let _ = fs::remove_file(input);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn executor_commands_are_reachable_from_cli() {
    let root = std::env::temp_dir().join(format!("ttyinv-v2-executor-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let input = root.join("invoice.md");
    fs::write(&input, SOURCE).unwrap();

    for mode in ["summary", "structure", "manifest"] {
        let out = run(&["inspect", input.to_str().unwrap(), "--mode", mode, "--json"]);
        assert!(out.status.success(), "inspect {mode}: {:?}", out);
    }

    let registry = run(&["registry"]);
    assert!(registry.status.success());
    assert!(String::from_utf8_lossy(&registry.stdout).contains("prepare_render"));
    let schema = run(&["schema"]);
    assert!(schema.status.success());
    assert!(String::from_utf8_lossy(&schema.stdout).contains("ttyinv/v2"));

    let presentation = run(&["resolve-presentation"]);
    assert!(presentation.status.success());
    assert!(String::from_utf8_lossy(&presentation.stdout).contains("geometry"));

    let draft = root.join("draft.json");
    fs::write(
        &draft,
        r#"{"title":"Created invoice","metadata":{"number":"INV-2026-001","issued":"2026-01-01","currency":"EUR"},"from":{"name":"Northstar Studio"},"bill_to":{"name":"Acme Research Ltd"}}"#,
    )
    .unwrap();
    let created = run(&["create", draft.to_str().unwrap(), "--stdout"]);
    assert!(created.status.success(), "{:?}", created);
    assert!(String::from_utf8_lossy(&created.stdout).contains("# Created invoice"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cli_adapter_has_no_operation_specific_core_calls() {
    const MAIN: &str = include_str!("../src/main.rs");
    for helper in [
        "document(",
        "validate(",
        "parse_json(",
        "parse_yaml(",
        "serialize_markdown(",
        "revision(",
        "apply_edit(",
        "render_document(",
        "presentation(",
        "schema_json(",
        "to_json(",
        "to_yaml(",
        "atomic_write(",
    ] {
        assert!(
            !MAIN.contains(helper),
            "legacy helper call remains: {helper}"
        );
    }
    for command in [
        "Create",
        "Validate",
        "Inspect",
        "Convert",
        "Edit",
        "PrepareRender",
        "Render",
        "ResolvePresentation",
        "Registry",
    ] {
        assert!(
            MAIN.contains(&format!("execute(InvoiceCommand::{command}")),
            "missing executor call for {command}"
        );
    }
}
