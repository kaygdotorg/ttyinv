use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;

mod model;

use model::*;
pub use model::{Money, SourcePosition, SourceSpan};
use std::collections::BTreeMap;

/// Typed diagnostic taxonomy for the Rust engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Frontmatter001,
    Yaml001,
    Schema001,
    Schema002,
    Schema003,
    Currency001,
    Date001,
    Date002,
    Markdown001,
    Markdown002,
    Markdown003,
    Table001,
    Table002,
    Table003,
    Table004,
    Html001,
    Limit001,
}

impl Code {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Frontmatter001 => "FRONTMATTER001",
            Self::Yaml001 => "YAML001",
            Self::Schema001 => "SCHEMA001",
            Self::Schema002 => "SCHEMA002",
            Self::Schema003 => "SCHEMA003",
            Self::Currency001 => "CURRENCY001",
            Self::Date001 => "DATE001",
            Self::Date002 => "DATE002",
            Self::Markdown001 => "MARKDOWN001",
            Self::Markdown002 => "MARKDOWN002",
            Self::Markdown003 => "MARKDOWN003",
            Self::Table001 => "TABLE001",
            Self::Table002 => "TABLE002",
            Self::Table003 => "TABLE003",
            Self::Table004 => "TABLE004",
            Self::Html001 => "HTML001",
            Self::Limit001 => "LIMIT001",
        }
    }

    pub const fn severity(self) -> Severity {
        match self {
            Self::Markdown002 | Self::Markdown003 => Severity::Warning,
            _ => Severity::Error,
        }
    }

    pub const fn default_message(self) -> &'static str {
        match self {
            Self::Frontmatter001 => "invalid frontmatter delimiter",
            Self::Yaml001 => "malformed YAML",
            Self::Schema001 => "invalid frontmatter",
            Self::Schema002 => "unsupported schema; expected ttyinv/v1",
            Self::Schema003 => "required value is absent or blank",
            Self::Currency001 => "currency must be three uppercase ASCII letters",
            Self::Date001 => "date must be a real YYYY-MM-DD date",
            Self::Date002 => "due date cannot be before issue date",
            Self::Markdown001 => "invalid Markdown heading",
            Self::Markdown002 => "invoice must contain at least one financial table",
            Self::Markdown003 => "invoice must contain at least one H2 section",
            Self::Table001 => "table has fewer than two headings",
            Self::Table002 => "table has no body rows",
            Self::Table003 => "table row has an invalid width",
            Self::Table004 => "financial section cannot contain a second table",
            Self::Html001 => "unsupported raw HTML",
            Self::Limit001 => "diagnostic limit reached",
        }
    }
}

/// Diagnostic code identifiers emitted by this crate.
pub mod codes {
    use super::Code;
    pub const FRONTMATTER001: &str = Code::Frontmatter001.as_str();
    pub const YAML001: &str = Code::Yaml001.as_str();
    pub const SCHEMA001: &str = Code::Schema001.as_str();
    pub const SCHEMA002: &str = Code::Schema002.as_str();
    pub const SCHEMA003: &str = Code::Schema003.as_str();
    pub const CURRENCY001: &str = Code::Currency001.as_str();
    pub const DATE001: &str = Code::Date001.as_str();
    pub const DATE002: &str = Code::Date002.as_str();
    pub const MARKDOWN001: &str = Code::Markdown001.as_str();
    pub const MARKDOWN002: &str = Code::Markdown002.as_str();
    pub const MARKDOWN003: &str = Code::Markdown003.as_str();
    pub const TABLE001: &str = Code::Table001.as_str();
    pub const TABLE002: &str = Code::Table002.as_str();
    pub const TABLE003: &str = Code::Table003.as_str();
    pub const TABLE004: &str = Code::Table004.as_str();
    pub const HTML001: &str = Code::Html001.as_str();
    pub const LIMIT001: &str = Code::Limit001.as_str();
}

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    /// Adapter-owned source file path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Canonical path in the parsed document tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Authored H2 title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// One-based H2 ordinal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_name: Option<String>,
}

impl Diagnostic {
    /// Attach the source path supplied by an adapter.
    pub fn set_path(&mut self, path: impl Into<String>) {
        self.path = Some(path.into());
    }
}

#[derive(Debug)]
struct SourceParts<'a> {
    yaml: &'a str,
    body: &'a str,
    body_line: usize,
}

#[derive(Debug, Clone)]
struct YamlLocation {
    path: String,
    line: usize,
    column: usize,
    value_column: usize,
    value: String,
}

#[derive(Debug, Clone, Copy)]
struct BodyLocation {
    line: usize,
    column: usize,
}

/// Maximum source size accepted by adapters.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// The authoritative invoice schema.
pub const SCHEMA_JSON: &str = include_str!("../../../schema/ttyinv-v1.schema.json");

/// Return the authoritative invoice schema.
pub fn schema_json() -> &'static str {
    SCHEMA_JSON
}

#[derive(Debug)]
pub struct ValidationReport {
    diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error)
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Attach one adapter-owned path to every diagnostic.
    pub fn set_path(&mut self, path: impl Into<String>) {
        let path = path.into();
        for diagnostic in &mut self.diagnostics {
            diagnostic.set_path(path.clone());
        }
    }
}

