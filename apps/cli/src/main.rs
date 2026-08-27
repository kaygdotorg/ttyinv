use std::borrow::Cow;
use std::collections::HashSet;
use std::env;
#[cfg(target_os = "linux")]
use std::ffi::CString;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(target_os = "linux")]
use std::os::raw::{c_int, c_long};
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStrExt;
#[cfg(target_os = "linux")]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use ttyinv_cli::exit;
use ttyinv_core::{
    execute, invalid_command_message, CanonicalFormat, CommandError, CommandErrorCode,
    CommandOutcome, Diagnostic, Document, EditOperationInput, FontWeight, InspectMode,
    InvoiceCommand, InvoiceDraft, PresentationConfigInput, RenderAssetInput, RenderFormat,
    RenderOptionsInput, RetryClass, Severity, Source, MAX_ASSET_BYTES, MAX_SOURCE_BYTES,
    PAGE_WIDTH,
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
/// Keep command envelopes bounded like the WASM adapter's decoded request budget.
const MAX_COMMAND_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputFormat {
    Markdown,
    Json,
    Yaml,
}

fn main() -> ! {
    process::exit(run(env::args().skip(1).collect()))
}

fn run(a: Vec<String>) -> i32 {
    match a.first().map(String::as_str) {
        Some("create") => create_cmd(&a[1..]),
        Some("validate") => validate_cmd(&a[1..]),
        Some("inspect") | Some("sections") => inspect_cmd(&a[1..], a[0] == "sections"),
        Some("convert") => convert_cmd(&a[1..]),
        Some("schema") => registry_cmd(&a[1..], false),
        Some("registry") => registry_cmd(&a[1..], true),
        Some("edit") => edit_cmd(&a[1..]),
        Some("prepare-render") => prepare_render_cmd(&a[1..]),
        Some("render") => render_cmd(&a[1..]),
        Some("presentation") | Some("resolve-presentation") => presentation_cmd(&a[1..]),
        Some("execute") => execute_cmd(&a[1..]),
        Some("--help") | Some("-h") | None => {
            print_help();
            exit::SUCCESS
        }
        _ => usage("unknown command"),
    }
}
fn print_help() {
    println!(
        "All commands use the shared ttyinv-core execute seam.\n\
ttyinv create DRAFT [--from json|yaml] [--output FILE|--stdout]\n\
ttyinv validate INPUT [--from markdown|json|yaml] [--json]\n\
ttyinv inspect INPUT [--from markdown|json|yaml] [--mode structure|summary|manifest] [--json]\n\
ttyinv convert INPUT --to markdown|json|yaml [--output FILE|--stdout] [--from markdown|json|yaml]\n\
ttyinv schema [--output FILE]\n\
ttyinv registry\n\
ttyinv execute [--input FILE]\n\
  Reads one JSON InvoiceCommand envelope from FILE or stdin and writes one JSON\n\
  CommandOutcome or CommandError. Rendered bytes are an array of unsigned octets.\n\
ttyinv render INPUT --format html|pdf|png [--output FILE|--stdout] [--force] [--from markdown|json|yaml] \
[--asset-base DIR] [--theme THEME] [--font FONT] [--font-weight regular|semibold] \
[--density comfortable|compact] [--accent #rrggbb] [--font-scale 100..=140] \
[--frame-inset 30..=60] [--png-scale 1|2]\n\
ttyinv prepare-render INPUT --format html|pdf|png [--output FILE|--stdout] [--from markdown|json|yaml]\n\
ttyinv sections FILE [--json]\n\
ttyinv presentation [--css]\n\
ttyinv edit move-section FILE --from N --to N [--stdout|--check|--json]\n\
ttyinv edit set-gap FILE --section N --gap GAP [--from markdown|json|yaml] [--stdout|--check|--json]\n\
ttyinv edit set-scalar FILE --path PATH --value VALUE [--from markdown|json|yaml] [--stdout|--check|--json]\n\
In-place edit writes Markdown only. Use --stdout or --json for structured input."
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
fn command_request_error(message: impl Into<String>) -> CommandError {
    CommandError {
        code: CommandErrorCode::InvalidRequest,
        diagnostics: vec![command_diagnostic("REQUEST001", message)],
        retry: RetryClass::AfterInputChange,
    }
}

fn command_diagnostic(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: code.to_owned(),
        message: message.into(),
        path: None,
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

fn command_allowed_fields(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "create" => Some(&["kind", "draft"]),
        "validate" => Some(&["kind", "source"]),
        "inspect" => Some(&["kind", "source", "mode"]),
        "convert" => Some(&["kind", "source", "to"]),
        "edit" => Some(&["kind", "source", "base_revision", "operation"]),
        "prepare_render" => Some(&["kind", "source", "options"]),
        "resolve_presentation" => Some(&["kind", "config"]),
        "render" => Some(&["kind", "source", "options"]),
        "registry" => Some(&["kind"]),
        _ => None,
    }
}

fn read_command(path: &str) -> Result<String, (CommandError, i32)> {
    let mut bytes = Vec::new();
    let result = if path == "-" {
        io::stdin()
            .take(MAX_COMMAND_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
    } else {
        let file = File::open(path).map_err(|error| {
            (
                command_request_error(format!("cannot read command input: {error}")),
                exit::INPUT,
            )
        })?;
        file.take(MAX_COMMAND_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
    };
    if result.is_err() {
        return Err((
            command_request_error("cannot read command input"),
            exit::INPUT,
        ));
    }
    if bytes.len() > MAX_COMMAND_BYTES {
        return Err((
            CommandError {
                code: CommandErrorCode::Limit,
                diagnostics: vec![command_diagnostic(
                    "LIMIT001",
                    format!("command envelope exceeds {MAX_COMMAND_BYTES} bytes"),
                )],
                retry: RetryClass::AfterInputChange,
            },
            exit::INPUT,
        ));
    }
    String::from_utf8(bytes).map_err(|_| {
        (
            command_request_error("command envelope must be UTF-8 JSON"),
            exit::INPUT,
        )
    })
}

fn write_json_line(value: String) -> bool {
    let mut stdout = io::stdout().lock();
    stdout.write_all(value.as_bytes()).is_ok() && stdout.write_all(b"\n").is_ok()
}

fn write_command_error(error: &CommandError) -> bool {
    match serde_json::to_string(error) {
        Ok(value) => write_json_line(value),
        Err(_) => false,
    }
}

fn write_command_outcome(outcome: &CommandOutcome) -> bool {
    match serde_json::to_string(outcome) {
        Ok(value) => write_json_line(value),
        Err(_) => false,
    }
}

fn execute_cmd(a: &[String]) -> i32 {
    let mut input: Option<&str> = None;
    let mut index = 0;
    while index < a.len() {
        match a[index].as_str() {
            "--input" => {
                if input.is_some() || index + 1 == a.len() {
                    return usage("--input requires exactly one FILE");
                }
                input = Some(&a[index + 1]);
                index += 1;
            }
            "--help" | "-h" => {
                if a.len() != 1 {
                    return usage("--help cannot be combined with other arguments");
                }
                print_help();
                return exit::SUCCESS;
            }
            _ => return usage("execute accepts only --input FILE"),
        }
        index += 1;
    }

    let source = match read_command(input.unwrap_or("-")) {
        Ok(source) => source,
        Err((error, fallback)) => {
            if !write_command_error(&error) {
                return exit::OUTPUT;
            }
            return if error.code == CommandErrorCode::Limit {
                command_error_exit(&error, fallback)
            } else {
                fallback
            };
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&source) {
        Ok(value) => value,
        Err(error) => {
            let command_error = command_request_error(format!("invalid command JSON: {error}"));
            if !write_command_error(&command_error) {
                return exit::OUTPUT;
            }
            return command_error_exit(&command_error, exit::INPUT);
        }
    };
    let object = match value.as_object() {
        Some(object) => object,
        None => {
            let command_error = command_request_error("command envelope must be a JSON object");
            if !write_command_error(&command_error) {
                return exit::OUTPUT;
            }
            return command_error_exit(&command_error, exit::INPUT);
        }
    };
    let kind = match object.get("kind").and_then(serde_json::Value::as_str) {
        Some(kind) => kind.to_owned(),
        None => {
            let command_error =
                command_request_error("command envelope requires string field `kind`");
            if !write_command_error(&command_error) {
                return exit::OUTPUT;
            }
            return command_error_exit(&command_error, exit::INPUT);
        }
    };
    let Some(allowed) = command_allowed_fields(&kind) else {
        let command_error = command_request_error(format!("unknown command kind `{kind}`"));
        if !write_command_error(&command_error) {
            return exit::OUTPUT;
        }
        return command_error_exit(&command_error, exit::INPUT);
    };
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        let command_error = command_request_error(format!("unknown command field: {field}"));
        if !write_command_error(&command_error) {
            return exit::OUTPUT;
        }
        return command_error_exit(&command_error, exit::INPUT);
    }
    let command: InvoiceCommand<'static> = match serde_json::from_value(value) {
        Ok(command) => command,
        Err(error) => {
            let command_error = command_request_error(invalid_command_message(error));
            if !write_command_error(&command_error) {
                return exit::OUTPUT;
            }
            return command_error_exit(&command_error, exit::INPUT);
        }
    };
    match execute(command) {
        Ok(outcome) => {
            if write_command_outcome(&outcome) {
                exit::SUCCESS
            } else {
                exit::OUTPUT
            }
        }
        Err(error) => {
            if !write_command_error(&error) {
                return exit::OUTPUT;
            }
            let fallback = if kind == "render" {
                exit::RENDER
            } else {
                exit::DOCUMENT_INVALID
            };
            command_error_exit(&error, fallback)
        }
    }
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

fn report_diagnostics(diagnostics: &[ttyinv_core::Diagnostic]) {
    for d in diagnostics {
        eprintln!(
            "{}[{}]: {}",
            if d.severity == ttyinv_core::Severity::Error {
                "error"
            } else {
                "warning"
            },
            d.code,
            d.message
        );
    }
}

fn source_input(source: String, format: InputFormat) -> Source<'static> {
    match format {
        InputFormat::Markdown => Source::Markdown(Cow::Owned(source)),
        InputFormat::Json => Source::Json(Cow::Owned(source)),
        InputFormat::Yaml => Source::Yaml(Cow::Owned(source)),
    }
}

fn borrowed_source_input<'a>(source: &'a str, format: InputFormat) -> Source<'a> {
    match format {
        InputFormat::Markdown => Source::Markdown(Cow::Borrowed(source)),
        InputFormat::Json => Source::Json(Cow::Borrowed(source)),
        InputFormat::Yaml => Source::Yaml(Cow::Borrowed(source)),
    }
}
fn document_images(document: &Document) -> impl Iterator<Item = &ttyinv_core::Image> {
    document
        .from
        .logo
        .iter()
        .chain(document.bill_to.logo.iter())
        .chain(
            document
                .signature
                .as_ref()
                .and_then(|signature| signature.image.as_ref()),
        )
}

fn asset_mime(path: &Path) -> Option<String> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("png") => Some("image/png".to_owned()),
        Some("jpg" | "jpeg") => Some("image/jpeg".to_owned()),
        Some("gif") => Some("image/gif".to_owned()),
        Some("webp") => Some("image/webp".to_owned()),
        Some("svg" | "svgz") => Some("image/svg+xml".to_owned()),
        _ => None,
    }
}
#[cfg(target_os = "linux")]
#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

