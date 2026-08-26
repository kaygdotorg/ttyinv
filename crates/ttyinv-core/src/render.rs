use crate::{
    currency_exponent, document, Config, Diagnostic, Document, Section, SectionBody, TableAlignment,
};
use fontdue::Font as RasterFont;
use image::{ImageFormat, ImageReader, Limits};
use krilla::{
    action::{Action, LinkAction},
    annotation::{Annotation, LinkAnnotation, Target},
    color::rgb,
    geom::{PathBuilder, Point, Rect, Size, Transform},
    metadata::Metadata as PdfMetadata,
    num::NormalizedF32,
    page::PageSettings,
    paint::{Fill, FillRule, Stroke, StrokeDash},
    text::{Font as PdfFont, GlyphId as PdfGlyphId, KrillaGlyph},
    Document as PdfDocument,
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use std::{fmt, io::Cursor, sync::Arc};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/render_fonts.rs"));
}
mod krilla {
    pub use krilla::image::Image;
    pub use krilla::*;
}

pub const PAGE_WIDTH: u32 = 595;
pub const PAGE_HEIGHT: u32 = 842;
pub const MAX_PAGES: usize = 64;
const MAX_EXPANDED_ROWS: usize = MAX_PAGES * 64;
pub const MAX_RENDERED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_PNG_PIXELS: usize = 8_000_000;
pub const MAX_ASSET_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RenderFormat {
    #[default]
    Html,
    Pdf,
    Png,
}
impl RenderFormat {
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Html => "text/html; charset=utf-8",
            Self::Pdf => "application/pdf",
            Self::Png => "image/png",
        }
    }
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
            Self::Png => "png",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderAsset {
    pub source: String,
    pub bytes: Vec<u8>,
    #[serde(default)]
    pub mime: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenderOptions {
    pub format: RenderFormat,
    pub theme: Option<String>,
    pub font: Option<String>,
    pub font_weight: Option<crate::FontWeight>,
    pub density: Option<String>,
    pub accent: Option<String>,
    pub font_scale: Option<u8>,
    pub frame_inset: Option<u8>,
    pub assets: Vec<RenderAsset>,
}
impl Default for RenderOptions {
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
            assets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderWarning {
    pub code: String,
    pub message: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderResult {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub extension: String,
    pub pages: usize,
    pub width: u32,
    pub height: u32,
    pub warnings: Vec<RenderWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    SourceTooLarge { limit: usize },
    InvalidDocument(Vec<Diagnostic>),
    UnsupportedTheme(String),
    UnsupportedFont(String),
    UnsupportedDensity(String),
    InvalidAccent(String),
    InvalidOption(String),
    InvalidAsset(String),
    OutputTooLarge { limit: usize },
    Encoding(String),
    Font(String),
    Backend(String),
}
impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLarge { limit } => {
                write!(f, "source exceeds render limit ({limit} bytes)")
            }
            Self::InvalidDocument(_) => f.write_str("document is invalid"),
            Self::UnsupportedTheme(v) => write!(f, "unsupported theme: {v}"),
            Self::UnsupportedFont(v) => write!(f, "unsupported font: {v}"),
            Self::UnsupportedDensity(v) => write!(f, "unsupported density: {v}"),
            Self::InvalidAccent(v) => write!(f, "invalid accent: {v}"),
            Self::InvalidOption(v) => write!(f, "invalid render option: {v}"),
            Self::InvalidAsset(v) => write!(f, "invalid asset: {v}"),
            Self::OutputTooLarge { limit } => {
                write!(f, "rendered output exceeds limit ({limit} bytes)")
            }
            Self::Encoding(v) => write!(f, "render encoding failed: {v}"),
            Self::Font(v) => write!(f, "font error: {v}"),
            Self::Backend(v) => write!(f, "render backend failed: {v}"),
        }
    }
}
impl std::error::Error for RenderError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThemeTokens {
    pub paper: [u8; 3],
    pub ink: [u8; 3],
    pub muted: [u8; 3],
    pub rule: [u8; 3],
    pub accent: [u8; 3],
}
const THEMES: &[(&str, ThemeTokens)] = &[
    (
        "printable",
        ThemeTokens {
            paper: [255, 255, 255],
            ink: [25, 32, 42],
            muted: [91, 99, 110],
            rule: [109, 117, 126],
            accent: [47, 111, 237],
        },
    ),
    (
        "paper-white",
        ThemeTokens {
            paper: [255, 255, 255],
            ink: [21, 27, 35],
            muted: [90, 96, 105],
            rule: [124, 129, 136],
            accent: [31, 94, 184],
        },
    ),
    (
        "graphite",
        ThemeTokens {
            paper: [245, 246, 247],
            ink: [29, 33, 38],
            muted: [86, 92, 99],
            rule: [100, 106, 113],
            accent: [74, 84, 96],
        },
    ),
    (
        "blueprint",
        ThemeTokens {
            paper: [232, 241, 249],
            ink: [14, 47, 77],
            muted: [55, 91, 121],
            rule: [76, 120, 157],
            accent: [12, 91, 148],
        },
    ),
    (
        "ledger-pad",
        ThemeTokens {
            paper: [247, 250, 235],
            ink: [42, 54, 37],
            muted: [91, 106, 77],
            rule: [126, 143, 103],
            accent: [72, 112, 45],
        },
    ),
    (
        "solarized-light",
        ThemeTokens {
            paper: [253, 246, 227],
            ink: [38, 59, 64],
            muted: [101, 123, 125],
            rule: [147, 161, 161],
            accent: [38, 139, 210],
        },
    ),
    (
        "parchment",
        ThemeTokens {
            paper: [247, 239, 218],
            ink: [67, fifty(), 36],
            muted: [123, 106, 79],
            rule: [163, 141, 105],
            accent: [154, 87, 44],
        },
    ),
    (
        "midnight",
        ThemeTokens {
            paper: [20, 24, 31],
            ink: [235, 239, 244],
            muted: [157, 168, 181],
            rule: [89, 101, 116],
            accent: [112, 170, 255],
        },
    ),
    (
        "nord",
        ThemeTokens {
            paper: [46, 52, 64],
            ink: [236, 239, 244],
            muted: [170, 181, 194],
            rule: [93, 107, 126],
            accent: [136, 192, 208],
        },
    ),
    (
        "gruvbox-dark",
        ThemeTokens {
            paper: [40, 40, 40],
            ink: [235, 219, 178],
            muted: [168, 153, 132],
            rule: [124, 111, 100],
            accent: [254, 128, 25],
        },
    ),
];
const fn fifty() -> u8 {
    50
}
const DENSITIES: &[&str] = &["comfortable", "compact"];
pub fn theme_tokens(id: &str) -> Option<ThemeTokens> {
    THEMES
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, tokens)| *tokens)
}
pub fn supported_themes() -> &'static [&'static str] {
    const IDS: &[&str] = &[
        "printable",
        "paper-white",
        "graphite",
        "blueprint",
        "ledger-pad",
        "solarized-light",
        "parchment",
        "midnight",
        "nord",
        "gruvbox-dark",
    ];
    IDS
}
pub fn supported_fonts() -> &'static [&'static str] {
    generated::FONT_IDS
}