/// One parsed Markdown section without rendered geometry.
#[derive(Debug, Clone)]
pub struct DocumentSection {
    pub title: String,
    pub body: DocumentSectionBody,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum DocumentSectionBody {
    Prose { text: String, span: SourceSpan },
    Table(DocumentTable),
}

#[derive(Debug, Clone)]
pub struct DocumentTable {
    pub headings: Vec<DocumentCell>,
    pub rows: Vec<DocumentRow>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct DocumentRow {
    pub cells: Vec<DocumentCell>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct DocumentCell {
    pub text: String,
    pub path: String,
    pub span: SourceSpan,
}

/// The ordered, typed document consumed by adapters and renderers.
#[derive(Debug)]
pub struct Document {
    pub frontmatter: Frontmatter,
    pub field_order: Vec<String>,
    pub sections: Vec<DocumentSection>,
    /// Source spans indexed by canonical field path.
    pub spans: BTreeMap<String, SourceSpan>,
    pub span: SourceSpan,
}

/// Parse one complete invoice into the shared document model.
pub fn document(source: &str) -> Result<Document, ValidationReport> {
    let parts = split_frontmatter(source).map_err(|message| ValidationReport {
        diagnostics: vec![diagnostic(codes::FRONTMATTER001, message)],
    })?;
    let locations = yaml_locations(parts.yaml, 2);
    let value =
        serde_yaml::from_str::<serde_yaml::Value>(parts.yaml).map_err(|_| ValidationReport {
            diagnostics: vec![diagnostic(codes::YAML001, Code::Yaml001.default_message())],
        })?;
    if let Some(field) = missing_required_fields(&value).first() {
        return Err(ValidationReport {
            diagnostics: vec![with_yaml_location(
                diagnostic(codes::SCHEMA003, format!("{field} is required")),
                locations.iter().find(|item| item.path == *field),
                *field,
            )],
        });
    }
    if contains_null(&value) {
        return Err(ValidationReport {
            diagnostics: vec![with_yaml_location(
                diagnostic(codes::SCHEMA001, Code::Schema001.default_message()),
                locations.first(),
                "frontmatter",
            )],
        });
    }
    let field_order = value
        .as_mapping()
        .into_iter()
        .flat_map(|mapping| mapping.keys())
        .filter_map(serde_yaml::Value::as_str)
        .map(str::to_owned)
        .collect();
    let mut frontmatter =
        serde_yaml::from_value::<Frontmatter>(value).map_err(|_| ValidationReport {
            diagnostics: vec![diagnostic(
                codes::SCHEMA001,
                Code::Schema001.default_message(),
            )],
        })?;
    let sections = document_sections(parts.body, parts.body_line);
    let spans = node_spans(&locations, &sections);
    let span = source_span(&locations);
    set_frontmatter_spans(&mut frontmatter, span);
    Ok(Document {
        frontmatter,
        field_order,
        sections,
        spans,
        span,
    })
}

fn node_spans(
    locations: &[YamlLocation],
    sections: &[DocumentSection],
) -> BTreeMap<String, SourceSpan> {
    let mut spans = BTreeMap::new();
    for location in locations {
        let position = SourcePosition {
            line: location.line,
            column: location.value_column.max(location.column),
        };
        spans.insert(
            location.path.clone(),
            SourceSpan {
                start: position,
                end: position,
            },
        );
    }
    for (index, section) in sections.iter().enumerate() {
        spans.insert(format!("sections[{index}].title"), section.span);
        if let DocumentSectionBody::Table(table) = &section.body {
            spans.insert(format!("sections[{index}].table"), table.span);
            for (row_index, row) in table.rows.iter().enumerate() {
                spans.insert(
                    format!("sections[{index}].table.rows[{row_index}]"),
                    row.span,
                );
                for cell in &row.cells {
                    spans.insert(cell.path.clone(), cell.span);
                }
            }
            for (column_index, cell) in table.headings.iter().enumerate() {
                spans.insert(
                    format!("sections[{index}].table.columns[{column_index}].name"),
                    cell.span,
                );
            }
        }
    }
    spans
}

fn source_span(locations: &[YamlLocation]) -> SourceSpan {
    let first = locations
        .first()
        .map_or(SourcePosition { line: 1, column: 1 }, |item| {
            SourcePosition {
                line: item.line,
                column: item.column,
            }
        });
    let last = locations.last().map_or(first, |item| SourcePosition {
        line: item.line,
        column: item.value_column.max(item.column),
    });
    SourceSpan {
        start: first,
        end: last,
    }
}

fn set_frontmatter_spans(frontmatter: &mut Frontmatter, span: SourceSpan) {
    frontmatter.span = span;
    frontmatter.invoice.span = span;
    frontmatter.from.span = span;
    frontmatter.to.span = span;
    if let Some(payment) = frontmatter.payment.as_mut() {
        payment.span = span;
        if let Some(methods) = payment.methods.as_mut() {
            for method in methods {
                method.span = span;
            }
        }
    }
    if let Some(settlements) = frontmatter.settlements.as_mut() {
        for settlement in settlements {
            settlement.span = span;
            settlement.paid.span = span;
            if let Some(received) = settlement.received.as_mut() {
                received.span = span;
            }
        }
    }
    if let Some(signature) = frontmatter.signature.as_mut() {
        signature.span = span;
    }
    if let Some(appearance) = frontmatter.appearance.as_mut() {
        appearance.span = span;
        if let Some(font) = appearance.font.as_mut() {
            font.span = span;
        }
    }
}

// This walk builds the renderer-facing table tree. It must agree with validate_markdown.
// Unify both walks before adding rendering rules.
fn document_sections(body: &str, first_line: usize) -> Vec<DocumentSection> {
    let parser = Parser::new_ext(body, Options::ENABLE_TABLES);
    let mut sections = Vec::new();
    let mut heading: Option<(String, usize)> = None;
    let mut active_table: Option<(usize, DocumentTable)> = None;
    let mut active_row: Option<(usize, Vec<DocumentCell>, usize)> = None;
    let mut current_cell: Option<(String, usize)> = None;
    let mut in_head = false;
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H2,
                ..
            }) => heading = Some((String::new(), range.start)),
            Event::Text(value) | Event::Code(value) => {
                if let Some((title, _)) = heading.as_mut() {
                    title.push_str(&value);
                }
                if let Some((cell_text, _)) = current_cell.as_mut() {
                    cell_text.push_str(&value);
                }
            }
            Event::End(TagEnd::Heading(HeadingLevel::H2)) => {
                if let Some((title, start)) = heading.take() {
                    let span = span_from_offsets(body, first_line, start, range.end);
                    sections.push(DocumentSection {
                        title: title.trim().to_owned(),
                        body: DocumentSectionBody::Prose {
                            text: String::new(),
                            span,
                        },
                        span,
                    });
                }
            }
            Event::Start(Tag::Table(_)) => {
                if let Some(index) = sections.len().checked_sub(1) {
                    active_table = Some((
                        index,
                        DocumentTable {
                            headings: Vec::new(),
                            rows: Vec::new(),
                            span: span_from_offsets(body, first_line, range.start, range.end),
                        },
                    ));
                }
            }
            Event::Start(Tag::TableHead) => in_head = true,
            Event::End(TagEnd::TableHead) => in_head = false,
            Event::Start(Tag::TableRow) => {
                active_row = Some((0, Vec::new(), range.start));
            }
            Event::Start(Tag::TableCell) => {
                current_cell = Some((String::new(), range.start));
            }
            Event::End(TagEnd::TableCell) => {
                if let Some((text, start)) = current_cell.take() {
                    let row_index = active_table
                        .as_ref()
                        .map_or(0, |(_, table)| table.rows.len());
                    let cell_index = active_row.as_ref().map_or(0, |(_, cells, _)| cells.len());
                    let path = if in_head {
                        format!(
                            "sections[{}].table.columns[{cell_index}].name",
                            active_table.as_ref().unwrap().0
                        )
                    } else {
                        format!(
                            "sections[{}].table.rows[{row_index}].cells[{cell_index}]",
                            active_table.as_ref().unwrap().0
                        )
                    };
                    let cell = DocumentCell {
                        text,
                        path,
                        span: span_from_offsets(body, first_line, start, range.end),
                    };
                    if in_head {
                        if let Some((_, table)) = active_table.as_mut() {
                            table.headings.push(cell);
                        }
                    } else if let Some((_, cells, _)) = active_row.as_mut() {
                        cells.push(cell);
                    }
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some((_, cells, start)) = active_row.take() {
                    if !in_head {
                        let row_index = active_table
                            .as_ref()
                            .map_or(0, |(_, table)| table.rows.len());
                        if let Some((_, table)) = active_table.as_mut() {
                            table.rows.push(DocumentRow {
                                cells,
                                span: span_from_offsets(body, first_line, start, range.end),
                            });
                            let _ = row_index;
                        }
                    }
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some((index, table)) = active_table.take() {
                    let table_span = span_from_offsets(body, first_line, range.start, range.end);
                    sections[index].body = DocumentSectionBody::Table(DocumentTable {
                        span: table_span,
                        ..table
                    });
                }
            }
            _ => {}
        }
    }
    sections
}
fn span_from_offsets(body: &str, first_line: usize, start: usize, end: usize) -> SourceSpan {
    let start = body_location(body, start, first_line);
    let end = body_location(body, end, first_line);
    SourceSpan {
        start: SourcePosition {
            line: start.line,
            column: start.column,
        },
        end: SourcePosition {
            line: end.line,
            column: end.column,
        },
    }
}
fn code_for(value: &str) -> Option<Code> {
    Some(match value {
        codes::FRONTMATTER001 => Code::Frontmatter001,
        codes::YAML001 => Code::Yaml001,
        codes::SCHEMA001 => Code::Schema001,
        codes::SCHEMA002 => Code::Schema002,
        codes::SCHEMA003 => Code::Schema003,
        codes::CURRENCY001 => Code::Currency001,
        codes::DATE001 => Code::Date001,
        codes::DATE002 => Code::Date002,
        codes::MARKDOWN001 => Code::Markdown001,
        codes::MARKDOWN002 => Code::Markdown002,
        codes::MARKDOWN003 => Code::Markdown003,
        codes::TABLE001 => Code::Table001,
        codes::TABLE002 => Code::Table002,
        codes::TABLE003 => Code::Table003,
        codes::TABLE004 => Code::Table004,
        codes::HTML001 => Code::Html001,
        codes::LIMIT001 => Code::Limit001,
        _ => return None,
    })
}

fn diagnostic(code: &str, message: impl Into<String>) -> Diagnostic {
    let metadata = code_for(code);
    Diagnostic {
        severity: metadata.map_or(Severity::Error, Code::severity),
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

fn warning(code: &str, message: impl Into<String>) -> Diagnostic {
    diagnostic(code, message)
}

fn with_yaml_location(
    mut diagnostic: Diagnostic,
    location: Option<&YamlLocation>,
    field_path: impl Into<String>,
) -> Diagnostic {
    diagnostic.field_path = Some(field_path.into());
    if let Some(location) = location {
        diagnostic.line = Some(location.line);
        diagnostic.column = Some(location.value_column.max(location.column));
    }
    diagnostic
}

fn with_body_location(mut diagnostic: Diagnostic, location: BodyLocation) -> Diagnostic {
    diagnostic.line = Some(location.line);
    diagnostic.column = Some(location.column);
    diagnostic
}
/// Validate a complete invoice source without performing file IO.
pub fn validate(source: &str) -> ValidationReport {
    let parts = match split_frontmatter(source) {
        Ok(parts) => parts,
        Err(message) => {
            let mut item = diagnostic(codes::FRONTMATTER001, message);
            item.line = Some(1);
            item.column = Some(1);
            return ValidationReport {
                diagnostics: vec![item],
            };
        }
    };
    let locations = yaml_locations(parts.yaml, 2);
    let yaml_value = match serde_yaml::from_str::<serde_yaml::Value>(parts.yaml) {
        Ok(value) => value,
        Err(error) => {
            let mut item = diagnostic(codes::YAML001, "malformed YAML");
            if let Some(location) = error.location() {
                item.line = Some(location.line() + 1);
                item.column = Some(location.column());
                item.field_path = locations
                    .iter()
                    .min_by_key(|candidate| candidate.line.abs_diff(item.line.unwrap()))
                    .map(|candidate| candidate.path.clone());
            }
            return ValidationReport {
                diagnostics: vec![item],
            };
        }
    };
    let missing = missing_required_fields(&yaml_value);
    if let Some(field) = missing.first() {
        return ValidationReport {
            diagnostics: vec![with_yaml_location(
                diagnostic(codes::SCHEMA003, format!("{field} is required")),
                locations.iter().find(|location| location.path == *field),
                *field,
            )],
        };
    }
    if contains_null(&yaml_value) {
        let location = locations
            .iter()
            .find(|location| location.value.trim_start().starts_with("null"));
        let field_path = location
            .map(|location| location.path.clone())
            .unwrap_or_else(|| "frontmatter".to_owned());
        return ValidationReport {
            diagnostics: vec![with_yaml_location(
                diagnostic(
                    codes::SCHEMA001,
                    "invalid frontmatter: explicit null is not allowed",
                ),
                location,
                field_path,
            )],
        };
    }
    let frontmatter = match serde_yaml::from_value::<Frontmatter>(yaml_value) {
        Ok(value) => value,
        Err(error) => {
            let error_text = error.to_string();
            let unknown = error_text.split('`').nth(1).unwrap_or("frontmatter");
            let location = locations
                .iter()
                .find(|location| location.path.ends_with(unknown));
            let field_path = location
                .map(|location| location.path.clone())
                .unwrap_or_else(|| unknown.to_owned());
            return ValidationReport {
                diagnostics: vec![with_yaml_location(
                    diagnostic(codes::SCHEMA001, "invalid frontmatter"),
                    location,
                    field_path,
                )],
            };
        }
    };

    let mut diagnostics = validate_frontmatter(&frontmatter, &locations);
    diagnostics.extend(validate_markdown(parts.body, parts.body_line));
    finish_diagnostics(diagnostics)
}

fn yaml_locations(yaml: &str, first_line: usize) -> Vec<YamlLocation> {
    let mut locations: Vec<YamlLocation> = Vec::new();
    let mut parents: Vec<(usize, String)> = Vec::new();
    for (index, raw_line) in yaml.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let list_item = trimmed.starts_with("- ");
        let content = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        let Some(colon) = content.find(':') else {
            continue;
        };
        while parents
            .last()
            .is_some_and(|(parent_indent, _)| *parent_indent >= indent)
        {
            parents.pop();
        }
        let key = content[..colon].trim().trim_matches(['\'', '"']);
        let prefix = parents.last().map(|(_, path)| path.as_str());
        let path = if list_item {
            let parent = prefix.unwrap_or("");
            let index = locations
                .iter()
                .filter_map(|item: &YamlLocation| {
                    item.path
                        .strip_prefix(parent)
                        .and_then(|rest| rest.strip_prefix('['))
                        .and_then(|rest| rest.split(']').next())
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .max()
                .map_or(0, |value| value + 1);
            if parent.is_empty() {
                format!("[{index}].{key}")
            } else {
                format!("{parent}[{index}].{key}")
            }
        } else if let Some(parent) = prefix {
            format!("{parent}.{key}")
        } else {
            key.to_owned()
        };
        let key_column = indent + 1;
        let value_text = content[colon + 1..].trim_start();
        let value_column = if value_text.is_empty() {
            key_column
        } else {
            indent + colon + 2 + (content[colon + 1..].len() - value_text.len())
        };
        locations.push(YamlLocation {
            path: path.clone(),
            line: first_line + index,
            column: key_column,
            value_column,
            value: value_text.to_owned(),
        });
        let parent_path = if list_item {
            path.rsplit_once('.')
                .map_or(path.as_str(), |(prefix, _)| prefix)
        } else {
            path.as_str()
        };
        if value_text.is_empty() || list_item {
            parents.push((
                if list_item {
                    indent.saturating_sub(1)
                } else {
                    indent
                },
                parent_path.to_owned(),
            ));
        }
    }
    locations
}

fn split_frontmatter(source: &str) -> Result<SourceParts<'_>, String> {
    let mut lines = source.split_inclusive('\n');
    let first = lines
        .next()
        .ok_or_else(|| "invoice must begin with YAML frontmatter delimited by ---".to_owned())?;
    let first_trimmed = first.trim_end_matches(['\r', '\n']);
    if first_trimmed.trim_end() != "---" {
        return Err("invoice must begin with YAML frontmatter delimited by ---".to_owned());
    }
    let yaml_start = first.len();
    let mut offset = yaml_start;
    for (line_index, line) in lines.enumerate() {
        let line_number = line_index + 2;
        let raw = line.trim_end_matches(['\r', '\n']);
        if raw.trim_end() == "---" {
            let close_end = offset + line.len();
            return Ok(SourceParts {
                yaml: &source[yaml_start..offset],
                body: &source[close_end..],
                body_line: line_number + 1,
            });
        }
        offset += line.len();
    }
    Err("invoice frontmatter is missing its closing --- delimiter".to_owned())
}

fn contains_null(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Null => true,
        serde_yaml::Value::Sequence(values) => values.iter().any(contains_null),
        serde_yaml::Value::Mapping(values) => values
            .iter()
            .any(|(key, value)| contains_null(key) || contains_null(value)),
        _ => false,
    }
}

fn missing_required_fields(value: &serde_yaml::Value) -> Vec<&'static str> {
    let Some(root) = value.as_mapping() else {
        return vec!["schema"];
    };
    let required_root = ["schema", "invoice", "from", "to"];
    for key in required_root {
        let Some(item) = root.get(serde_yaml::Value::String(key.to_owned())) else {
            return vec![key];
        };
        if key == "schema"
            && (!item.is_string() || item.as_str().is_none_or(|text| text.trim().is_empty()))
        {
            return vec!["schema"];
        }
        if key != "schema" && item.is_null() {
            return vec![key];
        }
    }
    let Some(invoice) = root
        .get(serde_yaml::Value::String("invoice".to_owned()))
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return vec!["invoice"];
    };
    for key in ["number", "issued", "currency"] {
        let path = match key {
            "number" => "invoice.number",
            "issued" => "invoice.issued",
            "currency" => "invoice.currency",
            _ => unreachable!(),
        };
        let Some(item) = invoice.get(serde_yaml::Value::String(key.to_owned())) else {
            return vec![path];
        };
        if item.as_str().is_none_or(|text| text.trim().is_empty()) {
            return vec![path];
        }
    }
    for key in ["from", "to"] {
        let path = if key == "from" {
            "from.name"
        } else {
            "to.name"
        };
        let Some(party) = root
            .get(serde_yaml::Value::String(key.to_owned()))
            .and_then(serde_yaml::Value::as_mapping)
        else {
            return vec![path];
        };
        let Some(name) = party.get(serde_yaml::Value::String("name".to_owned())) else {
            return vec![path];
        };
        if name.as_str().is_none_or(|text| text.trim().is_empty()) {
            return vec![path];
        }
    }
    Vec::new()
}