#[cfg(target_os = "linux")]
const AT_FDCWD: c_int = -100;
#[cfg(target_os = "linux")]
const SYS_OPENAT2: c_long = 437;
#[cfg(target_os = "linux")]
const O_CLOEXEC: u64 = 0o2000000;
#[cfg(target_os = "linux")]
const O_NONBLOCK: u64 = 0o4000;
#[cfg(target_os = "linux")]
const O_DIRECTORY: u64 = 0o200000;
#[cfg(target_os = "linux")]
const O_PATH: u64 = 0o10000000;
#[cfg(target_os = "linux")]
const RESOLVE_BENEATH: u64 = 0x08;
#[cfg(target_os = "linux")]
const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
#[cfg(target_os = "linux")]
const RESOLVE_NO_SYMLINKS: u64 = 0x04;

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn syscall(number: c_long, ...) -> c_long;
}

#[cfg(target_os = "linux")]
fn openat2(dirfd: c_int, path: &Path, flags: u64, resolve: u64) -> io::Result<File> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "asset path contains NUL"))?;
    let how = OpenHow {
        flags,
        mode: 0,
        resolve,
    };
    // SAFETY: `path` and `how` remain alive for the duration of the syscall;
    // the kernel only reads their NUL-terminated/ABI-stable representations.
    let fd = unsafe {
        syscall(
            SYS_OPENAT2,
            dirfd,
            path.as_ptr(),
            &how as *const OpenHow,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a nonnegative descriptor was returned and ownership transfers to
    // this File exactly once.
    Ok(unsafe { File::from_raw_fd(fd as i32) })
}

#[cfg(target_os = "linux")]
fn open_asset_base(path: &Path) -> Result<File, String> {
    openat2(
        AT_FDCWD,
        path,
        O_PATH | O_DIRECTORY | O_CLOEXEC,
        RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
    )
    .map_err(|error| format!("cannot open asset base {}: {error}", path.display()))
}

#[cfg(not(target_os = "linux"))]
fn open_asset_base(path: &Path) -> Result<File, String> {
    Err(format!(
        "local asset rendering is unsupported on this platform (asset base {})",
        path.display()
    ))
}

#[cfg(target_os = "linux")]
fn open_relative_asset(base: &File, path: &Path, source: &str) -> Result<File, String> {
    let file = openat2(
        base.as_raw_fd(),
        path,
        O_CLOEXEC | O_NONBLOCK,
        RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
    )
    .map_err(|error| format!("cannot read asset {source:?}: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("cannot stat asset {source:?}: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("cannot read asset {source:?}: not a regular file"));
    }
    Ok(file)
}

#[cfg(not(target_os = "linux"))]
fn open_relative_asset(_base: &File, _path: &Path, source: &str) -> Result<File, String> {
    Err(format!(
        "local asset rendering is unsupported on this platform (asset {source:?})"
    ))
}

fn read_asset(mut file: File, display: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    Read::take(&mut file, MAX_ASSET_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read asset {display}: {error}"))?;
    if bytes.len() > MAX_ASSET_BYTES {
        return Err(format!("asset {display} exceeds {} bytes", MAX_ASSET_BYTES));
    }
    Ok(bytes)
}
fn with_local_assets(
    options: &mut RenderOptionsInput<'static>,
    document: &Document,
    input: &str,
    asset_base: Option<&Path>,
) -> Result<(), String> {
    let input_parent = if input == "-" {
        None
    } else {
        Some(
            Path::new(input)
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
    };
    let base = asset_base.or(input_parent);
    let mut trusted_base: Option<File> = None;
    let mut seen = HashSet::new();
    for image in document_images(document) {
        let source = image.src.trim();
        if source.is_empty()
            || source.starts_with("data:")
            || source.starts_with("http://")
            || source.starts_with("https://")
        {
            continue;
        }
        let path = Path::new(source);
        if path.is_absolute() {
            return Err(format!("absolute image path {source:?} is not allowed"));
        }
        let Some(base) = base else {
            return Err(format!(
                "relative image path {source:?} requires --asset-base when reading stdin"
            ));
        };
        if !seen.insert(source.to_owned()) {
            continue;
        }
        if trusted_base.is_none() {
            trusted_base = Some(open_asset_base(base)?);
        }
        let base_descriptor = trusted_base
            .as_ref()
            .expect("asset base descriptor initialized above");
        let file = open_relative_asset(base_descriptor, path, source)?;
        let bytes = read_asset(file, source)?;
        options.assets.push(RenderAssetInput {
            source: Cow::Owned(source.to_owned()),
            bytes: Cow::Owned(bytes),
            mime: asset_mime(path).map(Cow::Owned),
        });
    }
    Ok(())
}

fn command_error_exit(error: &CommandError, fallback: i32) -> i32 {
    report_diagnostics(&error.diagnostics);
    match error.code {
        CommandErrorCode::InvalidDocument | CommandErrorCode::Conflict => exit::DOCUMENT_INVALID,
        CommandErrorCode::InvalidRequest | CommandErrorCode::Unsupported => exit::USAGE,
        CommandErrorCode::Limit => fallback,
        CommandErrorCode::InvalidAsset
        | CommandErrorCode::Encoding
        | CommandErrorCode::Font
        | CommandErrorCode::Backend => exit::RENDER,
    }
}

fn print_sections_text(sections: &[ttyinv_core::SafeSection]) {
    for section in sections {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            section.index,
            section.title,
            section.body,
            format!("{:?}", section.gap).to_lowercase(),
            section.page_break_before,
            section.summary_only
        );
    }
}

fn manifest_json(manifest: &ttyinv_core::SafeManifest) -> serde_json::Value {
    serde_json::json!({
        "fixed_blocks": manifest.fixed_blocks,
        "ordinary_sections": manifest.sections,
    })
}
fn source_error_exit(
    error: &CommandError,
    fallback: i32,
    source: &str,
    format: InputFormat,
) -> i32 {
    let malformed = match format {
        InputFormat::Markdown => false,
        InputFormat::Json => serde_json::from_str::<serde_json::Value>(source).is_err(),
        InputFormat::Yaml => serde_yaml::from_str::<serde_yaml::Value>(source).is_err(),
    };
    if malformed {
        report_diagnostics(&error.diagnostics);
        return exit::INPUT;
    }
    command_error_exit(error, fallback)
}

fn write_output(path: &Path, bytes: &[u8], force: bool) -> io::Result<()> {
    let output = prepare_output(path, force)?;
    atomic_render_output(&output, bytes, force)
}

fn presentation_cmd(a: &[String]) -> i32 {
    let css = match a {
        [] => false,
        [value] if value == "--css" => true,
        _ => return usage("presentation accepts only --css"),
    };
    let result = execute(InvoiceCommand::ResolvePresentation {
        config: PresentationConfigInput::default(),
    });
    let value = match result {
        Ok(CommandOutcome::ResolvedPresentation { presentation }) => presentation,
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            return exit::INTERNAL;
        }
        Err(error) => return command_error_exit(&error, exit::RENDER),
    };
    if css {
        let mut generated = String::from(":root{");
        for (name, number) in &value.geometry {
            let _ = write!(generated, "--invoice-{name}:{number}px;");
        }
        let _ = write!(
            generated,
            "--invoice-page-width:595px;--invoice-page-height:842px;--invoice-frame-inset:{}px;\
--invoice-content-left:{}px;--invoice-content-right:{}px;--invoice-content-top:{}px;\
--invoice-content-bottom:{}px;--invoice-type-scale:{};--invoice-density-space:{};\
--invoice-paper:{};--invoice-ink:{};--invoice-muted:{};--invoice-rule:{};--invoice-accent:{};--invoice-canvas:{};}}",
            value.frame_inset,
            value.content.left,
            value.content.right,
            value.content.top,
            value.content.bottom,
            value.scale.type_scale,
            value.scale.density_space,
            value.tokens.paper,
            value.tokens.ink,
            value.tokens.muted,
            value.tokens.rule,
            value.tokens.accent,
            value.tokens.canvas
        );
        println!("{generated}");
    } else {
        match serde_json::to_string_pretty(&value) {
            Ok(json) => println!("{json}"),
            Err(_) => return exit::INTERNAL,
        }
    }
    exit::SUCCESS
}

fn prepare_render_cmd(a: &[String]) -> i32 {
    let mut path = None;
    let mut explicit = None;
    let mut format = None;
    let mut output = None;
    let mut stdout = false;
    let mut options = RenderOptionsInput::default();
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--from" => {
                i += 1;
                let Some(value) = a.get(i) else {
                    return usage("--from requires FORMAT");
                };
                explicit = parse_format(value);
                if explicit.is_none() {
                    return usage("--from must be markdown, json, or yaml");
                }
            }
            "--format" => {
                i += 1;
                format = match a.get(i).map(String::as_str) {
                    Some("html") => Some(RenderFormat::Html),
                    Some("pdf") => Some(RenderFormat::Pdf),
                    Some("png") => Some(RenderFormat::Png),
                    _ => return usage("--format must be html, pdf, or png"),
                };
            }
            "--output" => {
                i += 1;
                let Some(value) = a.get(i) else {
                    return usage("--output requires FILE");
                };
                output = Some(PathBuf::from(value));
            }
            "--stdout" => stdout = true,
            value if value.starts_with('-') => return usage("unknown prepare-render option"),
            value if path.is_none() => path = Some(value),
            _ => return usage("prepare-render accepts one INPUT"),
        }
        i += 1;
    }
    let Some(path) = path else {
        return usage("prepare-render requires INPUT");
    };
    let Some(format) = format else {
        return usage("prepare-render requires --format");
    };
    if output.is_some() && stdout {
        return usage("choose --output or --stdout, not both");
    }
    options.format = format;
    let input_format = match input_format(path, explicit) {
        Ok(value) => value,
        Err(error) => return usage(&error),
    };
    let source = match read(path) {
        Ok(value) => value,
        Err(_) => return exit::INPUT,
    };
    let plan = match execute(InvoiceCommand::PrepareRender {
        source: borrowed_source_input(&source, input_format),
        options,
    }) {
        Ok(CommandOutcome::Prepared { plan }) => plan,
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            return exit::INTERNAL;
        }
        Err(error) => return source_error_exit(&error, exit::RENDER, &source, input_format),
    };
    let encoded = match serde_json::to_string_pretty(&plan) {
        Ok(value) => value + "\n",
        Err(_) => return exit::INTERNAL,
    };
    if let Some(path) = output {
        if write_output(&path, encoded.as_bytes(), true).is_err() {
            return exit::OUTPUT;
        }
    } else {
        print!("{encoded}");
    }
    exit::SUCCESS
}

