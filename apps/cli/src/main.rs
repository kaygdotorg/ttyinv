use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use ttyinv_cli::{codes, exit};
use ttyinv_core::{Diagnostic, MAX_SOURCE_BYTES, Severity, schema_json, validate};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ! {
    process::exit(run(env::args().skip(1)));
}

fn run<I>(args: I) -> i32
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
        return print_help();
    }
    if args.len() == 1 && matches!(args[0].as_str(), "--version" | "-V") {
        return write_stdout_line(&format!("ttyinv {VERSION}"));
    }

    match args.first().map(String::as_str) {
        Some("schema") => run_schema(&args[1..]),
        Some("validate") => run_validate(&args[1..]),
        _ => usage_error("expected one of: schema, validate"),
    }
}

fn run_schema(args: &[String]) -> i32 {
    let mut output: Option<&Path> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--output" | "-o" => {
                index += 1;
                let Some(path) = args.get(index) else {
                    return usage_error("--output requires a path");
                };
                if path.is_empty() || path.starts_with('-') {
                    return usage_error("--output requires a path");
                }
                output = Some(Path::new(path));
            }
            "--help" | "-h" => {
                return if args.len() == 1 {
                    print_schema_help()
                } else {
                    usage_error("unexpected argument for schema")
                };
            }
            _ => {
                return usage_error("unexpected argument for schema");
            }
        }
        index += 1;
    }

    match output {
        Some(path) => match write_atomic(path, schema_json().as_bytes()) {
            Ok(()) => exit::SUCCESS,
            Err(()) => {
                let _ = write_stderr_line("error: cannot write schema output");
                exit::OUTPUT
            }
        },
        None => write_stdout(schema_json()),
    }
}

fn run_validate(args: &[String]) -> i32 {
    let json_output = args.iter().any(|arg| arg == "--json");
    let mut input: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "--json" => {}
            "--help" | "-h" => {
                if args.len() == 1 {
                    return print_validate_help();
                }
                return validate_usage_error("unexpected argument for validate", json_output);
            }
            value if value.starts_with('-') => {
                return validate_usage_error("unexpected option for validate", json_output);
            }
            "" => {
                return validate_usage_error("validate requires an input file", json_output);
            }
            value if input.is_none() => input = Some(value),
            _ => {
                return validate_usage_error("validate requires one input file", json_output);
            }
        }
    }

    let Some(input) = input else {
        return validate_usage_error("validate requires an input file", json_output);
    };

    let source = match read_source(input) {
        Ok(source) => source,
        Err((code, message)) => {
            let diagnostic = adapter_diagnostic(code, message, Some(input));
            if json_output {
                return write_json_result(false, &[diagnostic], exit::INPUT);
            }
            if write_diagnostic_text(&diagnostic).is_err() {
                return exit::OUTPUT;
            }
            return exit::INPUT;
        }
    };

    let report = validate(&source);
    let diagnostics: Vec<Diagnostic> = report
        .diagnostics()
        .iter()
        .map(|diagnostic| with_path(diagnostic, input))
        .collect();
    if json_output {
        return write_json_result(
            report.is_valid(),
            &diagnostics,
            if report.is_valid() {
                exit::SUCCESS
            } else {
                exit::DOCUMENT_INVALID
            },
        );
    }

    for diagnostic in &diagnostics {
        if write_diagnostic_text(diagnostic).is_err() {
            return exit::OUTPUT;
        }
    }

    if report.is_valid() {
        exit::SUCCESS
    } else {
        exit::DOCUMENT_INVALID
    }
}

fn read_source(input: &str) -> Result<String, (&'static str, &'static str)> {
    let file = File::open(input).map_err(|_| (codes::INPUT001, "cannot read input"))?;
    let mut bytes = Vec::new();
    file.take((MAX_SOURCE_BYTES as u64) + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| (codes::INPUT001, "cannot read input"))?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err((codes::INPUT002, "input exceeds the source size limit"));
    }
    String::from_utf8(bytes).map_err(|_| (codes::INPUT001, "input is not valid UTF-8"))
}

fn adapter_diagnostic(code: &str, message: &str, path: Option<&str>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: code.to_owned(),
        message: message.to_owned(),
        path: path.map(str::to_owned),
        field_path: None,
        line: None,
        column: None,
        hint: None,
        section: None,
        section_index: None,
        row: None,
        column_name: None,
    }
}