fn is_valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let year = value[..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..].parse::<u32>().unwrap_or(0);
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=days).contains(&day)
}

fn finish_diagnostics(mut diagnostics: Vec<Diagnostic>) -> ValidationReport {
    if diagnostics.len() > 200 {
        diagnostics.truncate(199);
        diagnostics.push(diagnostic(codes::LIMIT001, "diagnostic limit reached"));
    }
    ValidationReport { diagnostics }
}

fn validate_frontmatter(frontmatter: &Frontmatter, locations: &[YamlLocation]) -> Vec<Diagnostic> {
    let location = |path: &str| locations.iter().find(|item| item.path == path);
    let mut diagnostics = Vec::new();
    if frontmatter.schema != "ttyinv/v1" {
        diagnostics.push(with_yaml_location(
            diagnostic(codes::SCHEMA002, "unsupported schema; expected ttyinv/v1"),
            location("schema"),
            "schema",
        ));
    }
    for (path, value) in [
        ("invoice.number", frontmatter.invoice.number.as_str()),
        ("from.name", frontmatter.from.name.as_str()),
        ("to.name", frontmatter.to.name.as_str()),
    ] {
        if value.trim().is_empty() {
            diagnostics.push(with_yaml_location(
                diagnostic(codes::SCHEMA003, format!("{path} is required")),
                location(path),
                path,
            ));
        }
    }
    validate_currency(
        &frontmatter.invoice.currency,
        "invoice.currency",
        location("invoice.currency"),
        &mut diagnostics,
    );
    validate_date(
        &frontmatter.invoice.issued,
        "invoice.issued",
        location("invoice.issued"),
        &mut diagnostics,
    );
    if let Some(due) = &frontmatter.invoice.due {
        let issue_valid = is_valid_date(&frontmatter.invoice.issued);
        let due_valid = is_valid_date(due);
        validate_date(
            due,
            "invoice.due",
            location("invoice.due"),
            &mut diagnostics,
        );
        if issue_valid && due_valid && due < &frontmatter.invoice.issued {
            diagnostics.push(with_yaml_location(
                diagnostic(
                    codes::DATE002,
                    "invoice.due cannot be before invoice.issued",
                ),
                location("invoice.due"),
                "invoice.due",
            ));
        }
    }
    if let Some(settlements) = &frontmatter.settlements {
        for (index, settlement) in settlements.iter().enumerate() {
            let date_path = format!("settlements[{index}].date");
            let paid_path = format!("settlements[{index}].paid.currency");
            validate_date(
                &settlement.date,
                &date_path,
                location(&date_path),
                &mut diagnostics,
            );
            validate_currency(
                &settlement.paid.currency,
                &paid_path,
                location(&paid_path),
                &mut diagnostics,
            );
            if let Some(received) = &settlement.received {
                let received_path = format!("settlements[{index}].received.currency");
                validate_currency(
                    &received.currency,
                    &received_path,
                    location(&received_path),
                    &mut diagnostics,
                );
            }
        }
    }
    diagnostics
}

