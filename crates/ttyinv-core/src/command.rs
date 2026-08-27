use crate::render::{
    font_capabilities, prepare_render, presentation, render_prepared, supported_densities,
    supported_themes, PresentationConfig, RenderAsset, RenderError, RenderOptions,
};
use crate::{
    apply_edit, document, parse_json, parse_yaml, revision, serialize_markdown, to_json, to_yaml,
    Config, Diagnostic, Document, EditOperation as OwnedEditOperation, FontWeight, Gap, Identifier,
    Image, LabelValue, Metadata, Party, Payment, PreparedRender, RenderFormat, RenderWarning,
    SectionBody, SectionDirectives, Signature, Table, TableAlignment,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

pub type Text<'a> = Cow<'a, str>;
pub type Bytes<'a> = Cow<'a, [u8]>;
pub type PlanDigest = [u8; 32];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source<'a> {
    Markdown(Text<'a>),
    Json(Text<'a>),
    Yaml(Text<'a>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalFormat {
    Markdown,
    Json,
    Yaml,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectMode {
    Structure,
    Summary,
    Manifest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderAssetInput<'a> {
    pub source: Text<'a>,
    pub bytes: Bytes<'a>,
    #[serde(default)]
    pub mime: Option<Text<'a>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderOptionsInput<'a> {
    pub format: RenderFormat,
    #[serde(default)]
    pub theme: Option<Text<'a>>,
    #[serde(default)]
    pub font: Option<Text<'a>>,
    #[serde(default)]
    pub font_weight: Option<FontWeight>,
    #[serde(default)]
    pub density: Option<Text<'a>>,
    #[serde(default)]
    pub accent: Option<Text<'a>>,
    #[serde(default)]
    pub font_scale: Option<u8>,
    #[serde(default)]
    pub frame_inset: Option<u8>,
    #[serde(default)]
    pub png_scale: Option<u8>,
    #[serde(default)]
    pub assets: Vec<RenderAssetInput<'a>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftLabelValue<'a> {
    pub label: Text<'a>,
    pub value: Text<'a>,
}

impl<'a> Default for RenderOptionsInput<'a> {
    fn default() -> Self {
        Self {
            format: RenderFormat::Html,
            theme: None,
            font: None,
            font_weight: None,
            density: None,
            accent: None,
            font_scale: None,
            frame_inset: None,
            png_scale: None,
            assets: Vec::new(),
        }
    }
}
impl<'a> From<RenderOptionsInput<'a>> for RenderOptions {
    fn from(x: RenderOptionsInput<'a>) -> Self {
        Self {
            format: x.format,
            theme: x.theme.map(Cow::into_owned),
            font: x.font.map(Cow::into_owned),
            font_weight: x.font_weight,
            density: x.density.map(Cow::into_owned),
            accent: x.accent.map(Cow::into_owned),
            font_scale: x.font_scale,
            frame_inset: x.frame_inset,
            png_scale: x.png_scale,
            assets: x
                .assets
                .into_iter()
                .map(|a| RenderAsset {
                    source: a.source.into_owned(),
                    bytes: a.bytes.into_owned(),
                    mime: a.mime.map(Cow::into_owned),
                })
                .collect(),
        }
    }
}
pub type RenderOptionsBorrowed<'a> = RenderOptionsInput<'a>;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PresentationConfigInput<'a> {
    pub theme: Text<'a>,
    pub font: Text<'a>,
    pub font_weight: crate::FontWeight,
    pub density: Text<'a>,
    pub accent: Option<Text<'a>>,
    pub font_scale: u8,
    pub frame_inset: u8,
}
impl<'a> Default for PresentationConfigInput<'a> {
    fn default() -> Self {
        let config = PresentationConfig::default();
        Self {
            theme: Cow::Owned(config.theme),
            font: Cow::Owned(config.font),
            font_weight: config.font_weight,
            density: Cow::Owned(config.density),
            accent: config.accent.map(Cow::Owned),
            font_scale: config.font_scale,
            frame_inset: config.frame_inset,
        }
    }
}
impl<'a> From<PresentationConfigInput<'a>> for PresentationConfig {
    fn from(config: PresentationConfigInput<'a>) -> Self {
        Self {
            theme: config.theme.into_owned(),
            font: config.font.into_owned(),
            font_weight: config.font_weight,
            density: config.density.into_owned(),
            accent: config.accent.map(Cow::into_owned),
            font_scale: config.font_scale,
            frame_inset: config.frame_inset,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum EditOperationInput<'a> {
    SetScalar { path: Text<'a>, value: Text<'a> },
    MoveSection { from: usize, to: usize },
    SetSectionGap { section: usize, gap: Gap },
}
impl<'a> From<EditOperationInput<'a>> for OwnedEditOperation {
    fn from(x: EditOperationInput<'a>) -> Self {
        match x {
            EditOperationInput::SetScalar { path, value } => Self::SetScalar {
                path: path.into_owned(),
                value: value.into_owned(),
            },
            EditOperationInput::MoveSection { from, to } => Self::MoveSection { from, to },
            EditOperationInput::SetSectionGap { section, gap } => {
                Self::SetSectionGap { section, gap }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftConfig<'a> {
    #[serde(default = "default_schema")]
    pub schema: Text<'a>,
    #[serde(default)]
    pub format: Option<Text<'a>>,
    #[serde(default)]
    pub theme: Option<Text<'a>>,
    #[serde(default)]
    pub font: Option<Text<'a>>,
    #[serde(default)]
    pub font_weight: Option<FontWeight>,
    #[serde(default)]
    pub density: Option<Text<'a>>,
    #[serde(default)]
    pub accent: Option<Text<'a>>,
    #[serde(default)]
    pub font_scale: Option<u8>,
    #[serde(default)]
    pub frame_inset: Option<u8>,
}
fn default_schema<'a>() -> Text<'a> {
    Cow::Borrowed("ttyinv/v2")
}
impl<'a> Default for DraftConfig<'a> {
    fn default() -> Self {
        Self {
            schema: default_schema(),
            format: None,
            theme: None,
            font: None,
            font_weight: None,
            density: None,
            accent: None,
            font_scale: None,
            frame_inset: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftMetadata<'a> {
    pub number: Text<'a>,
    pub issued: crate::Date,
    #[serde(default)]
    pub kind: Option<Text<'a>>,
    #[serde(default)]
    pub due: Option<crate::Date>,
    #[serde(default)]
    pub terms: Option<Text<'a>>,
    pub currency: Text<'a>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DraftParty<'a> {
    pub name: Text<'a>,
    #[serde(default)]
    pub address: Vec<Text<'a>>,
    #[serde(default)]
    pub email: Option<Text<'a>>,
    #[serde(default)]
    pub website: Option<Text<'a>>,
    #[serde(default)]
    pub identifiers: Vec<DraftIdentifier<'a>>,
    #[serde(default)]
    pub logo: Option<DraftImage<'a>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftIdentifier<'a> {
    pub key: Text<'a>,
    pub value: Text<'a>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftImage<'a> {
    pub alt: Text<'a>,
    pub src: Text<'a>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftTable<'a> {
    pub headings: Vec<Text<'a>>,
    #[serde(default)]
    pub alignments: Vec<TableAlignment>,
    pub rows: Vec<Vec<Text<'a>>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DraftSectionBody<'a> {
    Prose(Text<'a>),
    Table(DraftTable<'a>),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftSection<'a> {
    pub title: Text<'a>,
    pub body: DraftSectionBody<'a>,
    #[serde(default)]
    pub directives: SectionDirectives,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftPaymentMethod<'a> {
    pub title: Text<'a>,
    #[serde(default)]
    pub fields: Vec<DraftLabelValue<'a>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DraftPayment<'a> {
    #[serde(default)]
    pub methods: Vec<DraftPaymentMethod<'a>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftSignature<'a> {
    #[serde(default)]
    pub image: Option<DraftImage<'a>>,
    pub name: Text<'a>,
    pub label: Text<'a>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvoiceDraft<'a> {
    #[serde(default)]
    pub config: DraftConfig<'a>,
    pub title: Text<'a>,
    pub metadata: DraftMetadata<'a>,
    pub from: DraftParty<'a>,
    pub bill_to: DraftParty<'a>,
    #[serde(default)]
    pub ordinary_sections: Vec<DraftSection<'a>>,
    #[serde(default)]
    pub settlements: Option<DraftTable<'a>>,
    #[serde(default)]
    pub payment: Option<DraftPayment<'a>>,
    #[serde(default)]
    pub signature: Option<DraftSignature<'a>>,
}
impl<'a> InvoiceDraft<'a> {
    fn into_document(self) -> Result<Document, CommandError> {
        let cfg = self.config;
        let accent = cfg
            .accent
            .map(|x| {
                crate::Accent::new(x.into_owned()).map_err(|_| invalid_request("invalid accent"))
            })
            .transpose()?;
        let font_scale = match cfg.font_scale {
            Some(x) => {
                crate::FontScale::new(x).map_err(|_| invalid_request("invalid font-scale"))?
            }
            None => crate::FontScale::default(),
        };
        let frame_inset = match cfg.frame_inset {
            Some(x) => {
                crate::FrameInset::new(x).map_err(|_| invalid_request("invalid frame-inset"))?
            }
            None => crate::FrameInset::default(),
        };
        let d = Document {
            config: Config {
                schema: cfg.schema.into_owned(),
                format: cfg
                    .format
                    .map_or_else(crate::default_format, Cow::into_owned),
                theme: cfg.theme.map_or_else(crate::default_theme, Cow::into_owned),
                font: cfg.font.map_or_else(crate::default_font, Cow::into_owned),
                font_weight: cfg.font_weight.unwrap_or_default(),
                density: cfg
                    .density
                    .map_or_else(crate::default_density, Cow::into_owned),
                accent,
                font_scale,
                frame_inset,
            },
            title: self.title.into_owned(),
            metadata: Metadata {
                number: self.metadata.number.into_owned(),
                kind: self
                    .metadata
                    .kind
                    .map_or_else(|| "standard".into(), Cow::into_owned),
                issued: self.metadata.issued,
                due: self.metadata.due,
                terms: self.metadata.terms.map(Cow::into_owned),
                currency: self.metadata.currency.into_owned(),
            },
            from: party(self.from),
            bill_to: party(self.bill_to),
            ordinary_sections: self.ordinary_sections.into_iter().map(section).collect(),
            settlements: self.settlements.map(table),
            settlements_page_break_before: false,
            payment: self.payment.map(payment),
            payment_page_break_before: false,
            signature: self.signature.map(signature),
            signature_page_break_before: false,
            grand_total: rust_decimal::Decimal::ZERO,
            source: String::new(),
        };
        let source = serialize_markdown(&d);
        document(&source).map_err(|r| {
            error_from_diagnostics(r.diagnostics().to_vec(), CommandErrorCode::InvalidDocument)
        })
    }
}
fn party(x: DraftParty<'_>) -> Party {
    Party {
        name: x.name.into_owned(),
        address: x.address.into_iter().map(Cow::into_owned).collect(),
        email: x.email.map(Cow::into_owned),
        website: x.website.map(Cow::into_owned),
        identifiers: x
            .identifiers
            .into_iter()
            .map(|i| Identifier {
                key: i.key.into_owned(),
                value: i.value.into_owned(),
            })
            .collect(),
        logo: x.logo.map(|i| Image {
            alt: i.alt.into_owned(),
            src: i.src.into_owned(),
        }),
    }
}
fn table(x: DraftTable<'_>) -> Table {
    Table {
        headings: x.headings.into_iter().map(Cow::into_owned).collect(),
        alignments: x.alignments,
        rows: x
            .rows
            .into_iter()
            .map(|r| r.into_iter().map(Cow::into_owned).collect())
            .collect(),
    }
}
fn section(x: DraftSection<'_>) -> crate::Section {
    crate::Section {
        title: x.title.into_owned(),
        body: match x.body {
            DraftSectionBody::Prose(v) => SectionBody::Prose(v.into_owned()),
            DraftSectionBody::Table(v) => SectionBody::Table(table(v)),
        },
        directives: x.directives,
        total: None,
        span: Default::default(),
    }
}
fn payment(x: DraftPayment<'_>) -> Payment {
    Payment {
        methods: x
            .methods
            .into_iter()
            .map(|m| crate::PaymentMethod {
                title: m.title.into_owned(),
                fields: m
                    .fields
                    .into_iter()
                    .map(|f| LabelValue {
                        label: f.label.into_owned(),
                        value: f.value.into_owned(),
                    })
                    .collect(),
            })
            .collect(),
    }
}
fn signature(x: DraftSignature<'_>) -> Signature {
    Signature {
        image: x.image.map(|i| Image {
            alt: i.alt.into_owned(),
            src: i.src.into_owned(),
        }),
        name: x.name.into_owned(),
        label: x.label.into_owned(),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InvoiceCommand<'a> {
    Create {
        draft: Box<InvoiceDraft<'a>>,
    },
    Validate {
        source: Source<'a>,
    },
    Inspect {
        source: Source<'a>,
        mode: InspectMode,
    },
    Convert {
        source: Source<'a>,
        to: CanonicalFormat,
    },
    Edit {
        source: Source<'a>,
        base_revision: Text<'a>,
        operation: EditOperationInput<'a>,
    },
    PrepareRender {
        source: Source<'a>,
        options: RenderOptionsInput<'a>,
    },
    ResolvePresentation {
        config: PresentationConfigInput<'a>,
    },
    Render {
        source: Source<'a>,
        options: RenderOptionsInput<'a>,
    },
    Registry,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryClass {
    Never,
    AfterInputChange,
    Later,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandErrorCode {
    InvalidRequest,
    Limit,
    InvalidDocument,
    Conflict,
    Unsupported,
    InvalidAsset,
    Encoding,
    Font,
    Backend,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandError {
    pub code: CommandErrorCode,
    pub diagnostics: Vec<Diagnostic>,
    pub retry: RetryClass,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SafeSummary {
    pub schema: String,
    pub section_count: usize,
    pub table_count: usize,
    pub row_count: usize,
    pub has_settlements: bool,
    pub has_payment: bool,
    pub has_signature: bool,
    pub currency: String,
    pub grand_total: rust_decimal::Decimal,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SafeStructure {
    pub fixed_blocks: Vec<String>,
    pub sections: Vec<SafeSection>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SafeManifest {
    pub fixed_blocks: Vec<String>,
    pub sections: Vec<SafeSection>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeSection {
    pub index: usize,
    pub title: String,
    pub body: String,
    pub gap: Gap,
    pub page_break_before: bool,
    pub summary_only: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    Created {
        source: String,
        document: Document,
        revision: String,
    },
    Validated {
        revision: String,
        valid: bool,
        document: Option<Document>,
        diagnostics: Vec<Diagnostic>,
    },
    Inspected {
        revision: String,
        valid: bool,
        mode: InspectMode,
        structure: Option<SafeStructure>,
        summary: Option<SafeSummary>,
        manifest: Option<SafeManifest>,
        diagnostics: Vec<Diagnostic>,
    },
    Converted {
        format: CanonicalFormat,
        source: String,
        document: Document,
        revision: String,
    },
    Edited {
        source: String,
        document: Document,
        revision: String,
        diagnostics: Vec<Diagnostic>,
    },
    Prepared {
        plan: PreparedRender,
    },
    ResolvedPresentation {
        presentation: crate::Presentation,
    },
    Rendered {
        source_revision: String,
        plan_digest: PlanDigest,
        bytes: Vec<u8>,
        output_sha256: [u8; 32],
        mime: String,
        extension: String,
        pages: u32,
        width: u32,
        height: u32,
        warnings: Vec<RenderWarning>,
    },
    Registry(RegistrySnapshot),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDescriptor {
    pub id: String,
    pub description: String,
    pub input_schema: String,
    pub output_schema: String,
    pub limits: Vec<String>,
    pub formats: Vec<String>,
    pub adapters: Vec<String>,
    pub errors: Vec<CommandErrorCode>,
    pub retry: RetryClass,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrySnapshot {
    pub version: String,
    pub commands: Vec<CommandDescriptor>,
    pub document_schema: String,
    pub command_schema: String,
    pub capabilities: serde_json::Value,
}
#[cfg(test)]
const COMMAND_IDS: &[&str] = &[
    "create",
    "validate",
    "inspect",
    "convert",
    "edit",
    "prepare_render",
    "resolve_presentation",
    "render",
    "registry",
];
fn descriptors() -> Vec<CommandDescriptor> {
    fn descriptor(
        id: &str,
        description: &str,
        formats: &[&str],
        limits: &[&str],
        errors: &[CommandErrorCode],
        retry: RetryClass,
    ) -> CommandDescriptor {
        CommandDescriptor {
            id: id.into(),
            description: description.into(),
            input_schema: "ttyinv/v2/command".into(),
            output_schema: "ttyinv/v2/outcome".into(),
            limits: limits.iter().map(|value| (*value).into()).collect(),
            formats: formats.iter().map(|value| (*value).into()).collect(),
            adapters: vec![
                "native".into(),
                "cli".into(),
                "wasm".into(),
                "rest".into(),
                "mcp".into(),
                "webmcp".into(),
            ],
            errors: errors.to_vec(),
            retry,
        }
    }
    vec![
        descriptor(
            "create",
            "Create a document from a typed draft.",
            &["markdown"],
            &["source_bytes"],
            &[
                CommandErrorCode::InvalidRequest,
                CommandErrorCode::InvalidDocument,
                CommandErrorCode::Limit,
            ],
            RetryClass::AfterInputChange,
        ),
        descriptor(
            "validate",
            "Validate and canonicalize a source document.",
            &["markdown", "json", "yaml"],
            &["source_bytes"],
            &[CommandErrorCode::InvalidDocument, CommandErrorCode::Limit],
            RetryClass::AfterInputChange,
        ),
        descriptor(
            "inspect",
            "Inspect safe document structure, summary, or manifest.",
            &["markdown", "json", "yaml"],
            &["source_bytes"],
            &[CommandErrorCode::InvalidDocument, CommandErrorCode::Limit],
            RetryClass::AfterInputChange,
        ),
        descriptor(
            "convert",
            "Convert a document to a canonical format.",
            &["markdown", "json", "yaml"],
            &["source_bytes"],
            &[
                CommandErrorCode::InvalidDocument,
                CommandErrorCode::Unsupported,
                CommandErrorCode::Limit,
            ],
            RetryClass::AfterInputChange,
        ),
        descriptor(
            "edit",
            "Apply one typed edit to a document.",
            &["markdown", "json", "yaml"],
            &["source_bytes"],
            &[
                CommandErrorCode::Conflict,
                CommandErrorCode::InvalidDocument,
                CommandErrorCode::Limit,
            ],
            RetryClass::AfterInputChange,
        ),
        descriptor(
            "prepare_render",
            "Prepare a bounded render plan for preview.",
            &["html", "pdf", "png"],
            &["source_bytes", "asset_bytes", "asset_total_bytes"],
            &[
                CommandErrorCode::InvalidDocument,
                CommandErrorCode::InvalidAsset,
                CommandErrorCode::InvalidRequest,
                CommandErrorCode::Limit,
                CommandErrorCode::Font,
            ],
            RetryClass::AfterInputChange,
        ),
        descriptor(
            "resolve_presentation",
            "Resolve presentation configuration and geometry.",
            &[],
            &[],
            &[CommandErrorCode::InvalidRequest, CommandErrorCode::Font],
            RetryClass::AfterInputChange,
        ),
        descriptor(
            "render",
            "Render a source document.",
            &["html", "pdf", "png"],
            &[
                "source_bytes",
                "asset_bytes",
                "asset_total_bytes",
                "output_bytes",
            ],
            &[
                CommandErrorCode::InvalidDocument,
                CommandErrorCode::InvalidAsset,
                CommandErrorCode::InvalidRequest,
                CommandErrorCode::Limit,
                CommandErrorCode::Encoding,
                CommandErrorCode::Font,
                CommandErrorCode::Backend,
            ],
            RetryClass::AfterInputChange,
        ),
        descriptor(
            "registry",
            "Return command schemas and capabilities.",
            &[],
            &[],
            &[],
            RetryClass::Never,
        ),
    ]
}
fn command_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://ttyinv.com/schema/ttyinv-v2-command.schema.json",
        "title": "ttyinv/v2 command",
        "description": "Typed command envelope executed by the ttyinv core.",
        "oneOf": [
            {"type":"object","additionalProperties":false,"required":["kind","draft"],"properties":{"kind":{"const":"create"},"draft":{"$ref":"#/$defs/draft"}}},
            {"type":"object","additionalProperties":false,"required":["kind","source"],"properties":{"kind":{"const":"validate"},"source":{"$ref":"#/$defs/source"}}},
            {"type":"object","additionalProperties":false,"required":["kind","source","mode"],"properties":{"kind":{"const":"inspect"},"source":{"$ref":"#/$defs/source"},"mode":{"enum":["structure","summary","manifest"]}}},
            {"type":"object","additionalProperties":false,"required":["kind","source","to"],"properties":{"kind":{"const":"convert"},"source":{"$ref":"#/$defs/source"},"to":{"enum":["markdown","json","yaml"]}}},
            {"type":"object","additionalProperties":false,"required":["kind","source","base_revision","operation"],"properties":{"kind":{"const":"edit"},"source":{"$ref":"#/$defs/source"},"base_revision":{"type":"string"},"operation":{"$ref":"#/$defs/operation"}}},
            {"type":"object","additionalProperties":false,"required":["kind","source","options"],"properties":{"kind":{"const":"prepare_render"},"source":{"$ref":"#/$defs/source"},"options":{"$ref":"#/$defs/render_options"}}},
            {"type":"object","additionalProperties":false,"required":["kind","config"],"properties":{"kind":{"const":"resolve_presentation"},"config":{"$ref":"#/$defs/presentation_config"}}},
            {"type":"object","additionalProperties":false,"required":["kind","source","options"],"properties":{"kind":{"const":"render"},"source":{"$ref":"#/$defs/source"},"options":{"$ref":"#/$defs/render_options"}}},
            {"type":"object","additionalProperties":false,"required":["kind"],"properties":{"kind":{"const":"registry"}}}
        ],
        "$defs": {
            "source": {"oneOf":[
                {"type":"object","additionalProperties":false,"required":["markdown"],"properties":{"markdown":{"type":"string"}}},
                {"type":"object","additionalProperties":false,"required":["json"],"properties":{"json":{"type":"string"}}},
                {"type":"object","additionalProperties":false,"required":["yaml"],"properties":{"yaml":{"type":"string"}}}
            ]},
            "operation": {"oneOf":[
                {"type":"object","additionalProperties":false,"required":["kind","path","value"],"properties":{"kind":{"const":"set_scalar"},"path":{"type":"string"},"value":{"type":"string"}}},
                {"type":"object","additionalProperties":false,"required":["kind","from","to"],"properties":{"kind":{"const":"move_section"},"from":{"type":"integer","minimum":0},"to":{"type":"integer","minimum":0}}},
                {"type":"object","additionalProperties":false,"required":["kind","section","gap"],"properties":{"kind":{"const":"set_section_gap"},"section":{"type":"integer","minimum":0},"gap":{"enum":["none","tight","standard","roomy"]}}}
            ]},
            "render_asset": {"type":"object","additionalProperties":false,"required":["source","bytes"],"properties":{"source":{"type":"string"},"bytes":{},"mime":{"type":["string","null"]}}},
            "render_options": {"type":"object","additionalProperties":false,"required":["format"],"properties":{
                "format":{"enum":["html","pdf","png"]},"theme":{"type":["string","null"]},"font":{"type":["string","null"]},
                "font_weight":{"enum":["regular","semibold",null]},"density":{"type":["string","null"]},"accent":{"type":["string","null"]},
                "font_scale":{"type":["integer","null"],"minimum":100,"maximum":140},"frame_inset":{"type":["integer","null"],"minimum":30,"maximum":60},
                "png_scale":{"type":["integer","null"],"minimum":1,"maximum":2},"assets":{"type":"array","items":{"$ref":"#/$defs/render_asset"}}
            }},
            "presentation_config": {"type":"object","additionalProperties":false,"properties":{
                "theme":{"type":"string"},"font":{"type":"string"},"font_weight":{"enum":["regular","semibold"]},"density":{"type":"string"},
                "accent":{"type":["string","null"]},"font_scale":{"type":"integer","minimum":100,"maximum":140},"frame_inset":{"type":"integer","minimum":30,"maximum":60}
            }},
            "draft": {"type":"object","additionalProperties":false,"required":["title","metadata","from","bill_to"],"properties":{
                "config":{"$ref":"#/$defs/draft_config"},"title":{"type":"string"},"metadata":{"$ref":"#/$defs/metadata"},
                "from":{"$ref":"#/$defs/party"},"bill_to":{"$ref":"#/$defs/party"},"ordinary_sections":{"type":"array","items":{"$ref":"#/$defs/section"}},
                "settlements":{"anyOf":[{"$ref":"#/$defs/table"},{"type":"null"}]},"payment":{"anyOf":[{"$ref":"#/$defs/payment"},{"type":"null"}]},
                "signature":{"anyOf":[{"$ref":"#/$defs/signature"},{"type":"null"}]}
            }},
            "draft_config":{"type":"object","additionalProperties":false,"properties":{
                "schema":{"type":"string"},"format":{"type":["string","null"]},"theme":{"type":["string","null"]},"font":{"type":["string","null"]},
                "font_weight":{"enum":["regular","semibold",null]},"density":{"type":["string","null"]},"accent":{"type":["string","null"]},
                "font_scale":{"type":["integer","null"],"minimum":100,"maximum":140},"frame_inset":{"type":["integer","null"],"minimum":30,"maximum":60}
            }},
            "metadata":{"type":"object","additionalProperties":false,"required":["number","issued","currency"],"properties":{"number":{"type":"string"},"issued":{"type":"string"},"kind":{"type":["string","null"]},"due":{"type":["string","null"]},"terms":{"type":["string","null"]},"currency":{"type":"string"}}},
            "party":{"type":"object","additionalProperties":false,"required":["name"],"properties":{"name":{"type":"string"},"address":{"type":"array","items":{"type":"string"}},"email":{"type":["string","null"]},"website":{"type":["string","null"]},"identifiers":{"type":"array","items":{"$ref":"#/$defs/identifier"}},"logo":{"anyOf":[{"$ref":"#/$defs/image"},{"type":"null"}]}}},
            "identifier":{"type":"object","additionalProperties":false,"required":["key","value"],"properties":{"key":{"type":"string"},"value":{"type":"string"}}},
            "image":{"type":"object","additionalProperties":false,"required":["alt","src"],"properties":{"alt":{"type":"string"},"src":{"type":"string"}}},
            "table":{"type":"object","additionalProperties":false,"required":["headings","rows"],"properties":{"headings":{"type":"array","items":{"type":"string"}},"alignments":{"type":"array"},"rows":{"type":"array","items":{"type":"array","items":{"type":"string"}}}}},
            "section":{"type":"object","additionalProperties":false,"required":["title","body"],"properties":{"title":{"type":"string"},"body":{"oneOf":[{"type":"string"},{"$ref":"#/$defs/table"}]},"directives":{"$ref":"#/$defs/directives"}}},
            "directives":{"type":"object","additionalProperties":false,"required":["gap","page_break_before","summary_only"],"properties":{"gap":{"enum":["none","tight","standard","roomy"]},"page_break_before":{"type":"boolean"},"summary_only":{"type":"boolean"}}},
            "payment":{"type":"object","additionalProperties":false,"properties":{"methods":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["title"],"properties":{"title":{"type":"string"},"fields":{"type":"array","items":{"$ref":"#/$defs/label_value"}}}}}}},
            "label_value":{"type":"object","additionalProperties":false,"required":["label","value"],"properties":{"label":{"type":"string"},"value":{"type":"string"}}},
            "signature":{"type":"object","additionalProperties":false,"required":["name","label"],"properties":{"image":{"anyOf":[{"$ref":"#/$defs/image"},{"type":"null"}]},"name":{"type":"string"},"label":{"type":"string"}}}
        }
    })
}
pub(crate) fn registry() -> RegistrySnapshot {
    let commands = descriptors();
    let command_schema = command_schema();
    let capability_entries: Vec<_> = commands
        .iter()
        .map(|command| {
            serde_json::json!({
                "id": command.id, "description": command.description,
                "adapters": command.adapters, "formats": command.formats,
                "limits": command.limits, "errors": command.errors, "retry": command.retry,
            })
        })
        .collect();
    let capabilities = serde_json::json!({
        "version": "ttyinv/v2",
        "commands": capability_entries,
        "limits": {
            "max_source_bytes": crate::MAX_SOURCE_BYTES,
            "max_rendered_bytes": crate::MAX_RENDERED_BYTES,
            "max_asset_bytes": crate::MAX_ASSET_BYTES,
            "max_asset_total_bytes": crate::MAX_ASSET_TOTAL_BYTES,
            "max_pages": crate::MAX_PAGES,
            "max_png_pixels": crate::MAX_PNG_PIXELS,
            "max_png_total_pixels": crate::MAX_PNG_TOTAL_PIXELS,
        },
        "presentation": {
            "themes": supported_themes(),
            "fonts": font_capabilities().collect::<Vec<_>>(),
            "densities": supported_densities(),
            "font_scale": {"minimum": 100, "maximum": 140},
            "frame_inset": {"minimum": 30, "maximum": 60},
            "png_scale": {"minimum": 1, "maximum": 2}
        }
    });
    RegistrySnapshot {
        version: "ttyinv/v2".into(),
        commands,
        document_schema: include_str!("../schema/ttyinv-v2.schema.json").into(),
        command_schema: serde_json::to_string(&command_schema).expect("command schema"),
        capabilities,
    }
}

fn invalid_request(msg: impl Into<String>) -> CommandError {
    CommandError {
        code: CommandErrorCode::InvalidRequest,
        diagnostics: vec![Diagnostic::error("REQUEST001", msg)],
        retry: RetryClass::AfterInputChange,
    }
}
fn limit(msg: impl Into<String>) -> CommandError {
    CommandError {
        code: CommandErrorCode::Limit,
        diagnostics: vec![Diagnostic::error("LIMIT001", msg)],
        retry: RetryClass::AfterInputChange,
    }
}
fn error_from_diagnostics(diagnostics: Vec<Diagnostic>, code: CommandErrorCode) -> CommandError {
    CommandError {
        code,
        diagnostics,
        retry: RetryClass::AfterInputChange,
    }
}
fn decode(source: Source<'_>) -> Result<(Document, String), CommandError> {
    let text = match &source {
        Source::Markdown(x) | Source::Json(x) | Source::Yaml(x) => x.as_ref(),
    };
    if text.len() > crate::MAX_SOURCE_BYTES {
        return Err(limit("source exceeds source size limit"));
    }
    let d = match source {
        Source::Markdown(x) => document(x.as_ref()).map_err(|r| {
            error_from_diagnostics(r.diagnostics().to_vec(), CommandErrorCode::InvalidDocument)
        })?,
        Source::Json(x) => parse_json(x.as_ref()).map_err(|e| {
            error_from_diagnostics(
                vec![Diagnostic::error(
                    "DOCUMENT001",
                    format!("source could not be decoded as JSON: {e}"),
                )],
                CommandErrorCode::InvalidDocument,
            )
        })?,
        Source::Yaml(x) => parse_yaml(x.as_ref()).map_err(|e| {
            error_from_diagnostics(
                vec![Diagnostic::error(
                    "DOCUMENT001",
                    format!("source could not be decoded as YAML: {e}"),
                )],
                CommandErrorCode::InvalidDocument,
            )
        })?,
    };
    let canonical = serialize_markdown(&d);
    if canonical.len() > crate::MAX_SOURCE_BYTES {
        return Err(limit("canonical source exceeds source size limit"));
    }
    let mut d = d;
    d.source = canonical.clone();
    Ok((d, canonical))
}

pub fn execute(command: InvoiceCommand<'_>) -> Result<CommandOutcome, CommandError> {
    match command {
        InvoiceCommand::Registry => Ok(CommandOutcome::Registry(registry())),
        InvoiceCommand::Create { draft } => {
            let document = (*draft).into_document()?;
            let source = document.source.clone();
            Ok(CommandOutcome::Created {
                source: source.clone(),
                revision: revision(&source),
                document,
            })
        }
        InvoiceCommand::Validate { source } => {
            let (document, canonical) = decode(source)?;
            Ok(CommandOutcome::Validated {
                revision: revision(&canonical),
                valid: true,
                document: Some(document),
                diagnostics: Vec::new(),
            })
        }
        InvoiceCommand::Inspect { source, mode } => {
            let (document, canonical) = decode(source)?;
            let (structure, summary, manifest) = match mode {
                InspectMode::Structure => (Some(safe_structure(&document)), None, None),
                InspectMode::Summary => (None, Some(summary(&document)), None),
                InspectMode::Manifest => (None, None, Some(safe_manifest(&document))),
            };
            Ok(CommandOutcome::Inspected {
                revision: revision(&canonical),
                valid: true,
                mode,
                structure,
                summary,
                manifest,
                diagnostics: Vec::new(),
            })
        }
        InvoiceCommand::Convert { source, to } => {
            let (document, canonical) = decode(source)?;
            let converted = match to {
                CanonicalFormat::Markdown => canonical.clone(),
                CanonicalFormat::Json => to_json(&document).map_err(|e| {
                    invalid_request(format!("document cannot be serialized as JSON: {e}"))
                })?,
                CanonicalFormat::Yaml => to_yaml(&document).map_err(|e| {
                    invalid_request(format!("document cannot be serialized as YAML: {e}"))
                })?,
            };
            Ok(CommandOutcome::Converted {
                format: to,
                source: converted,
                document,
                revision: revision(&canonical),
            })
        }
        InvoiceCommand::Edit {
            source,
            base_revision,
            operation,
        } => {
            let (_, canonical) = decode(source)?;
            let actual = revision(&canonical);
            if base_revision.as_ref() != actual {
                return Err(CommandError {
                    code: CommandErrorCode::Conflict,
                    diagnostics: vec![Diagnostic::error("CONFLICT001", "stale source revision")],
                    retry: RetryClass::AfterInputChange,
                });
            }
            let response = apply_edit(crate::EditRequest {
                source: canonical.clone(),
                base_revision: actual.clone(),
                sequence: 0,
                operation: operation.into(),
            });
            if response.conflict {
                return Err(CommandError {
                    code: CommandErrorCode::Conflict,
                    diagnostics: response.diagnostics,
                    retry: RetryClass::AfterInputChange,
                });
            }
            if !response.diagnostics.is_empty() {
                return Err(error_from_diagnostics(
                    response.diagnostics,
                    CommandErrorCode::InvalidDocument,
                ));
            }
            let (document, edited) = decode(Source::Markdown(Cow::Owned(response.source)))?;
            Ok(CommandOutcome::Edited {
                source: edited.clone(),
                document,
                revision: revision(&edited),
                diagnostics: Vec::new(),
            })
        }
        InvoiceCommand::PrepareRender { source, options } => {
            let (document, canonical) = decode(source)?;
            let options: RenderOptions = options.into();
            let mut plan = prepare_render(&document, options).map_err(render_error)?;
            plan.source_revision = revision(&canonical);
            plan.plan_digest = plan.computed_digest();
            Ok(CommandOutcome::Prepared { plan })
        }
        InvoiceCommand::ResolvePresentation { config } => {
            let presentation = presentation(config.into()).map_err(render_error)?;
            Ok(CommandOutcome::ResolvedPresentation { presentation })
        }
        InvoiceCommand::Render { source, options } => {
            let (document, canonical) = decode(source)?;
            let options: RenderOptions = options.into();
            let mut plan = prepare_render(&document, options).map_err(render_error)?;
            plan.source_revision = revision(&canonical);
            plan.plan_digest = plan.computed_digest();
            let result = render_prepared(&plan).map_err(render_error)?;
            Ok(CommandOutcome::Rendered {
                source_revision: plan.source_revision,
                plan_digest: plan.plan_digest,
                output_sha256: crate::sha256_digest(&result.bytes),
                bytes: result.bytes,
                mime: result.mime,
                extension: result.extension,
                pages: result.pages as u32,
                width: result.width,
                height: result.height,
                warnings: result.warnings,
            })
        }
    }
}

fn render_error(e: RenderError) -> CommandError {
    let (code, diag_code) = match e {
        #[cfg(test)]
        RenderError::SourceTooLarge { .. } => (CommandErrorCode::Limit, "LIMIT001"),
        #[cfg(test)]
        RenderError::InvalidDocument(d) => {
            return error_from_diagnostics(d, CommandErrorCode::InvalidDocument);
        }
        RenderError::OutputTooLarge { .. } => (CommandErrorCode::Limit, "LIMIT001"),
        RenderError::UnsupportedTheme(_)
        | RenderError::UnsupportedFont(_)
        | RenderError::UnsupportedDensity(_)
        | RenderError::InvalidAccent(_)
        | RenderError::InvalidOption(_) => (CommandErrorCode::InvalidRequest, "REQUEST001"),
        RenderError::InvalidAsset(_) => (CommandErrorCode::InvalidAsset, "ASSET001"),
        RenderError::Encoding(_) => (CommandErrorCode::Encoding, "ENCODING001"),
        RenderError::Font(_) => (CommandErrorCode::Font, "FONT001"),
        RenderError::Backend(_) => (CommandErrorCode::Backend, "BACKEND001"),
    };
    error_from_diagnostics(
        vec![Diagnostic::error(diag_code, "render operation failed")],
        code,
    )
}
fn summary(d: &Document) -> SafeSummary {
    SafeSummary {
        schema: d.config.schema.clone(),
        section_count: d.ordinary_sections.len(),
        table_count: d
            .ordinary_sections
            .iter()
            .filter(|s| matches!(&s.body, SectionBody::Table(_)))
            .count(),
        row_count: d
            .ordinary_sections
            .iter()
            .map(|s| match &s.body {
                SectionBody::Table(t) => t.rows.len(),
                SectionBody::Prose(_) => 0,
            })
            .sum(),
        has_settlements: d.settlements.is_some(),
        has_payment: d.payment.is_some(),
        has_signature: d.signature.is_some(),
        currency: d.metadata.currency.clone(),
        grand_total: d.grand_total,
    }
}
fn safe_section(i: usize, s: &crate::Section) -> SafeSection {
    SafeSection {
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
    }
}
fn safe_structure(d: &Document) -> SafeStructure {
    SafeStructure {
        fixed_blocks: d
            .structure_manifest()
            .fixed_blocks
            .into_iter()
            .map(|x| x.name)
            .collect(),
        sections: d
            .ordinary_sections
            .iter()
            .enumerate()
            .map(|(i, s)| safe_section(i, s))
            .collect(),
    }
}
fn safe_manifest(d: &Document) -> SafeManifest {
    SafeManifest {
        fixed_blocks: d
            .structure_manifest()
            .fixed_blocks
            .into_iter()
            .map(|x| x.name)
            .collect(),
        sections: d
            .ordinary_sections
            .iter()
            .enumerate()
            .map(|(i, s)| safe_section(i, s))
            .collect(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../examples/simple.md");

    #[test]
    fn command_source_formats_share_canonical_revision() {
        let markdown = execute(InvoiceCommand::Validate {
            source: Source::Markdown(Cow::Borrowed(SAMPLE)),
        })
        .expect("markdown validates");
        let json_source = to_json(&document(SAMPLE).expect("sample")).expect("json");
        let json = execute(InvoiceCommand::Validate {
            source: Source::Json(Cow::Owned(json_source)),
        })
        .expect("json validates");
        let (a, b) = match (markdown, json) {
            (
                CommandOutcome::Validated { revision: a, .. },
                CommandOutcome::Validated { revision: b, .. },
            ) => (a, b),
            _ => panic!("wrong outcome"),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn inspect_modes_return_distinct_payloads() {
        let outcomes = [
            InspectMode::Structure,
            InspectMode::Summary,
            InspectMode::Manifest,
        ]
        .map(|mode| {
            execute(InvoiceCommand::Inspect {
                source: Source::Markdown(Cow::Borrowed(SAMPLE)),
                mode,
            })
            .expect("sample inspects")
        });
        assert!(matches!(
            &outcomes[0],
            CommandOutcome::Inspected {
                structure: Some(_),
                summary: None,
                manifest: None,
                ..
            }
        ));
        assert!(matches!(
            &outcomes[1],
            CommandOutcome::Inspected {
                structure: None,
                summary: Some(_),
                manifest: None,
                ..
            }
        ));
        assert!(matches!(
            &outcomes[2],
            CommandOutcome::Inspected {
                structure: None,
                summary: None,
                manifest: Some(_),
                ..
            }
        ));
        let encoded = serde_json::to_string(&outcomes[0]).expect("outcome serializes");
        assert!(encoded.contains("\"title\":\""));
        assert!(encoded.contains("\"structure\":"));
    }

    #[test]
    fn structured_decode_errors_are_document_errors() {
        let error = execute(InvoiceCommand::Validate {
            source: Source::Json(Cow::Borrowed("{")),
        })
        .expect_err("malformed JSON");
        assert_eq!(error.code, CommandErrorCode::InvalidDocument);
        assert_eq!(error.diagnostics[0].code, "DOCUMENT001");
    }

    #[test]
    fn input_structs_reject_unknown_fields() {
        let result = serde_json::from_str::<RenderOptionsInput<'_>>(
            r#"{"format":"html","unexpected":true}"#,
        );
        assert!(result.is_err());
        let result = serde_json::from_str::<DraftLabelValue<'_>>(
            r#"{"label":"x","value":"y","unexpected":true}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn stale_edit_is_a_stable_conflict() {
        let result = execute(InvoiceCommand::Edit {
            source: Source::Markdown(Cow::Borrowed(SAMPLE)),
            base_revision: Cow::Borrowed("stale"),
            operation: EditOperationInput::MoveSection { from: 0, to: 1 },
        })
        .expect_err("stale edit must fail");
        assert_eq!(result.code, CommandErrorCode::Conflict);
        assert_eq!(result.diagnostics[0].code, "CONFLICT001");
        assert_eq!(result.retry, RetryClass::AfterInputChange);
    }

    #[test]
    fn create_and_convert_are_typed_end_to_end() {
        let draft = InvoiceDraft {
            config: DraftConfig::default(),
            title: Cow::Borrowed("Typed invoice"),
            metadata: DraftMetadata {
                number: Cow::Borrowed("INV-1"),
                issued: crate::Date("2026-01-15".into()),
                kind: None,
                due: None,
                terms: None,
                currency: Cow::Borrowed("EUR"),
            },
            from: DraftParty {
                name: Cow::Borrowed("Seller"),
                ..Default::default()
            },
            bill_to: DraftParty {
                name: Cow::Borrowed("Buyer"),
                ..Default::default()
            },
            ordinary_sections: vec![DraftSection {
                title: Cow::Borrowed("Work"),
                body: DraftSectionBody::Prose(Cow::Borrowed("Description")),
                directives: SectionDirectives::default(),
            }],
            settlements: None,
            payment: None,
            signature: None,
        };
        let created = execute(InvoiceCommand::Create {
            draft: Box::new(draft),
        })
        .expect("create");
        let source = match created {
            CommandOutcome::Created {
                source,
                document,
                revision,
            } => {
                assert_eq!(revision, crate::revision(&source));
                assert_eq!(source, document.source);
                assert!(!source.is_empty());
                source
            }
            _ => panic!("wrong outcome"),
        };
        let converted = execute(InvoiceCommand::Convert {
            source: Source::Markdown(Cow::Owned(source)),
            to: CanonicalFormat::Yaml,
        })
        .expect("convert");
        match converted {
            CommandOutcome::Converted {
                format: CanonicalFormat::Yaml,
                source,
                ..
            } => {
                assert!(source.contains("schema: ttyinv/v2"));
            }
            _ => panic!("wrong outcome"),
        }
    }

    #[test]
    fn limits_and_render_options_use_stable_errors() {
        let oversized = "x".repeat(crate::MAX_SOURCE_BYTES + 1);
        let error = execute(InvoiceCommand::Validate {
            source: Source::Markdown(Cow::Owned(oversized)),
        })
        .expect_err("oversize");
        assert_eq!(error.code, CommandErrorCode::Limit);
        assert_eq!(error.diagnostics[0].code, "LIMIT001");
        let error = execute(InvoiceCommand::Render {
            source: Source::Markdown(Cow::Borrowed(SAMPLE)),
            options: RenderOptionsInput {
                format: RenderFormat::Html,
                theme: Some(Cow::Borrowed("missing")),
                ..Default::default()
            },
        })
        .expect_err("unknown theme");
        assert_eq!(error.code, CommandErrorCode::InvalidRequest);
        assert_eq!(error.diagnostics[0].code, "REQUEST001");
    }

    #[test]
    fn aggregate_asset_limit_includes_unreferenced_assets() {
        let assets = (0..9)
            .map(|index| RenderAssetInput {
                source: Cow::Owned(format!("unused-{index}")),
                bytes: Cow::Owned(vec![0; crate::MAX_ASSET_BYTES]),
                mime: None,
            })
            .collect();
        let error = execute(InvoiceCommand::PrepareRender {
            source: Source::Markdown(Cow::Borrowed(SAMPLE)),
            options: RenderOptionsInput {
                format: RenderFormat::Html,
                assets,
                ..Default::default()
            },
        })
        .expect_err("aggregate asset limit");
        assert_eq!(error.code, CommandErrorCode::InvalidAsset);
    }

    #[test]
    fn registry_is_deterministic_and_complete() {
        let first = registry();
        let second = registry();
        assert_eq!(first, second);
        assert_eq!(first.commands.len(), COMMAND_IDS.len());
        assert_eq!(
            first.document_schema,
            include_str!("../schema/ttyinv-v2.schema.json")
        );
        let command_schema: serde_json::Value =
            serde_json::from_str(&first.command_schema).expect("command schema JSON");
        assert_eq!(
            command_schema["oneOf"].as_array().map(Vec::len),
            Some(COMMAND_IDS.len())
        );
        let variants = command_schema["oneOf"]
            .as_array()
            .expect("command variants");
        let kinds: Vec<&str> = variants
            .iter()
            .filter_map(|variant| variant["properties"]["kind"]["const"].as_str())
            .collect();
        assert_eq!(kinds, COMMAND_IDS);
        let inspect = variants
            .iter()
            .find(|variant| variant["properties"]["kind"]["const"] == "inspect")
            .expect("inspect schema");
        assert_eq!(
            inspect["properties"]["mode"]["enum"],
            serde_json::json!(["structure", "summary", "manifest"])
        );
        for id in COMMAND_IDS {
            assert!(first.command_schema.contains(id), "{id}");
            assert!(first.capabilities["commands"]
                .as_array()
                .expect("command capabilities")
                .iter()
                .any(|entry| entry["id"] == *id));
        }
    }

    #[test]
    fn resolve_presentation_matches_authoritative_policy() {
        let config = PresentationConfigInput {
            theme: Cow::Borrowed("midnight"),
            accent: Some(Cow::Borrowed("#0f766e")),
            ..Default::default()
        };
        let expected = presentation(config.clone().into()).expect("presentation");
        let outcome =
            execute(InvoiceCommand::ResolvePresentation { config }).expect("resolve presentation");
        assert_eq!(
            outcome,
            CommandOutcome::ResolvedPresentation {
                presentation: expected
            }
        );
    }

    #[test]
    fn resolve_presentation_maps_invalid_options_to_stable_error() {
        let error = execute(InvoiceCommand::ResolvePresentation {
            config: PresentationConfigInput {
                font_scale: 99,
                ..Default::default()
            },
        })
        .expect_err("invalid font scale");
        assert_eq!(error.code, CommandErrorCode::InvalidRequest);
        assert_eq!(error.diagnostics[0].code, "REQUEST001");
        assert_eq!(error.retry, RetryClass::AfterInputChange);
    }
}