fn render_cmd(a: &[String]) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!(
            "ttyinv render INPUT --format html|pdf|png [--output FILE|--stdout] [--force] \
[--from markdown|json|yaml] [--asset-base DIR] [--theme THEME] [--font FONT] \
[--font-weight regular|semibold] [--density comfortable|compact] \
[--accent #rrggbb] [--font-scale 100..=140] [--frame-inset 30..=60] [--png-scale 1|2]"
        );
        return exit::SUCCESS;
    }
    let mut path = None;
    let mut format = None;
    let mut explicit = None;
    let mut asset_base = None;
    let mut output = None;
    let mut stdout = false;
    let mut force = false;
    let mut options = RenderOptionsInput::default();
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--format" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--format requires FORMAT");
                };
                format = match value.as_str() {
                    "html" => Some(RenderFormat::Html),
                    "pdf" => Some(RenderFormat::Pdf),
                    "png" => Some(RenderFormat::Png),
                    _ => return usage("--format must be html, pdf, or png"),
                };
                i += 1;
            }
            "--from" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--from requires FORMAT");
                };
                explicit = parse_format(value);
                if explicit.is_none() {
                    return usage("--from must be markdown, json, or yaml");
                }
                i += 1;
            }
            "--asset-base" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--asset-base requires DIR");
                };
                if value == "-" || value.starts_with('-') {
                    return usage("--asset-base requires DIR");
                }
                asset_base = Some(PathBuf::from(value));
                i += 1;
            }
            "--output" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--output requires FILE");
                };
                if value == "-" || value.starts_with('-') {
                    return usage("--output requires FILE");
                }
                output = Some(PathBuf::from(value));
                i += 1;
            }
            "--stdout" => stdout = true,
            "--force" => force = true,
            "--theme" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--theme requires THEME");
                };
                if value.starts_with('-') {
                    return usage("--theme requires THEME");
                }
                options.theme = Some(Cow::Owned(value.clone()));
                i += 1;
            }
            "--font" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--font requires FONT");
                };
                if value.starts_with('-') {
                    return usage("--font requires FONT");
                }
                options.font = Some(Cow::Owned(value.clone()));
                i += 1;
            }
            "--font-weight" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--font-weight requires regular or semibold");
                };
                options.font_weight = match value.as_str() {
                    "regular" => Some(FontWeight::Regular),
                    "semibold" => Some(FontWeight::Semibold),
                    _ => return usage("--font-weight must be regular or semibold"),
                };
                i += 1;
            }
            "--density" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--density requires comfortable or compact");
                };
                options.density = Some(Cow::Owned(value.clone()));
                i += 1;
            }
            "--accent" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--accent requires #rrggbb");
                };
                options.accent = Some(Cow::Owned(value.clone()));
                i += 1;
            }
            "--font-scale" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--font-scale requires 100..=140");
                };
                options.font_scale = match value.parse::<u8>() {
                    Ok(value) => Some(value),
                    Err(_) => return usage("--font-scale requires an integer from 100 to 140"),
                };
                i += 1;
            }
            "--frame-inset" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--frame-inset requires 30..=60");
                };
                options.frame_inset = match value.parse::<u8>() {
                    Ok(value) => Some(value),
                    Err(_) => return usage("--frame-inset requires an integer from 30 to 60"),
                };
                i += 1;
            }
            "--png-scale" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--png-scale requires 1 or 2");
                };
                options.png_scale = match value.parse::<u8>() {
                    Ok(value @ 1..=2) => Some(value),
                    _ => return usage("--png-scale requires 1 or 2"),
                };
                i += 1;
            }
            value if value == "-" => {
                if path.is_some() {
                    return usage("render accepts one INPUT");
                }
                path = Some(value);
            }
            value if value.starts_with('-') => return usage("unknown render option"),
            value if path.is_none() => path = Some(value),
            _ => return usage("render accepts one INPUT"),
        }
        i += 1;
    }
    let Some(path) = path else {
        return usage("render requires INPUT");
    };
    let Some(format) = format else {
        return usage("render requires --format");
    };
    options.format = format;
    if output.is_some() && stdout {
        return usage("choose exactly one of --output or --stdout");
    }
    let destination = match (stdout, output) {
        (true, _) => None,
        (false, Some(path)) => Some(path),
        (false, None) if path == "-" => return usage("stdin requires --output FILE or --stdout"),
        (false, None) => Some(Path::new(path).with_extension(format.extension())),
    };
    let prepared_output = match destination.as_deref() {
        Some(destination) => match prepare_output(destination, force) {
            Ok(output) => Some(output),
            Err(error) => {
                eprintln!("error: cannot prepare output: {error}");
                return exit::OUTPUT;
            }
        },
        None => None,
    };
    let input_format = match input_format(path, explicit) {
        Ok(value) => value,
        Err(message) => return usage(&message),
    };
    let source = match read(path) {
        Ok(value) => value,
        Err(code) => return code,
    };
    let requested_png_scale = options.png_scale.unwrap_or(1);
    let validated = match execute(InvoiceCommand::Validate {
        source: borrowed_source_input(&source, input_format),
    }) {
        Ok(CommandOutcome::Validated { document, .. }) => document,
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            return exit::INTERNAL;
        }
        Err(error) => {
            return source_error_exit(&error, exit::DOCUMENT_INVALID, &source, input_format)
        }
    };
    let Some(document) = validated else {
        eprintln!("error: executor returned no validated document");
        return exit::INTERNAL;
    };
    if let Err(message) = with_local_assets(&mut options, &document, path, asset_base.as_deref()) {
        eprintln!("error: {message}");
        return exit::RENDER;
    }
    let result = match execute(InvoiceCommand::Render {
        source: source_input(source, input_format),
        options,
    }) {
        Ok(CommandOutcome::Rendered {
            bytes,
            width,
            warnings,
            ..
        }) => (bytes, width, warnings),
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            return exit::INTERNAL;
        }
        Err(error) => return command_error_exit(&error, exit::RENDER),
    };
    let (bytes, width, warnings) = result;
    for warning in &warnings {
        if warning.code == "PNG_SCALE_REDUCED" {
            let actual_png_scale = width / PAGE_WIDTH;
            eprintln!(
                "warning[{}]: {} (requested scale {}; actual scale {})",
                warning.code, warning.message, requested_png_scale, actual_png_scale
            );
        } else {
            eprintln!("warning[{}]: {}", warning.code, warning.message);
        }
    }
    if stdout {
        let mut handle = io::stdout();
        if handle
            .write_all(&bytes)
            .and_then(|_| handle.flush())
            .is_err()
        {
            return exit::OUTPUT;
        }
    } else if let Some(output) = prepared_output {
        if atomic_render_output(&output, &bytes, force).is_err() {
            return exit::OUTPUT;
        }
    }
    exit::SUCCESS
}

