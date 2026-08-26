use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::process;
use ttyinv_cli::exit;
use ttyinv_core::{
    apply_edit, document, parse_json, parse_yaml, revision, schema_json, serialize_markdown,
    structure_manifest, to_json, to_yaml, validate, Document, EditOperation, EditRequest, Severity,
    ValidationReport, MAX_SOURCE_BYTES,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputFormat {
    Markdown,
    Json,
    Yaml,
}

impl InputFormat {
    fn name(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Json => "json",
            Self::Yaml => "yaml",
        }
    }
}

fn main() -> ! {
    process::exit(run(env::args().skip(1).collect()))
}

fn run(a: Vec<String>) -> i32 {
    match a.first().map(String::as_str) {
        Some("validate") => validate_cmd(&a[1..]),
        Some("convert") => convert_cmd(&a[1..]),
        Some("schema") => schema_cmd(&a[1..]),
        Some("sections") => sections_cmd(&a[1..]),
        Some("edit") => edit_cmd(&a[1..]),
        Some("--help") | Some("-h") | None => {
            print_help();
            exit::SUCCESS
        }
        _ => usage("unknown command"),
    }
}

fn print_help() {
    println!(
        "ttyinv validate INPUT [--from markdown|json|yaml] [--json]\n\
ttyinv convert INPUT --to markdown|json|yaml [--output FILE|--stdout] [--from markdown|json|yaml]\n\
ttyinv schema [--output FILE]\n\
ttyinv sections FILE [--json]\n\
ttyinv edit move-section FILE --from N --to N [--stdout|--check|--json]\n\
ttyinv edit set-gap FILE --section N --gap GAP [--stdout|--check|--json]\n\
ttyinv edit set-scalar FILE --path PATH --value VALUE [--stdout|--check|--json]"
    );
}

fn read(path: &str) -> Result<String, i32> {
    let mut b = Vec::new();
    let read_result = if path == "-" {
        io::stdin()
            .take(MAX_SOURCE_BYTES as u64 + 1)
            .read_to_end(&mut b)
    } else {
        let f = match File::open(path) {
            Ok(f) => f,
            Err(_) => return Err(exit::INPUT),
        };
        f.take(MAX_SOURCE_BYTES as u64 + 1).read_to_end(&mut b)
    };
    if read_result.is_err() || b.len() > MAX_SOURCE_BYTES {
        return Err(exit::INPUT);
    }
    String::from_utf8(b).map_err(|_| exit::INPUT)
}

fn usage(s: &str) -> i32 {
    eprintln!("error: {s}");
    exit::USAGE
}

fn is_edit_option(value: &str) -> bool {
    matches!(
        value,
        "--from"
            | "--to"
            | "--section"
            | "--gap"
            | "--path"
            | "--value"
            | "--stdout"
            | "--check"
            | "--json"
    )
}
fn parse_format(value: &str) -> Option<InputFormat> {
    match value {
        "markdown" => Some(InputFormat::Markdown),
        "json" => Some(InputFormat::Json),
        "yaml" => Some(InputFormat::Yaml),
        _ => None,
    }
}

fn auto_format(path: &str) -> Option<InputFormat> {
    match Path::new(path).extension().and_then(|x| x.to_str()) {
        Some("md") => Some(InputFormat::Markdown),
        Some("json") => Some(InputFormat::Json),
        Some("yaml" | "yml") => Some(InputFormat::Yaml),
        _ => None,
    }
}

fn input_format(path: &str, explicit: Option<InputFormat>) -> Result<InputFormat, String> {
    explicit.or_else(|| auto_format(path)).ok_or_else(|| {
        if path == "-" {
            "stdin requires explicit --from markdown|json|yaml".to_owned()
        } else {
            "cannot infer input format; use --from markdown|json|yaml".to_owned()
        }
    })
}

fn report_text(report: &ValidationReport) {
    for d in report.diagnostics() {
        eprintln!(
            "{}[{}]: {}",
            if d.severity == Severity::Error {
                "error"
            } else {
                "warning"
            },
            d.code,
            d.message
        )
    }
}

fn decode_document(source: &str, format: InputFormat) -> Result<Document, String> {
    match format {
        InputFormat::Markdown => document(source).map_err(|report| {
            report_text(&report);
            "document is invalid".to_owned()
        }),
        InputFormat::Json => parse_json(source).map_err(|error| error.to_string()),
        InputFormat::Yaml => parse_yaml(source).map_err(|error| error.to_string()),
    }
}

fn schema_cmd(a: &[String]) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!("ttyinv schema [--output FILE]");
        return exit::SUCCESS;
    }
    let Some(i) = a.iter().position(|x| x == "--output") else {
        print!("{}", schema_json());
        return exit::SUCCESS;
    };
    let Some(path) = a.get(i + 1) else {
        return usage("--output requires FILE");
    };
    if ttyinv_core::atomic_write(Path::new(path), schema_json()).is_err() {
        return exit::OUTPUT;
    }
    exit::SUCCESS
}