#[derive(Clone)]
struct Resolved {
    format: RenderFormat,
    tokens: ThemeTokens,
    font: String,
    accent: [u8; 3],
    font_scale: u8,
    line_advance: f32,
    frame_inset: u8,
    font_bytes: &'static [u8],
    semibold_bytes: &'static [u8],
    font_weight_number: u16,
    raster: RasterFont,
    semibold_raster: RasterFont,
    assets: Vec<ResolvedAsset>,
}
#[derive(Clone)]
struct ResolvedAsset {
    source: String,
    bytes: Arc<[u8]>,
    mime: Option<String>,
}
#[derive(Clone)]
struct ImageItem {
    alt: String,
    mime: String,
    bytes: Arc<[u8]>,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    x: f32,
    y: f32,
    display_width: f32,
    display_height: f32,
}
#[derive(Clone)]
struct TextRow {
    text: String,
    runs: Vec<InlineRun>,
    x: f32,
    width: f32,
    link: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
enum InlineKind {
    Text,
    Emphasis,
    Strong,
    EmphasisStrong,
    Code,
    Link(String),
    ListMarker,
    Break,
}
#[derive(Clone)]
struct InlineRun {
    kind: InlineKind,
    text: String,
}
#[derive(Clone)]
struct TableRow {
    cells: Vec<String>,
    widths: Vec<f32>,
    alignments: Vec<TableAlignment>,
}
#[derive(Clone)]
enum Block {
    Title {
        title: String,
        rows: Vec<TextRow>,
        gap: u8,
    },
    Text {
        title: String,
        rows: Vec<TextRow>,
        gap: u8,
    },
    Table {
        title: String,
        headings: Vec<String>,
        rows: Vec<TableRow>,
        gap: u8,
    },
    Images(Vec<ImageItem>),
    Total(String),
    PageBreak,
}
#[derive(Clone)]
struct Page {
    blocks: Vec<Block>,
}
#[derive(Clone)]
struct Plan {
    resolved: Resolved,
    pages: Vec<Page>,
    links: Vec<LinkBox>,
}
#[derive(Clone)]
struct LinkBox {
    href: String,
    label: String,
    page: usize,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

pub fn render(source: &str, options: RenderOptions) -> Result<RenderResult, RenderError> {
    if source.len() > crate::MAX_SOURCE_BYTES {
        return Err(RenderError::SourceTooLarge {
            limit: crate::MAX_SOURCE_BYTES,
        });
    }
    let doc =
        document(source).map_err(|r| RenderError::InvalidDocument(r.diagnostics().to_vec()))?;
    render_document(&doc, options)
}
pub fn render_document(
    doc: &Document,
    options: RenderOptions,
) -> Result<RenderResult, RenderError> {
    let resolved = resolve(&doc.config, options)?;
    let plan = layout(doc, resolved)?;
    let (bytes, width, height) = match plan.resolved.format {
        RenderFormat::Html => (
            encode_html(&plan)?,
            PAGE_WIDTH,
            PAGE_HEIGHT.saturating_mul(plan.pages.len() as u32),
        ),
        RenderFormat::Pdf => (encode_pdf(&plan)?, PAGE_WIDTH, PAGE_HEIGHT),
        RenderFormat::Png => encode_png(&plan)?,
    };
    if bytes.len() > MAX_RENDERED_BYTES {
        return Err(RenderError::OutputTooLarge {
            limit: MAX_RENDERED_BYTES,
        });
    }
    Ok(RenderResult {
        bytes,
        mime: plan.resolved.format.mime().into(),
        extension: plan.resolved.format.extension().into(),
        pages: plan.pages.len(),
        width,
        height,
        warnings: Vec::new(),
    })
}

fn parse_rgb(value: &str) -> Result<[u8; 3], RenderError> {
    if value.len() != 7
        || !value.starts_with('#')
        || !value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return Err(RenderError::InvalidAccent(value.into()));
    }
    Ok([
        u8::from_str_radix(&value[1..3], 16).unwrap(),
        u8::from_str_radix(&value[3..5], 16).unwrap(),
        u8::from_str_radix(&value[5..7], 16).unwrap(),
    ])
}
fn resolve(c: &Config, o: RenderOptions) -> Result<Resolved, RenderError> {
    let theme = o.theme.unwrap_or_else(|| c.theme.clone());
    let tokens =
        theme_tokens(&theme).ok_or_else(|| RenderError::UnsupportedTheme(theme.clone()))?;
    let font = o.font.unwrap_or_else(|| c.font.clone());
    let asset = generated::FONT_ASSETS
        .iter()
        .find(|a| a.id == font)
        .ok_or_else(|| RenderError::UnsupportedFont(font.clone()))?;
    let density = o.density.unwrap_or_else(|| c.density.clone());
    if !DENSITIES.contains(&density.as_str()) {
        return Err(RenderError::UnsupportedDensity(density));
    }
    let accent = o
        .accent
        .or_else(|| c.accent.as_ref().map(ToString::to_string))
        .unwrap_or_else(|| {
            format!(
                "#{:02x}{:02x}{:02x}",
                tokens.accent[0], tokens.accent[1], tokens.accent[2]
            )
        });
    let accent = parse_rgb(&accent)?;
    let font_scale = o.font_scale.unwrap_or_else(|| c.font_scale.value());
    if !(100..=140).contains(&font_scale) {
        return Err(RenderError::InvalidOption(
            "font-scale must be 100..=140".into(),
        ));
    }
    let frame_inset = o.frame_inset.unwrap_or_else(|| c.frame_inset.value());
    if !(30..=60).contains(&frame_inset) {
        return Err(RenderError::InvalidOption(
            "frame-inset must be 30..=60".into(),
        ));
    }
    let weight = o.font_weight.unwrap_or(c.font_weight);
    let (bytes, number) = match weight {
        crate::FontWeight::Regular => (asset.regular, asset.regular_weight),
        crate::FontWeight::Semibold => (asset.semibold, asset.semibold_weight),
    };
    let semibold_bytes = asset.semibold;
    let raster = RasterFont::from_bytes(bytes, fontdue::FontSettings::default())
        .map_err(|e| RenderError::Font(format!("{e:?}")))?;
    let semibold_raster = RasterFont::from_bytes(semibold_bytes, fontdue::FontSettings::default())
        .map_err(|e| RenderError::Font(format!("{e:?}")))?;
    let assets = o
        .assets
        .into_iter()
        .map(|asset| ResolvedAsset {
            source: asset.source,
            bytes: Arc::from(asset.bytes),
            mime: asset.mime,
        })
        .collect();
    let line_advance =
        if density == "compact" { 14.0 } else { 18.0 } * f32::from(font_scale) / 100.0;
    Ok(Resolved {
        format: o.format,
        tokens,
        font,
        accent,
        font_scale,
        line_advance,
        frame_inset,
        font_bytes: bytes,
        semibold_bytes,
        font_weight_number: number,
        raster,
        semibold_raster,
        assets,
    })
}

fn layout(doc: &Document, resolved: Resolved) -> Result<Plan, RenderError> {
    let mut blocks = Vec::new();
    let mut links = Vec::new();
    let mut image_budget = 0usize;
    blocks.push(Block::Title {
        title: doc.title.clone(),
        rows: vec![
            TextRow {
                text: format!(
                    "Number: {}    Kind: {}",
                    doc.metadata.number, doc.metadata.kind
                ),
                runs: Vec::new(),
                x: 0.,
                width: 0.,
                link: None,
            },
            TextRow {
                text: format!(
                    "Issued: {}{}    Currency: {}",
                    doc.metadata.issued,
                    doc.metadata
                        .due
                        .as_ref()
                        .map(|d| format!("    Due: {d}"))
                        .unwrap_or_default(),
                    doc.metadata.currency
                ),
                runs: Vec::new(),
                x: 0.,
                width: 0.,
                link: None,
            },
            TextRow {
                text: doc
                    .metadata
                    .terms
                    .as_ref()
                    .map(|x| format!("Terms: {x}"))
                    .unwrap_or_default(),
                runs: Vec::new(),
                x: 0.,
                width: 0.,
                link: None,
            },
        ],
        gap: 1,
    });
    if let Some(image) = party_block(
        "From",
        &doc.from,
        &mut blocks,
        &resolved.assets,
        &mut image_budget,
    )? {
        blocks.push(Block::Images(vec![image]));
    }
    if let Some(image) = party_block(
        "Bill to",
        &doc.bill_to,
        &mut blocks,
        &resolved.assets,
        &mut image_budget,
    )? {
        blocks.push(Block::Images(vec![image]));
    }
    for section in &doc.ordinary_sections {
        section_block(
            section,
            doc,
            &mut blocks,
            &resolved.assets,
            &mut image_budget,
        )?;
    }
    blocks.push(Block::Total(format!(
        "TOTAL: {} {}",
        format_money(doc.grand_total, &doc.metadata.currency, &doc.config.format),
        doc.metadata.currency
    )));
    if let Some(t) = &doc.settlements {
        if doc.settlements_page_break_before {
            blocks.push(Block::PageBreak);
        }
        blocks.push(table_block("Settlements", t, doc, None, 1));
    }
    if let Some(payment) = &doc.payment {
        if doc.payment_page_break_before {
            blocks.push(Block::PageBreak);
        }
        let rows = payment
            .methods
            .iter()
            .flat_map(|m| {
                std::iter::once(TextRow {
                    text: m.title.clone(),
                    runs: Vec::new(),
                    x: 0.,
                    width: 0.,
                    link: None,
                })
                .chain(m.fields.iter().map(|f| TextRow {
                    text: format!("{}: {}", f.label, f.value),
                    runs: Vec::new(),
                    x: 0.,
                    width: 0.,
                    link: None,
                }))
            })
            .collect();
        blocks.push(Block::Text {
            title: "Payment".into(),
            rows,
            gap: 1,
        });
    }
    if let Some(sig) = &doc.signature {
        if doc.signature_page_break_before {
            blocks.push(Block::PageBreak);
        }
        let mut rows = vec![TextRow {
            text: format!("{} — {}", sig.name, sig.label),
            runs: Vec::new(),
            x: 0.,
            width: 0.,
            link: None,
        }];
        let signature_image = if let Some(i) = &sig.image {
            if let Some(image) = decode_asset(i, &resolved.assets, &mut image_budget)? {
                Some(image)
            } else {
                rows.push(TextRow {
                    text: i.alt.clone(),
                    runs: Vec::new(),
                    x: 0.,
                    width: 0.,
                    link: safe_http_url(&i.src),
                });
                None
            }
        } else {
            None
        };
        blocks.push(Block::Text {
            title: "Signature".into(),
            rows,
            gap: 1,
        });
        if let Some(image) = signature_image {
            blocks.push(Block::Images(vec![image]));
        }
    }
    let line_height = resolved.line_advance;
    let page_budget = PAGE_HEIGHT as f32 - 2.0 * f32::from(resolved.frame_inset) - line_height;
    let mut pages = vec![Page { blocks: Vec::new() }];
    let mut used = 0.0f32;
    for block in blocks {
        if matches!(&block, Block::PageBreak) {
            if !pages.last().unwrap().blocks.is_empty() {
                pages.push(Page { blocks: Vec::new() });
                used = 0.0;
            }
            continue;
        }
        let title_block = matches!(&block, Block::Title { .. });
        match block {
            Block::Table {
                title,
                headings,
                rows,
                gap,
            } => {
                let mut chunk = Vec::new();
                for row in rows {
                    let mut candidate = chunk.clone();
                    candidate.push(row.clone());
                    let table = Block::Table {
                        title: title.clone(),
                        headings: headings.clone(),
                        rows: candidate,
                        gap,
                    };
                    if !chunk.is_empty()
                        && used + block_height(&table, &resolved, line_height) > page_budget
                    {
                        let table = Block::Table {
                            title: title.clone(),
                            headings: headings.clone(),
                            rows: std::mem::take(&mut chunk),
                            gap,
                        };
                        pages.last_mut().unwrap().blocks.push(table);
                        pages.push(Page { blocks: Vec::new() });
                        used = 0.0;
                    }
                    chunk.push(row);
                }
                if !chunk.is_empty() {
                    let table = Block::Table {
                        title,
                        headings,
                        rows: chunk,
                        gap,
                    };
                    let height = block_height(&table, &resolved, line_height);
                    if used > 0.0 && used + height > page_budget {
                        pages.push(Page { blocks: Vec::new() });
                        used = 0.0;
                    }
                    used += height;
                    pages.last_mut().unwrap().blocks.push(table);
                }
            }
            Block::Text { title, rows, gap } | Block::Title { title, rows, gap } => {
                let expanded = expand_text_rows(rows, &resolved)?;
                let mut chunk = Vec::new();
                for row in expanded {
                    let candidate = chunk.clone();
                    let height = if title_block {
                        block_height(
                            &Block::Title {
                                title: title.clone(),
                                rows: candidate,
                                gap,
                            },
                            &resolved,
                            line_height,
                        )
                    } else {
                        block_height(
                            &Block::Text {
                                title: title.clone(),
                                rows: candidate,
                                gap,
                            },
                            &resolved,
                            line_height,
                        )
                    };
                    if !chunk.is_empty() && used + height > page_budget {
                        let rows = std::mem::take(&mut chunk);
                        let part = if title_block {
                            Block::Title {
                                title: title.clone(),
                                rows,
                                gap,
                            }
                        } else {
                            Block::Text {
                                title: title.clone(),
                                rows,
                                gap,
                            }
                        };
                        pages.last_mut().unwrap().blocks.push(part);
                        pages.push(Page { blocks: Vec::new() });
                        used = 0.0;
                    }
                    chunk.push(row);
                }
                if !chunk.is_empty() {
                    let part = if title_block {
                        Block::Title {
                            title,
                            rows: chunk,
                            gap,
                        }
                    } else {
                        Block::Text {
                            title,
                            rows: chunk,
                            gap,
                        }
                    };
                    let height = block_height(&part, &resolved, line_height);
                    if used > 0.0 && used + height > page_budget {
                        pages.push(Page { blocks: Vec::new() });
                        used = 0.0;
                    }
                    used += height;
                    pages.last_mut().unwrap().blocks.push(part);
                }
            }
            block => {
                let height = block_height(&block, &resolved, line_height);
                if used > 0.0 && used + height > page_budget {
                    pages.push(Page { blocks: Vec::new() });
                    used = 0.0;
                }
                used += height;
                pages.last_mut().unwrap().blocks.push(block);
            }
        }
        if pages.len() > MAX_PAGES {
            return Err(RenderError::OutputTooLarge {
                limit: MAX_RENDERED_BYTES,
            });
        }
    }
    // Resolve row positions and link rectangles once, shared by all encoders.
    for (pi, page) in pages.iter_mut().enumerate() {
        let mut y = resolved.frame_inset as f32 + line_height;
        for block in &mut page.blocks {
            let is_title = matches!(block, Block::Title { .. });
            match block {
                Block::Title { title, rows, gap } | Block::Text { title, rows, gap } => {
                    if !title.is_empty() {
                        y += if is_title {
                            22.0 * f32::from(resolved.font_scale) / 100.0
                        } else {
                            line_height
                        };
                    }
                    for row in rows {
                        row.x = resolved.frame_inset as f32;
                        row.width = content_width(&resolved);
                        if let Some(href) = &row.link {
                            links.push(LinkBox {
                                href: href.clone(),
                                label: row.text.clone(),
                                page: pi,
                                x: row.x,
                                y,
                                width: row.text.chars().count() as f32 * char_width(&resolved),
                                height: line_height,
                            });
                        }
                        for run in &row.runs {
                            if let InlineKind::Link(href) = &run.kind {
                                if let Some(href) = safe_http_url(href) {
                                    links.push(LinkBox {
                                        href,
                                        label: run.text.clone(),
                                        page: pi,
                                        x: row.x,
                                        y,
                                        width: run.text.chars().count() as f32
                                            * char_width(&resolved),
                                        height: line_height,
                                    });
                                }
                            }
                        }
                        y += line_height;
                    }
                    y += f32::from(*gap) * line_height;
                }
                Block::Table { rows, gap, .. } => {
                    let heading_line = resolved.line_advance;
                    y += 2.0 * heading_line
                        + rows.len() as f32 * line_height
                        + f32::from(*gap) * line_height;
                }
                Block::Images(items) => {
                    for image in items {
                        image.x = resolved.frame_inset as f32;
                        image.y = y;
                        y += image.display_height + line_height;
                    }
                }
                Block::Total(_) => y += line_height,
                Block::PageBreak => {}
            }
        }
    }
    Ok(Plan {
        resolved,
        pages,
        links,
    })
}
fn content_width(r: &Resolved) -> f32 {
    PAGE_WIDTH as f32 - 2.0 * r.frame_inset as f32
}
fn char_width(r: &Resolved) -> f32 {
    let px = 14.0 * f32::from(r.font_scale) / 100.0;
    r.raster.metrics('M', px).advance_width.max(5.0)
}
fn block_height(block: &Block, r: &Resolved, line: f32) -> f32 {
    match block {
        Block::Title { rows, gap, .. } => {
            (rows.len() as f32) * line
                + 22.0 * f32::from(r.font_scale) / 100.0
                + f32::from(*gap) * line
        }
        Block::Text { rows, gap, .. } => (rows.len() as f32 + 1.0 + f32::from(*gap)) * line,
        Block::Table { rows, gap, .. } => {
            let heading_line = r.line_advance;
            2.0 * heading_line + rows.len() as f32 * line + f32::from(*gap) * line
        }
        Block::Images(v) => v.iter().map(|x| x.display_height + line).sum(),
        Block::Total(_) => 2.0 * line,
        Block::PageBreak => 0.0,
    }
}
fn wrapped_row(row: &TextRow, runs: Vec<InlineRun>, preserve_link: bool) -> TextRow {
    let text = runs.iter().map(|run| run.text.as_str()).collect();
    TextRow {
        text,
        runs,
        x: 0.,
        width: 0.,
        link: preserve_link.then(|| row.link.clone()).flatten(),
    }
}
fn expand_text_rows(rows: Vec<TextRow>, r: &Resolved) -> Result<Vec<TextRow>, RenderError> {
    let max = (content_width(r) / char_width(r)).floor().max(8.) as usize;
    let mut out = Vec::new();
    for row in rows {
        let runs = if row.runs.is_empty() {
            inline_runs(&row.text)
        } else {
            row.runs.clone()
        };
        let mut current = Vec::new();
        let mut width = 0usize;
        let mut preserve_link = true;
        for run in runs {
            if run.kind == InlineKind::Break {
                if out.len() >= MAX_EXPANDED_ROWS {
                    return Err(RenderError::OutputTooLarge {
                        limit: MAX_RENDERED_BYTES,
                    });
                }
                out.push(wrapped_row(
                    &row,
                    std::mem::take(&mut current),
                    preserve_link,
                ));
                preserve_link = false;
                width = 0;
                continue;
            }
            let mut chars = run.text.chars();
            loop {
                let remaining = max.saturating_sub(width);
                if remaining == 0 {
                    if out.len() >= MAX_EXPANDED_ROWS {
                        return Err(RenderError::OutputTooLarge {
                            limit: MAX_RENDERED_BYTES,
                        });
                    }
                    out.push(wrapped_row(
                        &row,
                        std::mem::take(&mut current),
                        preserve_link,
                    ));
                    preserve_link = false;
                    width = 0;
                    continue;
                }
                let part: String = chars.by_ref().take(remaining).collect();
                if part.is_empty() {
                    break;
                }
                width += part.chars().count();
                current.push(InlineRun {
                    kind: run.kind.clone(),
                    text: part,
                });
                if width == max {
                    if out.len() >= MAX_EXPANDED_ROWS {
                        return Err(RenderError::OutputTooLarge {
                            limit: MAX_RENDERED_BYTES,
                        });
                    }
                    out.push(wrapped_row(
                        &row,
                        std::mem::take(&mut current),
                        preserve_link,
                    ));
                    preserve_link = false;
                    width = 0;
                }
            }
        }
        if !current.is_empty() || out.is_empty() {
            if out.len() >= MAX_EXPANDED_ROWS {
                return Err(RenderError::OutputTooLarge {
                    limit: MAX_RENDERED_BYTES,
                });
            }
            let text = current.iter().map(|run| run.text.as_str()).collect();
            out.push(TextRow {
                text,
                runs: current,
                x: 0.,
                width: 0.,
                link: row.link,
            });
        }
    }
    Ok(out)
}
fn party_block(
    label: &str,
    p: &crate::Party,
    blocks: &mut Vec<Block>,
    assets: &[ResolvedAsset],
    image_budget: &mut usize,
) -> Result<Option<ImageItem>, RenderError> {
    let mut rows = vec![TextRow {
        text: p.name.clone(),
        runs: Vec::new(),
        x: 0.,
        width: 0.,
        link: None,
    }];
    rows.extend(p.address.iter().map(|x| TextRow {
        text: x.clone(),
        runs: Vec::new(),
        x: 0.,
        width: 0.,
        link: None,
    }));
    if let Some(x) = &p.email {
        rows.push(TextRow {
            text: x.clone(),
            runs: Vec::new(),
            x: 0.,
            width: 0.,
            link: None,
        });
    }
    if let Some(x) = &p.website {
        rows.push(TextRow {
            text: x.clone(),
            runs: Vec::new(),
            x: 0.,
            width: 0.,
            link: safe_http_url(x),
        });
    }
    rows.extend(p.identifiers.iter().map(|x| TextRow {
        text: format!("{}: {}", x.key, x.value),
        runs: Vec::new(),
        x: 0.,
        width: 0.,
        link: None,
    }));
    let image = if let Some(i) = &p.logo {
        if let Some(image) = decode_asset(i, assets, image_budget)? {
            rows.push(TextRow {
                text: i.alt.clone(),
                runs: Vec::new(),
                x: 0.,
                width: 0.,
                link: None,
            });
            Some(image)
        } else {
            rows.push(TextRow {
                text: i.alt.clone(),
                runs: Vec::new(),
                x: 0.,
                width: 0.,
                link: safe_http_url(&i.src),
            });
            None
        }
    } else {
        None
    };
    blocks.push(Block::Text {
        title: label.into(),
        rows,
        gap: 1,
    });
    Ok(image)
}
fn section_block(
    s: &Section,
    doc: &Document,
    blocks: &mut Vec<Block>,
    assets: &[ResolvedAsset],
    image_budget: &mut usize,
) -> Result<(), RenderError> {
    if s.directives.page_break_before {
        blocks.push(Block::PageBreak);
    }
    match &s.body {
        SectionBody::Prose(v) => {
            let rows = vec![TextRow {
                text: v.clone(),
                runs: Vec::new(),
                x: 0.,
                width: 0.,
                link: None,
            }];
            blocks.push(Block::Text {
                title: s.title.clone(),
                rows,
                gap: gap_value(s.directives.gap),
            });
        }
        SectionBody::Table(t) => {
            let subtotal = (!s.directives.summary_only).then_some(s.total).flatten();
            blocks.push(table_block(
                &s.title,
                t,
                doc,
                subtotal,
                gap_value(s.directives.gap),
            ));
        }
    }
    let _ = (assets, image_budget);
    Ok(())
}
fn gap_value(g: crate::Gap) -> u8 {
    match g {
        crate::Gap::None => 0,
        crate::Gap::Tight => 1,
        crate::Gap::Standard => 2,
        crate::Gap::Roomy => 3,
    }
}
fn heading_index(headings: &[String], terms: &[&str]) -> Option<usize> {
    headings.iter().position(|h| {
        let h = h.trim().to_ascii_lowercase().replace(['-', '_'], " ");
        terms.iter().any(|term| h == *term || h.contains(term))
    })
}
fn table_block(
    title: &str,
    t: &crate::Table,
    doc: &Document,
    total: Option<Decimal>,
    gap: u8,
) -> Block {
    let headings = t.headings.clone();
    let quantity = heading_index(&headings, &["quantity", "qty", "hours", "days", "units"]);
    let rate = heading_index(&headings, &["rate", "unit price", "price"]);
    let amount = heading_index(&headings, &["amount", "total", "subtotal"])
        .or_else(|| headings.len().checked_sub(1));
    let mut rows = Vec::new();
    for source in &t.rows {
        let mut cells = source.clone();
        for i in 0..cells.len() {
            if cells[i].eq_ignore_ascii_case("auto") && Some(i) == amount {
                if let (Some(qi), Some(ri)) = (quantity, rate) {
                    if let (Some(q), Some(rate)) = (
                        cells.get(qi).and_then(|x| x.parse::<Decimal>().ok()),
                        cells.get(ri).and_then(|x| x.parse::<Decimal>().ok()),
                    ) {
                        cells[i] =
                            format_money(q * rate, &doc.metadata.currency, &doc.config.format);
                    }
                }
            } else if let Ok(value) = cells[i].parse::<Decimal>() {
                let heading = headings
                    .get(i)
                    .map(|h| h.to_ascii_lowercase())
                    .unwrap_or_default();
                if Some(i) == amount
                    || heading.contains("amount")
                    || heading.contains("rate")
                    || heading.contains("price")
                {
                    cells[i] = format_money(value, &doc.metadata.currency, &doc.config.format);
                }
            }
        }
        rows.push(cells);
    }
    if let Some(total) = total {
        let n = headings
            .len()
            .max(rows.iter().map(Vec::len).max().unwrap_or(0));
        let mut subtotal = vec![String::new(); n.max(1)];
        subtotal[0] = "Subtotal".into();
        subtotal[n.max(1) - 1] = format_money(total, &doc.metadata.currency, &doc.config.format);
        rows.push(subtotal);
    }
    let widths = table_widths(&headings, &rows);
    let trs = rows
        .into_iter()
        .map(|cells| TableRow {
            cells,
            widths: widths.clone(),
            alignments: t.alignments.clone(),
        })
        .collect();
    Block::Table {
        title: title.into(),
        headings,
        rows: trs,
        gap,
    }
}
fn table_widths(headings: &[String], rows: &[Vec<String>]) -> Vec<f32> {
    let n = headings
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    let mut maxes = vec![8usize; n];
    for (i, h) in headings.iter().enumerate() {
        maxes[i] = maxes[i].max(h.chars().count());
    }
    for row in rows {
        for (i, v) in row.iter().enumerate() {
            maxes[i] = maxes[i].max(v.chars().count());
        }
    }
    let total = maxes.iter().sum::<usize>().max(1) as f32;
    maxes.into_iter().map(|x| 487. * x as f32 / total).collect()
}
fn format_money(amount: Decimal, currency: &str, format: &str) -> String {
    let exp = currency_exponent(currency);
    let rounded = amount.round_dp_with_strategy(exp, RoundingStrategy::MidpointNearestEven);
    let raw = rounded.abs().to_string();
    let mut parts = raw.split('.');
    let integer = parts.next().unwrap_or("0");
    let frac_raw = parts.next().unwrap_or("");
    let mut frac = frac_raw.to_owned();
    while frac.len() < exp as usize {
        frac.push('0');
    }
    let (sep, decimal) = match format {
        "code-dot-comma" => ('.', ','),
        "code-space-comma" => (' ', ','),
        "code-indian" => (',', '.'),
        _ => (',', '.'),
    };
    let grouped = if format == "code-plain" {
        integer.to_owned()
    } else {
        group_digits(integer, sep, format == "code-indian")
    };
    let mut out = if exp > 0 {
        format!("{grouped}{decimal}{frac}")
    } else {
        grouped
    };
    if amount.is_sign_negative() && !amount.is_zero() {
        out.insert(0, '-')
    }
    out
}
fn group_digits(s: &str, sep: char, indian: bool) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 {
            let rem = chars.len() - i;
            if (!indian && rem % 3 == 0) || (indian && ((rem > 3 && rem % 2 == 1) || rem == 3)) {
                out.push(sep)
            }
        }
        out.push(*c);
    }
    out
}
fn inline_runs(s: &str) -> Vec<InlineRun> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let mut out: Vec<InlineRun> = Vec::new();
    let mut styles: Vec<InlineKind> = Vec::new();
    let mut lists: Vec<(bool, u64)> = Vec::new();
    let parser = Parser::new_ext(s, options);
    for event in parser {
        match event {
            Event::Start(Tag::List(first)) => {
                if !out.is_empty() && !out.last().is_some_and(|run| run.kind == InlineKind::Break) {
                    out.push(InlineRun {
                        kind: InlineKind::Break,
                        text: String::new(),
                    });
                }
                lists.push((first.is_some(), first.unwrap_or(1)));
            }
            Event::Start(Tag::Item) => {
                if !out.is_empty() && !out.last().is_some_and(|run| run.kind == InlineKind::Break) {
                    out.push(InlineRun {
                        kind: InlineKind::Break,
                        text: String::new(),
                    });
                }
                let depth = lists.len().saturating_sub(1);
                if let Some((ordered, next)) = lists.last_mut() {
                    let marker = if *ordered {
                        format!("{}.", *next)
                    } else {
                        "-".to_owned()
                    };
                    *next = next.saturating_add(1);
                    out.push(InlineRun {
                        kind: InlineKind::ListMarker,
                        text: format!("{}{} ", "  ".repeat(depth), marker),
                    });
                }
            }
            Event::Start(Tag::Emphasis) => styles.push(InlineKind::Emphasis),
            Event::Start(Tag::Strong) => styles.push(InlineKind::Strong),
            Event::Start(Tag::Link { dest_url, .. }) => {
                styles.push(InlineKind::Link(dest_url.to_string()))
            }
            Event::End(TagEnd::Emphasis)
            | Event::End(TagEnd::Strong)
            | Event::End(TagEnd::Link) => {
                styles.pop();
            }
            Event::End(TagEnd::Paragraph) => {
                if !out.last().is_some_and(|run| run.kind == InlineKind::Break) {
                    out.push(InlineRun {
                        kind: InlineKind::Break,
                        text: String::new(),
                    });
                }
            }
            Event::Text(text) => {
                let kind = if styles.iter().any(|k| matches!(k, InlineKind::Link(_))) {
                    styles
                        .iter()
                        .rev()
                        .find(|k| matches!(k, InlineKind::Link(_)))
                        .cloned()
                        .unwrap_or(InlineKind::Text)
                } else if styles.iter().any(|k| matches!(k, InlineKind::Strong))
                    && styles.iter().any(|k| matches!(k, InlineKind::Emphasis))
                {
                    InlineKind::EmphasisStrong
                } else if styles.iter().any(|k| matches!(k, InlineKind::Strong)) {
                    InlineKind::Strong
                } else if styles.iter().any(|k| matches!(k, InlineKind::Emphasis)) {
                    InlineKind::Emphasis
                } else {
                    InlineKind::Text
                };
                out.push(InlineRun {
                    kind,
                    text: text.to_string(),
                });
            }
            Event::Code(text) => out.push(InlineRun {
                kind: InlineKind::Code,
                text: text.to_string(),
            }),
            Event::HardBreak => out.push(InlineRun {
                kind: InlineKind::Break,
                text: String::new(),
            }),
            Event::SoftBreak => out.push(InlineRun {
                kind: InlineKind::Text,
                text: " ".into(),
            }),
            _ => {}
        }
    }
    if out.is_empty() {
        out.push(InlineRun {
            kind: InlineKind::Text,
            text: String::new(),
        });
    }
    out
}
fn safe_http_url(s: &str) -> Option<String> {
    let value = s.trim();
    if value.bytes().any(|b| b.is_ascii_control()) {
        return None;
    }
    let lower = value.get(..7).map(str::to_ascii_lowercase);
    let lower_s = value.get(..8).map(str::to_ascii_lowercase);
    if lower.as_deref() == Some("http://") || lower_s.as_deref() == Some("https://") {
        Some(value.to_owned())
    } else {
        None
    }
}
fn inline_html(s: &str) -> String {
    let runs = inline_runs(s);
    inline_html_runs(&runs, s)
}
fn inline_html_runs(runs: &[InlineRun], fallback: &str) -> String {
    let mut out = String::new();
    if runs.is_empty() {
        return esc(fallback);
    }
    for (index, run) in runs.iter().enumerate() {
        if run.kind == InlineKind::Break && index + 1 == runs.len() {
            continue;
        }
        match &run.kind {
            InlineKind::Strong => out.push_str(&format!("<strong>{}</strong>", esc(&run.text))),
            InlineKind::Emphasis => out.push_str(&format!("<em>{}</em>", esc(&run.text))),
            InlineKind::EmphasisStrong => {
                out.push_str(&format!("<strong><em>{}</em></strong>", esc(&run.text)))
            }
            InlineKind::Code => out.push_str(&format!("<code>{}</code>", esc(&run.text))),
            InlineKind::ListMarker => out.push_str(&esc(&run.text)),
            InlineKind::Link(href) => {
                if let Some(href) = safe_http_url(href) {
                    out.push_str(&format!(
                        "<a href=\"{}\">{}</a>",
                        esc(&href),
                        esc(&run.text)
                    ));
                } else {
                    out.push_str(&esc(&run.text));
                }
            }
            InlineKind::Break => out.push_str("<br>"),
            InlineKind::Text => out.push_str(&esc(&run.text)),
        }
    }
    out
}
fn decode_base64(s: &str) -> Result<Vec<u8>, RenderError> {
    let mut out = Vec::with_capacity(s.len().saturating_mul(3) / 4);
    let mut value = 0u32;
    let mut bits = 0u8;
    for byte in s.bytes() {
        if byte == b'=' {
            break;
        }
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(RenderError::InvalidAsset("invalid base64".into())),
        };
        value = (value << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((value >> bits) as u8);
            if out.len() > MAX_ASSET_BYTES {
                return Err(RenderError::InvalidAsset("asset exceeds 1 MiB".into()));
            }
        }
    }
    if out.is_empty() {
        return Err(RenderError::InvalidAsset("empty image".into()));
    }
    Ok(out)
}