struct PreparedOutput {
    #[cfg(target_os = "linux")]
    dir: File,
    #[cfg(target_os = "linux")]
    name: std::ffi::OsString,
    #[cfg(not(target_os = "linux"))]
    path: PathBuf,
}

fn output_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
#[cfg(target_os = "linux")]
fn prepare_output(path: &Path, force: bool) -> io::Result<PreparedOutput> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "output path has no file name")
    })?;
    if name == std::ffi::OsStr::new(".") || name == std::ffi::OsStr::new("..") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output path has an invalid file name",
        ));
    }
    // Acquire the parent before reading or rendering the input. This descriptor
    // remains authoritative even if an ancestor is replaced later.
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(0o200000 | 0o400000)
        .open(output_parent(path))?;
    let dir_path = PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd()));
    let destination = dir_path.join(name);
    if !force {
        match fs::symlink_metadata(&destination) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "output already exists (use --force to replace it)",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(PreparedOutput {
        dir,
        name: name.to_owned(),
    })
}

#[cfg(not(target_os = "linux"))]
fn prepare_output(path: &Path, force: bool) -> io::Result<PreparedOutput> {
    let metadata = fs::metadata(output_parent(path))?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "output parent is not a directory",
        ));
    }
    if !force {
        match fs::symlink_metadata(path) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "output already exists (use --force to replace it)",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(PreparedOutput {
        path: path.to_owned(),
    })
}

