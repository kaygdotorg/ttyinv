use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

use super::{
    DocumentSectionBody, StructuralCounts, ValidationReport, codes, diagnostic, document, revision,
    split_frontmatter, structural_counts, validate, yaml_locations,
};

/// Numeric bounds for the scalar canvas edit boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScalarEditLimits {
    pub max_source_bytes: usize,
    pub max_value_bytes: usize,
    pub max_path_bytes: usize,
    pub max_segments: usize,
    pub max_key_bytes: usize,
    pub max_sections: usize,
    pub max_rows_per_table: usize,
    pub max_rows_total: usize,
    pub max_columns: usize,
    pub max_cells_total: usize,
    pub max_frontmatter_depth: usize,
}

/// The one policy used by the core and its adapters.
pub const SCALAR_EDIT_LIMITS: ScalarEditLimits = ScalarEditLimits {
    max_source_bytes: 128 * 1024,
    max_value_bytes: 128 * 1024,
    max_path_bytes: 512,
    max_segments: 16,
    max_key_bytes: 60,
    max_sections: 50,
    max_rows_per_table: 500,
    max_rows_total: 500,
    max_columns: 12,
    max_cells_total: 6_000,
    max_frontmatter_depth: 16,
};

/// A stateless scalar edit request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalarEditRequest {
    pub source: String,
    pub base_revision: String,
    pub sequence: u64,
    pub path: String,
    pub value: String,
}

/// The result of one scalar edit attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalarEditOutcome {
    Applied,
    AppliedWithErrors,
    Rejected,
    Stale,
}

/// A complete scalar edit response.
#[derive(Debug, Serialize)]
pub struct ScalarEditResponse {
    pub outcome: ScalarEditOutcome,
    pub base_revision: String,
    pub revision: String,
    pub sequence: u64,
    pub source: String,
    pub report: ValidationReport,
}

#[derive(Debug)]
enum Target {
    Frontmatter {
        path: String,
    },
    TableCell {
        section: usize,
        row: usize,
        cell: usize,
    },
}

#[derive(Debug)]
enum Segment {
    Name(String),
    Index(usize),
}

/// Apply one allowlisted scalar edit without rebuilding the source.
pub fn apply_scalar(request: ScalarEditRequest) -> ScalarEditResponse {
    let actual_revision = revision(&request.source);
    let original_source = request.source.clone();
    let base_revision = request.base_revision.clone();
    let sequence = request.sequence;

    if request.source.len() > SCALAR_EDIT_LIMITS.max_source_bytes {
        return response(
            ScalarEditOutcome::Rejected,
            base_revision,
            actual_revision,
            sequence,
            original_source,
            limit_report("source exceeds the canvas edit limit"),
        );
    }
    if request.base_revision != actual_revision {
        return response(
            ScalarEditOutcome::Stale,
            base_revision,
            actual_revision,
            sequence,
            original_source.clone(),
            validate(&original_source),
        );
    }
    if request.value.len() > SCALAR_EDIT_LIMITS.max_value_bytes {
        return response(
            ScalarEditOutcome::Rejected,
            base_revision,
            actual_revision,
            sequence,
            original_source,
            limit_report("value exceeds the canvas edit limit"),
        );
    }

    let target = match parse_target(&request.path) {
        Ok(target) => target,
        Err(message) => {
            return response(
                ScalarEditOutcome::Rejected,
                base_revision,
                actual_revision,
                sequence,
                original_source,
                edit_report(message),
            );
        }
    };
    if let Err(message) = enforce_document_limits(&request.source) {
        return response(
            ScalarEditOutcome::Rejected,
            base_revision,
            actual_revision,
            sequence,
            original_source,
            limit_report(message),
        );
    }

    let range = match target {
        Target::Frontmatter { path } => frontmatter_range(&request.source, &path),
        Target::TableCell { section, row, cell } => {
            table_cell_range(&request.source, section, row, cell)
        }
    };
    let Some((start, end, style)) = range else {
        return response(
            ScalarEditOutcome::Rejected,
            base_revision,
            actual_revision,
            sequence,
            original_source,
            edit_report("scalar edit target does not exist"),
        );
    };
    let replacement_length = match replacement_len(&request.value, &style) {
        Some(length) => length,
        None => {
            return response(
                ScalarEditOutcome::Rejected,
                base_revision,
                actual_revision,
                sequence,
                original_source,
                limit_report("patched source length cannot be computed"),
            );
        }
    };
    let replaced_length = end.checked_sub(start);
    let final_length = replaced_length
        .and_then(|length| request.source.len().checked_sub(length))
        .and_then(|length| length.checked_add(replacement_length));
    let Some(final_length) = final_length else {
        return response(
            ScalarEditOutcome::Rejected,
            base_revision,
            actual_revision,
            sequence,
            original_source,
            limit_report("patched source length exceeds the canvas edit limit"),
        );
    };
    if final_length > SCALAR_EDIT_LIMITS.max_source_bytes {
        return response(
            ScalarEditOutcome::Rejected,
            base_revision,
            actual_revision,
            sequence,
            original_source,
            limit_report("patched source exceeds the canvas edit limit"),
        );
    }
    let replacement = match style {
        ReplacementStyle::Yaml(raw) => yaml_scalar(&request.value, raw),
        ReplacementStyle::Table => markdown_cell(&request.value),
    };
    let Some(prefix) = request.source.get(..start) else {
        return response(
            ScalarEditOutcome::Rejected,
            base_revision,
            actual_revision,
            sequence,
            original_source,
            edit_report("scalar edit target has an invalid byte range"),
        );
    };
    let Some(suffix) = request.source.get(end..) else {
        return response(
            ScalarEditOutcome::Rejected,
            base_revision,
            actual_revision,
            sequence,
            original_source,
            edit_report("scalar edit target has an invalid byte range"),
        );
    };
    let mut source = String::with_capacity(final_length);
    source.push_str(prefix);
    source.push_str(&replacement);
    source.push_str(suffix);
    if source.len() != final_length {
        return response(
            ScalarEditOutcome::Rejected,
            base_revision,
            actual_revision,
            sequence,
            original_source,
            edit_report("patched source length does not match its checked length"),
        );
    }
    let report = validate(&source);
    let outcome = if report.is_valid() {
        ScalarEditOutcome::Applied
    } else {
        ScalarEditOutcome::AppliedWithErrors
    };
    let new_revision = revision(&source);
    response(
        outcome,
        base_revision,
        new_revision,
        sequence,
        source,
        report,
    )
}