fn decoder_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some((MAX_PNG_PIXELS as u64).saturating_mul(4));
    limits
}
fn decode_asset(
    image: &crate::Image,
    assets: &[ResolvedAsset],
    image_budget: &mut usize,
) -> Result<Option<ImageItem>, RenderError> {
    if safe_http_url(&image.src).is_some() {
        return Ok(None);
    }
    let (bytes, hinted): (Arc<[u8]>, Option<String>) = if let Some(asset) =
        assets.iter().find(|a| a.source == image.src)
    {
        (asset.bytes.clone(), asset.mime.clone())
    } else if let Some((head, body)) = image.src.split_once(',') {
        let mime = head
            .strip_prefix("data:image/")
            .and_then(|x| x.strip_suffix(";base64"))
            .ok_or_else(|| RenderError::InvalidAsset("images require base64 data URLs".into()))?;
        (Arc::from(decode_base64(body)?), Some(mime.to_owned()))
    } else {
        return Err(RenderError::InvalidAsset(format!(
            "asset not provided: {}",
            image.src
        )));
    };
    if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES {
        return Err(RenderError::InvalidAsset("asset exceeds 1 MiB".into()));
    }
    let mut reader = ImageReader::new(Cursor::new(bytes.as_ref()))
        .with_guessed_format()
        .map_err(|e| RenderError::InvalidAsset(format!("invalid image: {e}")))?;
    reader.limits(decoder_limits());
    let detected = reader
        .format()
        .ok_or_else(|| RenderError::InvalidAsset("unknown image format".into()))?;
    let mime = match detected {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Gif => "gif",
        ImageFormat::WebP => "webp",
        _ => return Err(RenderError::InvalidAsset("unsupported image format".into())),
    };
    if let Some(hint) = hinted {
        let hint = hint.to_ascii_lowercase();
        let hint = hint.strip_prefix("image/").unwrap_or(&hint);
        let hint = if hint == "jpg" { "jpeg" } else { hint };
        if hint != mime {
            return Err(RenderError::InvalidAsset(
                "image MIME does not match content".into(),
            ));
        }
    }
    let (w, h) = reader
        .into_dimensions()
        .map_err(|e| RenderError::InvalidAsset(format!("invalid image: {e}")))?;
    let pixels = (w as usize)
        .checked_mul(h as usize)
        .ok_or_else(|| RenderError::InvalidAsset("image dimensions overflow".into()))?;
    let bytes_needed = pixels
        .checked_mul(4)
        .ok_or_else(|| RenderError::InvalidAsset("image dimensions overflow".into()))?;
    if pixels > MAX_PNG_PIXELS
        || image_budget.saturating_add(bytes_needed) > MAX_PNG_PIXELS.saturating_mul(4)
    {
        return Err(RenderError::InvalidAsset(
            "image pixel budget exceeded".into(),
        ));
    }
    let mut reader = ImageReader::new(Cursor::new(bytes.as_ref()))
        .with_guessed_format()
        .map_err(|e| RenderError::InvalidAsset(format!("invalid image: {e}")))?;
    reader.limits(decoder_limits());
    let rgba = reader
        .decode()
        .map_err(|e| RenderError::InvalidAsset(format!("invalid image: {e}")))?
        .into_rgba8()
        .into_raw();
    *image_budget = image_budget.saturating_add(bytes_needed);
    let scale = (150.0 / w.max(h) as f32).min(1.0);
    Ok(Some(ImageItem {
        alt: image.alt.clone(),
        mime: mime.into(),
        bytes,
        rgba,
        width: w,
        height: h,
        x: 0.,
        y: 0.,
        display_width: w as f32 * scale,
        display_height: h as f32 * scale,
    }))
}
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        o.push(T[(n >> 18 & 63) as usize] as char);
        o.push(T[(n >> 12 & 63) as usize] as char);
        o.push(if c.len() > 1 {
            T[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        o.push(if c.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    o
}
fn color_hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}
fn encode_html(plan: &Plan) -> Result<Vec<u8>, RenderError> {
    let r = &plan.resolved;
    let mut o = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; object-src 'none'; script-src 'none'\"><style>\
        @font-face{{font-family:'ttyinv-{}';font-weight:{};src:url(data:font/ttf;base64,{}) format('truetype')}}\
        @font-face{{font-family:'ttyinv-{}';font-weight:600;src:url(data:font/ttf;base64,{}) format('truetype')}}\
        :root{{--paper:{};--ink:{};--muted:{};--rule:{};--accent:{}}}\
        *{{box-sizing:border-box}}body{{margin:0;background:#d8dadd;color:var(--ink);font-family:'ttyinv-{}',monospace}}\
        .page{{position:relative;width:{}px;min-height:{}px;margin:16px auto;padding:{}px;border:1px dashed var(--rule);background:var(--paper);line-height:{}px;font-size:{}px}}\
        .corner{{position:absolute;color:var(--rule);font-family:inherit;font-size:{}px;line-height:1;transform:translate(-50%,-50%)}}\
        .corner.tl{{left:0;top:0}}.corner.tr{{left:100%;top:0}}.corner.bl{{left:0;top:100%}}.corner.br{{left:100%;top:100%}}\
        h1,h2{{font-size:inherit;margin:0 0 8px;font-weight:{}}}h1{{text-align:center;color:var(--accent);border-bottom:1px dashed var(--rule);padding-bottom:10px}}\
        h2.gap-0{{margin-bottom:0}}h2.gap-1{{margin-bottom:4px}}h2.gap-2{{margin-bottom:8px}}h2.gap-3{{margin-bottom:16px}}\
        table{{width:100%;border-collapse:collapse;table-layout:fixed;margin:0 0 8px}}\
        thead{{border-bottom:1px dashed var(--rule)}}tbody tr:last-child{{border-bottom:1px dashed var(--rule)}}\
        td,th{{padding:0;text-align:left}}td.num,th.num{{text-align:right}}td.center,th.center{{text-align:center}}.total{{font-weight:{}}}.total{{color:var(--accent);border-top:1px dashed var(--rule);text-align:right}}\
        code{{font-family:inherit;background:color-mix(in srgb,var(--muted) 15%,transparent)}}\
        </style></head><body>",
        r.font, r.font_weight_number, b64(r.font_bytes),
        r.font, b64(r.semibold_bytes),
        color_hex(r.tokens.paper), color_hex(r.tokens.ink), color_hex(r.tokens.muted),
        color_hex(r.tokens.rule), color_hex(r.accent), r.font, PAGE_WIDTH, PAGE_HEIGHT,
        r.frame_inset, r.line_advance,
        14 * u32::from(r.font_scale) / 100, 14, 600, 600
    );
    for page in &plan.pages {
        o.push_str("<article class=\"page\"><span class=\"corner tl\">+</span><span class=\"corner tr\">+</span><span class=\"corner bl\">+</span><span class=\"corner br\">+</span>");
        for block in &page.blocks {
            match block {
                Block::Title { title, rows, .. } => {
                    o.push_str(&format!("<h1>{}</h1>", inline_html(title)));
                    for row in rows {
                        o.push_str("<div>");
                        o.push_str(&inline_html_runs(&row.runs, &row.text));
                        o.push_str("</div>");
                    }
                }
                Block::Text { title, rows, gap } => {
                    if !title.is_empty() {
                        o.push_str(&format!("<h2 class=\"gap-{}\">{}</h2>", gap, esc(title)));
                    }
                    for row in rows {
                        o.push_str("<div>");
                        o.push_str(&inline_html_runs(&row.runs, &row.text));
                        o.push_str("</div>");
                    }
                }
                Block::Table {
                    title,
                    headings,
                    rows,
                    gap,
                } => {
                    o.push_str(&format!(
                        "<h2 class=\"gap-{}\">{}</h2><table><colgroup>",
                        gap,
                        esc(title)
                    ));
                    if let Some(first) = rows.first() {
                        for width in &first.widths {
                            o.push_str(&format!("<col style=\"width:{width}px\">"));
                        }
                    }
                    o.push_str("</colgroup><thead><tr>");
                    for (i, h) in headings.iter().enumerate() {
                        let class = match row_alignment(rows.first(), i) {
                            TableAlignment::Right => " class=\"num\"",
                            TableAlignment::Center => " class=\"center\"",
                            _ => "",
                        };
                        o.push_str(&format!("<th{class}>{}</th>", esc(h)));
                    }
                    o.push_str("</tr></thead><tbody>");
                    for row in rows {
                        o.push_str("<tr>");
                        for (i, c) in row.cells.iter().enumerate() {
                            let class = match row.alignments.get(i).copied().unwrap_or_default() {
                                TableAlignment::Right => "num",
                                TableAlignment::Center => "center",
                                _ => "",
                            };
                            o.push_str(&format!("<td class=\"{class}\">{}</td>", esc(c)));
                        }
                        o.push_str("</tr>");
                    }
                    o.push_str("</tbody></table>");
                }
                Block::Images(items) => {
                    for image in items {
                        o.push_str(&format!("<img alt=\"{}\" src=\"data:image/{};base64,{}\" width=\"{}\" height=\"{}\">", esc(&image.alt), image.mime, b64(image.bytes.as_ref()), image.display_width, image.display_height));
                    }
                }
                Block::Total(text) => {
                    o.push_str(&format!("<div class=\"total\">{}</div>", esc(text)));
                }
                Block::PageBreak => {}
            }
        }
        o.push_str("</article>");
    }
    o.push_str("</body></html>");
    Ok(o.into_bytes())
}
fn row_alignment(row: Option<&TableRow>, i: usize) -> TableAlignment {
    row.and_then(|r| r.alignments.get(i).copied())
        .unwrap_or_default()
}

fn shape(
    text: &str,
    r: &Resolved,
    _size: f32,
    semibold: bool,
) -> Result<Vec<KrillaGlyph>, RenderError> {
    let bytes = if semibold {
        r.semibold_bytes
    } else {
        r.font_bytes
    };
    let face = rustybuzz::Face::from_slice(bytes, 0)
        .ok_or_else(|| RenderError::Font("invalid font face".into()))?;
    let mut b = rustybuzz::UnicodeBuffer::new();
    b.push_str(text);
    let shaped = rustybuzz::shape(&face, &[], b);
    let upem = face.units_per_em() as f32;
    let infos = shaped.glyph_infos();
    Ok(infos
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(info, position)| {
            let start = (info.cluster as usize).min(text.len());
            let end = infos
                .iter()
                .filter_map(|next| {
                    let cluster = next.cluster as usize;
                    (cluster > start).then_some(cluster)
                })
                .min()
                .unwrap_or(text.len())
                .min(text.len());
            let end = if end >= start { end } else { start };
            KrillaGlyph::new(
                PdfGlyphId::new(info.glyph_id),
                position.x_advance as f32 / upem,
                position.x_offset as f32 / upem,
                position.y_offset as f32 / upem,
                position.y_advance as f32 / upem,
                start..end,
                None,
            )
        })
        .collect())
}
fn measure_text(text: &str, r: &Resolved, size: f32, semibold: bool) -> f32 {
    let raster = if semibold {
        &r.semibold_raster
    } else {
        &r.raster
    };
    text.chars()
        .map(|ch| raster.metrics(ch, size).advance_width)
        .sum()
}
fn set_pdf_fill(surface: &mut krilla::surface::Surface<'_>, c: [u8; 3]) {
    surface.set_fill(Some(Fill {
        paint: rgb::Color::new(c[0], c[1], c[2]).into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::NonZero,
    }));
}
fn draw_pdf_line(
    surface: &mut krilla::surface::Surface<'_>,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: [u8; 3],
) {
    let mut line = PathBuilder::new();
    line.move_to(x1, y1);
    line.line_to(x2, y2);
    if let Some(line) = line.finish() {
        surface.set_stroke(Some(Stroke {
            paint: rgb::Color::new(color[0], color[1], color[2]).into(),
            width: 0.8,
            miter_limit: 4.,
            line_cap: Default::default(),
            line_join: Default::default(),
            opacity: NormalizedF32::ONE,
            dash: None,
        }));
        surface.draw_path(&line);
    }
}
fn encode_pdf(plan: &Plan) -> Result<Vec<u8>, RenderError> {
    let r = &plan.resolved;
    let font = PdfFont::new(r.font_bytes.to_vec().into(), 0)
        .ok_or_else(|| RenderError::Font("invalid PDF font".into()))?;
    let semibold_font = PdfFont::new(r.semibold_bytes.to_vec().into(), 0)
        .ok_or_else(|| RenderError::Font("invalid PDF semibold font".into()))?;
    let scale = f32::from(r.font_scale) / 100.0;
    let mut doc = PdfDocument::new();
    doc.set_metadata(
        PdfMetadata::new()
            .title("ttyinv invoice".into())
            .creator("ttyinv".into())
            .producer("ttyinv".into())
            .document_id("ttyinv-render-v2".into()),
    );
    for (page_index, page_plan) in plan.pages.iter().enumerate() {
        let settings = PageSettings::from_wh(PAGE_WIDTH as f32, PAGE_HEIGHT as f32)
            .ok_or_else(|| RenderError::Backend("invalid page size".into()))?;
        let mut page = doc.start_page_with(settings);
        let mut surface = page.surface();
        let inset = r.frame_inset as f32;
        let mut frame = PathBuilder::new();
        frame.move_to(inset, inset);
        frame.line_to(PAGE_WIDTH as f32 - inset, inset);
        frame.line_to(PAGE_WIDTH as f32 - inset, PAGE_HEIGHT as f32 - inset);
        frame.line_to(inset, PAGE_HEIGHT as f32 - inset);
        frame.close();
        let frame = frame
            .finish()
            .ok_or_else(|| RenderError::Backend("frame path".into()))?;
        surface.set_stroke(Some(Stroke {
            paint: rgb::Color::new(r.tokens.rule[0], r.tokens.rule[1], r.tokens.rule[2]).into(),
            width: 0.8,
            miter_limit: 4.,
            line_cap: Default::default(),
            line_join: Default::default(),
            opacity: NormalizedF32::ONE,
            dash: Some(StrokeDash {
                array: vec![3., 3.],
                offset: 0.,
            }),
        }));
        surface.set_fill(None);
        surface.draw_path(&frame);
        for (x, y_corner) in [
            (inset - 5.0, inset + 5.0),
            (PAGE_WIDTH as f32 - inset - 5.0, inset + 5.0),
            (inset - 5.0, PAGE_HEIGHT as f32 - inset + 5.0),
            (
                PAGE_WIDTH as f32 - inset - 5.0,
                PAGE_HEIGHT as f32 - inset + 5.0,
            ),
        ] {
            let glyphs = shape("+", r, 14. * scale, false)?;
            set_pdf_fill(&mut surface, r.tokens.rule);
            surface.draw_glyphs(
                Point::from_xy(x, y_corner),
                &glyphs,
                font.clone(),
                "+",
                14. * scale,
                false,
            );
        }
        let mut y = inset + 16. * scale;
        for block in &page_plan.blocks {
            match block {
                Block::Title { title, rows, gap } | Block::Text { title, rows, gap } => {
                    let title_size = if matches!(block, Block::Title { .. }) {
                        18. * scale
                    } else {
                        14. * scale
                    };
                    let body_size = 14. * scale;
                    if !title.is_empty() {
                        let glyphs = shape(title, r, title_size, true)?;
                        set_pdf_fill(
                            &mut surface,
                            if matches!(block, Block::Title { .. }) {
                                r.accent
                            } else {
                                r.tokens.muted
                            },
                        );
                        surface.draw_glyphs(
                            Point::from_xy(inset, y),
                            &glyphs,
                            semibold_font.clone(),
                            title,
                            title_size,
                            false,
                        );
                        y += title_size + 4. * scale;
                    }
                    for row in rows {
                        let mut x = inset;
                        for run in &row.runs {
                            if run.kind == InlineKind::Break {
                                y += r.line_advance;
                                x = inset;
                                continue;
                            }
                            if run.text.is_empty() {
                                continue;
                            }
                            let semibold = matches!(
                                &run.kind,
                                InlineKind::Strong | InlineKind::EmphasisStrong
                            );
                            let run_width = measure_text(&run.text, r, body_size, semibold);
                            if matches!(&run.kind, InlineKind::Code) {
                                if let Some(rect) =
                                    Rect::from_xywh(x, y - body_size * 0.82, run_width, body_size)
                                {
                                    let mut background = PathBuilder::new();
                                    background.push_rect(rect);
                                    if let Some(background) = background.finish() {
                                        set_pdf_fill(&mut surface, r.tokens.muted);
                                        surface.draw_path(&background);
                                    }
                                }
                            }
                            let glyphs = shape(&run.text, r, body_size, semibold)?;
                            set_pdf_fill(
                                &mut surface,
                                if matches!(&run.kind, InlineKind::Link(_)) {
                                    r.accent
                                } else {
                                    r.tokens.ink
                                },
                            );
                            surface.draw_glyphs(
                                Point::from_xy(x, y),
                                &glyphs,
                                if semibold {
                                    semibold_font.clone()
                                } else {
                                    font.clone()
                                },
                                &run.text,
                                body_size,
                                false,
                            );
                            if matches!(
                                &run.kind,
                                InlineKind::Emphasis
                                    | InlineKind::EmphasisStrong
                                    | InlineKind::Link(_)
                            ) {
                                draw_pdf_line(
                                    &mut surface,
                                    x,
                                    y + 2. * scale,
                                    x + run_width,
                                    y + 2. * scale,
                                    if matches!(&run.kind, InlineKind::Link(_)) {
                                        r.accent
                                    } else {
                                        r.tokens.ink
                                    },
                                );
                            }
                            x += run_width;
                        }
                        y += r.line_advance;
                    }
                    y += f32::from(*gap) * r.line_advance;
                }
                Block::Table {
                    title,
                    headings,
                    rows,
                    gap,
                } => {
                    let title_size = 14. * scale;
                    let cell_size = 13. * scale;
                    let widths = rows.first().map(|row| row.widths.as_slice()).unwrap_or(&[]);
                    let aligns = rows
                        .first()
                        .map(|row| row.alignments.as_slice())
                        .unwrap_or(&[]);
                    let glyphs = shape(title, r, title_size, true)?;
                    set_pdf_fill(&mut surface, r.tokens.muted);
                    surface.draw_glyphs(
                        Point::from_xy(inset, y),
                        &glyphs,
                        semibold_font.clone(),
                        title,
                        title_size,
                        false,
                    );
                    y += r.line_advance;
                    for (column, heading) in headings.iter().enumerate() {
                        let x0 = inset + widths.iter().take(column).sum::<f32>();
                        let width = widths.get(column).copied().unwrap_or(0.);
                        let text_width = measure_text(heading, r, cell_size, true);
                        let x = match aligns.get(column) {
                            Some(TableAlignment::Right) => x0 + (width - text_width).max(0.),
                            Some(TableAlignment::Center) => x0 + (width - text_width).max(0.) / 2.0,
                            _ => x0,
                        };
                        let glyphs = shape(heading, r, cell_size, true)?;
                        set_pdf_fill(&mut surface, r.tokens.ink);
                        surface.draw_glyphs(
                            Point::from_xy(x, y),
                            &glyphs,
                            semibold_font.clone(),
                            heading,
                            cell_size,
                            false,
                        );
                    }
                    let mut rule = PathBuilder::new();
                    rule.move_to(inset, y - 2. * scale);
                    rule.line_to(PAGE_WIDTH as f32 - inset, y - 2. * scale);
                    if let Some(rule) = rule.finish() {
                        surface.draw_path(&rule);
                    }
                    y += r.line_advance;
                    for row in rows {
                        for (column, cell) in row.cells.iter().enumerate() {
                            let x0 = inset + row.widths.iter().take(column).sum::<f32>();
                            let width = row.widths.get(column).copied().unwrap_or(0.);
                            let text_width = measure_text(cell, r, cell_size, false);
                            let x = match row.alignments.get(column) {
                                Some(TableAlignment::Right) => x0 + (width - text_width).max(0.),
                                Some(TableAlignment::Center) => {
                                    x0 + (width - text_width).max(0.) / 2.0
                                }
                                _ => x0,
                            };
                            let glyphs = shape(cell, r, cell_size, false)?;
                            surface.draw_glyphs(
                                Point::from_xy(x, y),
                                &glyphs,
                                font.clone(),
                                cell,
                                cell_size,
                                false,
                            );
                        }
                        y += r.line_advance;
                    }
                    let mut rule = PathBuilder::new();
                    rule.move_to(inset, y - 2. * scale);
                    rule.line_to(PAGE_WIDTH as f32 - inset, y - 2. * scale);
                    if let Some(rule) = rule.finish() {
                        surface.draw_path(&rule);
                    }
                    y += f32::from(*gap) * r.line_advance;
                }
                Block::Images(items) => {
                    for image in items {
                        let img = match image.mime.as_str() {
                            "png" => krilla::Image::from_png(image.bytes.to_vec().into(), true),
                            "jpeg" | "jpg" => {
                                krilla::Image::from_jpeg(image.bytes.to_vec().into(), true)
                            }
                            "gif" => krilla::Image::from_gif(image.bytes.to_vec().into(), true),
                            "webp" => krilla::Image::from_webp(image.bytes.to_vec().into(), true),
                            _ => {
                                return Err(RenderError::InvalidAsset(
                                    "unsupported image MIME".into(),
                                ))
                            }
                        }
                        .map_err(RenderError::Backend)?;
                        surface.push_transform(&Transform::from_translate(inset, y));
                        surface.draw_image(
                            img,
                            Size::from_wh(image.display_width, image.display_height)
                                .ok_or_else(|| RenderError::Backend("image size".into()))?,
                        );
                        surface.pop();
                        y += image.display_height + r.line_advance;
                    }
                }
                Block::Total(text) => {
                    let mut rule = PathBuilder::new();
                    rule.move_to(inset, y - 2. * scale);
                    rule.line_to(PAGE_WIDTH as f32 - inset, y - 2. * scale);
                    if let Some(rule) = rule.finish() {
                        surface.draw_path(&rule);
                    }
                    let size = 14. * scale;
                    let glyphs = shape(text, r, size, true)?;
                    set_pdf_fill(&mut surface, r.accent);
                    surface.draw_glyphs(
                        Point::from_xy(inset, y),
                        &glyphs,
                        semibold_font.clone(),
                        text,
                        size,
                        false,
                    );
                    y += r.line_advance;
                }
                Block::PageBreak => {}
            }
        }
        surface.finish();
        for link in plan.links.iter().filter(|link| link.page == page_index) {
            let target = Target::Action(Action::Link(LinkAction::new(link.href.clone())));
            if let Some(rect) = Rect::from_xywh(link.x, link.y, link.width.max(10.), link.height) {
                page.add_annotation(Annotation::new_link(
                    LinkAnnotation::new(rect, target),
                    Some(link.label.clone()),
                ));
            }
        }
        page.finish();
    }
    doc.finish()
        .map_err(|e| RenderError::Encoding(format!("{e:?}")))
}