#[cfg(target_os = "linux")]
fn atomic_render_output(output: &PreparedOutput, bytes: &[u8], force: bool) -> io::Result<()> {
    let dir_path = PathBuf::from(format!("/proc/self/fd/{}", output.dir.as_raw_fd()));
    let name = output.name.to_string_lossy();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = dir_path.join(format!(
        ".{name}.ttyinv-{stamp}-{}-{counter}.tmp",
        process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let write_result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    let destination = dir_path.join(output.name.as_os_str());
    let result = if force {
        fs::rename(&temp, &destination)
    } else {
        fs::hard_link(&temp, &destination).and_then(|_| fs::remove_file(&temp))
    };
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    output.dir.sync_all()
}

#[cfg(not(target_os = "linux"))]
fn atomic_render_output(output: &PreparedOutput, _bytes: &[u8], _force: bool) -> io::Result<()> {
    let _ = &output.path;
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic output requires Linux directory-handle primitives",
    ))
}

fn create_cmd(a: &[String]) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!("ttyinv create DRAFT [--from json|yaml] [--output FILE|--stdout]");
        return exit::SUCCESS;
    }
    let mut explicit = None;
    let mut output = None;
    let mut stdout = false;
    let mut path = None;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--from" => {
                i += 1;
                let Some(value) = a.get(i) else {
                    return usage("--from requires json or yaml");
                };
                explicit = match value.as_str() {
                    "json" => Some(InputFormat::Json),
                    "yaml" => Some(InputFormat::Yaml),
                    _ => return usage("--from must be json or yaml"),
                };
            }
            "--output" => {
                i += 1;
                let Some(value) = a.get(i) else {
                    return usage("--output requires FILE");
                };
                output = Some(PathBuf::from(value));
            }
            "--stdout" => stdout = true,
            value if value.starts_with('-') => return usage("unknown create option"),
            value if path.is_none() => path = Some(value),
            _ => return usage("create accepts one DRAFT"),
        }
        i += 1;
    }
    let Some(path) = path else {
        return usage("create requires DRAFT");
    };
    if output.is_some() && stdout {
        return usage("choose --output or --stdout, not both");
    }
    let format = match input_format(path, explicit) {
        Ok(InputFormat::Json) => InputFormat::Json,
        Ok(InputFormat::Yaml) => InputFormat::Yaml,
        Ok(InputFormat::Markdown) => return usage("create accepts JSON or YAML drafts"),
        Err(error) => return usage(&error),
    };
    let source = match read(path) {
        Ok(value) => value,
        Err(_) => return exit::INPUT,
    };
    let draft = match format {
        InputFormat::Json => {
            serde_json::from_str::<InvoiceDraft<'_>>(&source).map_err(|error| error.to_string())
        }
        InputFormat::Yaml => {
            serde_yaml::from_str::<InvoiceDraft<'_>>(&source).map_err(|error| error.to_string())
        }
        InputFormat::Markdown => unreachable!(),
    };
    let draft = match draft {
        Ok(value) => value,
        Err(error) => {
            eprintln!("error: invalid draft: {error}");
            return exit::INPUT;
        }
    };
    match execute(InvoiceCommand::Create {
        draft: Box::new(draft),
    }) {
        Ok(CommandOutcome::Created { source, .. }) => {
            if let Some(path) = output {
                if write_output(Path::new(&path), source.as_bytes(), true).is_err() {
                    return exit::OUTPUT;
                }
            } else {
                print!("{source}");
            }
            exit::SUCCESS
        }
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            exit::INTERNAL
        }
        Err(error) => command_error_exit(&error, exit::DOCUMENT_INVALID),
    }
}
fn registry_cmd(a: &[String], full: bool) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!("ttyinv schema [--output FILE]\nttyinv registry");
        return exit::SUCCESS;
    }
    let output = match a {
        [] => None,
        [flag, path] if flag == "--output" => Some(PathBuf::from(path)),
        _ => return usage("schema accepts only --output FILE"),
    };
    let registry = match execute(InvoiceCommand::Registry) {
        Ok(CommandOutcome::Registry(value)) => value,
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            return exit::INTERNAL;
        }
        Err(error) => return command_error_exit(&error, exit::OUTPUT),
    };
    let encoded = if full {
        match serde_json::to_string_pretty(&registry) {
            Ok(value) => value + "\n",
            Err(_) => return exit::INTERNAL,
        }
    } else {
        registry.document_schema
    };
    if let Some(path) = output {
        if write_output(&path, encoded.as_bytes(), true).is_err() {
            return exit::OUTPUT;
        }
    } else {
        print!("{encoded}");
    }
    exit::SUCCESS
}