fn response(
    outcome: ScalarEditOutcome,
    base_revision: String,
    revision: String,
    sequence: u64,
    source: String,
    report: ValidationReport,
) -> ScalarEditResponse {
    ScalarEditResponse {
        outcome,
        base_revision,
        revision,
        sequence,
        source,
        report,
    }
}

fn edit_report(message: &str) -> ValidationReport {
    ValidationReport {
        diagnostics: vec![diagnostic(codes::LIMIT001, message)],
    }
}

fn limit_report(message: &str) -> ValidationReport {
    edit_report(message)
}

fn parse_target(path: &str) -> Result<Target, &'static str> {
    if path.is_empty() || path.len() > SCALAR_EDIT_LIMITS.max_path_bytes {
        return Err("scalar edit path exceeds the path limit");
    }
    if !path.is_ascii() {
        return Err("scalar edit path must use ASCII syntax");
    }
    let segments = parse_segments(path)?;
    if segments.len() > SCALAR_EDIT_LIMITS.max_segments {
        return Err("scalar edit path has too many segments");
    }
    let mut names = Vec::new();
    for segment in segments {
        match segment {
            Segment::Name(name) => names.push(name),
            Segment::Index(index) => {
                if index > u32::MAX as usize {
                    return Err("scalar edit index is too large");
                }
                names.push(format!("[{index}]"));
            }
        }
    }
    match names.as_slice() {
        [root, field]
            if matches!(root.as_str(), "invoice" | "from" | "to")
                && is_scalar_field(root, field) =>
        {
            Ok(Target::Frontmatter {
                path: format!("{root}.{field}"),
            })
        }
        [root, section, table, rows, row, cells, cell]
            if root == "sections"
                && table == "table"
                && rows == "rows"
                && cells == "cells"
                && section.starts_with('[')
                && row.starts_with('[')
                && cell.starts_with('[') =>
        {
            let section = parse_index_segment(section)?;
            let row = parse_index_segment(row)?;
            let cell = parse_index_segment(cell)?;
            Ok(Target::TableCell { section, row, cell })
        }
        _ => Err("scalar edit path is not an allowed scalar target"),
    }
}

fn is_scalar_field(root: &str, field: &str) -> bool {
    match root {
        "invoice" => matches!(
            field,
            "number" | "title" | "issued" | "due" | "currency" | "locale"
        ),
        "from" | "to" => matches!(field, "name" | "email" | "website"),
        _ => false,
    }
}

fn parse_segments(path: &str) -> Result<Vec<Segment>, &'static str> {
    let bytes = path.as_bytes();
    let mut position = 0usize;
    let mut segments = Vec::new();
    let mut expect_name = true;
    while position < bytes.len() {
        if !expect_name {
            if bytes.get(position) == Some(&b'.') {
                position = position.saturating_add(1);
                expect_name = true;
                continue;
            }
            if bytes.get(position) != Some(&b'[') {
                return Err("scalar edit path has invalid separators");
            }
        }
        if bytes.get(position) == Some(&b'[') {
            let (index, next) = parse_index(bytes, position)?;
            segments.push(Segment::Index(index));
            position = next;
            expect_name = false;
            continue;
        }
        let start = position;
        while position < bytes.len()
            && bytes.get(position) != Some(&b'.')
            && bytes.get(position) != Some(&b'[')
        {
            position = position.saturating_add(1);
        }
        if start == position {
            return Err("scalar edit path has an empty name");
        }
        let name = path
            .get(start..position)
            .ok_or("scalar edit path is not UTF-8")?;
        if name.len() > SCALAR_EDIT_LIMITS.max_key_bytes || !valid_identifier(name) {
            return Err("scalar edit path has an invalid name");
        }
        segments.push(Segment::Name(name.to_owned()));
        expect_name = false;
    }
    if expect_name || segments.is_empty() {
        return Err("scalar edit path is incomplete");
    }
    Ok(segments)
}

