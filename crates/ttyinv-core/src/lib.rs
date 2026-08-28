#![forbid(unsafe_code)]
pub use model::*;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
mod amount_words;
mod command;
mod model;
mod render;
pub use amount_words::{
    amount_in_words, currency_capabilities, currency_words, CurrencyWords, CurrencyWordsCapability,
};
pub use command::*;
pub use render::{
    is_external_asset_source, PreparedAmountInWords, PreparedBlock, PreparedImage, PreparedItem,
    PreparedLink, PreparedNode, PreparedPage, PreparedParty, PreparedPrimitive, PreparedRender,
    PreparedSemantic, PreparedSpan, PreparedTableRow, PreparedTextRow, Presentation,
    PresentationAccent, PresentationContent, PresentationFont, PresentationScale,
    PresentationTokens, RenderFormat, RenderWarning, ThemeTokens, MAX_ASSET_BYTES,
    MAX_ASSET_TOTAL_BYTES, MAX_PAGES, MAX_PNG_PIXELS, MAX_PNG_TOTAL_PIXELS, MAX_RENDERED_BYTES,
    PAGE_HEIGHT, PAGE_WIDTH, PREPARED_RENDER_VERSION,
};
pub const MAX_SOURCE_BYTES: usize = 128 * 1024;
pub const MAX_EDIT_BYTES: usize = 128 * 1024;
/// Shared adapter wording for a command envelope that fails typed deserialization.
pub const INVALID_COMMAND_MESSAGE_PREFIX: &str = "invalid command: ";
/// Shared core wording for source values beyond the bounded input size.
pub const SOURCE_SIZE_LIMIT_MESSAGE: &str = "source exceeds source size limit";
/// Formats typed command deserialization errors consistently across adapters.
pub fn invalid_command_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    let message = message.strip_prefix("Error: ").unwrap_or(&message);
    format!("{INVALID_COMMAND_MESSAGE_PREFIX}{message}")
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub field_path: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub hint: Option<String>,
    pub section: Option<String>,
    pub section_index: Option<usize>,
    pub row: Option<usize>,
    pub column_name: Option<String>,
}
impl Diagnostic {
    fn error(code: &str, msg: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: code.into(),
            message: msg.into(),
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
}
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidationReport {
    diagnostics: Vec<Diagnostic>,
}
impl ValidationReport {
    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FixedBlock {
    pub name: String,
    pub present: bool,
    pub movable: bool,
    pub page_break_before: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ManifestSection {
    pub index: usize,
    pub title: String,
    pub body: String,
    pub gap: Gap,
    pub page_break_before: bool,
    pub summary_only: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct StructureManifest {
    pub fixed_blocks: Vec<FixedBlock>,
    pub ordinary_sections: Vec<ManifestSection>,
}
impl Document {
    pub(crate) fn structure_manifest(&self) -> StructureManifest {
        StructureManifest {
            fixed_blocks: [
                ("from", false, true),
                ("bill_to", false, true),
                (
                    "settlements",
                    self.settlements_page_break_before,
                    self.settlements.is_some(),
                ),
                (
                    "payment",
                    self.payment_page_break_before,
                    self.payment.is_some(),
                ),
                (
                    "signature",
                    self.signature_page_break_before,
                    self.signature.is_some(),
                ),
            ]
            .into_iter()
            .map(|(n, page_break_before, present)| FixedBlock {
                name: n.into(),
                present,
                movable: false,
                page_break_before,
            })
            .collect(),
            ordinary_sections: self
                .ordinary_sections
                .iter()
                .enumerate()
                .map(|(i, s)| ManifestSection {
                    index: i,
                    title: s.title.clone(),
                    body: match &s.body {
                        SectionBody::Table(_) => "table",
                        SectionBody::Prose(p) if p.trim().is_empty() => "empty",
                        SectionBody::Prose(_) => "prose",
                    }
                    .into(),
                    gap: s.directives.gap,
                    page_break_before: s.directives.page_break_before,
                    summary_only: s.directives.summary_only,
                })
                .collect(),
        }
    }
}
pub(crate) fn revision(source: &str) -> String {
    Sha256::digest(source.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut result, byte| {
            let _ = write!(result, "{byte:02x}");
            result
        })
}
pub(crate) fn sha256_digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScannedBlock {
    start: usize,
    end: usize,
    directive_start: usize,
    fixed: Option<&'static str>,
}
fn normalized_source(source: &str) -> String {
    source
        .strip_prefix('\u{feff}')
        .unwrap_or(source)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}
fn fixed_name(title: &str) -> Option<&'static str> {
    match title.trim_end() {
        "From" => Some("From"),
        "Bill to" => Some("Bill to"),
        "Settlements" => Some("Settlements"),
        "Payment" => Some("Payment"),
        "Signature" => Some("Signature"),
        _ => None,
    }
}
fn fence_start(line: &str) -> Option<(char, usize)> {
    let indent = line.chars().take_while(|c| matches!(c, ' ' | '\t')).count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let character = rest.chars().next()?;
    if character != '`' && character != '~' {
        return None;
    }
    let length = rest.chars().take_while(|c| *c == character).count();
    (length >= 3).then_some((character, length))
}

fn fence_close(line: &str, fence: (char, usize)) -> bool {
    let indent = line.chars().take_while(|c| matches!(c, ' ' | '\t')).count();
    if indent > 3 {
        return false;
    }
    let rest = &line[indent..];
    let count = rest.chars().take_while(|c| *c == fence.0).count();
    count >= fence.1 && rest[count..].chars().all(char::is_whitespace)
}

fn scan_blocks(lines: &[String]) -> Vec<ScannedBlock> {
    let mut headings = Vec::new();
    let mut fence = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(current) = fence {
            if fence_close(line, current) {
                fence = None;
            }
            continue;
        }
        if let Some(start) = fence_start(line) {
            fence = Some(start);
            continue;
        }
        if line.starts_with("## ") {
            headings.push(i);
        }
    }
    headings
        .iter()
        .enumerate()
        .map(|(n, &start)| {
            let end = *headings.get(n + 1).unwrap_or(&lines.len());
            let mut directive_start = start;
            while directive_start > 0 {
                let line = &lines[directive_start - 1];
                if line.starts_with("<!-- ttyinv:") {
                    directive_start -= 1;
                } else {
                    break;
                }
            }
            ScannedBlock {
                start,
                end,
                directive_start,
                fixed: fixed_name(&lines[start][3..]),
            }
        })
        .collect()
}
fn fixed_range(l: &[String], name: &str) -> Option<(usize, usize)> {
    scan_blocks(l)
        .into_iter()
        .find(|b| b.fixed == Some(name))
        .map(|b| (b.start, b.end))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrontmatterConfig {
    schema: String,
    #[serde(default = "model::default_format")]
    format: String,
    #[serde(default = "model::default_theme")]
    theme: String,
    #[serde(default = "model::default_font")]
    font: String,
    #[serde(rename = "font-weight", default)]
    font_weight: FontWeight,
    #[serde(default = "model::default_density")]
    density: String,
    #[serde(rename = "amount-in-words", default)]
    amount_in_words: bool,
    #[serde(default)]
    accent: Option<Accent>,
    #[serde(rename = "font-scale", default)]
    font_scale: FontScale,
    #[serde(rename = "frame-inset", default)]
    frame_inset: FrameInset,
}
impl From<FrontmatterConfig> for Config {
    fn from(value: FrontmatterConfig) -> Self {
        Self {
            schema: value.schema,
            format: value.format,
            theme: value.theme,
            font: value.font,
            font_weight: value.font_weight,
            density: value.density,
            amount_in_words: value.amount_in_words,
            accent: value.accent,
            font_scale: value.font_scale,
            frame_inset: value.frame_inset,
        }
    }
}
fn invalid_appearance_config(raw: &serde_yaml::Value) -> bool {
    let Some(mapping) = raw.as_mapping() else {
        return false;
    };
    let string_key = |key: &str| serde_yaml::Value::String(key.into());
    mapping
        .get(string_key("accent"))
        .is_some_and(|value| serde_yaml::from_value::<Accent>(value.clone()).is_err())
        || mapping
            .get(string_key("font-weight"))
            .is_some_and(|value| serde_yaml::from_value::<FontWeight>(value.clone()).is_err())
        || mapping
            .get(string_key("font-scale"))
            .is_some_and(|value| serde_yaml::from_value::<FontScale>(value.clone()).is_err())
        || mapping
            .get(string_key("frame-inset"))
            .is_some_and(|value| serde_yaml::from_value::<FrameInset>(value.clone()).is_err())
}
pub(crate) fn document(source: &str) -> Result<Document, ValidationReport> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ValidationReport {
            diagnostics: vec![Diagnostic::error(
                "LIMIT001",
                "source exceeds source size limit",
            )],
        });
    }
    let normalized = normalized_source(source);
    let mut e = Vec::new();
    let (yaml, body) = split_frontmatter(&normalized, &mut e);
    if !e.is_empty() {
        return Err(ValidationReport { diagnostics: e });
    }
    let raw: serde_yaml::Value = match serde_yaml::from_str(yaml) {
        Ok(v) => v,
        Err(_) => {
            return Err(ValidationReport {
                diagnostics: vec![Diagnostic::error(
                    "SCHEMA001",
                    "frontmatter must be a valid ttyinv/v2 configuration",
                )],
            });
        }
    };
    let appearance_invalid = invalid_appearance_config(&raw);
    let c: Config = match serde_yaml::from_value::<FrontmatterConfig>(raw) {
        Ok(v) => v.into(),
        Err(_) => {
            return Err(ValidationReport {
                diagnostics: vec![Diagnostic::error(
                    if appearance_invalid {
                        "SCHEMA007"
                    } else {
                        "SCHEMA001"
                    },
                    "frontmatter must be a valid ttyinv/v2 configuration",
                )],
            });
        }
    };
    validate_config(&c, &mut e);
    let mut l = body.lines().map(str::to_owned).collect::<Vec<_>>();
    while l.first().is_some_and(|x| x.trim().is_empty()) {
        l.remove(0);
    }
    while l.last().is_some_and(|x| x.trim().is_empty()) {
        l.pop();
    }
    let blocks = scan_blocks(&l);
    if l.first().map(|x| !x.starts_with("# ")).unwrap_or(true) {
        e.push(Diagnostic::error("MARKDOWN001", "H1 title is required"));
        return Err(ValidationReport { diagnostics: e });
    }
    let title: String = l[0][2..].trim().into();
    if title.is_empty() {
        e.push(Diagnostic::error("SCHEMA001", "H1 title cannot be empty"));
    }
    let mut i = 1;
    let mut m = BTreeMap::new();
    while i < l.len() && !l[i].starts_with("## ") {
        let x = l[i].as_str();
        if x.trim().is_empty() {
            i += 1;
            continue;
        }
        if let Some((k, v)) = label_line(x) {
            if !["Number", "Kind", "Issued", "Due", "Terms", "Currency"].contains(&k) {
                e.push(Diagnostic::error(
                    "SCHEMA008",
                    format!("unknown metadata label {k}"),
                ));
            }
            if m.insert(k.into(), v.into()).is_some() {
                e.push(Diagnostic::error(
                    "SCHEMA004",
                    "metadata labels must be unique",
                ));
            }
        } else {
            e.push(Diagnostic::error(
                "SCHEMA001",
                "metadata must be a labelled list",
            ));
        }
        i += 1;
    }
    let metadata = parse_metadata(&m, &mut e);
    let mut from = None;
    let mut bill = None;
    let mut settlements = None;
    let mut payment = None;
    let mut signature = None;
    let mut footer_breaks = [false; 3];
    let mut sections = Vec::new();
    let mut pending = SectionDirectives::default();
    let mut fixed_order = 0usize;
    let mut block_no = 0;
    while block_no < blocks.len() {
        let block = blocks[block_no];
        let mut d = block.directive_start;
        while d < block.start {
            if let Some(x) = parse_directive(&l[d]) {
                match x {
                    Directive::Gap(g) => pending.gap = g,
                    Directive::Page => pending.page_break_before = true,
                    Directive::Summary => pending.summary_only = true,
                }
            } else if l[d].starts_with("<!-- ttyinv:")
                || l[d].trim_start().starts_with("<!-- ttyinv:")
            {
                e.push(Diagnostic::error(
                    "DIRECTIVE001",
                    "unknown or indented directive",
                ));
            }
            d += 1;
        }
        let h: String = l[block.start][3..].trim().into();
        if h.is_empty() {
            e.push(Diagnostic::error(
                "MARKDOWN001",
                "H2 heading cannot be empty",
            ));
        }
        let body_end = blocks
            .get(block_no + 1)
            .map_or(block.end, |next| next.directive_start);
        let b = l[block.start + 1..body_end]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        if block.fixed.is_some()
            && (pending.gap != Gap::Standard
                || pending.summary_only
                || (pending.page_break_before && matches!(block.fixed, Some("From" | "Bill to"))))
        {
            e.push(Diagnostic::error(
                "DIRECTIVE002",
                "directive is not valid for this fixed block",
            ));
        }
        let rank = match block.fixed {
            Some("From") => 1,
            Some("Bill to") => 2,
            Some("Settlements") => 4,
            Some("Payment") => 5,
            Some("Signature") => 6,
            _ => 3,
        };
        if rank < fixed_order {
            e.push(Diagnostic::error(
                "SCHEMA006",
                "fixed blocks must use canonical source order",
            ))
        } else if rank > fixed_order {
            fixed_order = rank
        }
        match block.fixed {
            Some("From") => {
                if from.is_some() {
                    e.push(Diagnostic::error(
                        "SCHEMA006",
                        "fixed block cannot be repeated",
                    ));
                }
                from = Some(parse_party(&b, &mut e))
            }
            Some("Bill to") => {
                if bill.is_some() {
                    e.push(Diagnostic::error(
                        "SCHEMA006",
                        "fixed block cannot be repeated",
                    ));
                }
                bill = Some(parse_party(&b, &mut e))
            }
            Some("Settlements") => {
                if settlements.is_some() {
                    e.push(Diagnostic::error(
                        "SCHEMA006",
                        "fixed block cannot be repeated",
                    ));
                }
                settlements = parse_table(&b, &mut e);
                footer_breaks[0] = pending.page_break_before;
                if let Some(t) = settlements.as_ref() {
                    validate_settlements(t, &mut e)
                }
            }
            Some("Payment") => {
                if payment.is_some() {
                    e.push(Diagnostic::error(
                        "SCHEMA006",
                        "fixed block cannot be repeated",
                    ));
                }
                payment = Some(parse_payment(&b, &mut e));
                footer_breaks[1] = pending.page_break_before;
            }
            Some("Signature") => {
                if signature.is_some() {
                    e.push(Diagnostic::error(
                        "SCHEMA006",
                        "fixed block cannot be repeated",
                    ));
                }
                signature = Some(parse_signature(&b, &mut e));
                footer_breaks[2] = pending.page_break_before;
            }
            _ => {
                let directives = pending.clone();
                let body = parse_body(&b, &mut e);
                let total = match &body {
                    SectionBody::Table(table) => Some(table_total(
                        table,
                        &metadata.currency,
                        directives.summary_only,
                        &mut e,
                    )),
                    SectionBody::Prose(_) => None,
                };
                sections.push(Section {
                    title: h,
                    body,
                    directives,
                    total,
                    span: SourceSpan::default(),
                });
            }
        }
        pending = SectionDirectives::default();
        block_no += 1;
    }
    if from.is_none() {
        e.push(Diagnostic::error("SCHEMA005", "From section is required"))
    }
    if bill.is_none() {
        e.push(Diagnostic::error(
            "SCHEMA005",
            "Bill to section is required",
        ))
    }
    if let (Some(a), Some(b)) = (metadata.due.as_ref(), Some(&metadata.issued)) {
        if a.0 < b.0 {
            e.push(Diagnostic::error(
                "DATE002",
                "Due date cannot be before Issued date",
            ))
        }
    }
    let grand_total = sections
        .iter()
        .filter_map(|section| section.total)
        .fold(Decimal::ZERO, |sum, total| sum + total)
        .round_dp_with_strategy(
            currency_exponent(&metadata.currency),
            RoundingStrategy::MidpointNearestEven,
        );
    if !e.is_empty() {
        return Err(ValidationReport { diagnostics: e });
    }
    Ok(Document {
        config: c,
        title,
        metadata,
        from: from.unwrap(),
        bill_to: bill.unwrap(),
        ordinary_sections: sections,
        settlements,
        settlements_page_break_before: footer_breaks[0],
        payment,
        payment_page_break_before: footer_breaks[1],
        signature,
        signature_page_break_before: footer_breaks[2],
        grand_total,
        source: normalized,
    })
}
fn split_frontmatter<'a>(s: &'a str, e: &mut Vec<Diagnostic>) -> (&'a str, &'a str) {
    let Some(r) = s.strip_prefix("---\n") else {
        e.push(Diagnostic::error(
            "SCHEMA001",
            "frontmatter must start with ---",
        ));
        return ("", s);
    };
    let Some(n) = r.find("\n---\n") else {
        e.push(Diagnostic::error(
            "SCHEMA001",
            "frontmatter closing delimiter is missing",
        ));
        return ("", s);
    };
    (&r[..n], &r[n + 5..])
}
fn validate_config(c: &Config, e: &mut Vec<Diagnostic>) {
    if c.schema != "ttyinv/v2" {
        e.push(Diagnostic::error("SCHEMA002", "schema must be ttyinv/v2"))
    }
    if !render::supported_money_formats().contains(&c.format.as_str()) {
        e.push(Diagnostic::error("SCHEMA007", "unsupported format"))
    }
    if !render::supported_themes().contains(&c.theme.as_str()) {
        e.push(Diagnostic::error("SCHEMA007", "unsupported theme"))
    }
    if !render::font_capabilities().any(|font| font.id == c.font) {
        e.push(Diagnostic::error("SCHEMA007", "unsupported font"))
    }
    if !render::supported_densities().contains(&c.density.as_str()) {
        e.push(Diagnostic::error("SCHEMA007", "unsupported density"))
    }
}
fn label_line(s: &str) -> Option<(&str, &str)> {
    let x = s.strip_prefix("- ")?;
    let (i, v) = x.split_once(':')?;
    Some((i.trim(), v.trim()))
}
fn parse_metadata(m: &BTreeMap<String, String>, e: &mut Vec<Diagnostic>) -> Metadata {
    let g = |k: &str| m.get(k).cloned().unwrap_or_default();
    for k in ["Number", "Issued", "Currency"] {
        if !m.contains_key(k) {
            e.push(Diagnostic::error("SCHEMA003", format!("{k} is required")))
        }
    }
    let issued = Date::parse(&g("Issued")).unwrap_or_else(|| {
        e.push(Diagnostic::error(
            "DATE001",
            "Issued must be a real YYYY-MM-DD date",
        ));
        Date("0000-00-00".into())
    });
    let due = m.get("Due").and_then(|x| {
        let d = Date::parse(x);
        if d.is_none() {
            e.push(Diagnostic::error(
                "DATE001",
                "Due must be a real YYYY-MM-DD date",
            ))
        }
        d
    });
    let cur = g("Currency");
    if cur.len() != 3 || !cur.bytes().all(|b| b.is_ascii_uppercase()) {
        e.push(Diagnostic::error(
            "CURRENCY001",
            "Currency must be three uppercase ASCII letters",
        ))
    }
    Metadata {
        number: g("Number"),
        kind: m.get("Kind").cloned().unwrap_or_else(|| "standard".into()),
        issued,
        due,
        terms: m.get("Terms").cloned(),
        currency: cur,
    }
}
fn parse_image(s: &str) -> Option<(String, String)> {
    let x = s.strip_prefix("![")?.split_once("](")?;
    Some((x.0.into(), x.1.strip_suffix(')')?.into()))
}
fn safe_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn parse_party(b: &[&str], e: &mut Vec<Diagnostic>) -> Party {
    let mut p = Party {
        name: String::new(),
        address: vec![],
        email: None,
        website: None,
        identifiers: vec![],
        logo: None,
    };
    let mut saw_field = false;

    for x in b {
        if x.trim().is_empty() {
            continue;
        }
        if let Some((a, z)) = parse_image(x) {
            if saw_field {
                e.push(Diagnostic::error(
                    "SCHEMA008",
                    "party image must precede labelled fields",
                ));
            }
            if a.is_empty() || z.is_empty() || z.chars().any(char::is_whitespace) || z.contains(')')
            {
                e.push(Diagnostic::error(
                    "SCHEMA008",
                    "image alt and source are required and source cannot contain whitespace",
                ))
            }
            if p.logo.is_some() {
                e.push(Diagnostic::error(
                    "SCHEMA008",
                    "party allows one logo image",
                ))
            }
            p.logo = Some(Image { alt: a, src: z });
        } else if let Some((k, v)) = label_line(x) {
            saw_field = true;
            match k {
                "Name" => {
                    if !p.name.is_empty() {
                        e.push(Diagnostic::error("SCHEMA008", "party Name must be unique"))
                    } else {
                        p.name = v.into();
                    }
                }
                "Address" => p.address.push(v.into()),
                "Email" => {
                    if p.email.is_some() {
                        e.push(Diagnostic::error("SCHEMA008", "party Email must be unique"))
                    } else {
                        p.email = Some(v.into());
                    }
                }
                "Website" => {
                    if p.website.is_some() {
                        e.push(Diagnostic::error(
                            "SCHEMA008",
                            "party Website must be unique",
                        ))
                    } else {
                        p.website = Some(v.into());
                    }
                }
                k if k.starts_with("ID.") && safe_key(&k[3..]) => {
                    if p.identifiers.iter().any(|id| id.key == k[3..]) {
                        e.push(Diagnostic::error(
                            "SCHEMA008",
                            "party identifiers must be unique",
                        ))
                    } else {
                        p.identifiers.push(Identifier {
                            key: k[3..].into(),
                            value: v.into(),
                        });
                    }
                }
                k if k.starts_with("ID.") => e.push(Diagnostic::error(
                    "SCHEMA008",
                    "party identifier key is unsafe",
                )),
                _ => e.push(Diagnostic::error(
                    "SCHEMA008",
                    format!("unknown party label {k}"),
                )),
            }
        } else {
            e.push(Diagnostic::error(
                "SCHEMA008",
                "party content must be an image or labelled list",
            ))
        }
    }
    if p.name.is_empty() {
        e.push(Diagnostic::error("SCHEMA003", "party Name is required"))
    }
    p
}
fn split_pipe(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    let mut source = s.trim();
    if let Some(rest) = source.strip_prefix('|') {
        source = rest;
    }
    if let Some(rest) = source.strip_suffix('|') {
        source = rest;
    }
    for c in source.chars() {
        if escaped {
            cell.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '|' {
            out.push(cell.trim().replace("<br>", "\n"));
            cell.clear();
        } else {
            cell.push(c);
        }
    }
    if escaped {
        cell.push('\\');
    }
    out.push(cell.trim().replace("<br>", "\n"));
    out
}
fn valid_separator_cell(value: &str) -> bool {
    let value = value.trim();
    let value = value.strip_prefix(':').unwrap_or(value);
    let value = value.strip_suffix(':').unwrap_or(value);
    value.len() >= 3 && value.chars().all(|c| c == '-')
}
fn parse_table(b: &[&str], e: &mut Vec<Diagnostic>) -> Option<Table> {
    let mut ls = Vec::new();
    let mut prose = false;
    let mut fence = None;
    for x in b {
        if let Some(current) = fence {
            if fence_close(x, current) {
                fence = None;
            }
            continue;
        }
        if let Some(start) = fence_start(x) {
            fence = Some(start);
            continue;
        }
        if x.trim().is_empty() {
            continue;
        }
        if x.starts_with('|') {
            ls.push(*x);
        } else {
            prose = true;
        }
    }
    if prose {
        e.push(Diagnostic::error("TABLE004", "table cannot contain prose"));
    }
    if ls.len() < 2 {
        e.push(Diagnostic::error(
            "TABLE001",
            "table requires heading and separator",
        ));
        return None;
    }
    let h = split_pipe(ls[0]);
    let sep = split_pipe(ls[1]);
    if h.is_empty() || sep.len() != h.len() || sep.iter().any(|x| !valid_separator_cell(x)) {
        e.push(Diagnostic::error("TABLE001", "invalid table separator"));
        return None;
    }
    let alignments = sep
        .iter()
        .map(|x| {
            let y = x.trim();
            match (y.starts_with(':'), y.ends_with(':')) {
                (true, true) => TableAlignment::Center,
                (true, false) => TableAlignment::Left,
                (false, true) => TableAlignment::Right,
                _ => TableAlignment::None,
            }
        })
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    for x in &ls[2..] {
        let r = split_pipe(x);
        if r.len() != h.len() {
            e.push(Diagnostic::error(
                "TABLE003",
                "table row width does not match headings",
            ));
        } else {
            rows.push(r);
        }
    }
    if rows.is_empty() {
        e.push(Diagnostic::error("TABLE002", "table requires one body row"))
    }
    Some(Table {
        headings: h,
        alignments,
        rows,
    })
}
fn validate_settlements(t: &Table, e: &mut Vec<Diagnostic>) {
    let expected = [
        "Date",
        "Paid",
        "Paid currency",
        "Received",
        "Received currency",
    ];
    if t.headings.iter().map(String::as_str).collect::<Vec<_>>() != expected {
        e.push(Diagnostic::error(
            "TABLE005",
            "Settlements headings are fixed",
        ));
        return;
    }
    for r in &t.rows {
        if r.len() != 5 || Date::parse(&r[0]).is_none() {
            e.push(Diagnostic::error(
                "DATE001",
                "settlement date must be a real YYYY-MM-DD date",
            ));
        }
        for i in [1usize, 3] {
            if r.get(i)
                .and_then(|x| Decimal::from_str_exact(x).ok())
                .is_none()
            {
                e.push(Diagnostic::error(
                    "CURRENCY002",
                    "settlement amount must be a decimal",
                ));
            }
        }
        for i in [2usize, 4] {
            if r.get(i)
                .is_none_or(|x| x.len() != 3 || !x.bytes().all(|b| b.is_ascii_uppercase()))
            {
                e.push(Diagnostic::error(
                    "CURRENCY001",
                    "settlement currency must be three uppercase ASCII letters",
                ));
            }
        }
    }
}
fn currency_exponent(currency: &str) -> u32 {
    match currency {
        "BHD" | "IQD" | "JOD" | "KWD" | "LYD" | "OMR" | "TND" => 3,
        "BIF" | "CLP" | "DJF" | "GNF" | "ISK" | "JPY" | "KMF" | "KRW" | "PYG" | "RWF" | "UGX"
        | "UYI" | "VND" | "VUV" | "XAF" | "XOF" | "XPF" => 0,
        _ => 2,
    }
}
fn half_decimal_unit(scale: u32) -> Decimal {
    if scale >= 28 {
        Decimal::ZERO
    } else {
        Decimal::new(5, scale + 1)
    }
}

fn normalized_header(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn matches_amount_header(value: &str, aliases: &[&str]) -> bool {
    let value = normalized_header(value);
    aliases.iter().any(|alias| {
        value == *alias
            || (value.starts_with(alias)
                && value[alias.len()..].len() == 3
                && value[alias.len()..].bytes().all(|b| b.is_ascii_lowercase()))
    })
}

pub(crate) fn payable_amount_column(
    headings: &[String],
    currency: &str,
) -> Result<Option<usize>, ()> {
    let amount_aliases = ["amount", "total"];
    let mut candidates = Vec::new();
    let mut payable = Vec::new();
    for (index, heading) in headings.iter().enumerate() {
        if !matches_amount_header(heading, &amount_aliases) {
            continue;
        }
        candidates.push(index);
        let suffix = heading
            .rsplit_once('(')
            .and_then(|(_, value)| value.strip_suffix(')'))
            .map(str::trim);
        if suffix.is_some_and(|value| value.eq_ignore_ascii_case(currency)) {
            payable.push(index);
        }
    }
    if payable.len() > 1 {
        return Err(());
    }
    Ok(payable
        .first()
        .copied()
        .or_else(|| candidates.last().copied()))
}

fn numeric_amount(value: &str) -> Option<Decimal> {
    let value = value.trim();
    if value.is_empty()
        || value.contains(',')
        || value.contains('e')
        || value.contains('E')
        || value.chars().filter(|c| *c == '.').count() > 1
    {
        return None;
    }
    Decimal::from_str_exact(value).ok()
}

fn summary_row(value: &str) -> bool {
    let value = value
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`'))
        .collect::<String>()
        .trim()
        .to_ascii_lowercase();
    matches!(value.as_str(), "subtotal" | "total" | "grand total")
}

fn summary_row_at(row: &[String], headings: &[String]) -> bool {
    let description = headings
        .iter()
        .position(|heading| matches_amount_header(heading, &["description", "item", "service"]))
        .unwrap_or(0);
    row.get(description).is_some_and(|value| summary_row(value))
}

fn table_total(
    table: &Table,
    currency: &str,
    summary_only: bool,
    e: &mut Vec<Diagnostic>,
) -> Decimal {
    let quantity = table
        .headings
        .iter()
        .position(|h| matches_amount_header(h, &["qty", "quantity", "days", "hours", "units"]));
    let rate = table
        .headings
        .iter()
        .position(|h| matches_amount_header(h, &["rate", "unitprice", "price"]));
    let amount = match payable_amount_column(&table.headings, currency) {
        Ok(value) => value,
        Err(()) => {
            e.push(Diagnostic::error(
                "MONEY009",
                "table has duplicate payable amount columns",
            ));
            None
        }
    };
    let Some(amount) = amount else {
        return Decimal::ZERO;
    };
    let exponent = currency_exponent(currency);
    let mut total = Decimal::ZERO;
    for row in &table.rows {
        let Some(value) = row.get(amount) else {
            continue;
        };
        let is_summary = summary_row_at(row, &table.headings);
        let explicit = numeric_amount(value);
        if (value.trim().is_empty() || value.trim().eq_ignore_ascii_case("auto"))
            && (summary_only || is_summary)
        {
            e.push(Diagnostic::error(
                "MONEY008",
                "summary rows require an explicit amount",
            ));
            continue;
        }
        let value = if value.trim().is_empty() || value.trim().eq_ignore_ascii_case("auto") {
            let Some(q) = quantity
                .and_then(|i| row.get(i))
                .and_then(|x| numeric_amount(x))
            else {
                e.push(Diagnostic::error(
                    "MONEY003",
                    "auto amount requires a numeric quantity",
                ));
                continue;
            };
            let Some(r) = rate
                .and_then(|i| row.get(i))
                .and_then(|x| numeric_amount(x))
            else {
                e.push(Diagnostic::error(
                    "MONEY003",
                    "auto amount requires a numeric rate",
                ));
                continue;
            };
            (q * r).round_dp_with_strategy(exponent, RoundingStrategy::MidpointNearestEven)
        } else {
            let Some(explicit) = explicit else {
                e.push(Diagnostic::error("MONEY002", "amount must be a decimal"));
                continue;
            };
            let rounded =
                explicit.round_dp_with_strategy(exponent, RoundingStrategy::MidpointNearestEven);
            if let (Some(q), Some(r)) = (
                quantity
                    .and_then(|i| row.get(i))
                    .and_then(|x| numeric_amount(x)),
                rate.and_then(|i| row.get(i))
                    .and_then(|x| numeric_amount(x)),
            ) {
                let product = q * r;
                let difference = (explicit - product).abs();
                let rate_scale = rate
                    .and_then(|i| row.get(i))
                    .and_then(|x| numeric_amount(x))
                    .map_or(0, |value| value.scale());
                let rate_half_unit = half_decimal_unit(rate_scale);
                let amount_half_unit = half_decimal_unit(exponent);
                let tolerance = q.abs() * rate_half_unit + amount_half_unit;
                if difference > tolerance {
                    e.push(Diagnostic::error(
                        "MONEY004",
                        "amount differs from quantity times rate",
                    ));
                }
            }
            rounded
        };
        if !summary_only && !is_summary {
            total += value;
        }
    }
    total.round_dp_with_strategy(exponent, RoundingStrategy::MidpointNearestEven)
}

fn parse_body(b: &[&str], e: &mut Vec<Diagnostic>) -> SectionBody {
    let mut fence = None;
    let mut has_table = false;
    let mut nonblank = Vec::new();
    for line in b {
        if let Some(current) = fence {
            if fence_close(line, current) {
                fence = None;
            }
            continue;
        }
        if let Some(start) = fence_start(line) {
            fence = Some(start);
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("# ") {
            e.push(Diagnostic::error(
                "MARKDOWN001",
                "only the document title may use an H1 heading",
            ));
        }
        if trimmed.starts_with("## ") {
            e.push(Diagnostic::error(
                "MARKDOWN001",
                "reserved H2 headings must be column zero",
            ));
        }
        if line.starts_with("<!-- ttyinv:") || trimmed.starts_with("<!-- ttyinv:") {
            e.push(Diagnostic::error(
                "DIRECTIVE002",
                "directive must precede a block heading",
            ));
        }
        if !line.trim().is_empty() {
            nonblank.push(*line);
            has_table |= line.starts_with('|');
        }
    }
    if has_table {
        if let Some(t) = parse_table(b, e) {
            return SectionBody::Table(t);
        }
    }
    if nonblank.iter().any(|x| x.starts_with('|')) {
        e.push(Diagnostic::error(
            "TABLE004",
            "mixed table and prose content",
        ));
    }
    SectionBody::Prose(b.join("\n").trim().into())
}
fn parse_payment(b: &[&str], e: &mut Vec<Diagnostic>) -> Payment {
    let mut out = vec![];
    let mut cur: Option<PaymentMethod> = None;
    for x in b {
        if x.trim().is_empty() {
            continue;
        }
        if let Some(t) = x.strip_prefix("### ") {
            if t.trim().is_empty() {
                e.push(Diagnostic::error(
                    "SCHEMA009",
                    "payment method title cannot be empty",
                ));
            }
            if let Some(m) = cur.take() {
                if m.fields.is_empty() {
                    e.push(Diagnostic::error(
                        "SCHEMA009",
                        "payment method requires a field",
                    ));
                }
                out.push(m)
            }
            cur = Some(PaymentMethod {
                title: t.trim().into(),
                fields: vec![],
            });
        } else if let Some((k, v)) = label_line(x) {
            if let Some(m) = cur.as_mut() {
                if m.fields.iter().any(|field| field.label == k) {
                    e.push(Diagnostic::error(
                        "SCHEMA009",
                        "payment method fields must be unique",
                    ));
                } else {
                    m.fields.push(LabelValue {
                        label: k.into(),
                        value: v.into(),
                    });
                }
            } else {
                e.push(Diagnostic::error(
                    "SCHEMA009",
                    "payment fields need a method heading",
                ))
            }
        } else {
            e.push(Diagnostic::error(
                "SCHEMA009",
                "payment content must be H3 headings or labelled fields",
            ))
        }
    }
    if let Some(m) = cur {
        if m.fields.is_empty() {
            e.push(Diagnostic::error(
                "SCHEMA009",
                "payment method requires a field",
            ));
        }
        out.push(m)
    }
    if out.is_empty() {
        e.push(Diagnostic::error("SCHEMA009", "Payment requires a method"))
    }
    Payment { methods: out }
}

fn parse_signature(b: &[&str], e: &mut Vec<Diagnostic>) -> Signature {
    let mut im: Option<Image> = None;
    let mut n: Option<String> = None;
    let mut l: Option<String> = None;
    let mut saw_field = false;

    for x in b {
        if x.trim().is_empty() {
            continue;
        }
        if let Some((a, z)) = parse_image(x) {
            if saw_field {
                e.push(Diagnostic::error(
                    "SCHEMA008",
                    "Signature image must precede labelled fields",
                ));
            }
            if a.is_empty() || z.is_empty() || z.chars().any(char::is_whitespace) || z.contains(')')
            {
                e.push(Diagnostic::error(
                    "SCHEMA008",
                    "image alt and source are required and source cannot contain whitespace",
                ))
            }
            if im.is_some() {
                e.push(Diagnostic::error("SCHEMA008", "Signature allows one image"))
            }
            im = Some(Image { alt: a, src: z });
        } else if let Some((k, v)) = label_line(x) {
            saw_field = true;
            match k {
                "Name" => {
                    if n.is_some() {
                        e.push(Diagnostic::error(
                            "SCHEMA008",
                            "Signature Name must be unique",
                        ))
                    } else {
                        n = Some(v.into());
                    }
                }
                "Label" => {
                    if l.is_some() {
                        e.push(Diagnostic::error(
                            "SCHEMA008",
                            "Signature Label must be unique",
                        ))
                    } else {
                        l = Some(v.into());
                    }
                }
                _ => e.push(Diagnostic::error(
                    "SCHEMA008",
                    format!("unknown signature label {k}"),
                )),
            }
        } else {
            e.push(Diagnostic::error(
                "SCHEMA008",
                "signature content must be an image or labelled list",
            ))
        }
    }
    if n.as_ref().is_none_or(|x| x.is_empty()) {
        e.push(Diagnostic::error("SCHEMA003", "Signature Name is required"))
    }
    if l.as_ref().is_none_or(|x| x.is_empty()) {
        e.push(Diagnostic::error(
            "SCHEMA003",
            "Signature Label is required",
        ))
    }
    Signature {
        image: im,
        name: n.unwrap_or_default(),
        label: l.unwrap_or_default(),
    }
}
enum Directive {
    Gap(Gap),
    Page,
    Summary,
}
fn parse_directive(s: &str) -> Option<Directive> {
    let x = s.strip_prefix("<!-- ttyinv:")?.strip_suffix(" -->")?;
    match x {
        "page-break-before" => Some(Directive::Page),
        "summary-only" => Some(Directive::Summary),
        "gap-before none" => Some(Directive::Gap(Gap::None)),
        "gap-before tight" => Some(Directive::Gap(Gap::Tight)),
        "gap-before standard" => Some(Directive::Gap(Gap::Standard)),
        "gap-before roomy" => Some(Directive::Gap(Gap::Roomy)),
        _ => None,
    }
}
pub(crate) fn validate(s: &str) -> ValidationReport {
    match document(s) {
        Ok(_) => ValidationReport {
            diagnostics: vec![],
        },
        Err(r) => r,
    }
}
fn gap_str(g: Gap) -> &'static str {
    match g {
        Gap::None => "none",
        Gap::Tight => "tight",
        Gap::Standard => "standard",
        Gap::Roomy => "roomy",
    }
}
fn escape_line(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}
fn escape_table_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\r', "")
        .replace('\n', "<br>")
}
pub(crate) fn serialize_markdown(d: &Document) -> String {
    let accent = d
        .config
        .accent
        .as_ref()
        .map_or_else(String::new, |value| format!("accent: \"{value}\"\n"));
    let amount_in_words = if d.config.amount_in_words {
        "amount-in-words: true\n"
    } else {
        ""
    };
    let mut s = format!(
        "---\nschema: {}\nformat: {}\ntheme: {}\nfont: {}\nfont-weight: {}\ndensity: {}\n{}{}font-scale: {}\nframe-inset: {}\n---\n\n# {}\n\n",
        escape_line(&d.config.schema),
        escape_line(&d.config.format),
        escape_line(&d.config.theme),
        escape_line(&d.config.font),
        d.config.font_weight,
        escape_line(&d.config.density),
        accent,
        amount_in_words,
        d.config.font_scale,
        d.config.frame_inset,
        escape_line(&d.title)
    );
    for (k, v) in [
        ("Number", d.metadata.number.clone()),
        ("Kind", d.metadata.kind.clone()),
        ("Issued", d.metadata.issued.0.clone()),
        (
            "Due",
            d.metadata
                .due
                .as_ref()
                .map_or_else(String::new, |x| x.0.clone()),
        ),
        ("Terms", d.metadata.terms.clone().unwrap_or_default()),
        ("Currency", d.metadata.currency.clone()),
    ] {
        if !v.is_empty() {
            let _ = writeln!(s, "- {k}: {}", escape_line(&v));
        }
    }
    s.push('\n');
    write_party(&mut s, "From", &d.from);
    write_party(&mut s, "Bill to", &d.bill_to);
    for x in &d.ordinary_sections {
        if x.directives.page_break_before {
            s.push_str("<!-- ttyinv:page-break-before -->\n")
        }
        if x.directives.summary_only {
            s.push_str("<!-- ttyinv:summary-only -->\n")
        }
        if x.directives.gap != Gap::Standard {
            let _ = writeln!(
                s,
                "<!-- ttyinv:gap-before {} -->",
                gap_str(x.directives.gap)
            );
        };
        let _ = writeln!(s, "## {}\n", x.title);
        match &x.body {
            SectionBody::Prose(p) => {
                s.push_str(p);
                s.push_str("\n\n")
            }
            SectionBody::Table(t) => write_table(&mut s, t),
        }
    }
    if let Some(t) = &d.settlements {
        if d.settlements_page_break_before {
            s.push_str("<!-- ttyinv:page-break-before -->\n");
        }
        s.push_str("## Settlements\n\n");
        write_table(&mut s, t)
    }
    if let Some(p) = &d.payment {
        if d.payment_page_break_before {
            s.push_str("<!-- ttyinv:page-break-before -->\n");
        }
        s.push_str("## Payment\n\n");
        for m in &p.methods {
            let _ = writeln!(s, "### {}\n", escape_line(&m.title));
            for f in &m.fields {
                let _ = writeln!(s, "- {}: {}", escape_line(&f.label), escape_line(&f.value));
            }
            s.push('\n')
        }
    }
    if let Some(x) = &d.signature {
        if d.signature_page_break_before {
            s.push_str("<!-- ttyinv:page-break-before -->\n");
        }
        s.push_str("## Signature\n\n");
        if let Some(i) = &x.image {
            let _ = writeln!(s, "![{}]({})\n", escape_line(&i.alt), escape_line(&i.src));
        }
        let _ = writeln!(
            s,
            "- Name: {}\n- Label: {}",
            escape_line(&x.name),
            escape_line(&x.label)
        );
    }
    s
}
fn write_party(s: &mut String, t: &str, p: &Party) {
    let _ = writeln!(s, "## {t}\n");
    if let Some(i) = &p.logo {
        let _ = writeln!(s, "![{}]({})\n", escape_line(&i.alt), escape_line(&i.src));
    }
    let _ = writeln!(s, "- Name: {}", escape_line(&p.name));
    for x in &p.address {
        let _ = writeln!(s, "- Address: {}", escape_line(x));
    }
    if let Some(x) = &p.email {
        let _ = writeln!(s, "- Email: {}", escape_line(x));
    }
    if let Some(x) = &p.website {
        let _ = writeln!(s, "- Website: {}", escape_line(x));
    }
    for x in &p.identifiers {
        let _ = writeln!(s, "- ID.{}: {}", escape_line(&x.key), escape_line(&x.value));
    }
    s.push('\n')
}
fn write_table(s: &mut String, t: &Table) {
    s.push('|');
    for h in &t.headings {
        let _ = write!(s, " {} |", escape_table_cell(h));
    }
    s.push('\n');
    s.push('|');
    for (i, _) in t.headings.iter().enumerate() {
        let alignment = t.alignments.get(i).copied().unwrap_or_default();
        let marker = match alignment {
            TableAlignment::Left => ":---",
            TableAlignment::Center => ":---:",
            TableAlignment::Right => "---:",
            TableAlignment::None => "---",
        };
        let _ = write!(s, " {marker} |");
    }
    s.push('\n');
    for r in &t.rows {
        s.push('|');
        for c in r {
            let _ = write!(s, " {} |", escape_table_cell(c));
        }
        s.push('\n')
    }
    s.push('\n')
}
fn validate_model(d: &Document) -> Result<(), String> {
    let source = serialize_markdown(d);
    if source.len() > MAX_SOURCE_BYTES {
        return Err("structured document exceeds source size limit".into());
    }
    let mut actual =
        document(&source).map_err(|_| "structured document does not satisfy v2 grammar")?;
    let mut expected = d.clone();
    actual.source.clear();
    expected.source.clear();
    if actual != expected {
        return Err("structured document does not round-trip canonically".into());
    }
    Ok(())
}
pub(crate) fn parse_json(x: &str) -> Result<Document, serde_json::Error> {
    let d: Document = serde_json::from_str(x)?;
    validate_model(&d).map_err(|m| {
        serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, m))
    })?;
    Ok(d)
}
pub(crate) fn to_json(d: &Document) -> Result<String, serde_json::Error> {
    validate_model(d).map_err(|m| {
        serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, m))
    })?;
    serde_json::to_string_pretty(d)
}
pub(crate) fn parse_yaml(x: &str) -> Result<Document, serde_yaml::Error> {
    let d: Document = serde_yaml::from_str(x)?;
    validate_model(&d).map_err(serde_yaml::Error::custom)?;
    Ok(d)
}
pub(crate) fn to_yaml(d: &Document) -> Result<String, serde_yaml::Error> {
    validate_model(d).map_err(serde_yaml::Error::custom)?;
    serde_yaml::to_string(d)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum EditOperation {
    SetScalar { path: String, value: String },
    MoveSection { from: usize, to: usize },
    SetSectionGap { section: usize, gap: Gap },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EditRequest {
    pub source: String,
    pub base_revision: String,
    pub sequence: u64,
    pub operation: EditOperation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EditResponse {
    pub source: String,
    pub revision: String,
    pub sequence: u64,
    pub diagnostics: Vec<Diagnostic>,
    pub conflict: bool,
}
pub(crate) fn apply_edit(r: EditRequest) -> EditResponse {
    let rev = revision(&r.source);
    if r.source.len() > MAX_EDIT_BYTES {
        return EditResponse {
            source: r.source,
            revision: rev,
            sequence: r.sequence,
            diagnostics: vec![Diagnostic::error(
                "LIMIT001",
                "source exceeds edit size limit",
            )],
            conflict: false,
        };
    }
    let operation_size = match &r.operation {
        EditOperation::SetScalar { path, value } => path.len().saturating_add(value.len()),
        EditOperation::MoveSection { .. } | EditOperation::SetSectionGap { .. } => 0,
    };
    if operation_size > MAX_EDIT_BYTES
        || r.source.len().saturating_add(operation_size) > MAX_EDIT_BYTES
    {
        return EditResponse {
            source: r.source,
            revision: rev,
            sequence: r.sequence,
            diagnostics: vec![Diagnostic::error(
                "LIMIT001",
                "edit request exceeds edit size limit",
            )],
            conflict: false,
        };
    }
    if r.base_revision != rev {
        return EditResponse {
            source: r.source,
            revision: rev,
            sequence: r.sequence,
            diagnostics: vec![Diagnostic::error("CONFLICT001", "stale source revision")],
            conflict: true,
        };
    }
    let result = match r.operation {
        EditOperation::SetScalar { path, value } => set_scalar(&r.source, &path, &value),
        EditOperation::MoveSection { from, to } => move_section(&r.source, from, to),
        EditOperation::SetSectionGap { section, gap } => set_gap(&r.source, section, gap),
    };
    match result {
        Ok(source) => {
            let d = validate(&source).diagnostics().to_vec();
            EditResponse {
                revision: revision(&source),
                source,
                sequence: r.sequence,
                diagnostics: d,
                conflict: false,
            }
        }
        Err(d) => EditResponse {
            source: r.source,
            revision: rev,
            sequence: r.sequence,
            diagnostics: vec![d],
            conflict: false,
        },
    }
}
fn lines(s: &str) -> Vec<String> {
    normalized_source(s).lines().map(str::to_owned).collect()
}
fn ordinary(l: &[String]) -> Vec<ScannedBlock> {
    scan_blocks(l)
        .into_iter()
        .filter(|b| b.fixed.is_none())
        .collect()
}

fn source_line_ranges(s: &str) -> Vec<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut start = if bytes.starts_with(b"\xef\xbb\xbf") {
        3
    } else {
        0
    };
    let mut out = Vec::new();
    while start < bytes.len() {
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'\r' && bytes[end] != b'\n' {
            end += 1;
        }
        out.push((start, end));
        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if bytes[end] == b'\r' && start < bytes.len() && bytes[start] == b'\n' {
            start += 1;
        }
    }
    out
}
fn finish(s: &str, l: Vec<String>) -> String {
    let newline = if s.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = l.join(newline);
    if s.ends_with('\n') || s.ends_with('\r') {
        out.push_str(newline);
    }
    out
}
fn is_heading_line(value: &str) -> bool {
    let value = value.trim_start();
    let hashes = value.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes)
        && value[hashes..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
}

fn prose_scalar_value(value: &str) -> bool {
    if value.trim().is_empty() || value.chars().any(|c| c == '\r' || c == '\n') {
        return false;
    }
    let trimmed = value.trim_start();
    if trimmed.starts_with('|') || trimmed.starts_with("<!-- ttyinv:") {
        return false;
    }
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        return false;
    }
    if is_heading_line(trimmed)
        || trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
        || trimmed.starts_with("> ")
        || trimmed.split_once(". ").is_some_and(|(prefix, _)| {
            !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit())
        })
    {
        return false;
    }
    true
}

#[allow(clippy::result_large_err)]
fn replace_prose_scalar(
    source: &str,
    l: &[String],
    block: ScannedBlock,
    blocks: &[ScannedBlock],
    value: &str,
) -> Result<String, Diagnostic> {
    if !prose_scalar_value(value) {
        return Err(Diagnostic::error(
            "EDIT003",
            "prose value must be one paragraph without structure",
        ));
    }
    let body_end = blocks
        .iter()
        .find(|next| next.start > block.start)
        .map_or(block.end, |next| next.directive_start);
    let body_start = block.start.saturating_add(1);
    if body_start >= body_end || body_end > l.len() {
        return Err(Diagnostic::error("EDIT003", "section target is not prose"));
    }
    let body = &l[body_start..body_end];
    let Some(first) = body.iter().position(|line| !line.trim().is_empty()) else {
        return Err(Diagnostic::error("EDIT003", "section target is not prose"));
    };
    let last = body
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .expect("first nonblank line exists");
    if body[first..=last].iter().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with('|') || trimmed.starts_with("<!-- ttyinv:") || is_heading_line(trimmed)
    }) {
        return Err(Diagnostic::error("EDIT003", "section target is not prose"));
    }
    let ranges = source_line_ranges(source);
    let content_start = body_start + first;
    let content_end = body_start + last + 1;
    let (start, _) = ranges
        .get(content_start)
        .copied()
        .ok_or_else(|| Diagnostic::error("EDIT003", "section target is not prose"))?;
    let (_, end) = ranges
        .get(content_end - 1)
        .copied()
        .ok_or_else(|| Diagnostic::error("EDIT003", "section target is not prose"))?;
    let mut out = String::with_capacity(source.len() + value.len());
    out.push_str(&source[..start]);
    out.push_str(value);
    out.push_str(&source[end..]);
    Ok(out)
}
#[allow(clippy::result_large_err)]
fn move_section(s: &str, a: usize, b: usize) -> Result<String, Diagnostic> {
    let mut l = lines(s);
    let blocks = scan_blocks(&l);
    let ordinary_blocks = blocks
        .iter()
        .filter(|x| x.fixed.is_none())
        .collect::<Vec<_>>();
    if a >= ordinary_blocks.len() || b >= ordinary_blocks.len() {
        return Err(Diagnostic::error(
            "EDIT002",
            "section index is out of bounds",
        ));
    }
    if a == b {
        return Ok(s.into());
    }
    let selected = *ordinary_blocks[a];
    let selected_end = blocks
        .iter()
        .find(|x| x.start == selected.start)
        .and_then(|x| {
            blocks
                .iter()
                .find(|n| n.start > x.start)
                .map(|n| n.directive_start)
        })
        .unwrap_or(selected.end);
    let chunk = l[selected.directive_start..selected_end].to_vec();
    l.drain(selected.directive_start..selected_end);
    let blocks_after = scan_blocks(&l);
    let dest = blocks_after
        .iter()
        .filter(|x| x.fixed.is_none())
        .collect::<Vec<_>>();
    let at = if b < dest.len() {
        dest[b].directive_start
    } else {
        blocks_after
            .iter()
            .find(|x| matches!(x.fixed, Some("Settlements" | "Payment" | "Signature")))
            .map_or(l.len(), |x| x.directive_start)
    };
    l.splice(at..at, chunk);
    Ok(finish(s, l))
}
#[allow(clippy::result_large_err)]
fn set_gap(s: &str, n: usize, g: Gap) -> Result<String, Diagnostic> {
    let mut l = lines(s);
    let blocks = scan_blocks(&l);
    let ordinary_blocks = blocks
        .iter()
        .filter(|x| x.fixed.is_none())
        .collect::<Vec<_>>();
    let Some(block) = ordinary_blocks.get(n).copied() else {
        return Err(Diagnostic::error(
            "EDIT002",
            "section index is out of bounds",
        ));
    };
    let mut remove = Vec::new();
    for (i, line) in l
        .iter()
        .enumerate()
        .take(block.start)
        .skip(block.directive_start)
    {
        if matches!(parse_directive(line), Some(Directive::Gap(_))) {
            remove.push(i);
        }
    }
    for i in remove.into_iter().rev() {
        l.remove(i);
    }
    if g != Gap::Standard {
        let refreshed = scan_blocks(&l);
        let target = refreshed
            .iter()
            .filter(|x| x.fixed.is_none())
            .nth(n)
            .map_or(l.len(), |x| x.start);
        l.insert(target, format!("<!-- ttyinv:gap-before {} -->", gap_str(g)));
    }
    Ok(finish(s, l))
}
#[allow(clippy::result_large_err)]
fn replace_config_scalar(l: &mut Vec<String>, key: &str, value: &str) -> Result<(), Diagnostic> {
    let Some(end) = l
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, line)| (line == "---").then_some(index))
    else {
        return Err(Diagnostic::error(
            "EDIT004",
            "frontmatter configuration is absent",
        ));
    };
    let prefix = format!("{key}:");
    let rendered = if key == "accent" {
        format!("\"{}\"", escape_line(value))
    } else {
        escape_line(value)
    };
    for line in &mut l[1..end] {
        if line.starts_with(&prefix) {
            let label = line.split_once(':').map_or(key, |(label, _)| label);
            *line = format!("{label}: {rendered}");
            return Ok(());
        }
    }
    let insert_at = if key == "font-weight" {
        l[1..end]
            .iter()
            .position(|line| line.starts_with("font:"))
            .map_or(end, |index| index + 2)
    } else {
        end
    };
    l.insert(insert_at, format!("{key}: {rendered}"));
    Ok(())
}
#[allow(clippy::result_large_err)]
fn set_scalar(s: &str, p: &str, v: &str) -> Result<String, Diagnostic> {
    let mut l = lines(s);
    if let Some(field) = p.strip_prefix("config.") {
        let key = match field {
            "format" => "format",
            "theme" => "theme",
            "font" => "font",
            "font_weight" => "font-weight",
            "density" => "density",
            "amount_in_words" => "amount-in-words",
            "accent" => "accent",
            "font_scale" => "font-scale",
            "frame_inset" => "frame-inset",
            _ => "",
        };
        if !key.is_empty() {
            if v.is_empty() {
                if key == "accent" || key == "amount-in-words" {
                    return remove_config_scalar(&mut l, key).map(|_| finish(s, l));
                }
                return Err(Diagnostic::error(
                    "EDIT004",
                    "required config field cannot be empty",
                ));
            }
            return replace_config_scalar(&mut l, key, v).map(|_| finish(s, l));
        }
    }
    if p == "title" {
        if let Some(x) = l.iter_mut().find(|x| x.starts_with("# ")) {
            *x = format!("# {}", escape_line(v));
            return Ok(finish(s, l));
        }
    }
    if let Some(k) = p.strip_prefix("metadata.") {
        let label = match k {
            "number" => "Number",
            "kind" => "Kind",
            "issued" => "Issued",
            "due" => "Due",
            "terms" => "Terms",
            "currency" => "Currency",
            _ => "",
        };
        if !label.is_empty() {
            let metadata_end = scan_blocks(&l)
                .iter()
                .find(|block| block.fixed == Some("From"))
                .map_or(0, |block| block.start);
            if metadata_end < 1 {
                return Err(Diagnostic::error("EDIT004", "metadata field is absent"));
            }
            return replace_label(&mut l, 1, metadata_end, label, None, v).map(|_| finish(s, l));
        }
    }
    if let Some(rest) = p.strip_prefix("sections[") {
        if let Some((n, tail)) = rest.split_once(']') {
            let n: usize = n
                .parse()
                .map_err(|_| Diagnostic::error("EDIT003", "invalid section path"))?;
            let rs = ordinary(&l);
            if n >= rs.len() {
                return Err(Diagnostic::error(
                    "EDIT002",
                    "section index is out of bounds",
                ));
            }
            let block = rs[n];
            let a = block.start;
            let b = block.end;
            if tail == ".title" {
                l[a] = format!("## {}", escape_line(v));
                return Ok(finish(s, l));
            }
            if tail == ".prose" {
                let blocks = scan_blocks(&l);
                return replace_prose_scalar(s, &l, block, &blocks, v);
            }
            if let Some(x) = tail.strip_prefix(".table.headings[") {
                if let Some(j) = x.strip_suffix(']') {
                    let j: usize = j
                        .parse()
                        .map_err(|_| Diagnostic::error("EDIT003", "invalid table path"))?;
                    return replace_table_cell(&mut l, a, b, 0, j, v).map(|_| finish(s, l));
                }
            }
            if let Some(x) = tail.strip_prefix(".table.rows[") {
                if let Some((r, c)) = x.split_once("].cells[") {
                    let r: usize = r
                        .parse()
                        .map_err(|_| Diagnostic::error("EDIT003", "invalid table path"))?;
                    let c: usize = c
                        .strip_suffix(']')
                        .ok_or_else(|| Diagnostic::error("EDIT003", "invalid table path"))?
                        .parse()
                        .map_err(|_| Diagnostic::error("EDIT003", "invalid table path"))?;
                    return replace_table_cell(&mut l, a, b, r + 2, c, v).map(|_| finish(s, l));
                }
            }
        }
    }
    if let Some(rest) = p.strip_prefix("settlements.rows[") {
        if let Some((r, c)) = rest.split_once("].cells[") {
            let r: usize = r
                .parse()
                .map_err(|_| Diagnostic::error("EDIT003", "invalid settlement path"))?;
            let c: usize = c
                .strip_suffix(']')
                .ok_or_else(|| Diagnostic::error("EDIT003", "invalid settlement path"))?
                .parse()
                .map_err(|_| Diagnostic::error("EDIT003", "invalid settlement path"))?;
            let rs = fixed_range(&l, "Settlements")
                .ok_or_else(|| Diagnostic::error("EDIT004", "settlements block is absent"))?;
            return replace_table_cell(&mut l, rs.0, rs.1, r + 2, c, v).map(|_| finish(s, l));
        }
    }
    if let Some((root, tail)) = p.split_once('.') {
        if root == "from" || root == "bill_to" || root == "signature" {
            let block_name = match root {
                "from" => "From",
                "bill_to" => "Bill to",
                _ => "Signature",
            };
            let rs = fixed_range(&l, block_name)
                .ok_or_else(|| Diagnostic::error("EDIT004", "block is absent"))?;
            if tail == "name" || tail == "email" || tail == "website" || tail == "label" {
                let label = match tail {
                    "name" => "Name",
                    "email" => "Email",
                    "website" => "Website",
                    _ => "Label",
                };
                return replace_label(&mut l, rs.0 + 1, rs.1, label, None, v).map(|_| finish(s, l));
            }
            if let Some(x) = tail.strip_prefix("address[") {
                let n: usize = x
                    .strip_suffix(']')
                    .ok_or_else(|| Diagnostic::error("EDIT003", "invalid address path"))?
                    .parse()
                    .map_err(|_| Diagnostic::error("EDIT003", "invalid address path"))?;
                return replace_label(&mut l, rs.0 + 1, rs.1, "Address", Some(n), v)
                    .map(|_| finish(s, l));
            }
            if let Some(k) = tail.strip_prefix("identifiers.") {
                return replace_label(&mut l, rs.0 + 1, rs.1, &format!("ID.{k}"), None, v)
                    .map(|_| finish(s, l));
            }
            if tail == "logo.alt" || tail == "image.alt" {
                for x in &mut l[rs.0 + 1..rs.1] {
                    if let Some((_, z)) = parse_image(x) {
                        *x = format!("![{}]({})", escape_line(v), escape_line(&z));
                        return Ok(finish(s, l));
                    }
                }
            }
        }
    }
    if let Some(rest) = p.strip_prefix("payment.methods[") {
        if let Some((m, tail)) = rest.split_once("].") {
            let m: usize = m
                .parse()
                .map_err(|_| Diagnostic::error("EDIT003", "invalid payment path"))?;
            let rs = fixed_range(&l, "Payment")
                .ok_or_else(|| Diagnostic::error("EDIT004", "payment block is absent"))?;
            let hs = (rs.0 + 1..rs.1)
                .filter(|i| l[*i].starts_with("### "))
                .collect::<Vec<_>>();
            if m >= hs.len() {
                return Err(Diagnostic::error(
                    "EDIT002",
                    "payment method index is out of bounds",
                ));
            }
            let a = hs[m];
            let b = *hs.get(m + 1).unwrap_or(&rs.1);
            if tail == "title" {
                l[a] = format!("### {}", escape_line(v));
                return Ok(finish(s, l));
            }
            if let Some(k) = tail.strip_prefix("fields.") {
                return replace_label(&mut l, a + 1, b, k, None, v).map(|_| finish(s, l));
            }
        }
    }
    Err(Diagnostic::error("EDIT003", "scalar path is not editable"))
}
#[allow(clippy::result_large_err)]
fn remove_config_scalar(l: &mut Vec<String>, key: &str) -> Result<(), Diagnostic> {
    let Some(end) = l
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, line)| (line == "---").then_some(index))
    else {
        return Err(Diagnostic::error(
            "EDIT004",
            "frontmatter configuration is absent",
        ));
    };
    if let Some(index) = l[1..end]
        .iter()
        .position(|line| line.starts_with(&format!("{key}:")))
    {
        l.remove(index + 1);
    }
    Ok(())
}
#[allow(clippy::result_large_err)]
fn replace_label(
    l: &mut [String],
    a: usize,
    b: usize,
    label: &str,
    index: Option<usize>,
    v: &str,
) -> Result<(), Diagnostic> {
    let mut found = 0;
    for x in &mut l[a..b] {
        if x.starts_with(&format!("- {label}:")) {
            if index.is_none() || index == Some(found) {
                let pre = x
                    .split_once(':')
                    .map_or(format!("- {label}"), |(p, _)| p.to_owned());
                *x = format!("{pre}: {}", escape_line(v));
                return Ok(());
            }
            found += 1
        }
    }
    Err(Diagnostic::error("EDIT004", "scalar field is absent"))
}
#[allow(clippy::result_large_err)]
fn replace_table_cell(
    l: &mut [String],
    a: usize,
    b: usize,
    row: usize,
    col: usize,
    v: &str,
) -> Result<(), Diagnostic> {
    let mut n = 0;
    for x in &mut l[a..b] {
        if x.starts_with('|') {
            if n == row {
                let mut cells = split_pipe(x);
                if col >= cells.len() {
                    return Err(Diagnostic::error("EDIT004", "table cell is absent"));
                }
                cells[col] = v.into();
                *x = format!(
                    "| {} |",
                    cells
                        .iter()
                        .map(|c| escape_table_cell(c))
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
                return Ok(());
            }
            n += 1
        }
    }
    Err(Diagnostic::error("EDIT004", "table cell is absent"))
}

#[cfg(test)]
mod hash_tests {
    use super::sha256_digest;

    #[test]
    fn sha256_known_answer_vector() {
        assert_eq!(
            sha256_digest(b"abc"),
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad
            ]
        );
    }
}