fn validate_cmd(a: &[String]) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!("ttyinv validate INPUT [--from markdown|json|yaml] [--json]");
        return exit::SUCCESS;
    }
    let mut json = false;
    let mut explicit = None;
    let mut path = None;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--json" => json = true,
            "--from" => {
                i += 1;
                let Some(value) = a.get(i) else {
                    return usage("--from requires FORMAT");
                };
                if value.starts_with('-') {
                    return usage("--from requires FORMAT");
                }
                explicit = parse_format(value);
                if explicit.is_none() {
                    return usage("--from must be markdown, json, or yaml");
                }
            }
            value if value.starts_with('-') && value != "-" => {
                return usage("unknown validate option")
            }
            value if path.is_none() => path = Some(value),
            _ => return usage("validate accepts one INPUT"),
        }
        i += 1;
    }
    let Some(path) = path else {
        return usage("validate requires INPUT");
    };
    let format = match input_format(path, explicit) {
        Ok(format) => format,
        Err(error) => return usage(&error),
    };
    let Ok(source) = read(path) else {
        return exit::INPUT;
    };
    let report = match format {
        InputFormat::Markdown => validate(&source),
        _ => match decode_document(&source, format) {
            Ok(document) => validate(&serialize_markdown(&document)),
            Err(error) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"valid": false, "diagnostics": [], "error": format!("invalid {} input: {error}", format.name())})
                    );
                } else {
                    eprintln!("error: invalid {} input: {error}", format.name());
                }
                return exit::INPUT;
            }
        },
    };
    if json {
        println!(
            "{}",
            serde_json::json!({"valid":report.is_valid(),"diagnostics":report.diagnostics()})
        )
    } else {
        report_text(&report);
    }
    if report.is_valid() {
        exit::SUCCESS
    } else {
        exit::DOCUMENT_INVALID
    }
}

fn convert_cmd(a: &[String]) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!(
            "ttyinv convert INPUT --to markdown|json|yaml [--output FILE|--stdout] [--from markdown|json|yaml]"
        );
        return exit::SUCCESS;
    }
    let mut path = None;
    let mut explicit = None;
    let mut target = None;
    let mut output = None;
    let mut stdout = false;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--from" => {
                i += 1;
                let Some(value) = a.get(i) else {
                    return usage("--from requires FORMAT");
                };
                if value.starts_with('-') {
                    return usage("--from requires FORMAT");
                }
                explicit = parse_format(value);
                if explicit.is_none() {
                    return usage("--from must be markdown, json, or yaml");
                }
            }
            "--to" => {
                i += 1;
                let Some(value) = a.get(i) else {
                    return usage("--to requires FORMAT");
                };
                if value.starts_with('-') {
                    return usage("--to requires FORMAT");
                }
                target = parse_format(value);
                if target.is_none() {
                    return usage("--to must be markdown, json, or yaml");
                }
            }
            "--output" => {
                i += 1;
                let Some(value) = a.get(i) else {
                    return usage("--output requires FILE");
                };
                if value.starts_with('-') {
                    return usage("--output requires FILE");
                }
                output = Some(value.as_str());
            }
            "--stdout" => stdout = true,
            value if value.starts_with('-') && value != "-" => {
                return usage("unknown convert option")
            }
            value if path.is_none() => path = Some(value),
            _ => return usage("convert accepts one INPUT"),
        }
        i += 1;
    }
    let Some(path) = path else {
        return usage("convert requires INPUT");
    };
    let Some(target) = target else {
        return usage("convert requires --to markdown, json, or yaml");
    };
    if output.is_some() && stdout {
        return usage("choose --output or --stdout, not both");
    }
    let format = match input_format(path, explicit) {
        Ok(format) => format,
        Err(error) => return usage(&error),
    };
    let Ok(source) = read(path) else {
        return exit::INPUT;
    };
    let document = match decode_document(&source, format) {
        Ok(document) => document,
        Err(error) => {
            if format != InputFormat::Markdown {
                eprintln!("error: invalid {} input: {error}", format.name());
            }
            return if format == InputFormat::Markdown {
                exit::DOCUMENT_INVALID
            } else {
                exit::INPUT
            };
        }
    };
    let converted = match target {
        InputFormat::Markdown => serialize_markdown(&document),
        InputFormat::Json => match to_json(&document) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("error: cannot encode json: {error}");
                return exit::OUTPUT;
            }
        },
        InputFormat::Yaml => match to_yaml(&document) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("error: cannot encode yaml: {error}");
                return exit::OUTPUT;
            }
        },
    };
    if let Some(path) = output {
        if ttyinv_core::atomic_write(Path::new(path), &converted).is_err() {
            return exit::OUTPUT;
        }
    } else {
        print!("{converted}");
    }
    exit::SUCCESS
}