fn valid_identifier(name: &str) -> bool {
    let mut chars = name.bytes();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && chars.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn parse_index(bytes: &[u8], start: usize) -> Result<(usize, usize), &'static str> {
    if bytes.get(start) != Some(&b'[') {
        return Err("scalar edit path has an invalid index");
    }
    let mut position = start.saturating_add(1);
    let digit_start = position;
    let mut value = 0usize;
    while let Some(byte) = bytes.get(position) {
        if *byte == b']' {
            if digit_start == position {
                return Err("scalar edit path has an empty index");
            }
            if position.saturating_sub(digit_start) > 1 && bytes.get(digit_start) == Some(&b'0') {
                return Err("scalar edit path has a leading zero");
            }
            return Ok((value, position.saturating_add(1)));
        }
        if !byte.is_ascii_digit() {
            return Err("scalar edit path has an invalid index");
        }
        value = value
            .checked_mul(10)
            .and_then(|current| current.checked_add(usize::from(*byte - b'0')))
            .ok_or("scalar edit index is too large")?;
        position = position.saturating_add(1);
    }
    Err("scalar edit path has an unterminated index")
}

fn parse_index_segment(segment: &str) -> Result<usize, &'static str> {
    let bytes = segment.as_bytes();
    let (index, next) = parse_index(bytes, 0)?;
    if next != bytes.len() {
        return Err("scalar edit path has an invalid index");
    }
    Ok(index)
}

enum ReplacementStyle<'a> {
    Yaml(&'a str),
    Table,
}
fn frontmatter_range<'a>(
    source: &'a str,
    path: &str,
) -> Option<(usize, usize, ReplacementStyle<'a>)> {
    let parts = split_frontmatter(source).ok()?;
    let locations = yaml_locations(parts.yaml, 2);
    let location = locations.iter().find(|item| item.path == path)?;
    let (line_start, _, line) = source_line(source, location.line)?;
    let colon = line.find(':')?;
    let key_indent = line.len() - line.trim_start().len();
    let value_start_rel = line.get(colon + 1..)?.len() - line.get(colon + 1..)?.trim_start().len();
    let value_start = colon.checked_add(1)?.checked_add(value_start_rel)?;
    let raw_value = line.get(value_start..)?.trim_end_matches([' ', '\t', '\r']);
    if raw_value.is_empty()
        || yaml_scalar_is_multiline(source, location.line, key_indent, raw_value)
    {
        return None;
    }
    let value_end_rel = yaml_scalar_end(raw_value);
    let value_end = value_start.checked_add(value_end_rel)?;
    let start = line_start.checked_add(value_start)?;
    let end = line_start.checked_add(value_end)?;
    Some((start, end, ReplacementStyle::Yaml(raw_value)))
}

fn yaml_scalar_end(value: &str) -> usize {
    if value.starts_with('\'') {
        let bytes = value.as_bytes();
        let mut index = 1usize;
        while index < bytes.len() {
            if bytes.get(index) == Some(&b'\'') {
                if bytes.get(index + 1) == Some(&b'\'') {
                    index = index.saturating_add(2);
                } else {
                    return index.saturating_add(1);
                }
            } else {
                index = index.saturating_add(1);
            }
        }
        return value.len();
    }
    if value.starts_with('"') {
        let bytes = value.as_bytes();
        let mut index = 1usize;
        let mut escaped = false;
        while index < bytes.len() {
            let Some(byte) = bytes.get(index).copied() else {
                break;
            };
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                return index.saturating_add(1);
            }
            index = index.saturating_add(1);
        }
    }
    let bytes = value.as_bytes();
    for index in 0..bytes.len() {
        if bytes.get(index) == Some(&b'#')
            && (index == 0 || bytes.get(index - 1).is_some_and(u8::is_ascii_whitespace))
        {
            return value
                .get(..index)
                .map_or(0, |prefix| prefix.trim_end().len());
        }
    }
    value.len()
}

