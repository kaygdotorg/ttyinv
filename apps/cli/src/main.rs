use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process;
use ttyinv_cli::exit;
use ttyinv_core::{
    EditOperation, EditRequest, MAX_SOURCE_BYTES, Severity, apply_edit, revision, schema_json,
    structure_manifest, validate,
};
fn main() -> ! {
    process::exit(run(env::args().skip(1).collect()))
}
fn run(a: Vec<String>) -> i32 {
    match a.first().map(String::as_str) {
        Some("validate") => validate_cmd(&a[1..]),
        Some("schema") => schema_cmd(&a[1..]),
        Some("sections") => sections_cmd(&a[1..]),
        Some("edit") => edit_cmd(&a[1..]),
        Some("--help") | Some("-h") | None => {
            println!(
                "ttyinv validate FILE [--json]\\nttyinv schema [--output FILE]\\nttyinv sections FILE [--json]\\nttyinv edit move-section FILE --from N --to N [--stdout|--check|--json]\\nttyinv edit set-gap FILE --section N --gap GAP [--stdout|--check|--json]"
            );
            exit::SUCCESS
        }
        _ => usage("unknown command"),
    }
}
fn read(path: &str) -> Result<String, i32> {
    let f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return Err(exit::INPUT),
    };
    let mut b = Vec::new();
    if f.take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut b)
        .is_err()
    {
        return Err(exit::INPUT);
    }
    if b.len() > MAX_SOURCE_BYTES {
        return Err(exit::INPUT);
    }
    String::from_utf8(b).map_err(|_| exit::INPUT)
}
fn usage(s: &str) -> i32 {
    eprintln!("error: {s}");
    exit::USAGE
}
fn schema_cmd(a: &[String]) -> i32 {
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
    let json = a.iter().any(|x| x == "--json");
    let Some(path) = a.iter().find(|x| !x.starts_with('-')) else {
        return usage("validate requires FILE");
    };
    let Ok(s) = read(path) else {
        return exit::INPUT;
    };
    let r = validate(&s);
    if json {
        println!(
            "{}",
            serde_json::json!({"valid":r.is_valid(),"diagnostics":r.diagnostics()})
        )
    } else {
        for d in r.diagnostics() {
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
    if r.is_valid() {
        exit::SUCCESS
    } else {
        exit::DOCUMENT_INVALID
    }
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
        return if r.source == s { exit::SUCCESS } else { 1 };
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