fn blend_rgba(dst: &mut [u8], src: &[u8], alpha: u8) {
    let a = u16::from(alpha);
    for c in 0..3 {
        dst[c] = ((u16::from(src[c]) * a + u16::from(dst[c]) * (255 - a)) / 255) as u8;
    }
    dst[3] = 255;
}
fn draw_png_rule(raw: &mut [u8], width: u32, base: u32, y: f32, inset: u32, color: [u8; 3]) {
    let yy = base.saturating_add(y.max(0.).round() as u32);
    for x in inset..width.saturating_sub(inset) {
        if (x - inset) % 6 < 3 {
            let idx = ((yy as usize * width as usize) + x as usize) * 4;
            if idx + 4 <= raw.len() {
                raw[idx..idx + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn draw_png_box(
    raw: &mut [u8],
    width: u32,
    base: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 3],
) {
    let x0 = x.max(0.).round() as u32;
    let x1 = (x + w).max(0.).round() as u32;
    let y0 = base.saturating_add(y.max(0.).round() as u32);
    let y1 = base.saturating_add((y + h).max(0.).round() as u32);
    for yy in y0..y1 {
        for xx in x0.min(width)..x1.min(width) {
            let idx = ((yy as usize * width as usize) + xx as usize) * 4;
            if idx + 4 <= raw.len() {
                raw[idx..idx + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
            }
        }
    }
}
fn draw_png_line(raw: &mut [u8], width: u32, base: u32, x: f32, y: f32, w: f32, color: [u8; 3]) {
    draw_png_box(raw, width, base, x, y, w, 1., color);
}
#[allow(clippy::too_many_arguments)]
fn draw_glyphs_png(
    raw: &mut [u8],
    width: u32,
    page_y: u32,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    r: &Resolved,
) {
    draw_glyphs_png_face(raw, width, page_y, text, x, y, size, r, false, r.tokens.ink);
}
#[allow(clippy::too_many_arguments)]
fn draw_glyphs_png_semibold(
    raw: &mut [u8],
    width: u32,
    page_y: u32,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    r: &Resolved,
) {
    draw_glyphs_png_face(raw, width, page_y, text, x, y, size, r, true, r.accent);
}
#[allow(clippy::too_many_arguments)]
fn draw_glyphs_png_face(
    raw: &mut [u8],
    width: u32,
    page_y: u32,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    r: &Resolved,
    semibold: bool,
    color: [u8; 3],
) {
    let font_bytes = if semibold {
        r.semibold_bytes
    } else {
        r.font_bytes
    };
    let raster = if semibold {
        &r.semibold_raster
    } else {
        &r.raster
    };
    let shaped = rustybuzz::Face::from_slice(font_bytes, 0).map(|face| {
        let mut b = rustybuzz::UnicodeBuffer::new();
        b.push_str(text);
        rustybuzz::shape(&face, &[], b)
    });
    let Some(shaped) = shaped else { return };
    let mut pen = x;
    let px = size * f32::from(r.font_scale) / 100.;
    let upem = rustybuzz::Face::from_slice(font_bytes, 0)
        .map(|face| face.units_per_em())
        .unwrap_or(1000) as f32;
    for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        let (gm, bitmap) = raster.rasterize_indexed(info.glyph_id as u16, px);
        let gx = (pen + gm.xmin as f32).round() as i32;
        let gy = (page_y as f32 + y - gm.ymin as f32 - gm.height as f32).round() as i32;
        for by in 0..gm.height {
            for bx in 0..gm.width {
                let xx = gx + bx as i32;
                let yy = gy + by as i32;
                if xx >= 0 && yy >= 0 && xx < width as i32 && yy < (page_y + PAGE_HEIGHT) as i32 {
                    let idx = ((yy as u32 * width + xx as u32) * 4) as usize;
                    blend_rgba(&mut raw[idx..idx + 4], &color, bitmap[by * gm.width + bx]);
                }
            }
        }
        pen += pos.x_advance as f32 / upem * size;
    }
}
#[allow(clippy::too_many_arguments)]
fn draw_inline_png(
    raw: &mut [u8],
    width: u32,
    page_y: u32,
    runs: &[InlineRun],
    x: f32,
    y: f32,
    size: f32,
    r: &Resolved,
) -> usize {
    let mut line_breaks = 0;
    let mut line_y = y;
    let mut pen = x;
    for run in runs {
        if run.kind == InlineKind::Break {
            line_breaks += 1;
            line_y += r.line_advance;
            pen = x;
            continue;
        }
        if run.text.is_empty() {
            continue;
        }
        let semibold = matches!(&run.kind, InlineKind::Strong | InlineKind::EmphasisStrong);
        let run_width = measure_text(
            &run.text,
            r,
            size * f32::from(r.font_scale) / 100.,
            semibold,
        );
        if matches!(&run.kind, InlineKind::Code) {
            draw_png_box(
                raw,
                width,
                page_y,
                pen,
                line_y - size * 0.82,
                run_width,
                size,
                r.tokens.muted,
            );
        }
        draw_glyphs_png_face(
            raw,
            width,
            page_y,
            &run.text,
            pen,
            line_y,
            size,
            r,
            semibold,
            if matches!(&run.kind, InlineKind::Link(_)) {
                r.accent
            } else {
                r.tokens.ink
            },
        );
        if matches!(
            &run.kind,
            InlineKind::Emphasis | InlineKind::EmphasisStrong | InlineKind::Link(_)
        ) {
            draw_png_line(
                raw,
                width,
                page_y,
                pen,
                line_y + 2.,
                run_width,
                if matches!(&run.kind, InlineKind::Link(_)) {
                    r.accent
                } else {
                    r.tokens.ink
                },
            );
        }
        pen += run_width;
    }
    line_breaks
}
fn encode_png(plan: &Plan) -> Result<(Vec<u8>, u32, u32), RenderError> {
    let width = PAGE_WIDTH;
    let height =
        PAGE_HEIGHT
            .checked_mul(plan.pages.len() as u32)
            .ok_or(RenderError::OutputTooLarge {
                limit: MAX_RENDERED_BYTES,
            })?;
    let pixels =
        (width as usize)
            .checked_mul(height as usize)
            .ok_or(RenderError::OutputTooLarge {
                limit: MAX_RENDERED_BYTES,
            })?;
    if pixels > MAX_PNG_PIXELS {
        return Err(RenderError::OutputTooLarge {
            limit: MAX_RENDERED_BYTES,
        });
    }
    let mut raw = vec![0u8; pixels * 4];
    for p in 0..plan.pages.len() {
        let base = p as u32 * PAGE_HEIGHT;
        let bg = plan.resolved.tokens.paper;
        for y in 0..PAGE_HEIGHT {
            for x in 0..width {
                let idx = (base + y) as usize * width as usize * 4 + x as usize * 4;
                raw[idx..idx + 4].copy_from_slice(&[bg[0], bg[1], bg[2], 255]);
                let inset = plan.resolved.frame_inset as u32;
                let horizontal = y == inset || y + inset + 1 == PAGE_HEIGHT;
                let vertical = x == inset || x + inset + 1 == width;
                let along = if horizontal {
                    x.saturating_sub(inset)
                } else {
                    y.saturating_sub(inset)
                };
                if (horizontal || vertical) && along % 6 < 3 {
                    raw[idx..idx + 4].copy_from_slice(&[
                        plan.resolved.tokens.rule[0],
                        plan.resolved.tokens.rule[1],
                        plan.resolved.tokens.rule[2],
                        255,
                    ]);
                }
            }
        }
        let scale = f32::from(plan.resolved.font_scale) / 100.;
        let mut yy = plan.resolved.frame_inset as f32 + 16. * scale;
        let inset = plan.resolved.frame_inset as i32;
        let right = width as i32 - inset - 1;
        let bottom = PAGE_HEIGHT as i32 - inset - 1;
        for (cx, cy) in [
            (inset, inset),
            (right, inset),
            (inset, bottom),
            (right, bottom),
        ] {
            for (dx, dy) in [(-2, 0), (-1, 0), (0, 0), (1, 0), (2, 0), (0, -1), (0, 1)] {
                let x = cx + dx;
                let y = base as i32 + cy + dy;
                if x >= 0 && y >= 0 && x < width as i32 && y < height as i32 {
                    let idx = (y as usize * width as usize + x as usize) * 4;
                    raw[idx..idx + 4].copy_from_slice(&[
                        plan.resolved.tokens.rule[0],
                        plan.resolved.tokens.rule[1],
                        plan.resolved.tokens.rule[2],
                        255,
                    ]);
                }
            }
        }
        for (x, y_corner) in [
            (inset as f32 - 5., inset as f32 + 5.),
            (right as f32 - 5., inset as f32 + 5.),
            (inset as f32 - 5., bottom as f32 + 5.),
            (right as f32 - 5., bottom as f32 + 5.),
        ] {
            draw_glyphs_png_face(
                &mut raw,
                width,
                base,
                "+",
                x,
                y_corner,
                14.,
                &plan.resolved,
                false,
                plan.resolved.tokens.rule,
            );
        }
        for block in &plan.pages[p].blocks {
            match block {
                Block::Title { title, rows, gap } | Block::Text { title, rows, gap } => {
                    if !title.is_empty() {
                        draw_glyphs_png_semibold(
                            &mut raw,
                            width,
                            base,
                            title,
                            plan.resolved.frame_inset as f32,
                            yy,
                            if matches!(block, Block::Title { .. }) {
                                18.
                            } else {
                                14.
                            },
                            &plan.resolved,
                        );
                        yy += if matches!(block, Block::Title { .. }) {
                            22. * scale
                        } else {
                            plan.resolved.line_advance
                        };
                    }
                    for row in rows {
                        let breaks = draw_inline_png(
                            &mut raw,
                            width,
                            base,
                            &row.runs,
                            plan.resolved.frame_inset as f32,
                            yy,
                            14.,
                            &plan.resolved,
                        );
                        yy +=
                            plan.resolved.line_advance + breaks as f32 * plan.resolved.line_advance;
                    }
                    yy += f32::from(*gap) * plan.resolved.line_advance;
                }
                Block::Table {
                    title,
                    headings,
                    rows,
                    gap,
                } => {
                    let inset = plan.resolved.frame_inset as f32;
                    let line = plan.resolved.line_advance;
                    let heading_line = plan.resolved.line_advance;
                    draw_glyphs_png_semibold(
                        &mut raw,
                        width,
                        base,
                        title,
                        inset,
                        yy,
                        14.,
                        &plan.resolved,
                    );
                    yy += heading_line;
                    let widths = rows.first().map(|row| row.widths.as_slice()).unwrap_or(&[]);
                    let aligns = rows
                        .first()
                        .map(|row| row.alignments.as_slice())
                        .unwrap_or(&[]);
                    for (column, heading) in headings.iter().enumerate() {
                        let x0 = inset + widths.iter().take(column).sum::<f32>();
                        let cell_width = widths.get(column).copied().unwrap_or(0.);
                        let text_width = measure_text(heading, &plan.resolved, 13. * scale, true);
                        let x = match aligns.get(column) {
                            Some(TableAlignment::Right) => x0 + (cell_width - text_width).max(0.),
                            Some(TableAlignment::Center) => {
                                x0 + (cell_width - text_width).max(0.) / 2.0
                            }
                            _ => x0,
                        };
                        draw_glyphs_png_semibold(
                            &mut raw,
                            width,
                            base,
                            heading,
                            x,
                            yy,
                            13.,
                            &plan.resolved,
                        );
                    }
                    draw_png_rule(
                        &mut raw,
                        width,
                        base,
                        yy + 2.,
                        plan.resolved.frame_inset as u32,
                        plan.resolved.tokens.rule,
                    );
                    yy += line;
                    for row in rows {
                        for (column, cell) in row.cells.iter().enumerate() {
                            let x0 = inset + row.widths.iter().take(column).sum::<f32>();
                            let cell_width = row.widths.get(column).copied().unwrap_or(0.);
                            let text_width = measure_text(cell, &plan.resolved, 13. * scale, false);
                            let x = match row.alignments.get(column) {
                                Some(TableAlignment::Right) => {
                                    x0 + (cell_width - text_width).max(0.)
                                }
                                Some(TableAlignment::Center) => {
                                    x0 + (cell_width - text_width).max(0.) / 2.0
                                }
                                _ => x0,
                            };
                            draw_glyphs_png(
                                &mut raw,
                                width,
                                base,
                                cell,
                                x,
                                yy,
                                13.,
                                &plan.resolved,
                            );
                        }
                        yy += line;
                    }
                    draw_png_rule(
                        &mut raw,
                        width,
                        base,
                        yy,
                        plan.resolved.frame_inset as u32,
                        plan.resolved.tokens.rule,
                    );
                    yy += f32::from(*gap) * line;
                }
                Block::Images(items) => {
                    for image in items {
                        let dw = image.display_width.max(1.).round() as u32;
                        let dh = image.display_height.max(1.).round() as u32;
                        for dy_local in 0..dh {
                            for dx_local in 0..dw {
                                let sx = (((dx_local as u64 * image.width as u64) / dw as u64)
                                    .min(u64::from(image.width.saturating_sub(1))))
                                    as u32;
                                let sy = (((dy_local as u64 * image.height as u64) / dh as u64)
                                    .min(u64::from(image.height.saturating_sub(1))))
                                    as u32;
                                let dx = plan.resolved.frame_inset as u32 + dx_local;
                                let dy = base + yy as u32 + dy_local;
                                if dx < width && dy < height {
                                    let si = ((u64::from(sy) * u64::from(image.width)
                                        + u64::from(sx))
                                        * 4) as usize;
                                    let di = ((dy * width + dx) * 4) as usize;
                                    let alpha = image.rgba[si + 3];
                                    blend_rgba(
                                        &mut raw[di..di + 4],
                                        &image.rgba[si..si + 4],
                                        alpha,
                                    );
                                }
                            }
                        }
                        yy += image.display_height + plan.resolved.line_advance;
                    }
                }
                Block::Total(text) => {
                    draw_png_rule(
                        &mut raw,
                        width,
                        base,
                        yy - 2.,
                        plan.resolved.frame_inset as u32,
                        plan.resolved.tokens.rule,
                    );
                    draw_glyphs_png_semibold(
                        &mut raw,
                        width,
                        base,
                        text,
                        plan.resolved.frame_inset as f32,
                        yy,
                        14.,
                        &plan.resolved,
                    );
                    yy += plan.resolved.line_advance;
                }
                Block::PageBreak => {}
            }
        }
    }
    let mut out = Vec::with_capacity((pixels / 4).min(MAX_RENDERED_BYTES));
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut out), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| RenderError::Encoding(e.to_string()))?;
        writer
            .write_image_data(&raw)
            .map_err(|e| RenderError::Encoding(e.to_string()))?;
    }
    if out.len() > MAX_RENDERED_BYTES {
        return Err(RenderError::OutputTooLarge {
            limit: MAX_RENDERED_BYTES,
        });
    }
    Ok((out, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = include_str!("../../../render-compat/01-simple.md");
    #[test]
    fn formats_are_deterministic() {
        for f in [RenderFormat::Html, RenderFormat::Pdf, RenderFormat::Png] {
            let a = render(
                SOURCE,
                RenderOptions {
                    format: f,
                    ..Default::default()
                },
            )
            .unwrap();
            let b = render(
                SOURCE,
                RenderOptions {
                    format: f,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(a.bytes, b.bytes);
            assert_eq!(a.mime, f.mime());
            assert_eq!(a.extension, f.extension());
        }
    }
    #[test]
    fn all_themes_resolve() {
        for id in supported_themes() {
            let r = render(
                SOURCE,
                RenderOptions {
                    theme: Some((*id).into()),
                    ..Default::default()
                },
            );
            assert!(r.is_ok(), "{id}");
        }
    }
    #[test]
    fn amount_formats() {
        let d = Decimal::new(123456, 2);
        assert_eq!(format_money(d, "EUR", "code-comma-dot"), "1,234.56");
        assert_eq!(format_money(d, "EUR", "code-dot-comma"), "1.234,56");
        assert_eq!(format_money(d, "JPY", "code-plain"), "1235");
        assert_eq!(format_money(d, "KRW", "code-comma-dot"), "1,235");
        assert_eq!(
            format_money(Decimal::new(123456, 2), "OMR", "code-plain"),
            "1234.560"
        );
    }
    #[test]
    fn inline_semantics_are_shared_and_safe() {
        let source = "**bold** *em* `code` [docs](https://example.com)  \nnext";
        let html = inline_html(source);
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>em</em>"));
        assert!(html.contains("<code>code</code>"));
        assert!(html.contains("<a href=\"https://example.com\">docs</a>"));
        assert!(html.contains("<br>"));
        assert!(!html.contains("**"));
        assert!(safe_http_url("HTTPS://example.com").is_some());
        assert!(safe_http_url("javascript:alert(1)").is_none());
        let nested = inline_html("**bold and *italic*** and \\*literal\\*");
        assert!(nested.contains("<em>italic</em>"));
        assert!(nested.contains("literal"));
        assert!(nested.contains("*literal*"));
    }
    #[test]
    fn markdown_blocks_keep_paragraphs_and_list_items() {
        let runs = inline_runs(
            "first paragraph\n\nsecond paragraph\n\n- first\n- second\n\n1. one\n2. two",
        );
        let markers: Vec<&str> = runs
            .iter()
            .filter(|run| run.kind == InlineKind::ListMarker)
            .map(|run| run.text.as_str())
            .collect();
        assert_eq!(markers, vec!["- ", "- ", "  1. ", "  2. "]);
        assert!(
            runs.iter()
                .filter(|run| run.kind == InlineKind::Break)
                .count()
                >= 4
        );
        let content: String = runs.iter().map(|run| run.text.as_str()).collect();
        assert!(content.contains("first paragraph"));
        assert!(content.contains("second paragraph"));
        assert!(content.contains("- first"));
        assert!(content.contains("1. one"));
    }
    #[test]
    fn compact_advance_is_shared_by_plan_and_png() {
        let doc = document(SOURCE).unwrap();
        let plan = layout(
            &doc,
            resolve(
                &doc.config,
                RenderOptions {
                    format: RenderFormat::Png,
                    density: Some("compact".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(plan.resolved.line_advance, 14.0);
        let (_, width, height) = encode_png(&plan).unwrap();
        assert_eq!(width, PAGE_WIDTH);
        assert_eq!(height, PAGE_HEIGHT * plan.pages.len() as u32);
    }
    #[test]
    fn styled_runs_reach_native_encoders() {
        let styled_source = format!(
            "{}\n\n## Notes\n\n**bold** *emphasis* `code` [link](https://example.com)",
            SOURCE
        );
        let doc = document(&styled_source).unwrap();
        let source = "**bold** *emphasis* `code` [link](https://example.com)";
        let runs = inline_runs(source);
        assert!(runs.iter().any(|run| run.kind == InlineKind::Strong));
        assert!(runs.iter().any(|run| run.kind == InlineKind::Emphasis));
        assert!(runs.iter().any(|run| run.kind == InlineKind::Code));
        assert!(runs
            .iter()
            .any(|run| matches!(&run.kind, InlineKind::Link(_))));
        let resolved = resolve(
            &doc.config,
            RenderOptions {
                format: RenderFormat::Png,
                ..Default::default()
            },
        )
        .unwrap();
        let mut raw = vec![0u8; PAGE_WIDTH as usize * PAGE_HEIGHT as usize * 4];
        draw_inline_png(&mut raw, PAGE_WIDTH, 0, &runs, 40., 100., 14., &resolved);
        let muted = resolved.tokens.muted;
        assert!(raw.chunks_exact(4).any(|pixel| pixel[..3] == muted));
        let pdf_plan = layout(
            &doc,
            resolve(
                &doc.config,
                RenderOptions {
                    format: RenderFormat::Pdf,
                    ..Default::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!encode_pdf(&pdf_plan).unwrap().is_empty());
        assert!(render(
            &styled_source,
            RenderOptions {
                format: RenderFormat::Png,
                ..Default::default()
            }
        )
        .is_ok());
    }
    #[test]
    fn wrapping_near_source_limit_is_bounded() {
        let doc = document(SOURCE).unwrap();
        let resolved = resolve(&doc.config, RenderOptions::default()).unwrap();
        let text = "x".repeat(128 * 1024 - 32);
        let input_len = text.len();
        let expanded = expand_text_rows(
            vec![TextRow {
                text,
                runs: Vec::new(),
                x: 0.,
                width: 0.,
                link: None,
            }],
            &resolved,
        )
        .unwrap();
        assert!(expanded.len() < MAX_EXPANDED_ROWS);
        assert_eq!(
            expanded.iter().map(|row| row.text.len()).sum::<usize>(),
            input_len
        );
        let oversized = "x".repeat(MAX_EXPANDED_ROWS * 100);
        let error = expand_text_rows(
            vec![TextRow {
                text: oversized,
                runs: Vec::new(),
                x: 0.,
                width: 0.,
                link: None,
            }],
            &resolved,
        );
        assert!(matches!(error, Err(RenderError::OutputTooLarge { .. })));
    }
    #[test]
    fn styled_markdown_runs_are_canonical_and_linear() {
        let runs =
            inline_runs("**bold and *italic***  \nnext [docs](https://example.com) \\*literal\\*");
        assert!(runs.iter().any(|r| r.kind == InlineKind::EmphasisStrong));
        assert!(runs.iter().any(|r| matches!(&r.kind, InlineKind::Link(_))));
        assert!(runs.iter().any(|r| r.text.contains("literal")));
        let adversarial = format!("x  \n{}", "[".repeat(128 * 1024 - 4));
        let parsed = inline_runs(&adversarial);
        assert!(!parsed.is_empty());
        let prose = include_str!("../../../render-compat/04-prose.md");
        for format in [RenderFormat::Pdf, RenderFormat::Png] {
            assert!(render(
                prose,
                RenderOptions {
                    format,
                    ..Default::default()
                }
            )
            .is_ok());
        }
    }
    #[test]
    fn scaled_and_typed_layout_contracts() {
        let source = include_str!("../../../render-compat/10-multi-page-500.md");
        let normal = render(
            source,
            RenderOptions {
                format: RenderFormat::Html,
                ..Default::default()
            },
        )
        .unwrap();
        let scaled = render(
            source,
            RenderOptions {
                format: RenderFormat::Html,
                font_scale: Some(140),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(scaled.pages > normal.pages);
        let html = String::from_utf8(scaled.bytes).unwrap();
        assert!(html.contains("<h1>Five hundred line stress document</h1>"));
        assert!(html.contains("color:var(--accent)"));
        assert!(html.contains("thead{border-bottom:1px dashed var(--rule)}"));
        assert!(html.contains("class=\"corner tl\">+</span>"));
    }
    #[test]
    fn summary_only_keeps_authored_rows() {
        let source = include_str!("../../../render-compat/05-summary-only.md");
        let result = render(
            source,
            RenderOptions {
                format: RenderFormat::Html,
                ..Default::default()
            },
        )
        .unwrap();
        let html = String::from_utf8(result.bytes).unwrap();
        assert!(html.contains("500.00"));
        assert!(!html.contains("Subtotal: 0.00"));
    }
}