fn yaml_scalar_is_multiline(
    source: &str,
    line_number: usize,
    key_indent: usize,
    value: &str,
) -> bool {
    let trimmed = value.trim_start();
    if trimmed.starts_with('\'')
        && yaml_scalar_end(trimmed) == trimmed.len()
        && !trimmed.ends_with('\'')
    {
        return true;
    }
    if trimmed.starts_with('"')
        && yaml_scalar_end(trimmed) == trimmed.len()
        && !trimmed.ends_with('"')
    {
        return true;
    }
    for (index, raw_line) in source.split_inclusive('\n').enumerate() {
        let current = index + 1;
        if current <= line_number {
            continue;
        }
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if line.trim_end() == "---" {
            break;
        }
        let content = line.trim();
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent <= key_indent {
            break;
        }
        return true;
    }
    false
}

fn table_cell_range(
    source: &str,
    section: usize,
    row: usize,
    cell: usize,
) -> Option<(usize, usize, ReplacementStyle<'static>)> {
    let parsed = document(source).ok()?;
    let section_value = parsed.sections.get(section)?;
    let table = match &section_value.body {
        DocumentSectionBody::Table(table) => table,
        DocumentSectionBody::Prose { .. } => return None,
    };
    let row_value = table.rows.get(row)?;
    let cell_value = row_value.cells.get(cell)?;
    let (line_start, _, line) = source_line(source, cell_value.span.start.line)?;
    let (start, end) = markdown_cell_offsets(line, cell)?;
    Some((
        line_start.checked_add(start)?,
        line_start.checked_add(end)?,
        ReplacementStyle::Table,
    ))
}

fn source_line(source: &str, line_number: usize) -> Option<(usize, usize, &str)> {
    if line_number == 0 {
        return None;
    }
    let mut offset = 0usize;
    for (index, line_with_end) in source.split_inclusive('\n').enumerate() {
        let number = index.checked_add(1)?;
        let line_end = offset.checked_add(line_with_end.len())?;
        if number == line_number {
            return Some((
                offset,
                line_end,
                line_with_end.trim_end_matches(['\r', '\n']),
            ));
        }
        offset = line_end;
    }
    if offset == source.len() && line_number == 1 {
        return Some((0, source.len(), source));
    }
    None
}

fn markdown_cell_offsets(line: &str, cell_index: usize) -> Option<(usize, usize)> {
    let mut delimiters = Vec::new();
    let bytes = line.as_bytes();
    for index in 0..bytes.len() {
        if bytes.get(index) == Some(&b'|') {
            let mut slashes = 0usize;
            let mut previous = index;
            while previous > 0 && bytes.get(previous - 1) == Some(&b'\\') {
                slashes = slashes.saturating_add(1);
                previous = previous.saturating_sub(1);
            }
            if slashes % 2 == 0 {
                delimiters.push(index);
            }
        }
    }
    if delimiters.is_empty() {
        return None;
    }
    let leading = usize::from(line.trim_start().starts_with('|'));
    let trailing = usize::from(line.trim_end().ends_with('|'));
    let count = delimiters
        .len()
        .saturating_add(1)
        .saturating_sub(leading)
        .saturating_sub(trailing);
    if cell_index >= count {
        return None;
    }
    let mut start = if leading == 1 {
        delimiters.get(cell_index)?.saturating_add(1)
    } else if cell_index == 0 {
        0
    } else {
        delimiters.get(cell_index - 1)?.saturating_add(1)
    };
    let mut end = match delimiters.get(cell_index + leading).copied() {
        Some(value) => value,
        None => line.len(),
    };
    while start < end && bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
        start = start.saturating_add(1);
    }
    while end > start && bytes.get(end - 1).is_some_and(u8::is_ascii_whitespace) {
        end = end.saturating_sub(1);
    }
    Some((start, end))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YamlStyle {
    Plain,
    SingleQuote,
    DoubleQuote,
}

fn yaml_style(value: &str, current: &str) -> YamlStyle {
    if value.contains(['\n', '\r']) {
        YamlStyle::DoubleQuote
    } else if current.starts_with('\'') && !value.chars().any(char::is_control) {
        YamlStyle::SingleQuote
    } else if current.starts_with('"') || !plain_safe(value) {
        YamlStyle::DoubleQuote
    } else {
        YamlStyle::Plain
    }
}

fn yaml_scalar(value: &str, current: &str) -> String {
    match yaml_style(value, current) {
        YamlStyle::Plain => value.to_owned(),
        YamlStyle::SingleQuote => single_quote(value),
        YamlStyle::DoubleQuote => double_quote(value),
    }
}

fn replacement_len(value: &str, style: &ReplacementStyle<'_>) -> Option<usize> {
    match style {
        ReplacementStyle::Yaml(current) => yaml_scalar_len(value, current),
        ReplacementStyle::Table => markdown_cell_len(value),
    }
}

fn yaml_scalar_len(value: &str, current: &str) -> Option<usize> {
    match yaml_style(value, current) {
        YamlStyle::Plain => Some(value.len()),
        YamlStyle::SingleQuote => value
            .len()
            .checked_add(2)?
            .checked_add(value.chars().filter(|&character| character == '\'').count()),
        YamlStyle::DoubleQuote => double_quote_len(value),
    }
}