fn with_path(diagnostic: &Diagnostic, path: &str) -> Diagnostic {
    Diagnostic {
        severity: diagnostic.severity,
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        path: Some(path.to_owned()),
        field_path: diagnostic.field_path.clone(),
        line: diagnostic.line,
        column: diagnostic.column,
        hint: diagnostic.hint.clone(),
        section: diagnostic.section.clone(),
        section_index: diagnostic.section_index,
        row: diagnostic.row,
        column_name: diagnostic.column_name.clone(),
    }
}
fn write_diagnostic_text(diagnostic: &Diagnostic) -> Result<(), ()> {
    let severity = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let mut location = diagnostic.path.clone().unwrap_or_default();
    if let Some(line) = diagnostic.line {
        location.push_str(&format!(":{line}"));
    }
    if let Some(column) = diagnostic.column {
        location.push_str(&format!(":{column}"));
    }
    let prefix = if location.is_empty() {
        format!("{severity}[{}]", diagnostic.code)
    } else {
        format!("{location}: {severity}[{}]", diagnostic.code)
    };
    write_stderr_line(&format!("{prefix}: {}", diagnostic.message))?;
    if let Some(field_path) = &diagnostic.field_path {
        write_stderr_line(&format!("field: {field_path}"))?;
    }
    if let Some(section) = &diagnostic.section {
        write_stderr_line(&format!("section: {section}"))?;
    }
    if let Some(hint) = &diagnostic.hint {
        write_stderr_line(&format!("hint: {hint}"))?;
    }
    Ok(())
}

fn write_json_result(valid: bool, diagnostics: &[Diagnostic], status: i32) -> i32 {
    let value = serde_json::json!({
        "valid": valid,
        "diagnostics": diagnostics,
    });
    match serde_json::to_string(&value) {
        Ok(json) => {
            let output_status = write_stdout_line(&json);
            if output_status == exit::SUCCESS {
                status
            } else {
                exit::OUTPUT
            }
        }
        Err(_) => exit::OUTPUT,
    }
}

fn validate_usage_error(message: &str, json_output: bool) -> i32 {
    if json_output {
        let diagnostic = adapter_diagnostic(codes::USAGE001, message, None);
        return write_json_result(false, &[diagnostic], exit::USAGE);
    }
    usage_error(message)
}
fn usage_error(message: &str) -> i32 {
    if write_stderr_line(&format!("error: {message}")).is_err()
        || write_stderr_line("Run `ttyinv --help` for usage.").is_err()
    {
        return exit::OUTPUT;
    }
    exit::USAGE
}

fn print_help() -> i32 {
    let help = format!(
        "ttyinv {VERSION}\n\nValidate ttyinv invoices.\n\nUsage:\n  ttyinv schema [--output <path>]\n  ttyinv validate [--json] <file>\n  ttyinv --help\n  ttyinv --version\n",
    );
    write_stdout(&help)
}

fn print_schema_help() -> i32 {
    write_stdout_line("Usage: ttyinv schema [--output <path>]")
}

fn print_validate_help() -> i32 {
    write_stdout_line("Usage: ttyinv validate [--json] <file>")
}

fn write_stdout(text: &str) -> i32 {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    if handle.write_all(text.as_bytes()).is_err() || handle.flush().is_err() {
        exit::OUTPUT
    } else {
        exit::SUCCESS
    }
}

fn write_stdout_line(text: &str) -> i32 {
    let mut line = String::with_capacity(text.len() + 1);
    line.push_str(text);
    line.push('\n');
    write_stdout(&line)
}

fn write_stderr_line(text: &str) -> Result<(), ()> {
    let stderr = io::stderr();
    let mut handle = stderr.lock();
    handle.write_all(text.as_bytes()).map_err(|_| ())?;
    handle.write_all(b"\n").map_err(|_| ())?;
    handle.flush().map_err(|_| ())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or(())?.to_string_lossy();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ())?
        .as_nanos();
    let temp_path = parent.join(format!(
        ".{file_name}.ttyinv-{stamp}-{}",
        std::process::id()
    ));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|_| ())?;
        file.write_all(bytes).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        fs::rename(&temp_path, path).map_err(|_| ())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_schema_file() {
        let path = std::env::temp_dir().join(format!("ttyinv-cli-test-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        write_atomic(&path, b"schema").expect("write succeeds");
        assert_eq!(fs::read(&path).expect("read succeeds"), b"schema");
        fs::remove_file(path).expect("cleanup succeeds");
    }

    #[test]
    fn exit_codes_match_process_contract() {
        assert_eq!(
            [
                exit::SUCCESS,
                exit::DOCUMENT_INVALID,
                exit::USAGE,
                exit::INPUT,
                exit::OUTPUT,
                exit::RENDER,
                exit::INTERNAL,
            ],
            [0, 1, 2, 3, 4, 5, 70]
        );
    }
}