fn validate_currency(
    value: &str,
    path: &str,
    location: Option<&YamlLocation>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
        diagnostics.push(with_yaml_location(
            diagnostic(
                codes::CURRENCY001,
                format!("{path} must be a three-letter ASCII currency code"),
            ),
            location,
            path,
        ));
    }
}

fn validate_date(
    value: &str,
    path: &str,
    location: Option<&YamlLocation>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !is_valid_date(value) {
        diagnostics.push(with_yaml_location(
            diagnostic(
                codes::DATE001,
                format!("{path} must be a real YYYY-MM-DD date"),
            ),
            location,
            path,
        ));
    }
}

#[derive(Default)]
struct Section {
    title: String,
    tables: usize,
    financial_tables: usize,
    second_table_start: Option<BodyLocation>,
}

struct TableState {
    heading_cells: usize,
    body_rows: usize,
    row_cells: usize,
    in_head: bool,
    headings: Vec<String>,
    current_cell: String,
    row_widths: Vec<usize>,
    section_index: usize,
    start: BodyLocation,
}

// This walk validates tables and must agree with document_sections.
// Keep document_sections as the future shared table parser.
fn validate_markdown(body: &str, first_line: usize) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut current_section: Option<usize> = None;
    let mut heading_level: Option<HeadingLevel> = None;
    let mut heading_text = String::new();
    let mut table: Option<TableState> = None;
    let mut in_table_cell = false;
    let mut directive_pending = 0usize;
    let mut last_event_location = BodyLocation {
        line: first_line,
        column: 1,
    };
    let parser = Parser::new_ext(body, Options::ENABLE_TABLES);

    for (event, range) in parser.into_offset_iter() {
        let event_location = body_location(body, range.start, first_line);
        last_event_location = event_location;
        if directive_pending > 0
            && !matches!(
                event,
                Event::Start(Tag::Heading {
                    level: HeadingLevel::H2,
                    ..
                }) | Event::Html(_)
                    | Event::InlineHtml(_)
                    | Event::End(TagEnd::HtmlBlock)
                    | Event::Start(Tag::HtmlBlock)
                    | Event::SoftBreak
                    | Event::HardBreak
            )
        {
            diagnostics.push(with_body_location(
                diagnostic(
                    codes::HTML001,
                    "unsupported raw HTML; only literal <br> in table cells and exact ttyinv directives are allowed",
                ),
                event_location,
            ));
            directive_pending = 0;
        }
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                if level == HeadingLevel::H2 && directive_pending > 0 {
                    directive_pending = 0;
                }
                heading_level = Some(level);
                heading_text.clear();
            }
            Event::End(TagEnd::Heading(level)) => {
                if level == HeadingLevel::H1 {
                    diagnostics.push(with_body_location(
                        diagnostic(codes::MARKDOWN001, "H1 headings are not allowed"),
                        event_location,
                    ));
                } else if level == HeadingLevel::H2 {
                    if heading_text.trim().is_empty() {
                        diagnostics.push(with_body_location(
                            diagnostic(codes::MARKDOWN001, "H2 heading cannot be empty"),
                            event_location,
                        ));
                    } else {
                        sections.push(Section {
                            title: heading_text.trim().to_owned(),
                            tables: 0,
                            financial_tables: 0,
                            second_table_start: None,
                        });
                        current_section = Some(sections.len() - 1);
                    }
                }
                heading_level = None;
            }
            Event::Text(text) | Event::Code(text) => {
                if heading_level.is_some() {
                    heading_text.push_str(&text);
                }
                if let Some(active) = table.as_mut() {
                    if in_table_cell {
                        active.current_cell.push_str(&text);
                    }
                }
            }
            Event::Start(Tag::Table(_)) => {
                if let Some(section_index) = current_section {
                    sections[section_index].tables += 1;
                    if sections[section_index].tables == 2 {
                        sections[section_index].second_table_start = Some(event_location);
                    }
                    table = Some(TableState {
                        heading_cells: 0,
                        body_rows: 0,
                        row_cells: 0,
                        in_head: false,
                        headings: Vec::new(),
                        current_cell: String::new(),
                        row_widths: Vec::new(),
                        section_index,
                        start: event_location,
                    });
                }
            }
            Event::Start(Tag::TableHead) => {
                if let Some(active) = table.as_mut() {
                    active.in_head = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(active) = table.as_mut() {
                    active.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(active) = table.as_mut() {
                    active.row_cells = 0;
                    active.current_cell.clear();
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(active) = table.as_mut() {
                    active.row_widths.push(active.row_cells);
                    if active.in_head {
                        active.headings.push(active.current_cell.clone());
                    } else {
                        active.body_rows += 1;
                    }
                    active.current_cell.clear();
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(active) = table.as_mut() {
                    if active.in_head {
                        active.heading_cells += 1;
                    } else {
                        active.row_cells += 1;
                    }
                    active.current_cell.clear();
                }
                in_table_cell = true;
            }
            Event::End(TagEnd::TableCell) => {
                in_table_cell = false;
                if let Some(active) = table.as_mut() {
                    if active.in_head {
                        active.headings.push(active.current_cell.clone());
                    }
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(active) = table.take() {
                    if active.heading_cells < 2 {
                        let mut item = with_body_location(
                            diagnostic(codes::TABLE001, Code::Table001.default_message()),
                            active.start,
                        );
                        item.section = Some(sections[active.section_index].title.clone());
                        item.section_index = Some((active.section_index + 1) as u32);
                        diagnostics.push(item);
                    }
                    if active.body_rows == 0 {
                        let mut item = with_body_location(
                            diagnostic(codes::TABLE002, Code::Table002.default_message()),
                            active.start,
                        );
                        item.section = Some(sections[active.section_index].title.clone());
                        item.section_index = Some((active.section_index + 1) as u32);
                        diagnostics.push(item);
                    }
                    let financial = active
                        .headings
                        .iter()
                        .any(|heading| is_amount_heading(heading));
                    if financial {
                        sections[active.section_index].financial_tables += 1;
                    }
                }
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                let literal = html.as_ref();
                let directive = literal.trim_end_matches(['\r', '\n'])
                    == "<!-- ttyinv:page-break-before -->"
                    || literal.trim_end_matches(['\r', '\n']) == "<!-- ttyinv:summary-only -->";
                if directive {
                    directive_pending += 1;
                } else if !(in_table_cell && literal == "<br>") {
                    diagnostics.push(with_body_location(
                        diagnostic(
                            codes::HTML001,
                            "unsupported raw HTML; only literal <br> in table cells and exact ttyinv directives are allowed",
                        ),
                        event_location,
                    ));
                    directive_pending = 0;
                }
            }
            _ => {}
        }
    }
    if directive_pending > 0 {
        diagnostics.push(with_body_location(
            diagnostic(
                codes::HTML001,
                "unsupported raw HTML; only literal <br> in table cells and exact ttyinv directives are allowed",
            ),
            last_event_location,
        ));
    }
    validate_raw_table_widths(body, first_line, &mut diagnostics);
    let financial_table_count: usize = sections
        .iter()
        .map(|section| section.financial_tables)
        .sum();
    if financial_table_count == 0 {
        diagnostics.push(with_body_location(
            warning(
                codes::MARKDOWN002,
                "invoice must contain at least one financial table",
            ),
            first_body_location(body, first_line),
        ));
    }
    if sections.is_empty() {
        diagnostics.push(with_body_location(
            warning(
                codes::MARKDOWN003,
                "invoice must contain at least one H2 section",
            ),
            first_body_location(body, first_line),
        ));
    }
    for (section_index, section) in sections.iter().enumerate() {
        if section.financial_tables > 0 && section.tables > 1 {
            let mut item = with_body_location(
                diagnostic(codes::TABLE004, Code::Table004.default_message()),
                section
                    .second_table_start
                    .unwrap_or_else(|| first_body_location(body, first_line)),
            );
            item.section = Some(section.title.clone());
            item.section_index = Some((section_index + 1) as u32);
            diagnostics.push(item);
        }
    }
    diagnostics
}

fn body_location(body: &str, offset: usize, first_line: usize) -> BodyLocation {
    let prefix = &body[..offset.min(body.len())];
    let line = first_line + prefix.bytes().filter(|byte| *byte == b'\n').count();
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len() + 1, |(_, current)| current.len() + 1);
    BodyLocation { line, column }
}

fn first_body_location(body: &str, first_line: usize) -> BodyLocation {
    body_location(
        body,
        body.find(|character: char| !character.is_whitespace())
            .unwrap_or(0),
        first_line,
    )
}

fn is_amount_heading(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized == "amount" {
        return true;
    }
    let Some(currency) = normalized
        .strip_prefix("amount (")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return false;
    };
    currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn validate_raw_table_widths(body: &str, first_line: usize, diagnostics: &mut Vec<Diagnostic>) {
    let lines: Vec<&str> = body.lines().collect();
    let mut section = 0;
    let mut section_title = String::new();
    let mut seen_h2 = false;
    let mut fence: Option<&str> = None;
    let mut index = 0;
    while index + 1 < lines.len() {
        let raw_line = lines[index];
        let line = raw_line.trim();
        if let Some(marker) = fence {
            if line.starts_with(marker) {
                fence = None;
            }
            index += 1;
            continue;
        }
        if line.starts_with("```") {
            fence = Some("```");
            index += 1;
            continue;
        }
        if line.starts_with("~~~") {
            fence = Some("~~~");
            index += 1;
            continue;
        }
        if line.starts_with("## ") && !line.starts_with("### ") {
            seen_h2 = true;
            section += 1;
            section_title = line[3..].trim().to_owned();
            index += 1;
            continue;
        }
        if seen_h2 && line.contains('|') && is_separator_row(lines[index + 1]) {
            let headers = split_table_cells(line);
            let expected = headers.len();
            let mut row_index = index + 2;
            let mut row = 1;
            while row_index < lines.len() {
                let raw_row = lines[row_index];
                let row_text = raw_row.trim();
                if row_text.is_empty() || row_text.starts_with("## ") || !row_text.contains('|') {
                    break;
                }
                let actual = split_table_cells(row_text).len();
                if actual != expected {
                    let column_index = actual.min(expected);
                    let column_name = headers
                        .get(column_index)
                        .filter(|value| !value.is_empty())
                        .cloned();
                    let mut item = with_body_location(
                        diagnostic(
                            codes::TABLE003,
                            format!("table row has {actual} cells; expected {expected}"),
                        ),
                        BodyLocation {
                            line: first_line + row_index,
                            column: 1,
                        },
                    );
                    item.section = Some(section_title.clone());
                    item.section_index = Some(section as u32);
                    item.row = Some(row);
                    item.column_name = column_name;
                    diagnostics.push(item);
                }
                row += 1;
                row_index += 1;
            }
            index = row_index;
            continue;
        }
        index += 1;
    }
}

fn is_separator_row(line: &str) -> bool {
    let cells = split_table_cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let value = cell.trim().trim_matches(':').trim();
            value.len() >= 3 && value.bytes().all(|byte| byte == b'-')
        })
}