fn double_quote_len(value: &str) -> Option<usize> {
    let escaped = value.chars().try_fold(2usize, |length, character| {
        let added = match character {
            '"' | '\\' | '\n' | '\r' | '\t' | '\0' | '\u{0008}' | '\u{000c}' => 2,
            character if character.is_control() => 6,
            character => character.len_utf8(),
        };
        length.checked_add(added)
    })?;
    Some(escaped)
}

fn markdown_cell_len(value: &str) -> Option<usize> {
    let mut length = 0usize;
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        let added = if character == '\r' {
            if characters.peek() == Some(&'\n') {
                let _ = characters.next();
            }
            4
        } else {
            match character {
                '\n' => 4,
                '\\' | '|' => 2,
                '&' => 5,
                '<' | '>' => 4,
                '"' => 6,
                '\'' => 5,
                character => character.len_utf8(),
            }
        };
        length = length.checked_add(added)?;
    }
    Some(length)
}

fn plain_safe(value: &str) -> bool {
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        return false;
    }
    if date_like(value)
        || matches!(
            value.to_ascii_lowercase().as_str(),
            "null" | "~" | "true" | "false" | "yes" | "no" | "on" | "off"
        )
    {
        return false;
    }
    if value
        .chars()
        .any(|character| matches!(character, ':' | '#' | '\'' | '"' | '<' | '>' | '&'))
    {
        return false;
    }
    if value.starts_with(['-', '?', '!', '@', '`', '{', '}', '[', ']', '%']) {
        return false;
    }
    match serde_yaml::from_str::<serde_yaml::Value>(value) {
        Ok(serde_yaml::Value::String(parsed)) => parsed == value,
        _ => false,
    }
}

fn date_like(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}
fn single_quote(value: &str) -> String {
    let mut output = String::with_capacity(value.len().saturating_add(2));
    output.push('\'');
    for character in value.chars() {
        if character == '\'' {
            output.push_str("''");
        } else {
            output.push(character);
        }
    }
    output.push('\'');
    output
}

fn double_quote(value: &str) -> String {
    let mut output = String::with_capacity(value.len().saturating_add(2));
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\0' => output.push_str("\\0"),
            '\u{0008}' => output.push_str("\\b"),
            '\u{000c}' => output.push_str("\\f"),
            character if character.is_control() => {
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn markdown_cell(value: &str) -> String {
    let mut output = String::with_capacity(value.len().saturating_add(8));
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            if characters.peek() == Some(&'\n') {
                let _ = characters.next();
            }
            output.push_str("<br>");
            continue;
        }
        match character {
            '\n' => output.push_str("<br>"),
            '\\' => output.push_str("\\\\"),
            '|' => output.push_str("\\|"),
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            character => output.push(character),
        }
    }
    output
}

