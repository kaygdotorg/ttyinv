use std::collections::HashSet;
use std::env;
#[cfg(target_os = "linux")]
use std::ffi::CString;
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
    apply_edit, document, parse_json, parse_yaml, render, render_document, revision, schema_json,
    serialize_markdown, structure_manifest, to_json, to_yaml, validate, Document, EditOperation,
    EditRequest, FontWeight, RenderAsset, RenderError, RenderFormat, RenderOptions, Severity,
    ValidationReport, MAX_ASSET_BYTES, MAX_SOURCE_BYTES,
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
        Some("render") => render_cmd(&a[1..]),
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
ttyinv render INPUT --format html|pdf|png [--output FILE|--stdout] [--force] [--from markdown|json|yaml] \
[--theme THEME] [--font FONT] [--font-weight regular|semibold] [--density comfortable|compact] \
[--accent #rrggbb] [--font-scale 100..=140] [--frame-inset 30..=60]\n\
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
    openat2(
        base.as_raw_fd(),
        path,
        O_CLOEXEC,
        RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
    )
    .map_err(|error| format!("cannot read asset {source:?}: {error}"))
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
    options: &mut RenderOptions,
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
        options.assets.push(RenderAsset {
            source: source.to_owned(),
            bytes,
            mime: asset_mime(path),
        });
    }
    Ok(())
}

fn render_cmd(a: &[String]) -> i32 {
    if a.iter().any(|x| x == "--help" || x == "-h") {
        println!(
            "ttyinv render INPUT --format html|pdf|png [--output FILE|--stdout] [--force] \
[--from markdown|json|yaml] [--asset-base DIR] [--theme THEME] [--font FONT] \
[--font-weight regular|semibold] [--density comfortable|compact] \
[--accent #rrggbb] [--font-scale 100..=140] [--frame-inset 30..=60]"
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
    let mut options = RenderOptions::default();
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
                options.theme = Some(value.clone());
                i += 1;
            }
            "--font" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--font requires FONT");
                };
                if value.starts_with('-') {
                    return usage("--font requires FONT");
                }
                options.font = Some(value.clone());
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
                options.density = Some(value.clone());
                i += 1;
            }
            "--accent" => {
                let Some(value) = a.get(i + 1) else {
                    return usage("--accent requires #rrggbb");
                };
                options.accent = Some(value.clone());
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
    let result = if input_format == InputFormat::Markdown {
        if let Ok(doc) = document(&source) {
            if let Err(message) = with_local_assets(&mut options, &doc, path, asset_base.as_deref())
            {
                eprintln!("error: {message}");
                return exit::RENDER;
            }
        }
        render(&source, options)
    } else {
        let doc = match decode_document(&source, input_format) {
            Ok(value) => value,
            Err(message) => {
                eprintln!("error: {message}");
                return exit::DOCUMENT_INVALID;
            }
        };
        if let Err(message) = with_local_assets(&mut options, &doc, path, asset_base.as_deref()) {
            eprintln!("error: {message}");
            return exit::RENDER;
        }
        render_document(&doc, options)
    };
    let result = match result {
        Ok(value) => value,
        Err(error) => return render_error_exit(error),
    };
    if stdout {
        let mut handle = io::stdout();
        if handle
            .write_all(&result.bytes)
            .and_then(|_| handle.flush())
            .is_err()
        {
            return exit::OUTPUT;
        }
    } else if let Some(output) = prepared_output {
        if atomic_render_output(&output, &result.bytes, force).is_err() {
            return exit::OUTPUT;
        }
    }
    exit::SUCCESS
}

fn render_error_exit(error: RenderError) -> i32 {
    match error {
        RenderError::InvalidDocument(diagnostics) => {
            for diagnostic in &diagnostics {
                eprintln!("error[{}]: {}", diagnostic.code, diagnostic.message);
            }
            exit::DOCUMENT_INVALID
        }
        RenderError::SourceTooLarge { .. } => exit::INPUT,
        RenderError::UnsupportedTheme(_)
        | RenderError::UnsupportedFont(_)
        | RenderError::UnsupportedDensity(_)
        | RenderError::InvalidAccent(_)
        | RenderError::InvalidOption(_) => {
            eprintln!("error: {error}");
            exit::USAGE
        }
        RenderError::InvalidAsset(_)
        | RenderError::OutputTooLarge { .. }
        | RenderError::Encoding(_)
        | RenderError::Font(_)
        | RenderError::Backend(_) => {
            eprintln!("error: {error}");
            exit::RENDER
        }
    }
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