fn sections_cmd(a: &[String]) -> i32 {
    let json = a.iter().any(|x| x == "--json");
    let Some(path) = a.iter().find(|x| !x.starts_with('-')) else {
        return usage("sections requires FILE");
    };
    let Ok(s) = read(path) else {
        return exit::INPUT;
    };
    let Ok(m) = structure_manifest(&s) else {
        return exit::DOCUMENT_INVALID;
    };
    if json {
        println!("{}", serde_json::to_string(&m).unwrap_or_default())
    } else {
        for x in &m.ordinary_sections {
            println!(
                "{}\t{}\t{}\t{}",
                x.index + 1,
                x.title,
                x.body,
                match x.gap {
                    ttyinv_core::Gap::None => "none",
                    ttyinv_core::Gap::Tight => "tight",
                    ttyinv_core::Gap::Standard => "standard",
                    ttyinv_core::Gap::Roomy => "roomy",
                }
            )
        }
    }
    exit::SUCCESS
}

fn edit_cmd(a: &[String]) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!(
            "ttyinv edit move-section FILE --from N --to N [--stdout|--check|--json]\nttyinv edit set-gap FILE --section N --gap GAP [--stdout|--check|--json]\nttyinv edit set-scalar FILE --path PATH --value VALUE [--stdout|--check|--json]"
        );
        return exit::SUCCESS;
    }
    if a.len() < 2 {
        return usage("edit requires operation and FILE");
    };
    let op = a[0].as_str();
    let path = &a[1];
    let Ok(s) = read(path) else {
        return exit::INPUT;
    };
    let mut from = None;
    let mut to = None;
    let mut section = None;
    let mut gap = None;
    let mut scalar_path = None;
    let mut value = None;
    let mut stdout = false;
    let mut check = false;
    let mut json = false;
    let mut i = 2;
    while i < a.len() {
        match a[i].as_str() {
            "--from" => {
                i += 1;
                from = a.get(i).and_then(|x| x.parse::<usize>().ok())
            }
            "--to" => {
                i += 1;
                to = a.get(i).and_then(|x| x.parse::<usize>().ok())
            }
            "--section" => {
                i += 1;
                section = a.get(i).and_then(|x| x.parse::<usize>().ok())
            }
            "--gap" => {
                i += 1;
                gap = a.get(i).cloned()
            }
            "--path" => {
                i += 1;
                let Some(next) = a.get(i) else {
                    return usage("--path requires PATH");
                };
                if is_edit_option(next) {
                    return usage("--path requires PATH");
                }
                scalar_path = Some(next.clone());
            }
            "--value" => {
                i += 1;
                let Some(next) = a.get(i) else {
                    return usage("--value requires VALUE");
                };
                if is_edit_option(next) {
                    return usage("--value requires VALUE");
                }
                value = Some(next.clone());
            }
            "--stdout" => stdout = true,
            "--check" => check = true,
            "--json" => json = true,
            _ => return usage("unknown edit option"),
        };
        i += 1
    }
    let operation = match op {
        "move-section" => {
            let (Some(from), Some(to)) = (from, to) else {
                return usage("move-section requires --from and --to");
            };
            if from == 0 || to == 0 {
                return usage("section indices are one-based");
            }
            EditOperation::MoveSection {
                from: from - 1,
                to: to - 1,
            }
        }
        "set-gap" => {
            let Some(section) = section else {
                return usage("set-gap requires --section");
            };
            if section == 0 {
                return usage("section indices are one-based");
            }
            let g = match gap.as_deref() {
                Some("none") => ttyinv_core::Gap::None,
                Some("tight") => ttyinv_core::Gap::Tight,
                Some("standard") => ttyinv_core::Gap::Standard,
                Some("roomy") => ttyinv_core::Gap::Roomy,
                _ => return usage("invalid gap"),
            };
            EditOperation::SetSectionGap {
                section: section - 1,
                gap: g,
            }
        }
        "set-scalar" => {
            let Some(path) = scalar_path else {
                return usage("set-scalar requires --path");
            };
            let Some(value) = value else {
                return usage("set-scalar requires --value");
            };
            EditOperation::SetScalar { path, value }
        }
        _ => return usage("unknown edit operation"),
    };
    let r = apply_edit(EditRequest {
        source: s.clone(),
        base_revision: revision(&s),
        sequence: 1,
        operation,
    });
    if json {
        println!("{}", serde_json::to_string(&r).unwrap_or_default());
        return if r.conflict || !r.diagnostics.is_empty() {
            exit::DOCUMENT_INVALID
        } else {
            exit::SUCCESS
        };
    }
    if !r.diagnostics.is_empty() {
        for d in &r.diagnostics {
            eprintln!("{}: {}", d.code, d.message)
        }
        return exit::DOCUMENT_INVALID;
    }
    if check {
        return if r.source == s {
            exit::SUCCESS
        } else {
            exit::DOCUMENT_INVALID
        };
    }
    if stdout {
        print!("{}", r.source);
        return exit::SUCCESS;
    }
    if ttyinv_core::atomic_write(Path::new(path), &r.source).is_err() {
        return exit::OUTPUT;
    }
    exit::SUCCESS
}