fn enforce_document_limits(source: &str) -> Result<(), &'static str> {
    let StructuralCounts {
        sections,
        max_rows_per_table,
        rows_total,
        max_columns,
        cells_total,
        frontmatter_depth,
    } = structural_counts(source)?;
    if sections > SCALAR_EDIT_LIMITS.max_sections {
        return Err("source exceeds the section limit");
    }
    if max_rows_per_table > SCALAR_EDIT_LIMITS.max_rows_per_table {
        return Err("table exceeds the row limit");
    }
    if rows_total > SCALAR_EDIT_LIMITS.max_rows_total {
        return Err("source exceeds the total row limit");
    }
    if max_columns > SCALAR_EDIT_LIMITS.max_columns {
        return Err("table exceeds the column limit");
    }
    if cells_total > SCALAR_EDIT_LIMITS.max_cells_total {
        return Err("source exceeds the cell limit");
    }
    if frontmatter_depth > SCALAR_EDIT_LIMITS.max_frontmatter_depth {
        return Err("frontmatter exceeds the nesting limit");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn request(source: &str, path: &str, value: &str) -> ScalarEditRequest {
        ScalarEditRequest {
            source: source.to_owned(),
            base_revision: revision(source),
            sequence: 1,
            path: path.to_owned(),
            value: value.to_owned(),
        }
    }

    const SOURCE: &str = "---\nschema: ttyinv/v1\ninvoice:\n  number: INV-1\n  title: Original title\n  issued: 2026-01-01\n  due: 2026-01-15\n  currency: USD\n  locale: en-US\nfrom:\n  name: Alice\n  email: alice@example.com\n  website: https://from.example\nto:\n  name: Bob\n  email: bob@example.com\n  website: https://to.example\n---\n\n## Items\n\n| Description | Amount |\n| --- | --- |\n| One | 10 |\n";

    #[test]
    fn revision_is_sha256_of_exact_bytes() {
        assert_eq!(
            revision(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_ne!(revision("a"), revision("a\n"));
    }

    #[test]
    fn scalar_paths_apply() {
        for path in [
            "invoice.number",
            "invoice.title",
            "invoice.issued",
            "invoice.due",
            "invoice.currency",
            "invoice.locale",
            "from.name",
            "from.email",
            "from.website",
            "to.name",
            "to.email",
            "to.website",
        ] {
            let result = apply_scalar(request(SOURCE, path, "Changed"));
            assert!(matches!(
                result.outcome,
                ScalarEditOutcome::Applied | ScalarEditOutcome::AppliedWithErrors
            ));
            assert!(result.source.contains("Changed"));
        }
    }

    #[test]
    fn table_cell_escapes_hostile_text_and_newlines() {
        let result = apply_scalar(request(
            SOURCE,
            "sections[0].table.rows[0].cells[0]",
            "a|b\n<script>",
        ));
        assert!(result.source.contains("a\\|b<br>&lt;script&gt;"));
        assert!(!result.source.contains("|b\n"));
    }

    #[test]
    fn invalid_value_stays_in_source() {
        let result = apply_scalar(request(SOURCE, "invoice.currency", "usd"));
        assert_eq!(result.outcome, ScalarEditOutcome::AppliedWithErrors);
        assert!(result.source.contains("currency: usd"));
        assert!(!result.report.is_valid());
    }

    #[test]
    fn stale_keeps_original_source() {
        let mut item = request(SOURCE, "invoice.number", "Changed");
        item.base_revision = revision("other");
        let result = apply_scalar(item);
        assert_eq!(result.outcome, ScalarEditOutcome::Stale);
        assert_eq!(result.source, SOURCE);
        assert_eq!(result.revision, revision(SOURCE));
    }

    #[test]
    fn malformed_paths_never_panic() {
        for path in [
            "",
            "invoice..number",
            "sections[0].table.rows[0].cells[0",
            "sections[-1].table.rows[0].cells[0]",
            "sections[4294967295].table.rows[0].cells[0]",
            "sections[01].table.rows[0].cells[0]",
            "from.identifiers[\"x\"]",
        ] {
            let result = apply_scalar(request(SOURCE, path, "x"));
            assert_eq!(result.outcome, ScalarEditOutcome::Rejected);
        }
    }
    #[test]
    fn quote_values_as_strings() {
        for value in [
            "null",
            "true",
            "2026-01-15",
            "a: b",
            "x # y",
            "'quoted'",
            "<script>",
            "a&b",
        ] {
            let result = apply_scalar(request(SOURCE, "invoice.number", value));
            assert!(result.source.contains("number: \""));
            let Ok(parsed) = document(&result.source) else {
                panic!("patched source must parse");
            };
            assert_eq!(parsed.frontmatter.invoice.number, value);
        }
    }
    #[test]
    fn table_locator_preserves_escaped_pipes_spaces_empty_cells_and_alignment() {
        let table_source = SOURCE.replace(
            "| Description | Amount |\n| --- | --- |\n| One | 10 |\n",
            "| Description | Amount | Notes |\n| :--- | ---: | :--- |\n| a \\| b | 10 | <br> detail |\n|   | 20 | |\n",
        );
        let first = apply_scalar(request(
            &table_source,
            "sections[0].table.rows[0].cells[1]",
            "11",
        ));
        assert_eq!(first.outcome, ScalarEditOutcome::Applied);
        assert!(
            first
                .report
                .diagnostics()
                .iter()
                .all(|item| item.code != "TABLE003")
        );
        assert!(first.source.contains("| :--- | ---: | :--- |"));
        assert!(first.source.contains("|   | 20 | |"));

        let second = apply_scalar(request(
            &first.source,
            "sections[0].table.rows[1].cells[1]",
            "21",
        ));
        assert_eq!(second.outcome, ScalarEditOutcome::Applied);
        assert!(
            second
                .report
                .diagnostics()
                .iter()
                .all(|item| item.code != "TABLE003")
        );
        assert!(second.source.contains("|   | 21 | |"));
        assert!(second.source.contains("| a \\| b | 11 | <br> detail |"));

        let escaped = apply_scalar(request(
            &second.source,
            "sections[0].table.rows[0].cells[0]",
            "x|y",
        ));
        assert_eq!(escaped.outcome, ScalarEditOutcome::Applied);
        assert!(escaped.source.contains("| x\\|y | 11 | <br> detail |"));
        assert!(!escaped.source.contains("x\\\\|y"));
    }
    #[test]
    fn lossless_patch_and_warning_outcomes() {
        let needle = "| One | 10 |";
        let Some((prefix, suffix)) = SOURCE.split_once(needle) else {
            panic!("table fixture must contain the target");
        };
        let result = apply_scalar(request(
            SOURCE,
            "sections[0].table.rows[0].cells[0]",
            "updated",
        ));
        assert!(result.source.starts_with(prefix));
        assert!(result.source.ends_with(suffix));
        assert!(result.source.contains("| updated | 10 |"));

        let Some((frontmatter, _)) = SOURCE.split_once("\n---\n") else {
            panic!("frontmatter fixture must contain its delimiter");
        };
        let warning_source = format!("{frontmatter}\n---\n");
        let result = apply_scalar(request(&warning_source, "invoice.number", "Changed"));
        assert_eq!(result.outcome, ScalarEditOutcome::Applied);
        assert!(
            result
                .report
                .diagnostics()
                .iter()
                .all(|item| item.severity == super::super::Severity::Warning)
        );
    }

    #[test]
    fn rejects_targets_and_limits_without_panicking() {
        for path in [
            "sections[1].table.rows[0].cells[0]",
            "sections[0].table.rows[1].cells[0]",
            "sections[0].table.rows[0].cells[2]",
            "sections[0].table.rows[4294967295].cells[0]",
            "sections[0].table.rows[0].cells[4294967295]",
        ] {
            assert_eq!(
                apply_scalar(request(SOURCE, path, "x")).outcome,
                ScalarEditOutcome::Rejected
            );
        }
        let long_segment = format!(
            "invoice.{}",
            "x".repeat(SCALAR_EDIT_LIMITS.max_key_bytes + 1)
        );
        assert_eq!(
            apply_scalar(request(SOURCE, &long_segment, "x")).outcome,
            ScalarEditOutcome::Rejected
        );
        let long_path = "x".repeat(SCALAR_EDIT_LIMITS.max_path_bytes + 1);
        assert_eq!(
            apply_scalar(request(SOURCE, &long_path, "x")).outcome,
            ScalarEditOutcome::Rejected
        );
        let long_value = "x".repeat(SCALAR_EDIT_LIMITS.max_value_bytes + 1);
        assert_eq!(
            apply_scalar(request(SOURCE, "invoice.number", &long_value)).outcome,
            ScalarEditOutcome::Rejected
        );
        let long_source = "x".repeat(SCALAR_EDIT_LIMITS.max_source_bytes + 1);
        assert_eq!(
            apply_scalar(request(&long_source, "invoice.number", "x")).outcome,
            ScalarEditOutcome::Rejected
        );
        for path in [
            "invoice[",
            "invoice]",
            "invoice.number.extra",
            "sections[ 0].table.rows[0].cells[0]",
            "sections[00].table.rows[0].cells[0]",
            "sections[0].table.rows[0].cells[0]]",
            "sections[0].table.rows[0].cells[0].",
            "sections[0].table.rows[0].cells[0][1]",
            "from.identifiers[\"../invoice.number\"]",
        ] {
            assert_eq!(
                apply_scalar(request(SOURCE, path, "x")).outcome,
                ScalarEditOutcome::Rejected
            );
        }
    }
    #[test]
    fn rejects_multiline_yaml_scalars_without_changing_source() {
        let fixtures = [
            (
                SOURCE.replace(
                    "  title: Original title",
                    "  title: |\n    Original\n    title",
                ),
                "block",
            ),
            (
                SOURCE.replace(
                    "  title: Original title",
                    "  title: >\n    Original\n    title",
                ),
                "folded",
            ),
            (
                SOURCE.replace("  title: Original title", "  title: Original\n    title"),
                "plain continuation",
            ),
            (
                SOURCE.replace("  title: Original title", "  title: 'Original\n    title'"),
                "single quoted continuation",
            ),
            (
                SOURCE.replace(
                    "  title: Original title",
                    "  title: \"Original\n    title\"",
                ),
                "double quoted continuation",
            ),
        ];
        for (source, _name) in fixtures {
            let result = apply_scalar(request(&source, "invoice.title", "Changed"));
            assert_eq!(result.outcome, ScalarEditOutcome::Rejected);
            assert_eq!(result.source, source);
        }
    }

    #[test]
    fn parent_and_target_comments_remain_lossless() {
        let parent_comment = SOURCE.replace("invoice:\n", "invoice: # parent comment\n");
        let parent_result = apply_scalar(request(&parent_comment, "invoice.title", "Changed"));
        assert!(matches!(
            parent_result.outcome,
            ScalarEditOutcome::Applied | ScalarEditOutcome::AppliedWithErrors
        ));
        assert!(parent_result.source.contains("invoice: # parent comment\n"));

        let target_comment =
            SOURCE.replace("  number: INV-1\n", "  number: INV-1 # target comment\n");
        let target_result = apply_scalar(request(&target_comment, "invoice.number", "Changed"));
        assert!(matches!(
            target_result.outcome,
            ScalarEditOutcome::Applied | ScalarEditOutcome::AppliedWithErrors
        ));
        assert!(
            target_result
                .source
                .contains("number: Changed # target comment")
        );
    }

    #[test]
    fn rejects_escaped_result_above_source_limit() {
        let value = "\"".repeat(30_000);
        let result = apply_scalar(request(
            SOURCE,
            "sections[0].table.rows[0].cells[0]",
            &value,
        ));
        assert_eq!(result.outcome, ScalarEditOutcome::Rejected);
        assert_eq!(result.source, SOURCE);
        assert_eq!(result.revision, revision(SOURCE));
    }

    #[test]
    fn controls_round_trip_as_safe_yaml() {
        let source = SOURCE.replace("  number: INV-1\n", "  number: 'INV-1'\n");
        for value in ["a\u{0}b", "a\u{1}b", "a\u{0b}b", "a\tb"] {
            let result = apply_scalar(request(&source, "invoice.number", value));
            assert!(matches!(
                result.outcome,
                ScalarEditOutcome::Applied | ScalarEditOutcome::AppliedWithErrors
            ));
            let parsed = document(&result.source).expect("safe scalar must parse");
            assert_eq!(parsed.frontmatter.invoice.number, value);
        }
    }

    #[test]
    fn no_outer_pipe_tables_locate_every_cell_without_table_diagnostics() {
        let source = SOURCE.replace(
            "| Description | Amount |\n| --- | --- |\n| One | 10 |\n",
            "Description | Amount | Notes\n--- | --- | ---\na \\| b | 10 | tail\nOne | 20 | last\n",
        );
        let cases = [
            (
                "sections[0].table.rows[0].cells[0]",
                "first",
                "first | 10 | tail",
            ),
            (
                "sections[0].table.rows[0].cells[1]",
                "middle",
                "first | middle | tail",
            ),
            (
                "sections[0].table.rows[1].cells[1]",
                "empty",
                "One | empty | last",
            ),
            (
                "sections[0].table.rows[1].cells[2]",
                "last",
                "One | empty | last",
            ),
        ];
        let mut current = source;
        for (path, value, expected) in cases {
            let result = apply_scalar(request(&current, path, value));
            assert_eq!(result.outcome, ScalarEditOutcome::Applied);
            assert!(
                result
                    .report
                    .diagnostics()
                    .iter()
                    .all(|item| item.code != "TABLE003")
            );
            assert!(result.source.contains(expected));
            current = result.source;
        }
        assert!(current.contains("first | middle | tail"));
        assert!(current.contains("One | empty | last"));
    }

    #[test]
    fn structural_limits_apply_when_typed_frontmatter_is_invalid() {
        fn assert_rejected(source: &str) {
            let result = apply_scalar(request(source, "invoice.number", "Changed"));
            assert_eq!(result.outcome, ScalarEditOutcome::Rejected);
            assert_eq!(result.source, source);
        }

        let frontmatter = SOURCE
            .replace("  title: Original title\n", "")
            .split_once("\n---\n")
            .map(|(head, _)| head.to_owned())
            .expect("fixture delimiter");

        let mut rows = String::from("\n## Items\n\n| Description | Amount |\n| --- | --- |\n");
        for _ in 0..=SCALAR_EDIT_LIMITS.max_rows_per_table {
            rows.push_str("| One | 10 |\n");
        }
        assert_rejected(&format!("{frontmatter}\n---\n{rows}"));

        let mut sections = String::new();
        for index in 0..=SCALAR_EDIT_LIMITS.max_sections {
            sections.push_str(&format!("\n## Section {index}\n\ntext\n"));
        }
        assert_rejected(&format!("{frontmatter}\n---\n{sections}"));

        let headings = (0..=SCALAR_EDIT_LIMITS.max_columns)
            .map(|index| format!("Column {index}"))
            .collect::<Vec<_>>();
        let heading_line = format!("| {} |\n", headings.join(" | "));
        let separator_line = format!(
            "| {} |\n",
            headings
                .iter()
                .map(|_| "---")
                .collect::<Vec<_>>()
                .join(" | ")
        );
        let row_line = format!(
            "| {} |\n",
            headings.iter().map(|_| "x").collect::<Vec<_>>().join(" | ")
        );
        let columns = format!("\n## Items\n\n{heading_line}{separator_line}{row_line}");
        assert_rejected(&format!("{frontmatter}\n---\n{columns}"));

        let headings = (0..SCALAR_EDIT_LIMITS.max_columns)
            .map(|index| format!("Column {index}"))
            .collect::<Vec<_>>();
        let heading_line = format!("| {} |\n", headings.join(" | "));
        let separator_line = format!(
            "| {} |\n",
            headings
                .iter()
                .map(|_| "---")
                .collect::<Vec<_>>()
                .join(" | ")
        );
        let row_line = format!(
            "| {} |\n",
            headings.iter().map(|_| "x").collect::<Vec<_>>().join(" | ")
        );
        let mut cells = format!("\n## Items\n\n{heading_line}{separator_line}");
        for _ in 0..SCALAR_EDIT_LIMITS.max_rows_total {
            cells.push_str(&row_line);
        }
        assert_rejected(&format!("{frontmatter}\n---\n{cells}"));
    }
}