fn split_table_cells(line: &str) -> Vec<String> {
    let mut value = line.trim();
    if let Some(stripped) = value.strip_prefix('|') {
        value = stripped;
    }
    if value.ends_with('|') && !value.ends_with(r"\|") {
        value = &value[..value.len() - 1];
    }
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' {
            current.push(character);
            escaped = true;
        } else if character == '|' {
            cells.push(current.trim().to_owned());
            current.clear();
        } else {
            current.push(character);
        }
    }
    cells.push(current.trim().to_owned());
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(frontmatter: &str, body: &str) -> String {
        format!("---\n{frontmatter}\n---\n{body}")
    }

    fn valid_frontmatter() -> &'static str {
        "schema: ttyinv/v1\ninvoice:\n  number: INV-1\n  issued: 2026-01-01\n  due: 2026-01-02\n  currency: EUR\nfrom:\n  name: Sender\nto:\n  name: Receiver"
    }

    fn valid_body() -> &'static str {
        "\n## Services\n\n| Description | Amount (EUR) |\n| --- | ---: |\n| Work | 10 |\n"
    }

    #[test]
    fn embedded_schema_is_json() {
        let value: serde_json::Value = serde_json::from_str(SCHEMA_JSON).expect("schema JSON");
        assert_eq!(
            value["$id"],
            "https://github.com/kaygdotorg/ttyinv/blob/main/schema/ttyinv-v1.schema.json"
        );
        assert_eq!(schema_json(), SCHEMA_JSON);
    }

    #[test]
    fn simple_example_is_valid() {
        let source = include_str!("../../../examples/simple.md");
        assert!(
            validate(source).is_valid(),
            "{:?}",
            validate(source).diagnostics()
        );
    }

    #[test]
    fn frontmatter_codes() {
        let report = validate("text");
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|item| item.code == "FRONTMATTER001")
        );
        let report = validate(&source("schema: ttyinv/v1\ninvoice: [", valid_body()));
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|item| item.code == "YAML001")
        );
        assert_eq!(
            report
                .diagnostics()
                .iter()
                .find(|item| item.code == "YAML001")
                .map(|item| item.message.as_str()),
            Some("malformed YAML")
        );
        let report = validate(&source(
            "schema: ttyinv/v2\ninvoice:\n  number: x\n  issued: 2026-01-01\n  currency: EUR\nfrom:\n  name: x\nto:\n  name: y",
            valid_body(),
        ));
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|item| item.code == "SCHEMA002")
        );
        let report = validate(&source(
            "schema: ttyinv/v1\ninvoice:\n  number: x\n  issued: 2026-01-01\n  currency: EUR\n  nope: x\nfrom:\n  name: x\nto:\n  name: y",
            valid_body(),
        ));
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|item| item.code == "SCHEMA001")
        );
        assert_eq!(
            report
                .diagnostics()
                .iter()
                .find(|item| item.code == "SCHEMA001")
                .map(|item| item.message.as_str()),
            Some("invalid frontmatter")
        );
        let report = validate(&source(
            "schema: ttyinv/v1\ninvoice:\n  number: ' '\n  issued: 2026-01-01\n  currency: EUR\nfrom:\n  name: x\nto:\n  name: y",
            valid_body(),
        ));
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|item| item.code == "SCHEMA003")
        );
    }

    #[test]
    fn currency_and_date_codes() {
        let report = validate(&source(
            "schema: ttyinv/v1\ninvoice:\n  number: x\n  issued: 2026-01-01\n  due: 2025-12-31\n  currency: EU\nfrom:\n  name: x\nto:\n  name: y",
            valid_body(),
        ));
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|item| item.code == "CURRENCY001")
        );
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|item| item.code == "DATE002")
        );
        let report = validate(&source(
            "schema: ttyinv/v1\ninvoice:\n  number: x\n  issued: 2026-02-30\n  currency: EUR\nfrom:\n  name: x\nto:\n  name: y",
            valid_body(),
        ));
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|item| item.code == "DATE001")
        );
    }

    #[test]
    fn markdown_codes() {
        let frontmatter = valid_frontmatter();
        for (body, code) in [
            ("\nText\n", "MARKDOWN002"),
            ("\n## Services\n\nText\n", "MARKDOWN002"),
            ("\n## Services\n\n| One |\n| --- |\n| x |\n", "TABLE001"),
            (
                "\n## Services\n\n| One | Two |\n| --- | --- |\n",
                "TABLE002",
            ),
            (
                "\n## Services\n\n| One | Two |\n| --- | --- |\n| x | y | z |\n",
                "TABLE003",
            ),
            (
                "\n## Services\n\n| One | Amount |\n| --- | --- |\n| x | 1 |\n\n| Note | Value |\n| --- | --- |\n| x | y |\n",
                "TABLE004",
            ),
            (
                "\n## Services\n\n<div>x</div>\n\n| One | Amount |\n| --- | --- |\n| x | 1 |\n",
                "HTML001",
            ),
        ] {
            let report = validate(&source(frontmatter, body));
            let item = report
                .diagnostics()
                .iter()
                .find(|item| item.code == code)
                .unwrap_or_else(|| panic!("missing {code}: {:?}", report.diagnostics()));
            assert_eq!(
                item.severity,
                if code == "MARKDOWN002" {
                    Severity::Warning
                } else {
                    Severity::Error
                }
            );
            assert!(item.line.is_some());
            assert!(item.column.is_some_and(|column| column > 0));
            if code.starts_with("TABLE") {
                assert!(item.section.is_some());
            }
            if code == "TABLE003" {
                assert!(item.row.is_some());
                assert!(item.column_name.is_none());
            }
        }
    }

    #[test]
    fn schema_and_markdown_edge_cases() {
        let null = source(
            &valid_frontmatter().replace("due: 2026-01-02", "due: null"),
            valid_body(),
        );
        assert!(
            validate(&null)
                .diagnostics()
                .iter()
                .any(|item| item.code == "SCHEMA001")
        );
        let payment_null = source(
            &format!("{}\npayment: null", valid_frontmatter()),
            valid_body(),
        );
        assert!(
            validate(&payment_null)
                .diagnostics()
                .iter()
                .any(|item| item.code == "SCHEMA001")
        );

        let missing = source(
            &valid_frontmatter().replace("  currency: EUR\n", ""),
            valid_body(),
        );
        let missing_report = validate(&missing);
        assert!(
            missing_report
                .diagnostics()
                .iter()
                .any(|item| item.code == "SCHEMA003")
        );
        assert!(
            missing_report
                .diagnostics()
                .iter()
                .all(|item| item.code != "SCHEMA001")
        );

        let misplaced = source(
            valid_frontmatter(),
            "\n<!-- ttyinv:summary-only -->\nText\n\n## Services\n\n| One | Two |\n| --- | --- |\n| x | y |\n",
        );
        assert!(
            validate(&misplaced)
                .diagnostics()
                .iter()
                .any(|item| item.code == "HTML001")
        );

        let fenced = source(
            valid_frontmatter(),
            "\n## Services\n\n| One | Two |\n| --- | --- |\n| x | y |\n\n```markdown\n| One | Two |\n| --- | --- |\n| x | y | z |\n```\n",
        );
        assert!(
            validate(&fenced).is_valid(),
            "{:?}",
            validate(&fenced).diagnostics()
        );

        let h1 = source(
            valid_frontmatter(),
            "\n# Invoice\n\n## Services\n\n| One | Two |\n| --- | --- |\n| x | y |\n",
        );
        assert!(
            validate(&h1)
                .diagnostics()
                .iter()
                .any(|item| item.code == "MARKDOWN001")
        );

        let empty_h2 = source(
            valid_frontmatter(),
            "\n##\n\n| One | Two |\n| --- | --- |\n| x | y |\n",
        );
        assert!(
            validate(&empty_h2)
                .diagnostics()
                .iter()
                .any(|item| item.code == "MARKDOWN001")
        );
        let warning_report = validate(&source(
            valid_frontmatter(),
            "\n## Notes\n\nNarrative only.\n",
        ));
        assert!(warning_report.is_valid());
        assert!(
            warning_report
                .diagnostics()
                .iter()
                .any(|item| { item.code == "MARKDOWN002" && item.severity == Severity::Warning })
        );
        let sectionless = validate(&source(valid_frontmatter(), "\nNarrative only.\n"));
        assert!(sectionless.is_valid());
        assert!(
            sectionless
                .diagnostics()
                .iter()
                .any(|item| { item.code == "MARKDOWN003" && item.severity == Severity::Warning })
        );
    }

    #[test]
    fn diagnostics_have_typed_severity_and_locations() {
        let frontmatter = "schema: ttyinv/v2\ninvoice:\n  number: x\n  issued: 2026-01-01\n  due: 2025-12-31\n  currency: EU\nfrom:\n  name: x\nto:\n  name: y";
        let report = validate(&source(frontmatter, valid_body()));
        for code in ["SCHEMA002", "CURRENCY001", "DATE002"] {
            let item = report
                .diagnostics()
                .iter()
                .find(|item| item.code == code)
                .unwrap_or_else(|| panic!("missing {code}"));
            assert_eq!(item.severity, Severity::Error);
            assert!(
                item.field_path
                    .as_deref()
                    .is_some_and(|path| !path.is_empty())
            );
            assert!(item.line.is_some());
            assert!(item.column.is_some_and(|column| column > 0));
        }
        let invalid_date = validate(&source(
            &frontmatter.replace("2026-01-01", "2026-02-30"),
            valid_body(),
        ));
        let item = invalid_date
            .diagnostics()
            .iter()
            .find(|item| item.code == "DATE001")
            .expect("missing DATE001");
        assert_eq!(item.severity, Severity::Error);
        assert_eq!(item.field_path.as_deref(), Some("invoice.issued"));
        assert!(item.line.is_some());
        assert!(item.column.is_some_and(|column| column > 0));

        let table = validate(&source(
            valid_frontmatter(),
            "\n## Services\n\n| Description | Amount (EUR) |\n| --- | --- |\n| Work | 10 | extra |\n",
        ));
        let item = table
            .diagnostics()
            .iter()
            .find(|item| item.code == "TABLE003")
            .expect("missing TABLE003");
        assert_eq!(item.severity, Severity::Error);
        assert_eq!(item.section_index, Some(1));
        assert_eq!(item.row, Some(1));
        assert!(item.column_name.is_none());
        assert!(item.line.is_some());
        assert_eq!(item.column, Some(1));

        let warnings = validate(&source(valid_frontmatter(), "\nNarrative only.\n"));
        assert!(warnings.is_valid());
        for code in ["MARKDOWN002", "MARKDOWN003"] {
            let item = warnings
                .diagnostics()
                .iter()
                .find(|item| item.code == code)
                .unwrap_or_else(|| panic!("missing {code}"));
            assert_eq!(item.severity, Severity::Warning);
            assert!(item.line.is_some());
            assert!(item.column.is_some_and(|column| column > 0));
        }
    }

    #[test]
    fn diagnostic_limit_is_bounded() {
        let mut body = String::from("\n## Services\n\n| One | Two |\n| --- | --- |\n");
        for _ in 0..205 {
            body.push_str("| x | y | z |\n");
        }
        let report = validate(&source(valid_frontmatter(), &body));
        assert_eq!(report.diagnostics().len(), 200);
        assert_eq!(
            report.diagnostics().last().map(|item| item.code.as_str()),
            Some("LIMIT001")
        );
    }

    #[test]
    fn allowed_break_html_is_valid() {
        let report = validate(&source(
            valid_frontmatter(),
            "\n<!-- ttyinv:summary-only -->\n## Services\n\n| One | Amount |\n| --- | --- |\n| a<br>b | 1 |\n",
        ));
        assert!(
            report
                .diagnostics()
                .iter()
                .all(|item| item.code != "HTML001"),
            "{:?}",
            report.diagnostics()
        );
    }
    #[test]
    fn required_values_use_schema003() {
        for frontmatter in [
            "invoice:\n  number: x\n  issued: 2026-01-01\n  currency: EUR\nfrom:\n  name: x\nto:\n  name: y",
            "schema: ttyinv/v1\ninvoice:\n  number: x\n  issued: 2026-01-01\n  currency: EUR\nfrom:\n  name: x\nto: {}",
        ] {
            let report = validate(&source(frontmatter, valid_body()));
            assert_eq!(report.diagnostics()[0].code, "SCHEMA003");
        }
    }

    #[test]
    fn lowercase_currency_is_rejected() {
        let report = validate(&source(
            &valid_frontmatter().replace("currency: EUR", "currency: eur"),
            valid_body(),
        ));
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|item| item.code == "CURRENCY001")
        );
    }

    #[test]
    fn adjacent_directives_are_allowed() {
        let report = validate(&source(
            valid_frontmatter(),
            "\n<!-- ttyinv:page-break-before -->\n<!-- ttyinv:summary-only -->\n## Services\n\n| One | Amount |\n| --- | --- |\n| x | 1 |\n",
        ));
        assert!(
            report
                .diagnostics()
                .iter()
                .all(|item| item.code != "HTML001")
        );
    }

    #[test]
    fn inline_heading_table_width_is_checked() {
        let report = validate(&source(
            valid_frontmatter(),
            "\n## **Services**\n\n| One | Amount |\n| --- | --- |\n| x | 1 | extra |\n",
        ));
        assert!(
            report
                .diagnostics()
                .iter()
                .any(|item| item.code == "TABLE003")
        );
    }

    #[test]
    fn indexed_paths_and_decimal_document_are_preserved() {
        let input = source(
            &format!(
                "{}\nsettlements:\n  - date: 2026-01-18\n    paid:\n      amount: 10.005\n      currency: EUR\n  - date: 2026-01-19\n    paid:\n      amount: 20.00\n      currency: EUR\n  - date: 2026-02-30\n    paid:\n      amount: 30.00\n      currency: EUR",
                valid_frontmatter()
            ),
            valid_body(),
        );
        let report = validate(&input);
        let date = report
            .diagnostics()
            .iter()
            .find(|item| item.code == "DATE001")
            .expect("third settlement date");
        assert_eq!(date.field_path.as_deref(), Some("settlements[2].date"));
        let valid = source(
            &format!(
                "{}\nsettlements:\n  - date: 2026-01-18\n    paid:\n      amount: 10.005\n      currency: EUR",
                valid_frontmatter()
            ),
            valid_body(),
        );
        let model = document(&valid).expect("document");
        assert_eq!(model.field_order[0], "schema");
        assert!(model.spans.contains_key("settlements[0].paid.amount"));
        assert_eq!(
            model.frontmatter.settlements.as_ref().unwrap()[0]
                .paid
                .amount
                .to_string(),
            "10.005"
        );
    }
}