fn validate_cmd(a: &[String]) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!("ttyinv validate INPUT [--from markdown|json|yaml] [--json]");
        return exit::SUCCESS;
    }
    let json = a.iter().any(|x| x == "--json");
    let mut explicit = None;
    let mut path = None;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--json" => {}
            "--from" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--from requires FORMAT");
                };
                explicit = parse_format(value);
                if explicit.is_none() {
                    return usage("--from must be markdown, json, or yaml");
                }
                i += 1;
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
    let source = match read(path) {
        Ok(value) => value,
        Err(_) => return exit::INPUT,
    };
    match execute(InvoiceCommand::Validate {
        source: borrowed_source_input(&source, format),
    }) {
        Ok(CommandOutcome::Validated {
            revision,
            valid,
            diagnostics,
            ..
        }) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"valid": valid, "revision": revision, "diagnostics": diagnostics})
                );
            } else {
                report_diagnostics(&diagnostics);
            }
            if valid {
                exit::SUCCESS
            } else {
                exit::DOCUMENT_INVALID
            }
        }
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            exit::INTERNAL
        }
        Err(error) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"valid": false, "diagnostics": error.diagnostics})
                );
            } else {
                report_diagnostics(&error.diagnostics);
            }
            source_error_exit(&error, exit::DOCUMENT_INVALID, &source, format)
        }
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
                output = Some(PathBuf::from(value));
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
    let format = explicit
        .or_else(|| auto_format(path))
        .unwrap_or(InputFormat::Markdown);
    let source = match read(path) {
        Ok(value) => value,
        Err(_) => return exit::INPUT,
    };
    let target = match target {
        InputFormat::Markdown => CanonicalFormat::Markdown,
        InputFormat::Json => CanonicalFormat::Json,
        InputFormat::Yaml => CanonicalFormat::Yaml,
    };
    let converted = match execute(InvoiceCommand::Convert {
        source: borrowed_source_input(&source, format),
        to: target,
    }) {
        Ok(CommandOutcome::Converted { source, .. }) => source,
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            return exit::INTERNAL;
        }
        Err(error) => return source_error_exit(&error, exit::DOCUMENT_INVALID, &source, format),
    };
    if let Some(path) = output {
        if write_output(&path, converted.as_bytes(), true).is_err() {
            return exit::OUTPUT;
        }
    } else {
        print!("{converted}");
    }
    exit::SUCCESS
}

fn inspect_cmd(a: &[String], sections_alias: bool) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!(
            "ttyinv inspect INPUT [--from markdown|json|yaml] [--mode structure|summary|manifest] [--json]"
        );
        return exit::SUCCESS;
    }
    let json = a.iter().any(|x| x == "--json");
    let mut explicit = None;
    let mut mode = if sections_alias {
        InspectMode::Manifest
    } else {
        InspectMode::Summary
    };
    let mut path = None;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--json" => {}
            "--from" => {
                i += 1;
                let Some(value) = a.get(i) else {
                    return usage("--from requires FORMAT");
                };
                explicit = parse_format(value);
                if explicit.is_none() {
                    return usage("--from must be markdown, json, or yaml");
                }
            }
            "--mode" => {
                i += 1;
                mode = match a.get(i).map(String::as_str) {
                    Some("structure") => InspectMode::Structure,
                    Some("summary") => InspectMode::Summary,
                    Some("manifest") => InspectMode::Manifest,
                    _ => return usage("--mode must be structure, summary, or manifest"),
                };
            }
            value if value.starts_with('-') => return usage("unknown inspect option"),
            value if path.is_none() => path = Some(value),
            _ => return usage("inspect accepts one INPUT"),
        }
        i += 1;
    }
    let Some(path) = path else {
        return usage("inspect requires INPUT");
    };
    if sections_alias {
        mode = InspectMode::Manifest;
    }
    let format = explicit
        .or_else(|| auto_format(path))
        .unwrap_or(InputFormat::Markdown);
    let source = match read(path) {
        Ok(value) => value,
        Err(_) => return exit::INPUT,
    };
    let result = execute(InvoiceCommand::Inspect {
        source: borrowed_source_input(&source, format),
        mode,
    });
    match result {
        Ok(CommandOutcome::Inspected {
            revision,
            valid,
            mode: inspected_mode,
            structure,
            summary,
            manifest,
            diagnostics,
        }) => {
            if json {
                if sections_alias {
                    let payload = manifest.as_ref().map(manifest_json).unwrap_or_else(
                        || serde_json::json!({"fixed_blocks": [], "ordinary_sections": []}),
                    );
                    println!("{payload}");
                    return exit::SUCCESS;
                }
                println!(
                    "{}",
                    serde_json::json!({
                        "revision": revision,
                        "valid": valid,
                        "mode": inspected_mode,
                        "structure": structure,
                        "summary": summary,
                        "manifest": manifest,
                        "diagnostics": diagnostics
                    })
                );
                return exit::SUCCESS;
            }
            match inspected_mode {
                InspectMode::Structure => {
                    if let Some(value) = structure {
                        print_sections_text(&value.sections);
                    }
                }
                InspectMode::Summary => {
                    if let Some(value) = summary {
                        println!(
                            "revision: {revision}\nsections: {}\ntables: {}\nrows: {}",
                            value.section_count, value.table_count, value.row_count
                        );
                    }
                }
                InspectMode::Manifest => {
                    if let Some(value) = manifest {
                        for block in value.fixed_blocks {
                            println!("fixed\t{block}");
                        }
                        print_sections_text(&value.sections);
                    }
                }
            }
            exit::SUCCESS
        }
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            exit::INTERNAL
        }
        Err(error) => source_error_exit(&error, exit::DOCUMENT_INVALID, &source, format),
    }
}
fn edit_cmd(a: &[String]) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!(
            "ttyinv edit move-section FILE --from N --to N [--stdout|--check|--json]\nttyinv edit set-gap FILE --section N --gap GAP [--stdout|--check|--json]\nttyinv edit set-scalar FILE --path PATH --value VALUE [--stdout|--check|--json]\n\
Use --from markdown|json|yaml when FILE has no format extension. In-place edit writes Markdown only; use --stdout or --json for structured input."
        );
        return exit::SUCCESS;
    }
    if a.len() < 2 {
        return usage("edit requires operation and FILE");
    }
    let op = a[0].as_str();
    let path = &a[1];
    let source = match read(path) {
        Ok(value) => value,
        Err(_) => return exit::INPUT,
    };
    let mut from = None;
    let mut explicit_format = None;
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
                let Some(next) = a.get(i) else {
                    return usage("--from requires N or FORMAT");
                };
                if let Ok(index) = next.parse::<usize>() {
                    from = Some(index);
                } else {
                    explicit_format = parse_format(next);
                    if explicit_format.is_none() {
                        return usage("--from must be a section number or markdown, json, or yaml");
                    }
                }
            }
            "--to" => {
                i += 1;
                to = a.get(i).and_then(|x| x.parse::<usize>().ok());
            }
            "--section" => {
                i += 1;
                section = a.get(i).and_then(|x| x.parse::<usize>().ok());
            }
            "--gap" => {
                i += 1;
                gap = a.get(i).cloned();
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
        }
        i += 1;
    }
    let operation = match op {
        "move-section" => {
            let (Some(from), Some(to)) = (from, to) else {
                return usage("move-section requires --from and --to");
            };
            if from == 0 || to == 0 {
                return usage("section indices are one-based");
            }
            EditOperationInput::MoveSection {
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
            let gap = match gap.as_deref() {
                Some("none") => ttyinv_core::Gap::None,
                Some("tight") => ttyinv_core::Gap::Tight,
                Some("standard") => ttyinv_core::Gap::Standard,
                Some("roomy") => ttyinv_core::Gap::Roomy,
                _ => return usage("invalid gap"),
            };
            EditOperationInput::SetSectionGap {
                section: section - 1,
                gap,
            }
        }
        "set-scalar" => {
            let Some(path) = scalar_path else {
                return usage("set-scalar requires --path");
            };
            let Some(value) = value else {
                return usage("set-scalar requires --value");
            };
            EditOperationInput::SetScalar {
                path: Cow::Owned(path),
                value: Cow::Owned(value),
            }
        }
        _ => return usage("unknown edit operation"),
    };
    let format = explicit_format
        .or_else(|| auto_format(path))
        .unwrap_or(InputFormat::Markdown);
    let revision = match execute(InvoiceCommand::Validate {
        source: borrowed_source_input(&source, format),
    }) {
        Ok(CommandOutcome::Validated { revision, .. }) => revision,
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            return exit::INTERNAL;
        }
        Err(error) => return source_error_exit(&error, exit::DOCUMENT_INVALID, &source, format),
    };
    let original_source = source.clone();
    let result = execute(InvoiceCommand::Edit {
        source: source_input(source, format),
        base_revision: Cow::Owned(revision),
        operation,
    });
    let (edited, edited_revision, diagnostics) = match result {
        Ok(CommandOutcome::Edited {
            source,
            revision,
            diagnostics,
            ..
        }) => (source, revision, diagnostics),
        Ok(_) => {
            eprintln!("error: executor returned the wrong outcome");
            return exit::INTERNAL;
        }
        Err(error) => {
            if json {
                println!("{}", serde_json::json!({"diagnostics": error.diagnostics}));
            } else {
                report_diagnostics(&error.diagnostics);
            }
            return command_error_exit(&error, exit::DOCUMENT_INVALID);
        }
    };
    if !stdout && !check && !json && format != InputFormat::Markdown {
        eprintln!(
            "error[EDIT005]: in-place edit supports Markdown only; use --stdout or --json for structured input"
        );
        return exit::USAGE;
    }
    if json {
        println!(
            "{}",
            serde_json::json!({"source": edited, "revision": edited_revision, "diagnostics": diagnostics})
        );
        return exit::SUCCESS;
    }
    if check {
        return if edited == original_source {
            exit::SUCCESS
        } else {
            exit::DOCUMENT_INVALID
        };
    }
    if stdout {
        print!("{edited}");
        return exit::SUCCESS;
    }
    if write_output(Path::new(path), edited.as_bytes(), true).is_err() {
        return exit::OUTPUT;
    }
    exit::SUCCESS
}
