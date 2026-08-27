use crate::{currency_exponent, Config, Document, Section, SectionBody, TableAlignment};
#[cfg(test)]
use crate::{document, Diagnostic};
use fontdue::Font as RasterFont;
use image::{ImageFormat, ImageReader, Limits};
use krilla::tagging::{
    ContentTag, ListNumbering, TableHeaderScope, Tag as PdfTag, TagGroup, TagKind, TagTree,
};
use krilla::{
    action::{Action, LinkAction},
    annotation::{Annotation, LinkAnnotation, Target},
    color::rgb,
    geom::{PathBuilder, Point, Rect, Size, Transform},
    metadata::Metadata as PdfMetadata,
    num::NormalizedF32,
    page::PageSettings,
    paint::{Fill, FillRule},
    text::{Font as PdfFont, GlyphId as PdfGlyphId, KrillaGlyph},
    Document as PdfDocument,
};
use png::{BitDepth, ColorType, Encoder as PngEncoder, PixelDimensions, Unit};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, fmt, io::Cursor, num::NonZeroU16, rc::Rc, sync::Arc};
mod generated {
    include!(concat!(env!("OUT_DIR"), "/render_fonts.rs"));
}
/// Wire serializer for renderer f32 values.
///
/// The renderer computes in binary32 for layout fidelity. At the wire boundary,
/// each value is promoted exactly to binary64 and encoded with one canonical
/// shortest JSON spelling. Integral values use the integer serializer so
/// serde_json and serde-wasm-bindgen both emit `1` rather than adapter-specific
/// `1.0`/`1` spellings; fractional values use `serialize_f64`.
pub(crate) mod canonical_float {
    use serde::Serializer;

    pub fn serialize<S>(value: &f32, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value = f64::from(*value);
        if value.is_finite()
            && value.fract() == 0.0
            && value >= -(1_i64 << 53) as f64
            && value <= (1_i64 << 53) as f64
        {
            serializer.serialize_i64(value as i64)
        } else {
            serializer.serialize_f64(value)
        }
    }
}

mod canonical_float_map {
    use super::canonical_float;
    use serde::{ser::SerializeMap, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S>(values: &BTreeMap<String, f32>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(values.len()))?;
        for (name, value) in values {
            map.serialize_entry(name, &CanonicalFloat(*value))?;
        }
        map.end()
    }

    struct CanonicalFloat(f32);

    impl serde::Serialize for CanonicalFloat {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            canonical_float::serialize(&self.0, serializer)
        }
    }
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
pub const MAX_PNG_PIXELS: usize = 2_100_000;
pub const MAX_PNG_TOTAL_PIXELS: usize = 32_000_000;
pub const MAX_ASSET_BYTES: usize = 1024 * 1024;
pub const MAX_ASSET_TOTAL_BYTES: usize = MAX_ASSET_BYTES * 8;
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
pub(crate) struct RenderAsset {
    pub source: String,
    pub bytes: Vec<u8>,
    #[serde(default)]
    pub mime: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct RenderOptions {
    pub format: RenderFormat,
    pub theme: Option<String>,
    pub font: Option<String>,
    pub font_weight: Option<crate::FontWeight>,
    pub density: Option<String>,
    pub accent: Option<String>,
    pub font_scale: Option<u8>,
    pub frame_inset: Option<u8>,
    /// Raster scale in device pixels per logical document unit. `None` means 1.
    pub png_scale: Option<u8>,
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
            png_scale: None,
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
pub(crate) struct RenderResult {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub extension: String,
    pub pages: usize,
    pub width: u32,
    pub height: u32,
    pub warnings: Vec<RenderWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RenderError {
    #[cfg(test)]
    SourceTooLarge {
        limit: usize,
    },
    #[cfg(test)]
    InvalidDocument(Vec<Diagnostic>),
    UnsupportedTheme(String),
    UnsupportedFont(String),
    UnsupportedDensity(String),
    InvalidAccent(String),
    InvalidOption(String),
    InvalidAsset(String),
    OutputTooLarge {
        limit: usize,
    },
    Encoding(String),
    Font(String),
    Backend(String),
}
impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTheme(v) => write!(f, "unsupported theme: {v}"),
            Self::UnsupportedFont(v) => write!(f, "unsupported font: {v}"),
            #[cfg(test)]
            Self::SourceTooLarge { limit } => {
                write!(f, "source exceeds render limit ({limit} bytes)")
            }
            #[cfg(test)]
            Self::InvalidDocument(_) => f.write_str("document is invalid"),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeTokens {
    pub paper: [u8; 3],
    pub ink: [u8; 3],
    pub muted: [u8; 3],
    pub rule: [u8; 3],
    pub accent: [u8; 3],
    pub canvas: [u8; 3],
}
const THEMES: &[(&str, ThemeTokens)] = &[
    (
        "printable",
        ThemeTokens {
            paper: [255, 255, 255],
            ink: [20, 20, 22],
            muted: [93, 93, 99],
            rule: [163, 163, 170],
            accent: [18, 106, 168],
            canvas: [231, 231, 233],
        },
    ),
    (
        "paper-white",
        ThemeTokens {
            paper: [255, 255, 255],
            ink: [16, 18, 22],
            muted: [95, 102, 112],
            rule: [170, 178, 188],
            accent: [0, 111, 187],
            canvas: [238, 241, 244],
        },
    ),
    (
        "graphite",
        ThemeTokens {
            paper: [242, 243, 244],
            ink: [18, 20, 22],
            muted: [93, 98, 104],
            rule: [156, 161, 167],
            accent: [109, 63, 209],
            canvas: [217, 220, 223],
        },
    ),
    (
        "blueprint",
        ThemeTokens {
            paper: [244, 248, 251],
            ink: [16, 43, 61],
            muted: [86, 112, 132],
            rule: [139, 165, 183],
            accent: [0, 119, 182],
            canvas: [219, 232, 240],
        },
    ),
    (
        "ledger-pad",
        ThemeTokens {
            paper: [237, 243, 231],
            ink: [27, 36, 25],
            muted: [79, 92, 75],
            rule: [154, 171, 149],
            accent: [143, 47, 47],
            canvas: [216, 226, 209],
        },
    ),
    (
        "solarized-light",
        ThemeTokens {
            paper: [253, 246, 227],
            ink: [7, 54, 66],
            muted: [88, 110, 117],
            rule: [147, 161, 161],
            accent: [30, 118, 174],
            canvas: [238, 232, 213],
        },
    ),
    (
        "parchment",
        ThemeTokens {
            paper: [255, 250, 240],
            ink: [45, 36, 27],
            muted: [117, 104, 91],
            rule: [185, 170, 152],
            accent: [173, 93, 34],
            canvas: [236, 228, 215],
        },
    ),
    (
        "midnight",
        ThemeTokens {
            paper: [18, 18, 20],
            ink: [242, 242, 243],
            muted: [170, 170, 175],
            rule: [81, 81, 87],
            accent: [88, 169, 232],
            canvas: [39, 39, 42],
        },
    ),
    (
        "nord",
        ThemeTokens {
            paper: [46, 52, 64],
            ink: [236, 239, 244],
            muted: [160, 168, 182],
            rule: [97, 110, 136],
            accent: [136, 192, 208],
            canvas: [59, 66, 82],
        },
    ),
    (
        "gruvbox-dark",
        ThemeTokens {
            paper: [40, 40, 40],
            ink: [235, 219, 178],
            muted: [168, 153, 132],
            rule: [102, 92, 84],
            accent: [254, 128, 25],
            canvas: [60, 56, 54],
        },
    ),
];
const DENSITIES: &[&str] = &["comfortable", "compact"];
const MONEY_FORMATS: &[&str] = &[
    "code-comma-dot",
    "code-dot-comma",
    "code-space-comma",
    "code-indian",
    "code-plain",
];
pub(crate) fn supported_money_formats() -> &'static [&'static str] {
    MONEY_FORMATS
}
pub(crate) fn theme_tokens(id: &str) -> Option<ThemeTokens> {
    THEMES
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, tokens)| *tokens)
}
pub(crate) fn supported_themes() -> &'static [&'static str] {
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
pub(crate) fn supported_densities() -> &'static [&'static str] {
    DENSITIES
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct FontCapability {
    pub id: &'static str,
    pub label: &'static str,
    pub regular_weight: u16,
    pub semibold_weight: u16,
}

pub(crate) fn font_capabilities() -> impl Iterator<Item = FontCapability> {
    generated::FONT_ASSETS.iter().map(|font| FontCapability {
        id: font.id,
        label: font.label,
        regular_weight: font.regular_weight,
        semibold_weight: font.semibold_weight,
    })
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct PresentationConfig {
    pub theme: String,
    pub font: String,
    pub font_weight: crate::FontWeight,
    pub density: String,
    pub accent: Option<String>,
    pub font_scale: u8,
    pub frame_inset: u8,
}
impl Default for PresentationConfig {
    fn default() -> Self {
        Self {
            theme: "printable".into(),
            font: "geist-mono".into(),
            font_weight: crate::FontWeight::Regular,
            density: "comfortable".into(),
            accent: None,
            font_scale: 100,
            frame_inset: 54,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationTokens {
    pub paper: String,
    pub ink: String,
    pub muted: String,
    pub rule: String,
    pub accent: String,
    pub canvas: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationAccent {
    pub authored: Option<String>,
    pub resolved: String,
    pub corrected: bool,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub ratio: f32,
    pub steps: u8,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationScale {
    #[serde(rename = "type", serialize_with = "canonical_float::serialize")]
    pub type_scale: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub density_space: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub space: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub leading: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationContent {
    #[serde(serialize_with = "canonical_float::serialize")]
    pub left: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub right: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub top: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub bottom: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationFont {
    pub id: String,
    pub weight: u16,
    pub semibold_weight: u16,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Presentation {
    pub tokens: PresentationTokens,
    pub accent: PresentationAccent,
    pub font: PresentationFont,
    pub scale: PresentationScale,
    pub frame_inset: u8,
    pub content: PresentationContent,
    #[serde(serialize_with = "canonical_float_map::serialize")]
    pub geometry: std::collections::BTreeMap<String, f32>,
}
fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}
fn color_hex(c: [u8; 3]) -> String {
    hex(c)
}
pub(crate) fn presentation(config: PresentationConfig) -> Result<Presentation, RenderError> {
    let tokens = theme_tokens(&config.theme)
        .ok_or_else(|| RenderError::UnsupportedTheme(config.theme.clone()))?;
    let font = generated::FONT_ASSETS
        .iter()
        .find(|f| f.id == config.font)
        .ok_or_else(|| RenderError::UnsupportedFont(config.font.clone()))?;
    if !DENSITIES.contains(&config.density.as_str()) {
        return Err(RenderError::UnsupportedDensity(config.density));
    }
    if !(100..=140).contains(&config.font_scale) {
        return Err(RenderError::InvalidOption(
            "font-scale must be 100..=140".into(),
        ));
    }
    if !(30..=60).contains(&config.frame_inset) {
        return Err(RenderError::InvalidOption(
            "frame-inset must be 30..=60".into(),
        ));
    }
    let raw = config.accent.as_deref().map(parse_rgb).transpose()?;
    let (accent, steps, ratio) = match raw {
        Some(value) => readable_accent(value, tokens.paper, tokens.ink),
        None => (tokens.accent, 0, {
            let (_, _, ratio) = readable_accent(tokens.accent, tokens.paper, tokens.ink);
            ratio
        }),
    };
    let type_scale = f32::from(config.font_scale) / 100.0;
    let density_space = if config.density == "compact" {
        0.78
    } else {
        1.0
    } * type_scale;
    let i = f32::from(config.frame_inset);
    let canonical = Geometry::canonical();
    let mut geometry = std::collections::BTreeMap::new();
    for (name, value) in [
        ("page_w", canonical.page_w),
        ("page_h", canonical.page_h),
        ("gutter_x", canonical.gutter_x),
        ("gutter_top", canonical.gutter_top),
        ("gutter_bottom", canonical.gutter_bottom),
        ("font_body", canonical.font_body),
        ("font_heading", canonical.font_heading),
        ("font_detail", canonical.font_detail),
        ("hairline", canonical.hairline),
        ("line_body", canonical.line_body),
        ("line_heading", canonical.line_heading),
        ("line_detail", canonical.line_detail),
        ("brand_tracking", canonical.brand_tracking),
        ("badge_rise", canonical.badge_rise),
        ("badge_pad_x", canonical.badge_pad_x),
        ("badge_max_w", canonical.badge_max_w),
        ("corner_pad_x", canonical.corner_pad_x),
        ("header_min_h", canonical.header_min_h),
        ("header_gap", canonical.header_gap),
        ("header_pad_top", canonical.header_pad_top),
        ("header_pad_bottom", canonical.header_pad_bottom),
        ("meta_min_w", canonical.meta_min_w),
        ("meta_col_gap", canonical.meta_col_gap),
        ("meta_row_gap", canonical.meta_row_gap),
        ("parties_min_h", canonical.parties_min_h),
        ("parties_gap", canonical.parties_gap),
        ("parties_pad_top", canonical.parties_pad_top),
        ("parties_pad_bottom", canonical.parties_pad_bottom),
        ("label_gap", canonical.label_gap),
        ("logo_gap", canonical.logo_gap),
        ("gap_none", canonical.gap_none),
        ("gap_tight", canonical.gap_tight),
        ("gap_standard", canonical.gap_standard),
        ("gap_roomy", canonical.gap_roomy),
        ("section_pad_top", canonical.section_pad_top),
        ("notch_left", canonical.notch_left),
        ("notch_pad_x", canonical.notch_pad_x),
        ("cell_pad_x", canonical.cell_pad_x),
        ("cell_pad_top", canonical.cell_pad_top),
        ("cell_pad_bottom", canonical.cell_pad_bottom),
        ("table_end_gap", canonical.table_end_gap),
        ("cell_detail_gap", canonical.cell_detail_gap),
        ("summary_w", canonical.summary_w),
        ("summary_gap", canonical.summary_gap),
        ("summary_pad_top", canonical.summary_pad_top),
        ("total_margin_top", canonical.total_margin_top),
        ("footer_pad_top", canonical.footer_pad_top),
        ("settle_margin_bottom", canonical.settle_margin_bottom),
        ("settle_pad_top", canonical.settle_pad_top),
        ("pay_pad_top", canonical.pay_pad_top),
        ("pay_pad_x", canonical.pay_pad_x),
        ("pay_pad_bottom", canonical.pay_pad_bottom),
        ("pay_method_gap", canonical.pay_method_gap),
        ("pay_dt_min_w", canonical.pay_dt_min_w),
        ("pay_dl_col_gap", canonical.pay_dl_col_gap),
        ("pay_dl_margin_top", canonical.pay_dl_margin_top),
        ("sig_margin_top", canonical.sig_margin_top),
        ("sig_img_max_w", canonical.sig_img_max_w),
        ("sig_img_max_h", canonical.sig_img_max_h),
        ("sig_note_gap", canonical.sig_note_gap),
        ("prose_p_gap", canonical.prose_p_gap),
        ("prose_list_top", canonical.prose_list_top),
        ("prose_list_indent", canonical.prose_list_indent),
        ("quote_gap", canonical.quote_gap),
        ("quote_indent", canonical.quote_indent),
        ("link_underline_offset", canonical.link_underline_offset),
        ("blocked_asset_pad", canonical.blocked_asset_pad),
    ] {
        geometry.insert(name.to_owned(), value);
    }
    Ok(Presentation {
        tokens: PresentationTokens {
            paper: hex(tokens.paper),
            ink: hex(tokens.ink),
            muted: hex(tokens.muted),
            rule: hex(tokens.rule),
            accent: hex(accent),
            canvas: hex(tokens.canvas),
        },
        accent: PresentationAccent {
            authored: config.accent,
            resolved: hex(accent),
            corrected: steps != 0,
            ratio,
            steps,
        },
        font: PresentationFont {
            id: config.font,
            weight: match config.font_weight {
                crate::FontWeight::Regular => font.regular_weight,
                crate::FontWeight::Semibold => font.semibold_weight,
            },
            semibold_weight: font.semibold_weight,
        },
        scale: PresentationScale {
            type_scale,
            density_space,
            space: 1.0,
            leading: 1.0,
        },
        frame_inset: config.frame_inset,
        content: PresentationContent {
            left: i + canonical.gutter_x,
            right: canonical.page_w - i - canonical.gutter_x,
            top: i + canonical.gutter_top,
            bottom: canonical.page_h - i - canonical.gutter_bottom,
        },
        geometry,
    })
}
#[derive(Clone)]
struct Geometry {
    /// Canonical A4-unit tokens. Font sizes are scaled separately.
    page_w: f32,
    page_h: f32,
    gutter_x: f32,
    gutter_top: f32,
    gutter_bottom: f32,
    font_body: f32,
    font_heading: f32,
    font_detail: f32,
    hairline: f32,
    line_body: f32,
    line_heading: f32,
    line_detail: f32,
    brand_tracking: f32,
    badge_rise: f32,
    badge_pad_x: f32,
    badge_max_w: f32,
    corner_pad_x: f32,
    header_min_h: f32,
    header_gap: f32,
    header_pad_top: f32,
    header_pad_bottom: f32,
    meta_min_w: f32,
    meta_col_gap: f32,
    meta_row_gap: f32,
    parties_min_h: f32,
    parties_gap: f32,
    parties_pad_top: f32,
    parties_pad_bottom: f32,
    label_gap: f32,
    logo_gap: f32,
    gap_none: f32,
    gap_tight: f32,
    gap_standard: f32,
    gap_roomy: f32,
    section_pad_top: f32,
    notch_left: f32,
    notch_pad_x: f32,
    cell_pad_x: f32,
    cell_pad_top: f32,
    cell_pad_bottom: f32,
    table_end_gap: f32,
    cell_detail_gap: f32,
    summary_w: f32,
    summary_gap: f32,
    summary_pad_top: f32,
    total_margin_top: f32,
    footer_pad_top: f32,
    settle_margin_bottom: f32,
    settle_pad_top: f32,
    pay_pad_top: f32,
    pay_pad_x: f32,
    pay_pad_bottom: f32,
    pay_method_gap: f32,
    pay_dt_min_w: f32,
    pay_dl_col_gap: f32,
    pay_dl_margin_top: f32,
    sig_margin_top: f32,
    sig_img_max_w: f32,
    sig_img_max_h: f32,
    sig_note_gap: f32,
    prose_p_gap: f32,
    prose_list_top: f32,
    prose_list_indent: f32,
    quote_gap: f32,
    quote_indent: f32,
    link_underline_offset: f32,
    blocked_asset_pad: f32,
}
impl Geometry {
    fn canonical() -> Self {
        Self {
            page_w: 595.0,
            page_h: 842.0,
            gutter_x: 11.57,
            gutter_top: 23.14,
            gutter_bottom: 17.35,
            font_body: 8.26,
            font_heading: 14.05,
            font_detail: 6.61,
            hairline: 0.83,
            line_body: 12.06,
            line_heading: 15.74,
            line_detail: 9.65,
            brand_tracking: -0.562,
            badge_rise: 4.96,
            badge_pad_x: 12.40,
            badge_max_w: 416.50,
            corner_pad_x: 3.31,
            header_min_h: 91.73,
            header_gap: 36.36,
            header_pad_top: 14.88,
            header_pad_bottom: 28.10,
            meta_min_w: 181.81,
            meta_col_gap: 24.79,
            meta_row_gap: 3.31,
            parties_min_h: 162.80,
            parties_gap: 59.50,
            parties_pad_top: 14.88,
            parties_pad_bottom: 20.66,
            label_gap: 5.78,
            logo_gap: 4.96,
            gap_none: 0.0,
            gap_tight: 7.44,
            gap_standard: 15.70,
            gap_roomy: 36.36,
            section_pad_top: 15.70,
            notch_left: 12.40,
            notch_pad_x: 6.61,
            cell_pad_x: 6.61,
            cell_pad_top: 8.26,
            cell_pad_bottom: 5.78,
            table_end_gap: 2.48,
            cell_detail_gap: 1.65,
            summary_w: 0.48,
            summary_gap: 13.22,
            summary_pad_top: 6.61,
            total_margin_top: 9.92,
            footer_pad_top: 45.45,
            settle_margin_bottom: 19.83,
            settle_pad_top: 15.70,
            pay_pad_top: 23.14,
            pay_pad_x: 18.18,
            pay_pad_bottom: 19.83,
            pay_method_gap: 13.22,
            pay_dt_min_w: 90.90,
            pay_dl_col_gap: 16.53,
            pay_dl_margin_top: 4.13,
            sig_margin_top: 14.88,
            sig_img_max_w: 148.75,
            sig_img_max_h: 51.24,
            sig_note_gap: 3.31,
            prose_p_gap: 6.61,
            prose_list_top: 3.31,
            prose_list_indent: 18.18,
            quote_gap: 6.61,
            quote_indent: 9.92,
            link_underline_offset: 1.65,
            blocked_asset_pad: 6.61,
        }
    }
}
#[derive(Clone)]
struct Resolved {
    geometry: Geometry,
    format: RenderFormat,
    tokens: ThemeTokens,
    accent: [u8; 3],
    font_id: String,
    font_weight: crate::FontWeight,
    font_scale: u8,
    density_space: f32,
    png_scale: u8,
    line_advance: f32,
    frame_inset: u8,
    upem: f32,
    ascender: f32,
    descender: f32,
    advance: f32,
    font_bytes: &'static [u8],
    semibold_bytes: &'static [u8],
    font_weight_number: u16,
    semibold_weight_number: u16,
    raster: RasterFont,
    semibold_raster: RasterFont,
    /// Parsed shaping faces are kept for the complete render.
    shaping: Arc<rustybuzz::Face<'static>>,
    semibold_shaping: Arc<rustybuzz::Face<'static>>,
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
    display_width: f32,
    display_height: f32,
}
#[derive(Clone)]
struct TextRow {
    text: String,
    runs: Vec<InlineRun>,
    link: Option<String>,
    edit_path: Option<String>,
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
    QuoteMarker,
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
    alignments: Vec<TableAlignment>,
    edit_paths: Vec<Option<String>>,
}
#[derive(Clone)]
enum Block {
    Title,
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
    OwnedImage {
        image: ImageItem,
        owner: &'static str,
    },
    Payment {
        methods: Vec<crate::PaymentMethod>,
        gap: u8,
    },
    Signature {
        name: String,
        label: String,
        image: Option<ImageItem>,
        image_alt: Option<String>,
        gap: u8,
    },
    AmountInWords {
        label: String,
        text: String,
    },
    Total,
    PageBreak,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedAmountInWords {
    pub label: String,
    pub amount: Decimal,
    pub currency: String,
    pub text: String,
}
#[derive(Clone)]
struct Page {
    blocks: Vec<Block>,
}
pub const PREPARED_RENDER_VERSION: u16 = 1;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedRender {
    pub version: u16,
    pub format: RenderFormat,
    pub pages: Vec<PreparedPage>,
    pub images: Vec<PreparedImage>,
    pub width: u32,
    pub height: u32,
    pub currency: String,
    pub grand_total: Decimal,
    pub money_format: String,
    pub amount_in_words: Option<Vec<PreparedAmountInWords>>,
    pub warnings: Vec<RenderWarning>,
    pub source_revision: String,
    pub plan_digest: [u8; 32],
    pub tokens: ThemeTokens,
    pub accent: [u8; 3],
    pub font: String,
    pub font_weight: crate::FontWeight,
    pub font_scale: u8,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub density_space: f32,
    pub png_scale: u8,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub line_advance: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub upem: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub ascender: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub descender: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub advance: f32,
    pub tree: Vec<PreparedNode>,
    pub semantic: PreparedSemantic,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedSemantic {
    pub title: String,
    pub number: String,
    pub kind: String,
    pub issued: String,
    pub due: Option<String>,
    pub terms: Option<String>,
    pub currency: String,
    pub from: PreparedParty,
    pub bill_to: PreparedParty,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedParty {
    pub name: String,
    pub address: Vec<String>,
    pub email: Option<String>,
    pub website: Option<String>,
    pub identifiers: Vec<(String, String)>,
    pub logo_alt: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedPage {
    pub items: Vec<PreparedItem>,
    pub links: Vec<PreparedLink>,
    pub blocks: Vec<PreparedBlock>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedNode {
    pub role: String,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedTextRow {
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedTableRow {
    pub cells: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PreparedBlock {
    Text {
        title: String,
        rows: Vec<PreparedTextRow>,
        gap: u8,
    },
    Table {
        title: String,
        headings: Vec<String>,
        rows: Vec<PreparedTableRow>,
        gap: u8,
    },
    OwnedImage {
        image: usize,
        owner: String,
    },
    Payment {
        methods: Vec<crate::PaymentMethod>,
        gap: u8,
    },
    Signature {
        name: String,
        label: String,
        image: Option<usize>,
        image_alt: Option<String>,
        gap: u8,
    },
    AmountInWords {
        label: String,
        text: String,
    },
    Total,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedItem {
    pub node: usize,
    pub edit_path: Option<String>,
    pub primitive: PreparedPrimitive,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedLink {
    pub href: String,
    pub label: String,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub x: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub y: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub width: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub height: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedImage {
    pub alt: String,
    pub mime: String,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub display_width: f32,
    #[serde(serialize_with = "canonical_float::serialize")]
    pub display_height: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedSpan {
    pub text: String,
    pub face: String,
    pub slant: String,
    pub underline: bool,
    pub href: Option<String>,
    pub color: [u8; 3],
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PreparedPrimitive {
    Rect {
        #[serde(serialize_with = "canonical_float::serialize")]
        x: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        y: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        w: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        h: f32,
        fill: [u8; 3],
    },
    Stroke {
        #[serde(serialize_with = "canonical_float::serialize")]
        x: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        y: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        w: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        h: f32,
        dash: String,
        color: [u8; 3],
    },
    Rule {
        #[serde(serialize_with = "canonical_float::serialize")]
        x: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        y: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        w: f32,
        dash: String,
        color: [u8; 3],
    },
    VRule {
        #[serde(serialize_with = "canonical_float::serialize")]
        x: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        y: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        h: f32,
        dash: String,
        color: [u8; 3],
    },
    Text {
        #[serde(serialize_with = "canonical_float::serialize")]
        x: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        baseline: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        size: f32,
        align: String,
        #[serde(serialize_with = "canonical_float::serialize")]
        advance: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        tracking: f32,
        spans: Vec<PreparedSpan>,
    },
    Image {
        #[serde(serialize_with = "canonical_float::serialize")]
        x: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        y: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        w: f32,
        #[serde(serialize_with = "canonical_float::serialize")]
        h: f32,
        index: usize,
    },
}
impl PreparedRender {
    pub(crate) fn computed_digest(&self) -> [u8; 32] {
        let mut copy = self.clone();
        copy.plan_digest = [0; 32];
        let bytes = serde_json::to_vec(&copy).expect("PreparedRender is serializable");
        crate::sha256_digest(&bytes)
    }
}
#[derive(Clone)]
struct LinkBox {
    href: String,
    label: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Face {
    Regular,
    Semibold,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slant {
    Upright,
    Oblique,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Align {
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dash {
    Solid,
    Dashed,
}
#[derive(Clone)]
struct Span {
    text: String,
    face: Face,
    slant: Slant,
    underline: bool,
    href: Option<String>,
    color: [u8; 3],
}
#[derive(Clone)]
enum Primitive {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: [u8; 3],
    },
    Stroke {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        dash: Dash,
        color: [u8; 3],
    },
    Rule {
        x: f32,
        y: f32,
        w: f32,
        dash: Dash,
        color: [u8; 3],
    },
    VRule {
        x: f32,
        y: f32,
        h: f32,
        dash: Dash,
        color: [u8; 3],
    },
    Text {
        x: f32,
        baseline: f32,
        size: f32,
        align: Align,
        advance: f32,
        tracking: f32,
        spans: Vec<Span>,
        edit_path: Option<String>,
    },
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        index: usize,
        edit_path: Option<String>,
    },
}
#[derive(Clone, Copy)]
enum Band {
    Fill = 0,
    Knockout = 1,
    Ink = 2,
}
#[derive(Clone)]
struct Placed {
    band: Band,
    node: usize,
    primitive: Primitive,
}
#[derive(Clone)]
struct DisplayPage {
    items: Vec<Placed>,
    links: Vec<LinkBox>,
}
#[derive(Clone)]
struct Plan {
    resolved: Resolved,
    pages: Vec<Page>,
    positioned: Vec<DisplayPage>,
    tree: Vec<Node>,
    images: Vec<ImageItem>,
    grand_total: Decimal,
    currency: String,
    semantic: PreparedSemantic,
    money_format: String,
    amount_in_words: Option<Vec<PreparedAmountInWords>>,
    warnings: Vec<RenderWarning>,
}
#[derive(Clone)]
struct Node {
    role: &'static str,
    label: String,
}
impl PreparedSemantic {
    fn from_document(doc: &Document) -> Self {
        let party = |party: &crate::Party| PreparedParty {
            name: party.name.clone(),
            address: party.address.clone(),
            email: party.email.clone(),
            website: party.website.clone(),
            identifiers: party
                .identifiers
                .iter()
                .map(|id| (id.key.clone(), id.value.clone()))
                .collect(),
            logo_alt: party.logo.as_ref().map(|image| image.alt.clone()),
        };
        Self {
            title: doc.title.clone(),
            number: doc.metadata.number.clone(),
            kind: doc.metadata.kind.clone(),
            issued: doc.metadata.issued.to_string(),
            due: doc.metadata.due.as_ref().map(ToString::to_string),
            terms: doc.metadata.terms.clone(),
            currency: doc.metadata.currency.clone(),
            from: party(&doc.from),
            bill_to: party(&doc.bill_to),
        }
    }
}
impl PreparedRender {
    fn from_plan(plan: &Plan, warnings: Vec<RenderWarning>) -> Result<Self, RenderError> {
        let image_index = |image: &ImageItem| {
            plan.images.iter().position(|candidate| {
                candidate.alt == image.alt
                    && candidate.mime == image.mime
                    && candidate.bytes.as_ref() == image.bytes.as_ref()
            })
        };
        let prepared_block = |block: &Block| -> Result<Option<PreparedBlock>, RenderError> {
            Ok(match block {
                Block::Text { title, rows, gap } => Some(PreparedBlock::Text {
                    title: title.clone(),
                    rows: rows
                        .iter()
                        .map(|row| PreparedTextRow {
                            text: row.text.clone(),
                        })
                        .collect(),
                    gap: *gap,
                }),
                Block::Table {
                    title,
                    headings,
                    rows,
                    gap,
                } => Some(PreparedBlock::Table {
                    title: title.clone(),
                    headings: headings.clone(),
                    rows: rows
                        .iter()
                        .map(|row| PreparedTableRow {
                            cells: row.cells.clone(),
                        })
                        .collect(),
                    gap: *gap,
                }),
                Block::OwnedImage { image, owner } => Some(PreparedBlock::OwnedImage {
                    image: image_index(image).ok_or_else(|| {
                        RenderError::InvalidOption("prepared image is missing".into())
                    })?,
                    owner: (*owner).into(),
                }),
                Block::Payment { methods, gap } => Some(PreparedBlock::Payment {
                    methods: methods.clone(),
                    gap: *gap,
                }),
                Block::Signature {
                    name,
                    label,
                    image,
                    image_alt,
                    gap,
                } => Some(PreparedBlock::Signature {
                    name: name.clone(),
                    label: label.clone(),
                    image: image.as_ref().and_then(image_index),
                    image_alt: image_alt.clone(),
                    gap: *gap,
                }),
                Block::AmountInWords { label, text } => Some(PreparedBlock::AmountInWords {
                    label: label.clone(),
                    text: text.clone(),
                }),
                Block::Total => Some(PreparedBlock::Total),
                Block::PageBreak | Block::Title => None,
            })
        };
        let prepared_page = |page: &DisplayPage,
                             source: &Page|
         -> Result<PreparedPage, RenderError> {
            Ok(PreparedPage {
                items: page
                    .items
                    .iter()
                    .map(|placed| {
                        let edit_path = match &placed.primitive {
                            Primitive::Text { edit_path, .. }
                            | Primitive::Image { edit_path, .. } => edit_path.clone(),
                            _ => None,
                        };
                        PreparedItem {
                            node: placed.node,
                            edit_path,
                            primitive: match &placed.primitive {
                                Primitive::Rect { x, y, w, h, fill } => PreparedPrimitive::Rect {
                                    x: *x,
                                    y: *y,
                                    w: *w,
                                    h: *h,
                                    fill: *fill,
                                },
                                Primitive::Stroke {
                                    x,
                                    y,
                                    w,
                                    h,
                                    dash,
                                    color,
                                } => PreparedPrimitive::Stroke {
                                    x: *x,
                                    y: *y,
                                    w: *w,
                                    h: *h,
                                    dash: dash_name(*dash).into(),
                                    color: *color,
                                },
                                Primitive::Rule {
                                    x,
                                    y,
                                    w,
                                    dash,
                                    color,
                                } => PreparedPrimitive::Rule {
                                    x: *x,
                                    y: *y,
                                    w: *w,
                                    dash: dash_name(*dash).into(),
                                    color: *color,
                                },
                                Primitive::VRule {
                                    x,
                                    y,
                                    h,
                                    dash,
                                    color,
                                } => PreparedPrimitive::VRule {
                                    x: *x,
                                    y: *y,
                                    h: *h,
                                    dash: dash_name(*dash).into(),
                                    color: *color,
                                },
                                Primitive::Text {
                                    x,
                                    baseline,
                                    size,
                                    align,
                                    advance,
                                    tracking,
                                    spans,
                                    ..
                                } => PreparedPrimitive::Text {
                                    x: *x,
                                    baseline: *baseline,
                                    size: *size,
                                    align: align_name(*align).into(),
                                    advance: *advance,
                                    tracking: *tracking,
                                    spans: spans
                                        .iter()
                                        .map(|span| PreparedSpan {
                                            text: span.text.clone(),
                                            face: face_name(span.face).into(),
                                            slant: slant_name(span.slant).into(),
                                            underline: span.underline,
                                            href: span.href.clone(),
                                            color: span.color,
                                        })
                                        .collect(),
                                },
                                Primitive::Image {
                                    x, y, w, h, index, ..
                                } => PreparedPrimitive::Image {
                                    x: *x,
                                    y: *y,
                                    w: *w,
                                    h: *h,
                                    index: *index,
                                },
                            },
                        }
                    })
                    .collect(),
                links: page
                    .links
                    .iter()
                    .map(|link| PreparedLink {
                        href: link.href.clone(),
                        label: link.label.clone(),
                        x: link.x,
                        y: link.y,
                        width: link.width,
                        height: link.height,
                    })
                    .collect(),
                blocks: source
                    .blocks
                    .iter()
                    .filter_map(|block| prepared_block(block).transpose())
                    .collect::<Result<_, _>>()?,
            })
        };
        let width = match plan.resolved.format {
            RenderFormat::Png => PAGE_WIDTH * u32::from(plan.resolved.png_scale),
            _ => PAGE_WIDTH,
        };
        let height = match plan.resolved.format {
            RenderFormat::Html => PAGE_HEIGHT * plan.positioned.len() as u32,
            RenderFormat::Png => {
                PAGE_HEIGHT * plan.positioned.len() as u32 * u32::from(plan.resolved.png_scale)
            }
            RenderFormat::Pdf => PAGE_HEIGHT,
        };
        let pages = plan
            .positioned
            .iter()
            .zip(&plan.pages)
            .map(|(positioned, source)| prepared_page(positioned, source))
            .collect::<Result<_, _>>()?;
        let images = plan
            .images
            .iter()
            .map(|image| PreparedImage {
                alt: image.alt.clone(),
                mime: image.mime.clone(),
                bytes: image.bytes.to_vec(),
                width: image.width,
                height: image.height,
                display_width: image.display_width,
                display_height: image.display_height,
            })
            .collect();
        let mut prepared = Self {
            version: PREPARED_RENDER_VERSION,
            format: plan.resolved.format,
            pages,
            semantic: plan.semantic.clone(),
            images,
            width,
            height,
            currency: plan.currency.clone(),
            grand_total: plan.grand_total,
            money_format: plan.money_format.clone(),
            amount_in_words: plan.amount_in_words.clone(),
            warnings,
            source_revision: String::new(),
            plan_digest: [0; 32],
            tokens: plan.resolved.tokens,
            accent: plan.resolved.accent,
            font: plan.resolved.font_id.clone(),
            font_weight: plan.resolved.font_weight,
            font_scale: plan.resolved.font_scale,
            density_space: plan.resolved.density_space,
            png_scale: plan.resolved.png_scale,
            line_advance: plan.resolved.line_advance,
            upem: plan.resolved.upem,
            ascender: plan.resolved.ascender,
            descender: plan.resolved.descender,
            advance: plan.resolved.advance,
            tree: plan
                .tree
                .iter()
                .map(|node| PreparedNode {
                    role: node.role.to_string(),
                    label: node.label.clone(),
                })
                .collect(),
        };
        prepared.plan_digest = prepared.computed_digest();
        Ok(prepared)
    }
}
pub(crate) fn prepare_render(
    doc: &Document,
    options: RenderOptions,
) -> Result<PreparedRender, RenderError> {
    let requested_png_scale = options.png_scale.unwrap_or(1);
    let mut plan = layout(doc, resolve(&doc.config, options)?)?;
    let mut warnings = std::mem::take(&mut plan.warnings);
    if plan.resolved.format == RenderFormat::Png && requested_png_scale == 2 {
        let page_pixels =
            PAGE_WIDTH as usize * PAGE_HEIGHT as usize * usize::from(requested_png_scale).pow(2);
        let total_pixels = page_pixels.saturating_mul(plan.pages.len());
        if page_pixels > MAX_PNG_PIXELS || total_pixels > MAX_PNG_TOTAL_PIXELS {
            plan.resolved.png_scale = 1;
            warnings.push(RenderWarning {
                code: "PNG_SCALE_REDUCED".into(),
                message: format!("png-scale reduced to 1 for {} pages", plan.pages.len()),
            });
        }
    }
    PreparedRender::from_plan(&plan, warnings)
}

fn prepared_block_to_block(
    block: &PreparedBlock,
    images: &[ImageItem],
) -> Result<Block, RenderError> {
    Ok(match block {
        PreparedBlock::Text { title, rows, gap } => Block::Text {
            title: title.clone(),
            rows: rows
                .iter()
                .map(|row| TextRow {
                    text: row.text.clone(),
                    runs: Vec::new(),
                    link: None,
                    edit_path: None,
                })
                .collect(),
            gap: *gap,
        },
        PreparedBlock::Table {
            title,
            headings,
            rows,
            gap,
        } => Block::Table {
            title: title.clone(),
            headings: headings.clone(),
            rows: rows
                .iter()
                .map(|row| TableRow {
                    cells: row.cells.clone(),
                    alignments: vec![TableAlignment::None; row.cells.len()],
                    edit_paths: vec![None; row.cells.len()],
                })
                .collect(),
            gap: *gap,
        },
        PreparedBlock::OwnedImage { image, owner } => Block::OwnedImage {
            image: images.get(*image).cloned().ok_or_else(|| {
                RenderError::InvalidOption("prepared image index out of bounds".into())
            })?,
            owner: match owner.as_str() {
                "From" => "From",
                "Bill to" => "Bill to",
                _ => "section",
            },
        },
        PreparedBlock::Payment { methods, gap } => Block::Payment {
            methods: methods.clone(),
            gap: *gap,
        },
        PreparedBlock::Signature {
            name,
            label,
            image,
            image_alt,
            gap,
        } => Block::Signature {
            name: name.clone(),
            label: label.clone(),
            image: image
                .map(|index| {
                    images.get(index).cloned().ok_or_else(|| {
                        RenderError::InvalidOption("prepared image index out of bounds".into())
                    })
                })
                .transpose()?,
            image_alt: image_alt.clone(),
            gap: *gap,
        },
        PreparedBlock::AmountInWords { label, text } => Block::AmountInWords {
            label: label.clone(),
            text: text.clone(),
        },
        PreparedBlock::Total => Block::Total,
    })
}

fn prepared_encoding(plan: &PreparedRender) -> Result<Plan, RenderError> {
    if plan.version != PREPARED_RENDER_VERSION {
        return Err(RenderError::InvalidOption(
            "unsupported prepared render version".into(),
        ));
    }
    if plan.plan_digest != plan.computed_digest() {
        return Err(RenderError::InvalidOption(
            "prepared render plan digest is invalid".into(),
        ));
    }
    if plan.pages.is_empty() || plan.pages.len() > MAX_PAGES {
        return Err(RenderError::InvalidOption(
            "prepared render plan has an invalid page count".into(),
        ));
    }
    let asset = generated::FONT_ASSETS
        .iter()
        .find(|asset| asset.id == plan.font)
        .ok_or_else(|| RenderError::UnsupportedFont(plan.font.clone()))?;
    let (font_bytes, font_weight_number) = match plan.font_weight {
        crate::FontWeight::Regular => (asset.regular, asset.regular_weight),
        crate::FontWeight::Semibold => (asset.semibold, asset.semibold_weight),
    };
    let semibold_bytes = asset.semibold;
    let raster = RasterFont::from_bytes(font_bytes, fontdue::FontSettings::default())
        .map_err(|e| RenderError::Font(format!("{e:?}")))?;
    let semibold_raster = RasterFont::from_bytes(semibold_bytes, fontdue::FontSettings::default())
        .map_err(|e| RenderError::Font(format!("{e:?}")))?;
    let shaping = Arc::new(
        rustybuzz::Face::from_slice(font_bytes, 0)
            .ok_or_else(|| RenderError::Font("invalid font face".into()))?,
    );
    let semibold_shaping = Arc::new(
        rustybuzz::Face::from_slice(semibold_bytes, 0)
            .ok_or_else(|| RenderError::Font("invalid semibold font face".into()))?,
    );
    let resolved = Resolved {
        geometry: Geometry::canonical(),
        format: plan.format,
        tokens: plan.tokens,
        accent: plan.accent,
        font_id: plan.font.clone(),
        font_weight: plan.font_weight,
        font_scale: plan.font_scale,
        density_space: plan.density_space,
        png_scale: plan.png_scale,
        line_advance: plan.line_advance,
        frame_inset: 54,
        upem: plan.upem,
        ascender: plan.ascender,
        descender: plan.descender,
        advance: plan.advance,
        font_bytes,
        semibold_bytes,
        font_weight_number,
        semibold_weight_number: asset.semibold_weight,
        raster,
        semibold_raster,
        shaping,
        semibold_shaping,
        assets: Vec::new(),
    };
    let images = plan
        .images
        .iter()
        .map(|image| {
            let decoded = ImageReader::new(Cursor::new(&image.bytes))
                .with_guessed_format()
                .map_err(|e| RenderError::InvalidAsset(e.to_string()))?
                .decode()
                .map_err(|e| RenderError::InvalidAsset(e.to_string()))?;
            let rgba = decoded.to_rgba8();
            if rgba.width() != image.width || rgba.height() != image.height {
                return Err(RenderError::InvalidAsset(
                    "prepared image dimensions changed".into(),
                ));
            }
            Ok(ImageItem {
                alt: image.alt.clone(),
                mime: image.mime.clone(),
                bytes: Arc::from(image.bytes.clone()),
                rgba: rgba.into_raw(),
                width: image.width,
                height: image.height,
                display_width: image.display_width,
                display_height: image.display_height,
            })
        })
        .collect::<Result<Vec<_>, RenderError>>()?;
    let positioned = plan
        .pages
        .iter()
        .map(|page| {
            let items = page
                .items
                .iter()
                .map(|item| {
                    let primitive = match &item.primitive {
                        PreparedPrimitive::Rect { x, y, w, h, fill } => Primitive::Rect {
                            x: *x,
                            y: *y,
                            w: *w,
                            h: *h,
                            fill: *fill,
                        },
                        PreparedPrimitive::Stroke {
                            x,
                            y,
                            w,
                            h,
                            dash,
                            color,
                        } => Primitive::Stroke {
                            x: *x,
                            y: *y,
                            w: *w,
                            h: *h,
                            dash: parse_dash(dash)?,
                            color: *color,
                        },
                        PreparedPrimitive::Rule {
                            x,
                            y,
                            w,
                            dash,
                            color,
                        } => Primitive::Rule {
                            x: *x,
                            y: *y,
                            w: *w,
                            dash: parse_dash(dash)?,
                            color: *color,
                        },
                        PreparedPrimitive::VRule {
                            x,
                            y,
                            h,
                            dash,
                            color,
                        } => Primitive::VRule {
                            x: *x,
                            y: *y,
                            h: *h,
                            dash: parse_dash(dash)?,
                            color: *color,
                        },
                        PreparedPrimitive::Text {
                            x,
                            baseline,
                            size,
                            align,
                            advance,
                            tracking,
                            spans,
                        } => Primitive::Text {
                            x: *x,
                            baseline: *baseline,
                            size: *size,
                            align: parse_align(align)?,
                            advance: *advance,
                            tracking: *tracking,
                            spans: spans
                                .iter()
                                .map(|span| {
                                    Ok(Span {
                                        text: span.text.clone(),
                                        face: parse_face(&span.face)?,
                                        slant: parse_slant(&span.slant)?,
                                        underline: span.underline,
                                        href: span.href.clone(),
                                        color: span.color,
                                    })
                                })
                                .collect::<Result<_, RenderError>>()?,
                            edit_path: None,
                        },
                        PreparedPrimitive::Image { x, y, w, h, index } => {
                            if *index >= plan.images.len() {
                                return Err(RenderError::InvalidOption(
                                    "prepared image index out of bounds".into(),
                                ));
                            }
                            Primitive::Image {
                                x: *x,
                                y: *y,
                                w: *w,
                                h: *h,
                                index: *index,
                                edit_path: None,
                            }
                        }
                    };
                    Ok(Placed {
                        band: Band::Ink,
                        node: item.node,
                        primitive,
                    })
                })
                .collect::<Result<Vec<_>, RenderError>>()?;
            let links = page
                .links
                .iter()
                .map(|link| LinkBox {
                    href: link.href.clone(),
                    label: link.label.clone(),
                    x: link.x,
                    y: link.y,
                    width: link.width,
                    height: link.height,
                })
                .collect();
            Ok(DisplayPage { items, links })
        })
        .collect::<Result<Vec<_>, RenderError>>()?;
    let pages = plan
        .pages
        .iter()
        .map(|page| {
            Ok(Page {
                blocks: page
                    .blocks
                    .iter()
                    .map(|block| prepared_block_to_block(block, &images))
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect::<Result<Vec<_>, RenderError>>()?;
    let tree = plan
        .tree
        .iter()
        .map(|node| {
            let role = match node.role.as_str() {
                "sheet" => "sheet",
                "header" => "header",
                "metadata" => "metadata",
                "parties" => "parties",
                "party" => "party",
                "sections" => "sections",
                "footer" => "footer",
                "settlements" => "settlements",
                "payment" => "payment",
                "signature" => "signature",
                "section" => "section",
                "caption" => "caption",
                "thead" => "thead",
                "tbody" => "tbody",
                "tr" => "tr",
                "th" => "th",
                "td" => "td",
                "prose" => "prose",
                "list" => "list",
                _ => {
                    return Err(RenderError::InvalidOption(
                        "invalid prepared semantic role".into(),
                    ));
                }
            };
            Ok(Node {
                role,
                label: node.label.clone(),
            })
        })
        .collect::<Result<Vec<_>, RenderError>>()?;
    Ok(Plan {
        resolved,
        pages,
        positioned,
        tree,
        images,
        grand_total: plan.grand_total,
        currency: plan.currency.clone(),
        semantic: plan.semantic.clone(),
        money_format: plan.money_format.clone(),
        amount_in_words: plan.amount_in_words.clone(),
        warnings: plan.warnings.clone(),
    })
}
fn dash_name(value: Dash) -> &'static str {
    match value {
        Dash::Solid => "solid",
        Dash::Dashed => "dashed",
    }
}
fn face_name(value: Face) -> &'static str {
    match value {
        Face::Regular => "regular",
        Face::Semibold => "semibold",
    }
}
fn slant_name(value: Slant) -> &'static str {
    match value {
        Slant::Upright => "upright",
        Slant::Oblique => "oblique",
    }
}
fn align_name(value: Align) -> &'static str {
    match value {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
    }
}
fn parse_dash(value: &str) -> Result<Dash, RenderError> {
    match value {
        "solid" => Ok(Dash::Solid),
        "dashed" => Ok(Dash::Dashed),
        _ => Err(RenderError::InvalidOption("invalid prepared dash".into())),
    }
}
fn parse_face(value: &str) -> Result<Face, RenderError> {
    match value {
        "regular" => Ok(Face::Regular),
        "semibold" => Ok(Face::Semibold),
        _ => Err(RenderError::InvalidOption("invalid prepared face".into())),
    }
}
fn parse_slant(value: &str) -> Result<Slant, RenderError> {
    match value {
        "upright" => Ok(Slant::Upright),
        "oblique" => Ok(Slant::Oblique),
        _ => Err(RenderError::InvalidOption("invalid prepared slant".into())),
    }
}
fn parse_align(value: &str) -> Result<Align, RenderError> {
    match value {
        "left" => Ok(Align::Left),
        "center" => Ok(Align::Center),
        "right" => Ok(Align::Right),
        _ => Err(RenderError::InvalidOption(
            "invalid prepared alignment".into(),
        )),
    }
}
#[cfg(test)]
fn render(source: &str, options: RenderOptions) -> Result<RenderResult, RenderError> {
    if source.len() > crate::MAX_SOURCE_BYTES {
        return Err(RenderError::SourceTooLarge {
            limit: crate::MAX_SOURCE_BYTES,
        });
    }
    let doc = document(source)
        .map_err(|report| RenderError::InvalidDocument(report.diagnostics().to_vec()))?;
    render_document(&doc, options)
}

#[cfg(test)]
fn render_document(doc: &Document, options: RenderOptions) -> Result<RenderResult, RenderError> {
    let prepared = prepare_render(doc, options)?;
    render_prepared(&prepared)
}
pub(crate) fn render_prepared(plan: &PreparedRender) -> Result<RenderResult, RenderError> {
    let encoded_plan = prepared_encoding(plan)?;
    let (bytes, width, height) = match encoded_plan.resolved.format {
        RenderFormat::Html => (
            encode_html(&encoded_plan)?,
            PAGE_WIDTH,
            PAGE_HEIGHT.saturating_mul(encoded_plan.pages.len() as u32),
        ),
        RenderFormat::Pdf => (encode_pdf(&encoded_plan)?, PAGE_WIDTH, PAGE_HEIGHT),
        RenderFormat::Png => encode_png(&encoded_plan)?,
    };
    if bytes.len() > MAX_RENDERED_BYTES {
        return Err(RenderError::OutputTooLarge {
            limit: MAX_RENDERED_BYTES,
        });
    }
    Ok(RenderResult {
        bytes,
        mime: plan.format.mime().into(),
        extension: plan.format.extension().into(),
        pages: plan.pages.len(),
        width,
        height,
        warnings: plan.warnings.clone(),
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
/// Returns an authored accent adjusted toward ink until it reaches WCAG AA.
///
/// Mixing intentionally happens in sRGB byte space to match the editor.
pub(crate) fn readable_accent(accent: [u8; 3], paper: [u8; 3], ink: [u8; 3]) -> ([u8; 3], u8, f32) {
    fn channel(v: u8) -> f32 {
        let v = f32::from(v) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    fn luminance(c: [u8; 3]) -> f32 {
        0.2126 * channel(c[0]) + 0.7152 * channel(c[1]) + 0.0722 * channel(c[2])
    }
    fn ratio(a: [u8; 3], b: [u8; 3]) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    let raw_ratio = ratio(accent, paper);
    if raw_ratio >= 4.5 {
        return (accent, 0, raw_ratio);
    }
    for step in 1..=20u8 {
        let t = f32::from(step) / 20.0;
        let mixed = [
            (f32::from(accent[0]) + (f32::from(ink[0]) - f32::from(accent[0])) * t).round() as u8,
            (f32::from(accent[1]) + (f32::from(ink[1]) - f32::from(accent[1])) * t).round() as u8,
            (f32::from(accent[2]) + (f32::from(ink[2]) - f32::from(accent[2])) * t).round() as u8,
        ];
        let value = ratio(mixed, paper);
        if value >= 4.5 {
            return (mixed, step, value);
        }
    }
    (ink, 20, ratio(ink, paper))
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
    let authored_accent = o
        .accent
        .or_else(|| c.accent.as_ref().map(ToString::to_string));
    let accent = authored_accent
        .as_deref()
        .map(parse_rgb)
        .transpose()?
        .map(|value| readable_accent(value, tokens.paper, tokens.ink).0)
        .unwrap_or(tokens.accent);
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
    let png_scale = o.png_scale.unwrap_or(1);
    if !matches!(png_scale, 1 | 2) {
        return Err(RenderError::InvalidOption(
            "png-scale must be 1 or 2".into(),
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
    let metrics_face = rustybuzz::Face::from_slice(bytes, 0)
        .ok_or_else(|| RenderError::Font("invalid font face".into()))?;
    let shaping = Arc::new(metrics_face);
    let semibold_shaping = Arc::new(
        rustybuzz::Face::from_slice(semibold_bytes, 0)
            .ok_or_else(|| RenderError::Font("invalid semibold font face".into()))?,
    );
    let upem = shaping.units_per_em() as f32;
    let ascender = shaping.ascender() as f32;
    let descender = (-shaping.descender()) as f32;
    let advance = shaping
        .glyph_index('M')
        .and_then(|gid| shaping.glyph_hor_advance(gid))
        .unwrap_or(shaping.units_per_em() as u16) as f32;
    let geometry = Geometry::canonical();
    let type_scale = f32::from(font_scale) / 100.0;
    let density_space = if density == "compact" { 0.78 } else { 1.0 } * type_scale;
    let mut asset_total = 0usize;
    let assets = o
        .assets
        .into_iter()
        .map(|asset| {
            if asset.bytes.is_empty() || asset.bytes.len() > MAX_ASSET_BYTES {
                return Err(RenderError::InvalidAsset("asset exceeds 1 MiB".into()));
            }
            asset_total = asset_total
                .checked_add(asset.bytes.len())
                .ok_or_else(|| RenderError::InvalidAsset("asset byte budget overflow".into()))?;
            if asset_total > MAX_ASSET_TOTAL_BYTES {
                return Err(RenderError::InvalidAsset(
                    "aggregate asset byte budget exceeded".into(),
                ));
            }
            Ok(ResolvedAsset {
                source: asset.source,
                bytes: Arc::from(asset.bytes),
                mime: asset.mime,
            })
        })
        .collect::<Result<Vec<_>, RenderError>>()?;
    Ok(Resolved {
        geometry,
        format: o.format,
        tokens,
        accent,
        font_id: font,
        font_weight: weight,
        font_scale,
        density_space,
        png_scale,
        line_advance: 12.06 * f32::from(font_scale) / 100.0,
        frame_inset,
        upem,
        ascender,
        descender,
        advance,
        font_bytes: bytes,
        shaping,
        semibold_shaping,
        semibold_bytes,
        font_weight_number: number,
        semibold_weight_number: asset.semibold_weight,
        raster,
        semibold_raster,
        assets,
    })
}
fn first_flow_origin(doc: &Document, r: &Resolved) -> f32 {
    let g = &r.geometry;
    let content_top = f32::from(r.frame_inset) + g.gutter_top;
    let heading = g.font_heading * f32::from(r.font_scale) / 100.0;
    let meta_len =
        3 + usize::from(doc.metadata.due.is_some()) + usize::from(doc.metadata.terms.is_some()) + 1;
    let meta_h =
        meta_len as f32 * r.line_advance + meta_len.saturating_sub(1) as f32 * g.meta_row_gap;
    let header_h = g
        .header_min_h
        .max(g.header_pad_top + (heading * 1.12).max(meta_h) + g.header_pad_bottom);
    let party_h = |party: &crate::Party| {
        let mut rows = 2 + party.address.len() + party.identifiers.len();
        rows += usize::from(party.email.is_some()) + usize::from(party.website.is_some());
        rows += usize::from(party.logo.is_some());
        2.0 * r.line_advance
            + g.label_gap
            + rows.saturating_sub(2) as f32 * r.line_advance
            + if party.identifiers.is_empty() {
                0.0
            } else {
                g.label_gap
            }
            + if party.email.is_some() || party.website.is_some() {
                g.label_gap
            } else {
                0.0
            }
    };
    let parties_h = g.parties_min_h.max(
        g.parties_pad_top + party_h(&doc.from).max(party_h(&doc.bill_to)) + g.parties_pad_bottom,
    );
    content_top + header_h + g.parties_pad_top + parties_h
}
fn positioned_height(block: &Block, r: &Resolved) -> f32 {
    let g = &r.geometry;
    let line = r.line_advance;
    let density = r.density_space;
    match block {
        Block::Title => 0.0,
        Block::Text { rows, title, gap } => {
            if title == "From" || title == "Bill to" {
                0.0
            } else {
                let title_h = if title == "Payment" {
                    0.0
                } else {
                    g.section_pad_top * density
                };
                if title == "Payment" {
                    g.footer_pad_top * density
                        + rows.len() as f32 * (line + g.pay_dl_margin_top * density)
                } else {
                    title_h + rows.len() as f32 * line + f32::from(*gap) * line
                }
            }
        }
        Block::Table { headings, rows, .. } => {
            let widths = column_widths(headings.len(), content_width(r));
            let mut body_height = 0.0;
            for row in rows.iter().filter(|row| {
                !row.cells
                    .first()
                    .is_some_and(|v| v.eq_ignore_ascii_case("subtotal"))
            }) {
                let lines = row
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| {
                        let right = i + 1 == headings.len()
                            || row.alignments.get(i).copied() == Some(TableAlignment::Right);
                        if right {
                            1
                        } else {
                            let pad = if i == 0 { 0.0 } else { g.cell_pad_x };
                            let right_pad = if i + 1 == headings.len() {
                                0.0
                            } else {
                                g.cell_pad_x
                            };
                            wrap_runs(
                                r,
                                &inline_runs(cell),
                                (widths.get(i).copied().unwrap_or(0.0) - pad - right_pad).max(1.0),
                                g.font_body * f32::from(r.font_scale) / 100.0,
                            )
                            .len()
                        }
                    })
                    .max()
                    .unwrap_or(1);
                body_height +=
                    lines as f32 * line + g.cell_pad_bottom * density + g.cell_pad_top * density;
            }
            g.section_pad_top * density
                + line
                + g.cell_pad_bottom * density
                + g.cell_pad_top * density
                + body_height
                + g.table_end_gap * density
        }
        Block::Payment { methods, .. } => {
            g.pay_pad_top * density
                + g.pay_pad_bottom * density
                + methods
                    .iter()
                    .map(|m| {
                        line + g.pay_dl_margin_top * density
                            + m.fields.len() as f32 * (line + g.pay_dl_margin_top * density)
                    })
                    .sum::<f32>()
        }
        Block::Signature {
            image, image_alt, ..
        } => {
            line + g.sig_margin_top * density
                + image.as_ref().map_or_else(
                    || {
                        image_alt
                            .as_ref()
                            .map_or(0.0, |_| line + g.sig_note_gap * density)
                    },
                    |i| i.display_height + g.sig_note_gap * density,
                )
        }
        Block::OwnedImage { owner, .. } if *owner == "From" || *owner == "Bill to" => 0.0,
        Block::OwnedImage { image, .. } => image.display_height + line,
        Block::AmountInWords { text, .. } => {
            let width = r.geometry.page_w - 2.0 * (f32::from(r.frame_inset) + r.geometry.gutter_x);
            let lines = wrap_runs(r, &inline_runs(text), width, r.geometry.font_detail)
                .len()
                .max(1);
            lines as f32 * r.line_advance + r.geometry.summary_pad_top * density
        }
        Block::Total => g.total_margin_top * density + line + g.summary_pad_top * density,
        Block::PageBreak => 0.0,
    }
}

fn amount_in_words_entries(doc: &Document) -> Option<Vec<PreparedAmountInWords>> {
    if !doc.config.amount_in_words {
        return None;
    }
    let mut entries = Vec::new();
    for section in &doc.ordinary_sections {
        if !section.directives.summary_only {
            if let Some(amount) = section.total {
                entries.push(PreparedAmountInWords {
                    label: format!("{} subtotal", section.title),
                    amount,
                    currency: doc.metadata.currency.clone(),
                    text: crate::amount_in_words(amount, &doc.metadata.currency),
                });
            }
        }
    }
    if doc.ordinary_sections.iter().any(|section| {
        matches!(&section.body, SectionBody::Table(_))
            && !section.directives.summary_only
            && section.total.is_some()
    }) {
        entries.push(PreparedAmountInWords {
            label: "Total due".into(),
            amount: doc.grand_total,
            currency: doc.metadata.currency.clone(),
            text: crate::amount_in_words(doc.grand_total, &doc.metadata.currency),
        });
    }
    if let Some(settlements) = &doc.settlements {
        for row in &settlements.rows {
            if let (Some(value), Some(currency)) = (row.get(3), row.get(4)) {
                if let Ok(amount) = value.parse::<Decimal>() {
                    entries.push(PreparedAmountInWords {
                        label: "Received".into(),
                        amount,
                        currency: currency.clone(),
                        text: crate::amount_in_words(amount, currency),
                    });
                }
            }
        }
    }
    Some(entries)
}
fn is_footer_block(block: &Block) -> bool {
    match block {
        Block::Table { title, .. } => title == "Settlements",
        Block::Payment { .. } | Block::Signature { .. } => true,
        _ => false,
    }
}
fn layout(doc: &Document, resolved: Resolved) -> Result<Plan, RenderError> {
    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    let mut image_budget = 0usize;
    blocks.push(Block::Title);
    if let Some(image) = party_block(
        "From",
        &doc.from,
        &mut blocks,
        &resolved.assets,
        &mut image_budget,
        &mut warnings,
    )? {
        blocks.push(Block::OwnedImage {
            image,
            owner: "From",
        });
    }
    if let Some(image) = party_block(
        "Bill to",
        &doc.bill_to,
        &mut blocks,
        &resolved.assets,
        &mut image_budget,
        &mut warnings,
    )? {
        blocks.push(Block::OwnedImage {
            image,
            owner: "Bill to",
        });
    }
    for (section_index, section) in doc.ordinary_sections.iter().enumerate() {
        section_block(
            section,
            section_index,
            doc,
            &mut blocks,
            &resolved.assets,
            &mut image_budget,
            &mut warnings,
        )?;
        if doc.config.amount_in_words && !section.directives.summary_only {
            if let Some(amount) = section.total {
                blocks.push(Block::AmountInWords {
                    label: format!("{} subtotal in words", section.title),
                    text: crate::amount_in_words(amount, &doc.metadata.currency),
                });
            }
        }
    }
    if doc
        .ordinary_sections
        .iter()
        .any(|section| matches!(&section.body, SectionBody::Table(_)) && section.total.is_some())
    {
        blocks.push(Block::Total);
        if doc.config.amount_in_words {
            blocks.push(Block::AmountInWords {
                label: "Total due in words".into(),
                text: crate::amount_in_words(doc.grand_total, &doc.metadata.currency),
            });
        }
    }
    if let Some(t) = &doc.settlements {
        if doc.settlements_page_break_before {
            blocks.push(Block::PageBreak);
        }
        blocks.push(table_block("Settlements", t, doc, None, 1, None));
        if doc.config.amount_in_words {
            for row in &t.rows {
                if let (Some(value), Some(currency)) = (row.get(3), row.get(4)) {
                    if let Ok(amount) = value.parse::<Decimal>() {
                        blocks.push(Block::AmountInWords {
                            label: "Received in words".into(),
                            text: crate::amount_in_words(amount, currency),
                        });
                    }
                }
            }
        }
    }
    if let Some(payment) = &doc.payment {
        if doc.payment_page_break_before {
            blocks.push(Block::PageBreak);
        }
        blocks.push(Block::Payment {
            methods: payment.methods.clone(),
            gap: 1,
        });
    }
    if let Some(sig) = &doc.signature {
        if doc.signature_page_break_before {
            blocks.push(Block::PageBreak);
        }
        let (image, image_alt) = if let Some(asset) = &sig.image {
            let mut decoded = decode_asset(asset, &resolved.assets, &mut image_budget)?;
            if decoded.is_none() {
                push_asset_warning(&mut warnings, asset);
            }
            if let Some(image) = decoded.as_mut() {
                let scale = (resolved.geometry.sig_img_max_w / image.display_width)
                    .min(resolved.geometry.sig_img_max_h / image.display_height)
                    .min(1.0);
                image.display_width *= scale;
                image.display_height *= scale;
            }
            let fallback = decoded.is_none().then(|| asset.alt.clone());
            (decoded, fallback)
        } else {
            (None, None)
        };
        blocks.push(Block::Signature {
            name: sig.name.clone(),
            label: sig.label.clone(),
            image,
            image_alt,
            gap: 1,
        });
    }
    let mut pages = vec![Page { blocks: Vec::new() }];
    let mut used = 0.0f32;
    let first_budget = (PAGE_HEIGHT as f32
        - f32::from(resolved.frame_inset)
        - 17.35
        - first_flow_origin(doc, &resolved))
    .max(1.0);
    let page_budget = PAGE_HEIGHT as f32
        - f32::from(resolved.frame_inset)
        - 17.35
        - (f32::from(resolved.frame_inset) + 23.14);
    for block in blocks {
        if matches!(block, Block::PageBreak) {
            if !pages.last().is_some_and(|p| p.blocks.is_empty()) {
                pages.push(Page { blocks: Vec::new() });
                used = 0.0;
            }
            continue;
        }
        let budget = if pages.len() == 1 {
            first_budget
        } else {
            page_budget
        };
        match block {
            Block::Table {
                title,
                headings,
                rows,
                gap,
            } => {
                if title == "Settlements" {
                    let height = positioned_height(
                        &Block::Table {
                            title: title.clone(),
                            headings: headings.clone(),
                            rows: rows.clone(),
                            gap,
                        },
                        &resolved,
                    );
                    if used > 0.0 && used + height > budget {
                        pages.push(Page { blocks: Vec::new() });
                        used = 0.0;
                    }
                    used += height;
                    pages.last_mut().unwrap().blocks.push(Block::Table {
                        title,
                        headings,
                        rows,
                        gap,
                    });
                    continue;
                }
                let mut chunk = Vec::new();
                for row in rows {
                    let mut candidate = chunk.clone();
                    candidate.push(row.clone());
                    let current_budget = if pages.len() == 1 {
                        first_budget
                    } else {
                        page_budget
                    };
                    let height = positioned_height(
                        &Block::Table {
                            title: title.clone(),
                            headings: headings.clone(),
                            rows: candidate,
                            gap,
                        },
                        &resolved,
                    );
                    if !chunk.is_empty() && used + height > current_budget {
                        let part = Block::Table {
                            title: title.clone(),
                            headings: headings.clone(),
                            rows: std::mem::take(&mut chunk),
                            gap,
                        };
                        pages.last_mut().unwrap().blocks.push(part);
                        pages.push(Page { blocks: Vec::new() });
                        used = 0.0;
                    }
                    chunk.push(row);
                }
                if !chunk.is_empty() {
                    let part = Block::Table {
                        title,
                        headings,
                        rows: chunk,
                        gap,
                    };
                    used += positioned_height(&part, &resolved);
                    pages.last_mut().unwrap().blocks.push(part);
                }
            }
            Block::Text { title, rows, gap } => {
                let expanded = expand_text_rows(rows, &resolved)?;
                let mut chunk = Vec::new();
                for row in expanded {
                    let height = positioned_height(
                        &Block::Text {
                            title: title.clone(),
                            rows: vec![row.clone()],
                            gap,
                        },
                        &resolved,
                    );
                    let current_budget = if pages.len() == 1 {
                        first_budget
                    } else {
                        page_budget
                    };
                    if !chunk.is_empty() && used + height > current_budget {
                        let part = Block::Text {
                            title: title.clone(),
                            rows: std::mem::take(&mut chunk),
                            gap,
                        };
                        pages.last_mut().unwrap().blocks.push(part);
                        pages.push(Page { blocks: Vec::new() });
                        used = 0.0;
                    }
                    used += height;
                    chunk.push(row);
                }
                if !chunk.is_empty() {
                    pages.last_mut().unwrap().blocks.push(Block::Text {
                        title,
                        rows: chunk,
                        gap,
                    });
                }
            }
            block => {
                let height = positioned_height(&block, &resolved);
                if used > 0.0 && used + height > budget {
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
    let images = pages
        .iter()
        .flat_map(|page| page.blocks.iter())
        .filter_map(|block| match block {
            Block::OwnedImage { image, .. } => Some(image.clone()),
            Block::Signature {
                image: Some(image), ..
            } => Some(image.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut tree = vec![
        Node {
            role: "sheet",
            label: format!("Invoice {}", doc.metadata.number),
        },
        Node {
            role: "header",
            label: doc.title.clone(),
        },
        Node {
            role: "metadata",
            label: "Invoice metadata".into(),
        },
        Node {
            role: "parties",
            label: "Parties".into(),
        },
        Node {
            role: "party",
            label: "From".into(),
        },
        Node {
            role: "party",
            label: "Bill to".into(),
        },
        Node {
            role: "sections",
            label: "Invoice sections".into(),
        },
        Node {
            role: "footer",
            label: "Invoice footer".into(),
        },
        Node {
            role: "settlements",
            label: "Settlements".into(),
        },
        Node {
            role: "payment",
            label: "Payment".into(),
        },
        Node {
            role: "signature",
            label: "Signature".into(),
        },
    ];
    for section in &doc.ordinary_sections {
        tree.push(Node {
            role: "section",
            label: section.title.clone(),
        });
        match &section.body {
            SectionBody::Table(_) => {
                tree.extend([
                    Node {
                        role: "caption",
                        label: section.title.clone(),
                    },
                    Node {
                        role: "thead",
                        label: "Table headings".into(),
                    },
                    Node {
                        role: "tbody",
                        label: "Table body".into(),
                    },
                    Node {
                        role: "tr",
                        label: "Table row".into(),
                    },
                    Node {
                        role: "th",
                        label: "Table heading".into(),
                    },
                    Node {
                        role: "td",
                        label: "Table cell".into(),
                    },
                ]);
            }
            SectionBody::Prose(_) => {
                tree.push(Node {
                    role: "prose",
                    label: section.title.clone(),
                });
                tree.push(Node {
                    role: "list",
                    label: "List".into(),
                });
            }
        }
    }
    let positioned = build_positioned(doc, &resolved, &pages)?;
    Ok(Plan {
        resolved,
        pages,
        positioned,
        tree,
        images,
        grand_total: doc.grand_total,
        currency: doc.metadata.currency.clone(),
        semantic: PreparedSemantic::from_document(doc),
        money_format: doc.config.format.clone(),
        amount_in_words: amount_in_words_entries(doc),
        warnings,
    })
}
fn baseline_on_rule(r: &Resolved, y: f32, size: f32) -> f32 {
    y + size * (r.ascender - r.descender) / (2.0 * r.upem)
}

fn html_baseline_offset(r: &Resolved, size: f32, line: f32) -> f32 {
    (line - size * (r.ascender + r.descender) / r.upem) / 2.0 + size * r.ascender / r.upem
}

fn baseline_from_top(r: &Resolved, top: f32, size: f32, line: f32) -> f32 {
    top + html_baseline_offset(r, size, line)
}

fn text_spans(text: &str, kind: InlineKind, color: [u8; 3], face: Face) -> Vec<Span> {
    let (face, slant, underline, href) = match kind {
        InlineKind::Strong => (Face::Semibold, Slant::Upright, false, None),
        InlineKind::Emphasis => (face, Slant::Oblique, false, None),
        InlineKind::EmphasisStrong => (Face::Semibold, Slant::Oblique, false, None),
        InlineKind::Code => (face, Slant::Upright, false, None),
        InlineKind::Link(href) => (face, Slant::Upright, true, safe_href(&href)),
        _ => (face, Slant::Upright, false, None),
    };
    vec![Span {
        text: text.to_owned(),
        face,
        slant,
        color,
        underline,
        href,
    }]
}
#[derive(Clone, Copy)]
struct TextPlacement {
    x: f32,
    y: f32,
    width: f32,
    size: f32,
    line: f32,
}
#[derive(Clone, Copy)]
struct TextStyle {
    color: [u8; 3],
    face: Face,
    align: Align,
    tracking: f32,
}
macro_rules! push_text {
    ($page:expr, $r:expr, $text:expr, $x:expr, $top:expr, $size:expr, $line:expr, $color:expr, $face:expr, $align:expr, $tracking:expr $(,)?) => {
        push_text_impl(
            $page,
            $r,
            $text,
            TextPlacement {
                x: $x,
                y: $top,
                width: 0.0,
                size: $size,
                line: $line,
            },
            TextStyle {
                color: $color,
                face: $face,
                align: $align,
                tracking: $tracking,
            },
        )
    };
}
macro_rules! push_wrapped_text {
    ($page:expr, $r:expr, $text:expr, $runs:expr, $x:expr, $y:expr, $width:expr, $size:expr, $line:expr, $color:expr, $face:expr, $align:expr, $tracking:expr $(,)?) => {
        push_wrapped_text_impl(
            $page,
            $r,
            $text,
            $runs,
            TextPlacement {
                x: $x,
                y: $y,
                width: $width,
                size: $size,
                line: $line,
            },
            TextStyle {
                color: $color,
                face: $face,
                align: $align,
                tracking: $tracking,
            },
        )
    };
}
macro_rules! render_table_primitives {
    ($page:expr, $r:expr, $title:expr, $headings:expr, $rows:expr, $x:expr, $y:expr, $width:expr $(,)?) => {
        render_table_primitives_impl(
            $page,
            $r,
            $title,
            $headings,
            $rows,
            TablePlacement {
                x: $x,
                y: $y,
                width: $width,
            },
        )
    };
}
struct TablePlacement<'a> {
    x: f32,
    y: &'a mut f32,
    width: f32,
}
fn push_text_impl(
    page: &mut DisplayPage,
    r: &Resolved,
    text: &str,
    placement: TextPlacement,
    style: TextStyle,
) {
    if text.is_empty() {
        return;
    }
    let advance = r.advance * placement.size / r.upem;
    let natural = text.chars().count() as f32 * (advance + style.tracking) - style.tracking;
    let pen = match style.align {
        Align::Left => placement.x,
        Align::Center => placement.x - natural / 2.0,
        Align::Right => placement.x - natural,
    };
    page.items.push(Placed {
        band: Band::Ink,
        node: 0,
        primitive: Primitive::Text {
            x: pen,
            baseline: baseline_from_top(r, placement.y, placement.size, placement.line),
            size: placement.size,
            align: Align::Left,
            advance,
            tracking: style.tracking,
            spans: text_spans(text, InlineKind::Text, style.color, style.face),
            edit_path: None,
        },
    });
}
/// Wraps an inline stream at Unicode-safe word and separator boundaries.
/// Long unbroken tokens fall back to character boundaries only when necessary.
fn wrap_runs(r: &Resolved, runs: &[InlineRun], width: f32, size: f32) -> Vec<Vec<InlineRun>> {
    let advance = (r.advance * size / r.upem).max(0.01);
    let limit = (width / advance).floor().max(1.0) as usize;
    let mut rows = Vec::new();
    let mut units: Vec<(InlineKind, char)> = Vec::new();
    let mut flush = |units: &mut Vec<(InlineKind, char)>| {
        if units.is_empty() {
            return;
        }
        let mut end = units.len().min(limit);
        if end < units.len() {
            let candidate = units[..end]
                .iter()
                .rposition(|(_, ch)| ch.is_whitespace() || ". /\\-_=?:&@".contains(*ch))
                .map(|i| i + 1);
            if let Some(boundary) = candidate {
                end = boundary;
            }
        }
        while end < units.len()
            && matches!(
                units[end].1,
                ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}'
            )
            && end < limit
        {
            end += 1;
        }
        let mut line = units.drain(..end).collect::<Vec<_>>();
        while line.first().is_some_and(|(_, ch)| ch.is_whitespace()) {
            line.remove(0);
        }
        while line.last().is_some_and(|(_, ch)| ch.is_whitespace()) {
            line.pop();
        }
        if !line.is_empty() {
            let mut runs: Vec<InlineRun> = Vec::new();
            for (kind, ch) in line {
                if let Some(last) = runs.last_mut() {
                    if last.kind == kind {
                        last.text.push(ch);
                        continue;
                    }
                }
                runs.push(InlineRun {
                    kind,
                    text: ch.to_string(),
                });
            }
            rows.push(runs);
        }
    };
    for run in runs {
        if run.kind == InlineKind::Break {
            flush(&mut units);
            continue;
        }
        for ch in run.text.chars() {
            units.push((run.kind.clone(), ch));
            if units.len() > limit {
                flush(&mut units);
            }
        }
    }
    flush(&mut units);
    if rows.is_empty() {
        rows.push(Vec::new());
    }
    rows
}
fn push_wrapped_text_impl(
    page: &mut DisplayPage,
    r: &Resolved,
    text: &str,
    runs: &[InlineRun],
    placement: TextPlacement,
    style: TextStyle,
) -> usize {
    let source = if runs.is_empty() {
        vec![InlineRun {
            kind: InlineKind::Text,
            text: text.to_owned(),
        }]
    } else {
        runs.to_vec()
    };
    let rows = if style.align == Align::Right {
        vec![source]
    } else {
        wrap_runs(r, &source, placement.width, placement.size)
    };
    for (index, row) in rows.iter().enumerate() {
        let mut spans = Vec::new();
        for run in row {
            if run.kind != InlineKind::Break {
                let span_face =
                    if matches!(run.kind, InlineKind::Strong | InlineKind::EmphasisStrong) {
                        Face::Semibold
                    } else {
                        style.face
                    };
                let span_color = if matches!(run.kind, InlineKind::Link(_)) {
                    r.accent
                } else {
                    style.color
                };
                spans.extend(text_spans(
                    &run.text,
                    run.kind.clone(),
                    span_color,
                    span_face,
                ));
            }
        }
        let row_text: String = row.iter().map(|run| run.text.as_str()).collect();
        let row_width = measure_text(&row_text, r, placement.size, style.face == Face::Semibold)
            + style.tracking * row_text.chars().count().saturating_sub(1) as f32;
        let pen = match style.align {
            Align::Left => placement.x,
            Align::Right => placement.x + placement.width - row_width,
            Align::Center => placement.x + (placement.width - row_width) / 2.0,
        };
        if !spans.is_empty() {
            page.items.push(Placed {
                band: Band::Ink,
                node: 0,
                primitive: Primitive::Text {
                    x: pen,
                    baseline: baseline_from_top(
                        r,
                        placement.y + index as f32 * placement.line,
                        placement.size,
                        placement.line,
                    ),
                    size: placement.size,
                    align: Align::Left,
                    advance: r.advance * placement.size / r.upem,
                    tracking: style.tracking,
                    edit_path: None,
                    spans,
                },
            });
        }
    }
    rows.len()
}
fn push_contact_links(
    page: &mut DisplayPage,
    r: &Resolved,
    contact: &str,
    email: Option<&str>,
    website: Option<&str>,
    placement: TextPlacement,
) {
    let rows = wrap_runs(
        r,
        &[InlineRun {
            kind: InlineKind::Text,
            text: contact.to_owned(),
        }],
        placement.width,
        placement.size,
    );
    let chars: Vec<char> = contact.chars().collect();
    let email_end = email.map_or(0, |value| value.chars().count());
    let website_start = email_end + usize::from(email.is_some()) * 3;
    let links = [
        email.map(|value| (0, email_end, safe_mailto(value), value)),
        website.map(|value| {
            (
                website_start,
                website_start + value.chars().count(),
                safe_http_url(value),
                value,
            )
        }),
    ];
    let mut source_start = 0usize;
    for (row_index, row) in rows.iter().enumerate() {
        while source_start < chars.len() && chars[source_start].is_whitespace() {
            source_start += 1;
        }
        let row_len: usize = row.iter().map(|run| run.text.chars().count()).sum();
        let row_end = (source_start + row_len).min(chars.len());
        for (start, end, href, label) in links.iter().flatten() {
            let overlap_start = (*start).max(source_start);
            let overlap_end = (*end).min(row_end);
            if overlap_start >= overlap_end {
                continue;
            }
            let prefix: String = chars[source_start..overlap_start].iter().collect();
            let visible: String = chars[overlap_start..overlap_end].iter().collect();
            if let Some(href) = href {
                page.links.push(LinkBox {
                    href: href.clone(),
                    label: (*label).to_owned(),
                    x: placement.x + measure_text(&prefix, r, placement.size, false),
                    y: placement.y + row_index as f32 * placement.line,
                    width: measure_text(&visible, r, placement.size, false),
                    height: placement.line,
                });
            }
        }
        source_start = row_end;
    }
}

fn push_rule(page: &mut DisplayPage, x: f32, y: f32, w: f32, color: [u8; 3]) {
    page.items.push(Placed {
        band: Band::Fill,
        node: 0,
        primitive: Primitive::Rule {
            x,
            y,
            w,
            dash: Dash::Dashed,
            color,
        },
    });
}
fn mark_text_items(page: &mut DisplayPage, start: usize, path: Option<String>) {
    if path.is_none() {
        return;
    }
    for item in page.items.iter_mut().skip(start) {
        if let Primitive::Text { edit_path, .. } = &mut item.primitive {
            *edit_path = path.clone();
        }
    }
}
fn path_section_index(path: &str) -> Option<usize> {
    let rest = path.strip_prefix("sections[")?;
    let (index, _) = rest.split_once(']')?;
    index.parse().ok()
}
fn node_for_path(doc: &Document, path: &str) -> Option<usize> {
    if path == "title" {
        return Some(1);
    }
    if path.starts_with("metadata.") {
        return Some(2);
    }
    for (root, node) in [("from.", 4), ("bill_to.", 5)] {
        if path.starts_with(root) {
            return Some(node);
        }
    }
    if path.starts_with("settlements.") {
        return Some(8);
    }
    if path.starts_with("payment.") {
        return Some(9);
    }
    if path.starts_with("signature.") {
        return Some(10);
    }
    let rest = path.strip_prefix("sections[")?;
    let (index, tail) = rest.split_once(']')?;
    let index: usize = index.parse().ok()?;
    let mut node = 11;
    for section in doc.ordinary_sections.iter().take(index) {
        node += if matches!(&section.body, SectionBody::Table(_)) {
            7
        } else {
            3
        };
    }
    if index >= doc.ordinary_sections.len() {
        return None;
    }
    if tail.starts_with(".table.") {
        node += if tail.starts_with(".table.headings[") {
            5
        } else {
            6
        };
    } else if tail == ".prose" {
        node += 1;
    }
    Some(node)
}
fn push_notch(page: &mut DisplayPage, r: &Resolved, title: &str, x: f32, y: f32) {
    let g = &r.geometry;
    let size = g.font_body * f32::from(r.font_scale) / 100.0;
    let line = r.line_advance;
    let text = format!("[ {title} ]");
    let advance = r.advance * size / r.upem;
    let width = text.chars().count() as f32 * advance;
    page.items.push(Placed {
        band: Band::Knockout,
        node: 0,
        primitive: Primitive::Rect {
            x: x - g.notch_pad_x,
            y: y - line / 2.0,
            w: width + 2.0 * g.notch_pad_x,
            h: line,
            fill: r.tokens.paper,
        },
    });
    page.items.push(Placed {
        band: Band::Ink,
        node: 0,
        primitive: Primitive::Text {
            x,
            baseline: baseline_on_rule(r, y, size),
            size,
            align: Align::Left,
            advance,
            tracking: 0.0,
            spans: text_spans(&text, InlineKind::Text, r.tokens.muted, Face::Regular),
            edit_path: None,
        },
    });
}

fn column_widths(n: usize, width: f32) -> Vec<f32> {
    let fractions: &[f32] = match n {
        1 => &[1.0],
        2 => &[0.72, 0.28],
        3 => &[0.55, 0.22, 0.23],
        4 => &[0.55, 0.10, 0.17, 0.18],
        5 => &[0.45, 0.10, 0.17, 0.10, 0.18],
        _ => &[],
    };
    if fractions.is_empty() {
        let tail = 0.60 / n.saturating_sub(1).max(1) as f32;
        (0..n)
            .map(|i| width * if i == 0 { 0.40 } else { tail })
            .collect()
    } else {
        fractions.iter().map(|v| width * *v).collect()
    }
}

fn render_table_primitives_impl(
    page: &mut DisplayPage,
    r: &Resolved,
    title: &str,
    headings: &[String],
    rows: &[TableRow],
    placement: TablePlacement<'_>,
) {
    let section_index = rows
        .iter()
        .flat_map(|row| row.edit_paths.iter().flatten())
        .find_map(|path| path_section_index(path));
    let x = placement.x;
    let y = placement.y;
    let width = placement.width;
    let g = &r.geometry;
    let size = g.font_body * f32::from(r.font_scale) / 100.0;
    let line = r.line_advance;
    let pad_top = g.cell_pad_top * r.density_space;
    let pad_bottom = g.cell_pad_bottom * r.density_space;
    push_rule(page, x, *y, width, r.tokens.rule);
    let notch_start = page.items.len();
    push_notch(page, r, title, x + g.notch_left, *y);
    mark_text_items(
        page,
        notch_start,
        section_index.map(|n| format!("sections[{n}].title")),
    );
    let widths = column_widths(headings.len(), width);
    let mut header_lines = 1usize;
    for (i, heading) in headings.iter().enumerate() {
        let x0 = x + widths.iter().take(i).sum::<f32>();
        let right = rows.first().and_then(|row| row.alignments.get(i)).copied()
            == Some(TableAlignment::Right)
            || i + 1 == headings.len();
        let left_pad = if i == 0 { 0.0 } else { g.cell_pad_x };
        let right_pad = if i + 1 == headings.len() {
            0.0
        } else {
            g.cell_pad_x
        };
        let start = page.items.len();
        let lines = push_wrapped_text!(
            page,
            r,
            heading,
            &[],
            x0 + left_pad,
            *y,
            (widths[i] - left_pad - right_pad).max(1.0),
            size,
            line,
            r.tokens.muted,
            Face::Regular,
            if right { Align::Right } else { Align::Left },
            0.0,
        );
        mark_text_items(
            page,
            start,
            section_index.map(|n| format!("sections[{n}].table.headings[{i}]")),
        );
        header_lines = header_lines.max(lines);
    }
    *y += header_lines as f32 * line + pad_bottom;
    push_rule(page, x, *y, width, r.tokens.rule);
    *y += pad_top;
    for row in rows {
        if row
            .cells
            .first()
            .is_some_and(|v| v.eq_ignore_ascii_case("subtotal"))
        {
            continue;
        }
        let row_top = *y;
        let mut row_lines = 1usize;
        for (i, cell) in row.cells.iter().enumerate() {
            let x0 = x + widths.iter().take(i).sum::<f32>();
            let right_aligned = i + 1 == headings.len()
                || row.alignments.get(i).copied() == Some(TableAlignment::Right);
            let left_pad = if i == 0 { 0.0 } else { g.cell_pad_x };
            let right_pad = if i + 1 == headings.len() {
                0.0
            } else {
                g.cell_pad_x
            };
            let cell_width =
                (widths.get(i).copied().unwrap_or(0.0) - left_pad - right_pad).max(1.0);
            let start = page.items.len();
            let lines = push_wrapped_text!(
                page,
                r,
                cell,
                &inline_runs(cell),
                x0 + left_pad,
                row_top,
                cell_width,
                size,
                line,
                r.tokens.ink,
                Face::Regular,
                if right_aligned {
                    Align::Right
                } else {
                    Align::Left
                },
                0.0,
            );
            mark_text_items(page, start, row.edit_paths.get(i).cloned().flatten());
            row_lines = row_lines.max(lines);
        }
        *y += row_lines as f32 * line + pad_bottom + pad_top;
    }
    *y += g.table_end_gap * r.density_space;
    push_rule(page, x, *y, width, r.tokens.rule);
}
fn build_positioned(
    doc: &Document,
    r: &Resolved,
    pages: &[Page],
) -> Result<Vec<DisplayPage>, RenderError> {
    let g = &r.geometry;
    let inset = f32::from(r.frame_inset);
    let left = inset + g.gutter_x;
    let right = g.page_w - inset - g.gutter_x;
    let width = right - left;
    let content_top = inset + g.gutter_top;
    let body = g.font_body * f32::from(r.font_scale) / 100.0;
    let heading = g.font_heading * f32::from(r.font_scale) / 100.0;
    let detail = g.font_detail * f32::from(r.font_scale) / 100.0;
    let payable_sections = doc
        .ordinary_sections
        .iter()
        .filter(|s| {
            matches!(&s.body, SectionBody::Table(_))
                && !s.directives.summary_only
                && s.total.is_some()
        })
        .count();
    let mut result = Vec::with_capacity(pages.len());
    let mut image_index = 0usize;
    for (page_index, source_page) in pages.iter().enumerate() {
        let mut page = DisplayPage {
            items: Vec::new(),
            links: Vec::new(),
        };
        let content_bottom = r.geometry.page_h - inset - r.geometry.gutter_bottom;
        let mut footer_started = false;
        let mut section_slot = 0usize;
        let mut y = if page_index == 0 {
            first_flow_origin(doc, r)
        } else {
            inset + g.gutter_top
        };
        page.items.push(Placed {
            band: Band::Fill,
            node: 0,
            primitive: Primitive::Stroke {
                x: inset,
                y: inset,
                w: r.geometry.page_w - 2.0 * inset,
                h: r.geometry.page_h - 2.0 * inset,
                dash: Dash::Dashed,
                color: r.tokens.rule,
            },
        });
        let corner_size = g.font_body * f32::from(r.font_scale) / 100.0;
        let corner_advance = r.advance * corner_size / r.upem;
        for (cx, cy) in [
            (inset, inset),
            (r.geometry.page_w - inset, inset),
            (inset, r.geometry.page_h - inset),
            (r.geometry.page_w - inset, r.geometry.page_h - inset),
        ] {
            page.items.push(Placed {
                band: Band::Knockout,
                node: 0,
                primitive: Primitive::Rect {
                    x: cx - corner_advance / 2.0 - g.corner_pad_x,
                    y: cy - r.line_advance / 2.0,
                    w: corner_advance + 2.0 * g.corner_pad_x,
                    h: r.line_advance,
                    fill: r.tokens.paper,
                },
            });
            push_text!(
                &mut page,
                r,
                "+",
                cx,
                cy - r.line_advance / 2.0,
                corner_size,
                r.line_advance,
                r.tokens.ink,
                Face::Regular,
                Align::Center,
                0.0,
            );
        }
        if page_index == 0 {
            let meta = [
                ("Ref", doc.metadata.number.clone(), Face::Semibold),
                ("Kind", doc.metadata.kind.clone(), Face::Regular),
                ("Issued", doc.metadata.issued.to_string(), Face::Regular),
            ];
            let mut meta_rows: Vec<(&str, String, Face)> = meta.into_iter().collect();
            if let Some(due) = &doc.metadata.due {
                meta_rows.push(("Due", due.to_string(), Face::Regular));
            }
            if let Some(terms) = &doc.metadata.terms {
                meta_rows.push(("Terms", terms.clone(), Face::Regular));
            }
            meta_rows.push(("Currency", doc.metadata.currency.clone(), Face::Regular));
            let meta_content = meta_rows.len() as f32 * r.line_advance
                + meta_rows.len().saturating_sub(1) as f32 * g.meta_row_gap;
            let brand_width = (right - g.meta_min_w - g.header_gap - left).max(1.0);
            let meta_x = right - g.meta_min_w;
            let brand_lines =
                ((measure_text(&doc.from.name, r, heading, true) / brand_width).ceil() as usize)
                    .max(1);
            let header_content = (heading * 1.12)
                .max(meta_content)
                .max(brand_lines as f32 * g.line_heading);
            let header_h = g
                .header_min_h
                .max(g.header_pad_top + header_content + g.header_pad_bottom);
            let header_top = content_top + g.header_pad_top;
            let badge = format!(
                "[ {} — {} ]",
                doc.title,
                format_money(doc.grand_total, &doc.metadata.currency, &doc.config.format)
            );
            let max_chars = ((g.badge_max_w - 2.0 * g.badge_pad_x) / (r.advance * body / r.upem))
                .floor()
                .max(1.0) as usize;
            let badge = if badge.chars().count() > max_chars {
                let suffix = format!(
                    " — {} ]",
                    format_money(doc.grand_total, &doc.metadata.currency, &doc.config.format)
                );
                let budget = max_chars.saturating_sub(suffix.chars().count() + 3);
                format!(
                    "[ {}…{}",
                    doc.title.chars().take(budget).collect::<String>(),
                    suffix
                )
            } else {
                badge
            };
            let badge_advance = r.advance * body / r.upem;
            let badge_width = badge.chars().count() as f32 * badge_advance;
            page.items.push(Placed {
                band: Band::Knockout,
                node: 0,
                primitive: Primitive::Rect {
                    x: 297.5 - badge_width / 2.0 - g.badge_pad_x,
                    y: inset - g.badge_rise,
                    w: badge_width + 2.0 * g.badge_pad_x,
                    h: r.line_advance,
                    fill: r.tokens.paper,
                },
            });
            push_text!(
                &mut page,
                r,
                &badge,
                297.5,
                inset - g.badge_rise,
                body,
                r.line_advance,
                r.accent,
                Face::Regular,
                Align::Center,
                0.0,
            );
            let start = page.items.len();
            let _ = push_wrapped_text!(
                &mut page,
                r,
                &doc.title,
                &[],
                left,
                header_top,
                brand_width,
                heading,
                g.line_heading,
                r.tokens.ink,
                Face::Semibold,
                Align::Left,
                g.brand_tracking * f32::from(r.font_scale) / 100.0,
            );
            mark_text_items(&mut page, start, Some("title".into()));
            for (i, (label, value, face)) in meta_rows.iter().enumerate() {
                let top = header_top + i as f32 * (r.line_advance + 3.31);
                push_text!(
                    &mut page,
                    r,
                    label,
                    meta_x,
                    top,
                    body,
                    r.line_advance,
                    r.tokens.muted,
                    Face::Regular,
                    Align::Left,
                    0.0,
                );
                let start = page.items.len();
                push_text!(
                    &mut page,
                    r,
                    value,
                    right,
                    top,
                    body,
                    r.line_advance,
                    r.tokens.ink,
                    *face,
                    Align::Right,
                    0.0,
                );
                let path = match *label {
                    "Ref" => "metadata.number",
                    "Kind" => "metadata.kind",
                    "Issued" => "metadata.issued",
                    "Due" => "metadata.due",
                    "Terms" => "metadata.terms",
                    "Currency" => "metadata.currency",
                    _ => "",
                };
                mark_text_items(&mut page, start, (!path.is_empty()).then(|| path.into()));
            }
            let header_bottom = content_top + header_h;
            push_rule(&mut page, left, header_bottom, width, r.tokens.rule);
            let party_top = header_bottom + 14.88;
            let party_width = (width - 59.50) / 2.0;
            for (col, (label, party)) in [("From", &doc.from), ("Bill to", &doc.bill_to)]
                .iter()
                .enumerate()
            {
                let g = &r.geometry;
                let px = left + col as f32 * (party_width + g.parties_gap);
                push_text!(
                    &mut page,
                    r,
                    &format!("[ {label} ]"),
                    px,
                    party_top,
                    body,
                    r.line_advance,
                    r.tokens.muted,
                    Face::Regular,
                    Align::Left,
                    0.0,
                );
                let name_top = party_top + r.line_advance + g.label_gap;
                let name_start = page.items.len();
                let name_lines = push_wrapped_text!(
                    &mut page,
                    r,
                    &party.name,
                    &[],
                    px,
                    name_top,
                    party_width,
                    body,
                    r.line_advance,
                    r.tokens.ink,
                    Face::Semibold,
                    Align::Left,
                    0.0,
                );
                let party_root = if col == 0 { "from" } else { "bill_to" };
                mark_text_items(&mut page, name_start, Some(format!("{party_root}.name")));
                let mut py = name_top + name_lines as f32 * r.line_advance;
                if let Some(image) = &party.logo {
                    if safe_http_url(&image.src).is_some()
                        || (image.src.contains(':') && !image.src.starts_with("data:image/"))
                    {
                        let image_alt_start = page.items.len();
                        push_text!(
                            &mut page,
                            r,
                            &format!("[ img ] {}", image.alt),
                            px,
                            py,
                            detail,
                            g.line_detail,
                            r.tokens.muted,
                            Face::Regular,
                            Align::Left,
                            0.0,
                        );
                        mark_text_items(
                            &mut page,
                            image_alt_start,
                            Some(format!("{party_root}.logo.alt")),
                        );
                        py += g.line_detail + g.logo_gap;
                    }
                }
                for (address_index, address) in party.address.iter().enumerate() {
                    let address_start = page.items.len();
                    let lines = push_wrapped_text!(
                        &mut page,
                        r,
                        address,
                        &[],
                        px + body * 0.6,
                        py,
                        (party_width - body * 0.6).max(1.0),
                        body,
                        r.line_advance,
                        r.tokens.muted,
                        Face::Regular,
                        Align::Left,
                        0.0,
                    );
                    mark_text_items(
                        &mut page,
                        address_start,
                        Some(format!("{party_root}.address[{address_index}]")),
                    );
                    py += lines as f32 * r.line_advance;
                }
                for id in &party.identifiers {
                    let id_start = page.items.len();
                    let lines = push_wrapped_text!(
                        &mut page,
                        r,
                        &format!("{} {}", id.key, id.value),
                        &[],
                        px,
                        py,
                        party_width,
                        body,
                        r.line_advance,
                        r.tokens.muted,
                        Face::Regular,
                        Align::Left,
                        0.0,
                    );
                    mark_text_items(
                        &mut page,
                        id_start,
                        Some(format!("{party_root}.identifiers.{}", id.key)),
                    );
                    py += lines as f32 * r.line_advance;
                }
                if party.email.is_some() || party.website.is_some() {
                    py += g.label_gap;
                }
                let mut contact = String::new();
                if let Some(email) = &party.email {
                    contact.push_str(email);
                }
                if let Some(website) = &party.website {
                    if !contact.is_empty() {
                        contact.push_str(" · ");
                    }
                    contact.push_str(website);
                }
                if !contact.is_empty() {
                    push_contact_links(
                        &mut page,
                        r,
                        &contact,
                        party.email.as_deref(),
                        party.website.as_deref(),
                        TextPlacement {
                            x: px,
                            y: py,
                            width: party_width,
                            size: body,
                            line: r.line_advance,
                        },
                    );
                    let mut contact_x = px;
                    if let Some(email) = &party.email {
                        let start = page.items.len();
                        push_text!(
                            &mut page,
                            r,
                            email,
                            contact_x,
                            py,
                            body,
                            r.line_advance,
                            r.tokens.muted,
                            Face::Regular,
                            Align::Left,
                            0.0,
                        );
                        mark_text_items(&mut page, start, Some(format!("{party_root}.email")));
                        contact_x += measure_text(email, r, body, false);
                    }
                    if let Some(website) = &party.website {
                        if party.email.is_some() {
                            push_text!(
                                &mut page,
                                r,
                                " · ",
                                contact_x,
                                py,
                                body,
                                r.line_advance,
                                r.tokens.muted,
                                Face::Regular,
                                Align::Left,
                                0.0,
                            );
                            contact_x += measure_text(" · ", r, body, false);
                        }
                        let start = page.items.len();
                        push_text!(
                            &mut page,
                            r,
                            website,
                            contact_x,
                            py,
                            body,
                            r.line_advance,
                            r.tokens.muted,
                            Face::Regular,
                            Align::Left,
                            0.0,
                        );
                        mark_text_items(&mut page, start, Some(format!("{party_root}.website")));
                    }
                }
            }
        }
        let footer_height = source_page
            .blocks
            .iter()
            .filter(|block| is_footer_block(block))
            .map(|block| {
                positioned_height(block, r)
                    + if matches!(block, Block::Table { title, .. } if title == "Settlements") {
                        r.geometry.settle_margin_bottom * r.density_space
                    } else {
                        0.0
                    }
            })
            .sum::<f32>()
            + r.geometry.footer_pad_top * r.density_space;
        for block in &source_page.blocks {
            if !footer_started && is_footer_block(block) {
                y += (content_bottom - y - footer_height).max(0.0)
                    + r.geometry.footer_pad_top * r.density_space;
                footer_started = true;
            }
            match block {
                Block::Title => {}
                Block::Text { title, .. } if title == "From" || title == "Bill to" => {}
                Block::Table {
                    title,
                    headings,
                    rows,
                    gap,
                } => {
                    let gap_space = if page_index == 0 && section_slot == 0 {
                        0.0
                    } else {
                        match gap {
                            0 => r.geometry.gap_none,
                            1 => r.geometry.gap_tight,
                            2 => r.geometry.gap_standard,
                            _ => r.geometry.gap_roomy,
                        }
                    } * r.density_space;
                    y += gap_space;
                    section_slot += 1;
                    render_table_primitives!(
                        &mut page, r, title, headings, rows, left, &mut y, width,
                    );
                    if title == "Settlements" {
                        y += r.geometry.settle_margin_bottom * r.density_space;
                    }
                    if payable_sections > 1
                        && rows.last().is_some_and(|row| {
                            row.cells
                                .first()
                                .is_some_and(|v| v.eq_ignore_ascii_case("subtotal"))
                        })
                    {
                        let summary = rows
                            .last()
                            .and_then(|row| row.cells.last())
                            .cloned()
                            .unwrap_or_default();
                        y += r.geometry.summary_pad_top * r.density_space;
                        push_text!(
                            &mut page,
                            r,
                            title,
                            right - width * r.geometry.summary_w,
                            y,
                            body,
                            r.line_advance,
                            r.tokens.muted,
                            Face::Regular,
                            Align::Left,
                            0.0,
                        );
                        push_text!(
                            &mut page,
                            r,
                            &summary,
                            right,
                            y,
                            body,
                            r.line_advance,
                            r.tokens.ink,
                            Face::Semibold,
                            Align::Right,
                            0.0,
                        );
                        y += r.line_advance + r.geometry.summary_pad_top * r.density_space;
                    }
                }
                Block::AmountInWords { label, text } => {
                    let start = page.items.len();
                    let _ = push_wrapped_text!(
                        &mut page,
                        r,
                        &format!("{label}: {text}"),
                        &[],
                        left,
                        y,
                        width,
                        detail,
                        r.line_advance,
                        r.tokens.muted,
                        Face::Regular,
                        Align::Left,
                        0.0,
                    );
                    mark_text_items(&mut page, start, None);
                    y += positioned_height(block, r);
                }
                Block::Text { title, rows, gap } => {
                    let gap_space = if page_index == 0 && section_slot == 0 {
                        0.0
                    } else {
                        match gap {
                            0 => r.geometry.gap_none,
                            1 => r.geometry.gap_tight,
                            2 => r.geometry.gap_standard,
                            _ => r.geometry.gap_roomy,
                        }
                    } * r.density_space;
                    section_slot += 1;
                    y += gap_space;
                    if title != "Payment" {
                        push_rule(&mut page, left, y, width, r.tokens.rule);
                    }
                    let notch_start = page.items.len();
                    push_notch(&mut page, r, title, left + r.geometry.notch_left, y);
                    let section_title_path = rows.iter().find_map(|row| {
                        row.edit_path
                            .as_deref()
                            .and_then(path_section_index)
                            .map(|index| format!("sections[{index}].title"))
                    });
                    mark_text_items(&mut page, notch_start, section_title_path);
                    y += r.geometry.section_pad_top * r.density_space;
                    if title == "Payment" {
                        let box_h = 46.28 * r.density_space
                            + rows.len() as f32 * (r.line_advance + 4.13 * r.density_space);
                        page.items.push(Placed {
                            band: Band::Fill,
                            node: 0,
                            primitive: Primitive::Stroke {
                                x: left,
                                y,
                                w: width,
                                h: box_h,
                                dash: Dash::Dashed,
                                color: r.tokens.rule,
                            },
                        });
                    }
                    for row in rows {
                        let runs = if row.runs.is_empty() {
                            inline_runs(&row.text)
                        } else {
                            row.runs.clone()
                        };
                        let is_list = runs.iter().any(|run| run.kind == InlineKind::ListMarker);
                        let is_quote = runs.iter().any(|run| run.kind == InlineKind::QuoteMarker);
                        let indent = if is_list {
                            r.geometry.prose_list_indent
                        } else if is_quote {
                            r.geometry.quote_indent
                        } else {
                            0.0
                        };
                        let color = if is_quote {
                            r.tokens.muted
                        } else {
                            r.tokens.ink
                        };
                        let start = page.items.len();
                        let rows_written = push_wrapped_text!(
                            &mut page,
                            r,
                            &row.text,
                            &runs,
                            left + indent,
                            y,
                            (width - indent).max(1.0),
                            body,
                            r.line_advance,
                            color,
                            Face::Regular,
                            Align::Left,
                            0.0,
                        );
                        mark_text_items(&mut page, start, row.edit_path.clone());
                        let row_height = rows_written as f32 * r.line_advance;
                        if is_quote {
                            page.items.push(Placed {
                                band: Band::Fill,
                                node: 0,
                                primitive: Primitive::VRule {
                                    x: left,
                                    y,
                                    h: row_height,
                                    dash: Dash::Dashed,
                                    color: r.tokens.rule,
                                },
                            });
                        }
                        y += row_height;
                        y += if is_list {
                            r.geometry.prose_p_gap + r.geometry.prose_list_top
                        } else if is_quote {
                            r.geometry.quote_gap
                        } else {
                            r.geometry.prose_p_gap
                        };
                    }
                    if title == "Payment" {
                        y += (46.28 - 15.70) * r.density_space;
                    }
                }
                Block::Payment { methods, gap } => {
                    let top = y;
                    let box_h = positioned_height(block, r);
                    page.items.push(Placed {
                        band: Band::Fill,
                        node: 0,
                        primitive: Primitive::Stroke {
                            x: left,
                            y: top,
                            w: width,
                            h: box_h,
                            dash: Dash::Dashed,
                            color: r.tokens.rule,
                        },
                    });
                    y += 23.14 * r.density_space;
                    for (method_index, method) in methods.iter().enumerate() {
                        let title_start = page.items.len();
                        push_text!(
                            &mut page,
                            r,
                            &method.title,
                            left + 18.18,
                            y,
                            body,
                            r.line_advance,
                            r.tokens.ink,
                            Face::Semibold,
                            Align::Left,
                            0.0,
                        );
                        mark_text_items(
                            &mut page,
                            title_start,
                            Some(format!("payment.methods[{method_index}].title")),
                        );
                        y += r.line_advance + 4.13 * r.density_space;
                        for field in &method.fields {
                            let inner_left = left + g.hairline + g.pay_pad_x;
                            let label_w = g.pay_dt_min_w;
                            push_text!(
                                &mut page,
                                r,
                                &field.label,
                                inner_left,
                                y,
                                body,
                                r.line_advance,
                                r.tokens.muted,
                                Face::Regular,
                                Align::Left,
                                0.0,
                            );
                            let value_x = inner_left + label_w + g.pay_dl_col_gap;
                            let value_start = page.items.len();
                            let value_lines = push_wrapped_text!(
                                &mut page,
                                r,
                                &field.value,
                                &[],
                                value_x,
                                y,
                                (right - value_x).max(1.0),
                                body,
                                r.line_advance,
                                r.tokens.ink,
                                Face::Regular,
                                Align::Left,
                                0.0,
                            );
                            mark_text_items(
                                &mut page,
                                value_start,
                                Some(format!(
                                    "payment.methods[{method_index}].fields.{}",
                                    field.label
                                )),
                            );
                            y += value_lines.saturating_sub(1) as f32 * r.line_advance;
                            y += r.line_advance + g.pay_dl_margin_top * r.density_space;
                        }
                        if method_index + 1 < methods.len() {
                            push_rule(
                                &mut page,
                                left + g.hairline + g.pay_pad_x,
                                y,
                                width - 2.0 * (g.hairline + g.pay_pad_x),
                                r.tokens.rule,
                            );
                            y += g.pay_method_gap * r.density_space;
                        }
                    }
                    y = top + box_h + f32::from(*gap) * r.line_advance;
                }
                Block::Signature {
                    name,
                    label,
                    image,
                    image_alt,
                    gap,
                } => {
                    y += g.sig_margin_top * r.density_space;
                    let name_start = page.items.len();
                    push_text!(
                        &mut page,
                        r,
                        name,
                        left,
                        y,
                        body,
                        r.line_advance,
                        r.tokens.ink,
                        Face::Semibold,
                        Align::Left,
                        0.0,
                    );
                    mark_text_items(&mut page, name_start, Some("signature.name".into()));
                    let label_start = page.items.len();
                    push_text!(
                        &mut page,
                        r,
                        label,
                        left + measure_text(name, r, body, true) + g.summary_gap / 2.0,
                        y,
                        body,
                        r.line_advance,
                        r.tokens.muted,
                        Face::Regular,
                        Align::Left,
                        0.0,
                    );
                    mark_text_items(&mut page, label_start, Some("signature.label".into()));
                    y += r.line_advance + g.sig_note_gap * r.density_space;
                    if let Some(image) = image {
                        page.items.push(Placed {
                            band: Band::Ink,
                            node: 0,
                            primitive: Primitive::Image {
                                x: left,
                                y,
                                w: image.display_width,
                                h: image.display_height,
                                index: image_index,
                                edit_path: Some("signature.image.alt".into()),
                            },
                        });
                        image_index += 1;
                        y += image.display_height + g.sig_note_gap * r.density_space;
                    } else if let Some(alt) = image_alt {
                        let alt_start = page.items.len();
                        push_text!(
                            &mut page,
                            r,
                            &format!("[ img ] {alt}"),
                            left,
                            y,
                            detail,
                            g.line_detail,
                            r.tokens.muted,
                            Face::Regular,
                            Align::Left,
                            0.0,
                        );
                        mark_text_items(&mut page, alt_start, Some("signature.image.alt".into()));
                        y += g.line_detail + g.sig_note_gap * r.density_space;
                    }
                    y += f32::from(*gap) * r.line_advance;
                }
                Block::Total => {
                    y += 9.92 * r.density_space;
                    push_rule(
                        &mut page,
                        right - width * 0.48,
                        y,
                        width * 0.48,
                        r.tokens.rule,
                    );
                    push_text!(
                        &mut page,
                        r,
                        "Total due",
                        right - width * 0.48,
                        y + 6.61,
                        body,
                        r.line_advance,
                        r.tokens.ink,
                        Face::Semibold,
                        Align::Left,
                        0.0,
                    );
                    push_text!(
                        &mut page,
                        r,
                        &format_money(doc.grand_total, &doc.metadata.currency, &doc.config.format),
                        right,
                        y + 6.61,
                        body,
                        r.line_advance,
                        r.tokens.ink,
                        Face::Semibold,
                        Align::Right,
                        0.0,
                    );
                    y += r.line_advance + 6.61 * r.density_space;
                }
                Block::OwnedImage { image, owner } => {
                    let (ix, iy) = if *owner == "From" || *owner == "Bill to" {
                        let col = usize::from(*owner == "Bill to");
                        (
                            left + col as f32 * ((width - g.parties_gap) / 2.0 + g.parties_gap),
                            content_top
                                + g.header_min_h
                                    .max(g.header_pad_top + heading * 1.12 + g.header_pad_bottom)
                                + g.parties_pad_top
                                - image.display_height
                                - g.logo_gap,
                        )
                    } else {
                        (left, y)
                    };
                    page.items.push(Placed {
                        band: Band::Ink,
                        node: 0,
                        primitive: Primitive::Image {
                            x: ix,
                            y: iy,
                            w: image.display_width,
                            h: image.display_height,
                            index: image_index,
                            edit_path: if *owner == "From" {
                                Some("from.logo.alt".into())
                            } else if *owner == "Bill to" {
                                Some("bill_to.logo.alt".into())
                            } else {
                                None
                            },
                        },
                    });
                    image_index += 1;
                }
                Block::PageBreak => {}
            }
        }
        for item in &mut page.items {
            if let Primitive::Text {
                edit_path: Some(path),
                ..
            } = &item.primitive
            {
                if let Some(node) = node_for_path(doc, path) {
                    item.node = node;
                    continue;
                }
            }
            item.node = match &item.primitive {
                Primitive::Image { .. } => 3,
                Primitive::Text { spans, .. } => {
                    let text: String = spans.iter().map(|span| span.text.as_str()).collect();
                    if text == "+" {
                        0
                    } else if text.starts_with("[ From") {
                        4
                    } else if text.starts_with("[ Bill to") {
                        5
                    } else if text.starts_with("[ Settlements") {
                        8
                    } else if text.starts_with("[ Payment") {
                        9
                    } else if text.starts_with("[ Signature") {
                        10
                    } else if text == "Total due" {
                        6
                    } else if text == "Ref"
                        || text == "Kind"
                        || text == "Due"
                        || text == "Terms"
                        || text == "Currency"
                    {
                        2
                    } else {
                        6
                    }
                }
                Primitive::Rule { .. } | Primitive::VRule { .. } => 6,
                Primitive::Rect { .. } | Primitive::Stroke { .. } => 0,
            };
        }
        page.items.sort_by_key(|item| item.band as u8);
        result.push(page);
    }
    Ok(result)
}
fn content_width(r: &Resolved) -> f32 {
    r.geometry.page_w - 2.0 * f32::from(r.frame_inset) - 2.0 * r.geometry.gutter_x
}
fn char_width(r: &Resolved) -> f32 {
    r.advance * r.geometry.font_body * f32::from(r.font_scale) / 100.0 / r.upem
}
fn wrapped_row(row: &TextRow, runs: Vec<InlineRun>, preserve_link: bool) -> TextRow {
    let text = runs.iter().map(|run| run.text.as_str()).collect();
    TextRow {
        text,
        runs,
        link: preserve_link.then(|| row.link.clone()).flatten(),
        edit_path: row.edit_path.clone(),
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
                link: row.link,
                edit_path: row.edit_path.clone(),
            });
        }
    }
    Ok(out)
}
fn push_asset_warning(warnings: &mut Vec<RenderWarning>, image: &crate::Image) {
    warnings.push(RenderWarning {
        code: "ASSET_UNRESOLVED".into(),
        message: format!("asset reference could not be resolved: {}", image.src),
    });
}
fn party_block(
    label: &str,
    p: &crate::Party,
    blocks: &mut Vec<Block>,
    assets: &[ResolvedAsset],
    image_budget: &mut usize,
    warnings: &mut Vec<RenderWarning>,
) -> Result<Option<ImageItem>, RenderError> {
    let mut rows = vec![TextRow {
        text: p.name.clone(),
        runs: Vec::new(),
        link: None,
        edit_path: None,
    }];
    rows.extend(p.address.iter().map(|x| TextRow {
        text: x.clone(),
        runs: Vec::new(),
        link: None,
        edit_path: None,
    }));
    if let Some(x) = &p.email {
        rows.push(TextRow {
            text: x.clone(),
            runs: Vec::new(),
            link: safe_mailto(x),
            edit_path: None,
        });
    }
    if let Some(x) = &p.website {
        rows.push(TextRow {
            text: x.clone(),
            runs: Vec::new(),
            link: safe_http_url(x),
            edit_path: None,
        });
    }
    rows.extend(p.identifiers.iter().map(|x| TextRow {
        text: format!("{} {}", x.key, x.value),
        runs: Vec::new(),
        link: None,
        edit_path: None,
    }));
    let image = if let Some(i) = &p.logo {
        if let Some(image) = decode_asset(i, assets, image_budget)? {
            Some(image)
        } else {
            push_asset_warning(warnings, i);
            rows.push(TextRow {
                text: format!("[ img ] {}", i.alt),
                runs: Vec::new(),
                link: safe_http_url(&i.src),
                edit_path: None,
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
    section_index: usize,
    doc: &Document,
    blocks: &mut Vec<Block>,
    assets: &[ResolvedAsset],
    image_budget: &mut usize,
    warnings: &mut Vec<RenderWarning>,
) -> Result<(), RenderError> {
    if s.directives.page_break_before {
        blocks.push(Block::PageBreak);
    }
    match &s.body {
        SectionBody::Prose(v) => {
            let rows = vec![TextRow {
                text: if v.is_empty() {
                    "No details provided.".into()
                } else {
                    v.clone()
                },
                runs: Vec::new(),
                link: None,
                edit_path: Some(format!("sections[{section_index}].prose")),
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
                Some(section_index),
            ));
        }
    }
    let _ = (assets, image_budget, warnings);
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
fn table_block(
    title: &str,
    t: &crate::Table,
    doc: &Document,
    total: Option<Decimal>,
    gap: u8,
    section_index: Option<usize>,
) -> Block {
    let headings = t.headings.clone();
    let column_index = |terms: &[&str]| {
        headings.iter().position(|h| {
            let h = h.trim().to_ascii_lowercase().replace(['-', '_'], " ");
            terms.iter().any(|term| h == *term || h.contains(term))
        })
    };
    let quantity = column_index(&["quantity", "qty", "hours", "days", "units"]);
    let rate = column_index(&["rate", "unit price", "price"]);
    let amount = crate::payable_amount_column(&headings, &doc.metadata.currency)
        .ok()
        .flatten()
        .or_else(|| headings.len().checked_sub(1));
    let mut rows = Vec::new();
    for source in &t.rows {
        let raw_cells = source.clone();
        let mut cells = raw_cells.clone();
        let mut edit_paths = Vec::with_capacity(cells.len());
        for i in 0..cells.len() {
            let mut computed = false;
            if (raw_cells[i].is_empty() || raw_cells[i].eq_ignore_ascii_case("auto"))
                && Some(i) == amount
            {
                if let (Some(qi), Some(ri)) = (quantity, rate) {
                    if let (Some(q), Some(rate)) = (
                        raw_cells.get(qi).and_then(|x| x.parse::<Decimal>().ok()),
                        raw_cells.get(ri).and_then(|x| x.parse::<Decimal>().ok()),
                    ) {
                        cells[i] =
                            format_money(q * rate, &doc.metadata.currency, &doc.config.format);
                        computed = true;
                    }
                }
            } else if let Ok(value) = raw_cells[i].parse::<Decimal>() {
                if Some(i) == amount {
                    cells[i] = format_money(value, &doc.metadata.currency, &doc.config.format);
                }
            }
            edit_paths.push(if computed {
                None
            } else if let Some(n) = section_index {
                Some(format!(
                    "sections[{n}].table.rows[{}].cells[{i}]",
                    rows.len()
                ))
            } else if title == "Settlements" {
                Some(format!("settlements.rows[{}].cells[{i}]", rows.len()))
            } else {
                None
            });
        }
        rows.push(TableRow {
            cells,
            alignments: t.alignments.clone(),
            edit_paths,
        });
    }
    if let Some(total) = total {
        let n = headings
            .len()
            .max(rows.iter().map(|row| row.cells.len()).max().unwrap_or(0));
        let mut subtotal = vec![String::new(); n.max(1)];
        subtotal[0] = "Subtotal".into();
        subtotal[n.max(1) - 1] = format_money(total, &doc.metadata.currency, &doc.config.format);
        rows.push(TableRow {
            cells: subtotal,
            alignments: t.alignments.clone(),
            edit_paths: vec![None; n.max(1)],
        });
    }
    Block::Table {
        title: title.into(),
        headings,
        rows,
        gap,
    }
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
        "code-space-comma" => ('\u{a0}', ','),
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
        out.insert(0, '-');
    }
    format!("{currency}\u{a0}{out}")
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
            Event::Start(Tag::BlockQuote(_)) => {
                out.push(InlineRun {
                    kind: InlineKind::QuoteMarker,
                    text: String::new(),
                });
                out.push(InlineRun {
                    kind: InlineKind::Break,
                    text: String::new(),
                });
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                out.push(InlineRun {
                    kind: InlineKind::Break,
                    text: String::new(),
                });
            }
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
    let http = value.get(..7).map(str::to_ascii_lowercase);
    let https = value.get(..8).map(str::to_ascii_lowercase);
    if http.as_deref() == Some("http://") || https.as_deref() == Some("https://") {
        Some(value.to_owned())
    } else {
        None
    }
}
fn safe_mailto(s: &str) -> Option<String> {
    let value = s.trim();
    if value.is_empty()
        || value
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
        || value.matches('@').count() != 1
    {
        return None;
    }
    Some(format!("mailto:{value}"))
}
#[cfg(test)]
fn inline_html(s: &str) -> String {
    inline_html_runs(&inline_runs(s), s)
}
#[cfg(test)]
fn inline_html_runs(runs: &[InlineRun], fallback: &str) -> String {
    let mut out = String::new();
    if runs.is_empty() {
        return esc(fallback);
    }
    for run in runs {
        match &run.kind {
            InlineKind::Strong => out.push_str(&format!("<strong>{}</strong>", esc(&run.text))),
            InlineKind::Emphasis => out.push_str(&format!("<em>{}</em>", esc(&run.text))),
            InlineKind::EmphasisStrong => {
                out.push_str(&format!("<strong><em>{}</em></strong>", esc(&run.text)))
            }
            InlineKind::Code => out.push_str(&format!("<code>{}</code>", esc(&run.text))),
            InlineKind::ListMarker | InlineKind::QuoteMarker | InlineKind::Text => {
                out.push_str(&esc(&run.text))
            }
            InlineKind::Link(href) => {
                if let Some(href) = safe_href(href) {
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
        }
    }
    out
}
fn safe_href(s: &str) -> Option<String> {
    safe_http_url(s).or_else(|| {
        let prefix = s.get(..7)?;
        if !prefix.eq_ignore_ascii_case("mailto:") {
            return None;
        }
        safe_mailto(&s[7..])
    })
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
    if safe_http_url(&image.src).is_some()
        || (image.src.contains(':') && !image.src.starts_with("data:image/"))
    {
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
        display_width: w as f32 * scale,
        display_height: h as f32 * scale,
    }))
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
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn encode_html(plan: &Plan) -> Result<Vec<u8>, RenderError> {
    let r = &plan.resolved;
    let semantic = &plan.semantic;
    let mut o = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:; base-uri 'none'; object-src 'none'; script-src 'none'\"><style>\
@font-face{{font-family:'ttyinv';font-weight:{};src:url(data:font/ttf;base64,{}) format('truetype')}}\
@font-face{{font-family:'ttyinv';font-weight:{};src:url(data:font/ttf;base64,{}) format('truetype')}}\
*{{box-sizing:border-box}}html,body{{margin:0;background:{}}}.page{{position:relative;width:{}px;height:{}px;margin:16px auto;background:{};overflow:hidden;font-family:'ttyinv',monospace;font-size:{}px;font-weight:{};font-variant-numeric:tabular-nums;font-variant-ligatures:none}}\
.primitive{{position:absolute;white-space:pre;line-height:{}px}}.link-overlay{{position:absolute;display:block;color:transparent;text-decoration:none;z-index:2}}.rule{{background:repeating-linear-gradient(90deg,currentColor 0 3px,transparent 3px 6px);height:1px}}.stroke{{border:0!important}}.invoice-frame{{pointer-events:none;background:repeating-linear-gradient(90deg,currentColor 0 3px,transparent 3px 6px) top/100% 1px no-repeat,repeating-linear-gradient(180deg,currentColor 0 3px,transparent 3px 6px) right/1px 100% no-repeat,repeating-linear-gradient(90deg,currentColor 0 3px,transparent 3px 6px) bottom/100% 1px no-repeat,repeating-linear-gradient(180deg,currentColor 0 3px,transparent 3px 6px) left/1px 100% no-repeat}}\
.semantic-layer{{position:absolute;left:-10000px;top:0;width:1px;overflow:hidden}}\
@media print{{.page{{break-after:page;margin:0}}}}</style></head><body>",
        r.font_weight_number,
        b64(r.font_bytes),
        r.semibold_weight_number,
        b64(r.semibold_bytes),
        color_hex(r.tokens.canvas),
        PAGE_WIDTH,
        PAGE_HEIGHT,
        color_hex(r.tokens.paper),
        8.26 * f32::from(r.font_scale) / 100.0,
        r.font_weight_number,
        r.line_advance
    );
    for (page_index, page) in plan.positioned.iter().enumerate() {
        o.push_str(&format!(
            "<article class=\"page invoice-sheet\" data-node-role=\"sheet\" aria-label=\"{}\">",
            esc(&format!("Invoice {}", semantic.number))
        ));
        o.push_str("<div class=\"semantic-layer\">");
        if page_index == 0 {
            o.push_str(&format!("<header><h1>{}</h1><dl>", esc(&semantic.title)));
            let metadata = [
                ("Ref", semantic.number.clone()),
                ("Kind", semantic.kind.clone()),
                ("Issued", semantic.issued.clone()),
            ];
            for (label, value) in metadata {
                o.push_str(&format!("<dt>{}</dt><dd>{}</dd>", esc(label), esc(&value)));
            }
            if let Some(due) = &semantic.due {
                o.push_str(&format!("<dt>Due</dt><dd>{}</dd>", esc(due)));
            }
            if let Some(terms) = &semantic.terms {
                o.push_str(&format!("<dt>Terms</dt><dd>{}</dd>", esc(terms)));
            }
            o.push_str(&format!(
                "<dt>Currency</dt><dd>{}</dd></dl></header><div class=\"invoice-parties\">",
                esc(&semantic.currency)
            ));
            for (label, party) in [("From", &semantic.from), ("Bill to", &semantic.bill_to)] {
                o.push_str(&format!(
                    "<section class=\"invoice-party\"><h2>{}</h2><p>{}</p>",
                    label,
                    esc(&party.name)
                ));
                for address in &party.address {
                    o.push_str(&format!("<p>{}</p>", esc(address)));
                }
                if let Some(email) = &party.email {
                    o.push_str(&format!("<p>{}</p>", esc(email)));
                }
                if let Some(website) = &party.website {
                    o.push_str(&format!(
                        "<p><a href=\"{}\">{}</a></p>",
                        esc(website),
                        esc(website)
                    ));
                }
                for (key, value) in &party.identifiers {
                    o.push_str(&format!("<p>{} {}</p>", esc(key), esc(value)));
                }
                if let Some(alt) = &party.logo_alt {
                    o.push_str(&format!("<p>[ img ] {}</p>", esc(alt)));
                }
                o.push_str("</section>");
            }
            o.push_str("</div>");
        }
        o.push_str("<main>");
        let mut footer_open = false;
        if let Some(source_page) = plan.pages.get(page_index) {
            for block in &source_page.blocks {
                if is_footer_block(block) && !footer_open {
                    o.push_str("</main><footer>");
                    footer_open = true;
                }
                match block {
                    Block::Text { title, rows, .. } if title != "From" && title != "Bill to" => {
                        o.push_str(&format!("<section><h2>{}</h2>", esc(title)));
                        for row in rows {
                            o.push_str(&format!("<p>{}</p>", esc(&row.text)));
                        }
                        o.push_str("</section>");
                    }
                    Block::Table {
                        title,
                        headings,
                        rows,
                        ..
                    } => {
                        o.push_str(&format!(
                            "<section><h2>{}</h2><table><caption>{}</caption><thead><tr>",
                            esc(title),
                            esc(title)
                        ));
                        for heading in headings {
                            o.push_str(&format!("<th scope=\"col\">{}</th>", esc(heading)));
                        }
                        o.push_str("</tr></thead><tbody>");
                        for row in rows {
                            if row
                                .cells
                                .first()
                                .is_some_and(|v| v.eq_ignore_ascii_case("subtotal"))
                            {
                                continue;
                            }
                            o.push_str("<tr>");
                            for cell in &row.cells {
                                o.push_str(&format!("<td>{}</td>", esc(cell)));
                            }
                            o.push_str("</tr>");
                        }
                        o.push_str("</tbody></table></section>");
                    }
                    Block::Payment { methods, .. } => {
                        o.push_str("<section aria-label=\"Payment\"><h2>Payment</h2>");
                        for method in methods {
                            o.push_str(&format!("<h3>{}</h3><dl>", esc(&method.title)));
                            for field in &method.fields {
                                o.push_str(&format!(
                                    "<dt>{}</dt><dd>{}</dd>",
                                    esc(&field.label),
                                    esc(&field.value)
                                ));
                            }
                            o.push_str("</dl>");
                        }
                        o.push_str("</section>");
                    }
                    Block::Signature {
                        name,
                        label,
                        image_alt,
                        ..
                    } => {
                        o.push_str(&format!("<section aria-label=\"Signature\"><h2>Signature</h2><p><strong>{}</strong> {}</p>",
                            esc(name), esc(label)));
                        if let Some(alt) = image_alt {
                            o.push_str(&format!("<p>[ img ] {}</p>", esc(alt)));
                        }
                        o.push_str("</section>");
                    }
                    Block::AmountInWords { label, text } => o.push_str(&format!(
                        "<p class=\"amount-in-words\"><strong>{}</strong> {}</p>",
                        esc(label),
                        esc(text)
                    )),
                    Block::Total => o.push_str(&format!(
                        "<p><strong>Total due</strong> {}</p>",
                        esc(&format_money(
                            plan.grand_total,
                            &plan.currency,
                            &plan.money_format
                        ))
                    )),
                    _ => {}
                }
            }
        }
        if footer_open {
            o.push_str("</footer>");
        } else {
            o.push_str("</main>");
        }
        o.push_str("</div>");
        for placed in &page.items {
            match &placed.primitive {
                Primitive::Rect { x, y, w, h, fill } => o.push_str(&format!(
                    "<div class=\"primitive\" style=\"left:{x}px;top:{y}px;width:{w}px;height:{h}px;background:{}\"></div>",
                    color_hex(*fill))),
                Primitive::Stroke { x, y, w, h, color, .. } => o.push_str(&format!(
                    "<div class=\"primitive stroke invoice-frame\" style=\"left:{x}px;top:{y}px;width:{w}px;height:{h}px;color:{}\"></div>",
                    color_hex(*color))),
                Primitive::VRule { x, y, h, color, .. } => o.push_str(&format!(
                    "<div class=\"primitive\" style=\"left:{x}px;top:{y}px;width:1px;height:{h}px;background:repeating-linear-gradient(180deg,currentColor 0 3px,transparent 3px 6px);color:{}\"></div>",
                    color_hex(*color))),
                Primitive::Rule { x, y, w, color, .. } => o.push_str(&format!(
                    "<div class=\"primitive rule\" style=\"left:{x}px;top:{y}px;width:{w}px;color:{}\"></div>",
                    color_hex(*color))),
                Primitive::Text { x, baseline, size, tracking, spans, .. } => {
                    let top = *baseline - html_baseline_offset(r, *size, r.line_advance);
                    o.push_str(&format!("<div class=\"primitive\" style=\"left:{x}px;top:{top}px;font-size:{size}px;letter-spacing:{tracking}px\">"));
                    for span in spans {
                        let tag = if span.href.is_some() { "a" } else if span.face == Face::Semibold { "strong" } else { "span" };
                        let href = span.href.as_ref().map(|v| format!(" href=\"{}\"", esc(v))).unwrap_or_default();
                        let style = format!("color:{};{}{}", color_hex(span.color),
                            if span.slant == Slant::Oblique { "font-style:oblique 11.5deg;" } else { "" },
                            if span.underline { "text-decoration:underline;text-underline-offset:1.65px;" } else { "" });
                        o.push_str(&format!("<{tag}{href} style=\"{style}\">{}</{tag}>", esc(&span.text)));
                    }
                    o.push_str("</div>");
                }
                Primitive::Image { x, y, w, h, index, .. } => {
                    if let Some(image) = plan.images.get(*index) {
                        o.push_str(&format!("<img class=\"primitive\" alt=\"{}\" src=\"data:image/{};base64,{}\" style=\"left:{x}px;top:{y}px;width:{w}px;height:{h}px;object-fit:contain;object-position:left bottom\">",
                            esc(&image.alt), esc(&image.mime), b64(image.bytes.as_ref())));
                    }
                }
            }
        }
        for link in &page.links {
            if let Some(href) = safe_href(&link.href) {
                o.push_str(&format!(
                    "<a class=\"link-overlay\" href=\"{}\" aria-label=\"{}\" style=\"left:{}px;top:{}px;width:{}px;height:{}px\"></a>",
                    esc(&href),
                    esc(&link.label),
                    link.x,
                    link.y,
                    link.width,
                    link.height
                ));
            }
        }
        o.push_str("</article>");
    }
    o.push_str("</body></html>");
    Ok(o.into_bytes())
}

fn shape(
    text: &str,
    r: &Resolved,
    _size: f32,
    semibold: bool,
) -> Result<Vec<KrillaGlyph>, RenderError> {
    let face = if semibold {
        &r.semibold_shaping
    } else {
        &r.shaping
    };
    let mut b = rustybuzz::UnicodeBuffer::new();
    b.push_str(text);
    let features = [rustybuzz::Feature::new(
        rustybuzz::ttf_parser::Tag::from_bytes(b"liga"),
        0,
        ..,
    )];
    let shaped = rustybuzz::shape(face.as_ref(), &features, b);
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
    let face = if semibold {
        &r.semibold_shaping
    } else {
        &r.shaping
    };
    if text
        .chars()
        .all(|ch| ch.is_ascii() && (' '..='~').contains(&ch))
    {
        return text.chars().count() as f32 * r.advance * size / r.upem;
    }
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    let features = [rustybuzz::Feature::new(
        rustybuzz::ttf_parser::Tag::from_bytes(b"liga"),
        0,
        ..,
    )];
    rustybuzz::shape(face.as_ref(), &features, buffer)
        .glyph_positions()
        .iter()
        .map(|position| position.x_advance as f32 * size / face.units_per_em() as f32)
        .sum()
}
fn set_pdf_fill(surface: &mut krilla::surface::Surface<'_>, c: [u8; 3]) {
    surface.set_fill(Some(Fill {
        paint: rgb::Color::new(c[0], c[1], c[2]).into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::NonZero,
    }));
}
fn pdf_group_for(node: &Node) -> TagGroup {
    let tag = match node.role {
        "header" => TagKind::Hn(PdfTag::Hn(
            NonZeroU16::new(1).unwrap(),
            Some(node.label.clone()),
        )),
        "caption" => TagKind::Caption(PdfTag::Caption),
        "thead" => TagKind::THead(PdfTag::THead),
        "tbody" => TagKind::TBody(PdfTag::TBody),
        "tr" => TagKind::TR(PdfTag::TR),
        "th" => TagKind::TH(PdfTag::TH(TableHeaderScope::Column)),
        "td" => TagKind::TD(PdfTag::TD),
        "prose" => TagKind::P(PdfTag::P),
        "list" => TagKind::L(PdfTag::L(ListNumbering::None)),
        "party" | "metadata" | "section" | "footer" | "settlements" | "payment" | "signature" => {
            TagKind::Section(PdfTag::Section)
        }
        "parties" => TagKind::Figure(PdfTag::Figure(None)),
        "sections" => TagKind::Table(PdfTag::Table),
        _ => TagKind::P(PdfTag::P),
    };
    TagGroup::new(tag)
}
fn encode_pdf(plan: &Plan) -> Result<Vec<u8>, RenderError> {
    encode_pdf_positioned(plan)
}
fn encode_pdf_positioned(plan: &Plan) -> Result<Vec<u8>, RenderError> {
    let r = &plan.resolved;
    let font = PdfFont::new(r.font_bytes.to_vec().into(), 0)
        .ok_or_else(|| RenderError::Font("invalid PDF font".into()))?;
    let semibold_font = PdfFont::new(r.semibold_bytes.to_vec().into(), 0)
        .ok_or_else(|| RenderError::Font("invalid PDF font".into()))?;
    let mut doc = PdfDocument::new();
    let title = plan
        .tree
        .get(1)
        .map(|n| n.label.clone())
        .or_else(|| plan.tree.first().map(|n| n.label.clone()))
        .unwrap_or_else(|| "ttyinv invoice".into());
    doc.set_metadata(
        PdfMetadata::new()
            .title(title)
            .language("en-US".into())
            .creator("ttyinv".into())
            .producer("ttyinv".into())
            .document_id("ttyinv-render-v2".into()),
    );
    let mut tag_tree = TagTree::new();
    let mut content_group = TagGroup::new(PdfTag::Part);
    for display in &plan.positioned {
        let settings = PageSettings::from_wh(PAGE_WIDTH as f32, PAGE_HEIGHT as f32)
            .ok_or_else(|| RenderError::Backend("invalid page size".into()))?;
        let mut page = doc.start_page_with(settings);
        let mut surface = page.surface();
        let mut tagged = Vec::new();
        for placed in &display.items {
            match &placed.primitive {
                Primitive::VRule {
                    x,
                    y,
                    h,
                    color,
                    dash,
                } => {
                    draw_pdf_vertical_rule(&mut surface, *x, *y, *h, *color, *dash);
                }
                Primitive::Rect { x, y, w, h, fill } => {
                    draw_pdf_rect(&mut surface, *x, *y, *w, *h, *fill)
                }
                Primitive::Rule {
                    x,
                    y,
                    w,
                    color,
                    dash,
                } => {
                    draw_pdf_rule(&mut surface, *x, *y, *w, *color, *dash);
                }
                Primitive::Stroke {
                    x,
                    y,
                    w,
                    h,
                    color,
                    dash,
                } => {
                    draw_pdf_rule(&mut surface, *x, *y, *w, *color, *dash);
                    draw_pdf_vertical_rule(&mut surface, *x + *w - 0.83, *y, *h, *color, *dash);
                    draw_pdf_rule(&mut surface, *x, *y + *h - 0.83, *w, *color, *dash);
                    draw_pdf_vertical_rule(&mut surface, *x, *y, *h, *color, *dash);
                }
                Primitive::Text {
                    x,
                    baseline,
                    size,
                    tracking,
                    spans,
                    ..
                } => {
                    let tag_id = surface.start_tagged(ContentTag::Other);
                    let mut pen = *x;
                    for span in spans {
                        if span.text.is_empty() {
                            continue;
                        }
                        let strong = span.face == Face::Semibold;
                        let mut glyphs = shape(&span.text, r, *size, strong)?;
                        if *tracking != 0.0 && span.text.chars().count() > 1 {
                            let extra = *tracking / *size;
                            for glyph in glyphs.iter_mut().take(span.text.chars().count() - 1) {
                                glyph.x_advance += extra;
                            }
                        }
                        set_pdf_fill(&mut surface, span.color);
                        if span.slant == Slant::Oblique {
                            surface.push_transform(&Transform::from_skew(0.2036, 0.0));
                        }
                        surface.draw_glyphs(
                            Point::from_xy(pen, *baseline),
                            &glyphs,
                            if strong {
                                semibold_font.clone()
                            } else {
                                font.clone()
                            },
                            &span.text,
                            *size,
                            false,
                        );
                        if span.slant == Slant::Oblique {
                            surface.pop();
                        }
                        if span.underline {
                            draw_pdf_rule(
                                &mut surface,
                                pen,
                                *baseline + 1.65,
                                measure_text(&span.text, r, *size, strong),
                                span.color,
                                Dash::Solid,
                            );
                        }
                        pen += measure_text(&span.text, r, *size, strong)
                            + *tracking * span.text.chars().count().saturating_sub(1) as f32;
                    }
                    surface.end_tagged();
                    tagged.push((placed.node, tag_id));
                }
                Primitive::Image {
                    x, y, w, h, index, ..
                } => {
                    if let Some(image) = plan.images.get(*index) {
                        let img = match image.mime.as_str() {
                            "png" => krilla::Image::from_png(image.bytes.to_vec().into(), true),
                            "jpeg" => krilla::Image::from_jpeg(image.bytes.to_vec().into(), true),
                            "gif" => krilla::Image::from_gif(image.bytes.to_vec().into(), true),
                            "webp" => krilla::Image::from_webp(image.bytes.to_vec().into(), true),
                            _ => return Err(RenderError::Backend("unsupported image MIME".into())),
                        }
                        .map_err(RenderError::Backend)?;
                        let tag_id = surface.start_tagged(ContentTag::Other);
                        surface.push_transform(&Transform::from_translate(*x, *y));
                        surface.draw_image(
                            img,
                            Size::from_wh(*w, *h)
                                .ok_or_else(|| RenderError::Backend("image size".into()))?,
                        );
                        surface.pop();
                        surface.end_tagged();
                        tagged.push((placed.node, tag_id));
                    }
                }
            }
        }
        let mut page_group = TagGroup::new(PdfTag::Article);
        for (node_index, tag_id) in tagged {
            if let Some(node) = plan.tree.get(node_index) {
                let mut group = pdf_group_for(node);
                group.push(tag_id);
                page_group.push(group);
            } else {
                page_group.push(tag_id);
            }
        }
        content_group.push(page_group);
        surface.finish();
        for link in &display.links {
            let target = Target::Action(Action::Link(LinkAction::new(link.href.clone())));
            if let Some(rect) = Rect::from_xywh(link.x, link.y, link.width, link.height) {
                page.add_annotation(Annotation::new_link(
                    LinkAnnotation::new(rect, target),
                    Some(link.label.clone()),
                ));
            }
        }
        page.finish();
    }
    tag_tree.push(content_group);
    doc.set_tag_tree(tag_tree);
    doc.finish()
        .map_err(|e| RenderError::Encoding(format!("{e:?}")))
}
fn draw_pdf_rect(
    surface: &mut krilla::surface::Surface<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 3],
) {
    let mut path = PathBuilder::new();
    path.move_to(x, y);
    path.line_to(x + w, y);
    path.line_to(x + w, y + h);
    path.line_to(x, y + h);
    path.close();
    if let Some(path) = path.finish() {
        set_pdf_fill(surface, color);
        surface.set_stroke(None);
        surface.draw_path(&path);
    }
}
fn draw_pdf_rule(
    surface: &mut krilla::surface::Surface<'_>,
    x: f32,
    y: f32,
    w: f32,
    color: [u8; 3],
    dash: Dash,
) {
    let mut path = PathBuilder::new();
    let mut start = 0.0;
    while start < w {
        let len = match dash {
            Dash::Solid => w,
            Dash::Dashed => 3.0_f32.min(w - start),
        };
        path.move_to(x + start, y);
        path.line_to(x + start + len, y);
        path.line_to(x + start + len, y + 0.83);
        path.line_to(x + start, y + 0.83);
        path.close();
        if matches!(dash, Dash::Solid) {
            break;
        }
        start += 6.0;
    }
    if let Some(path) = path.finish() {
        set_pdf_fill(surface, color);
        surface.set_stroke(None);
        surface.draw_path(&path);
    }
}
fn draw_pdf_vertical_rule(
    surface: &mut krilla::surface::Surface<'_>,
    x: f32,
    y: f32,
    h: f32,
    color: [u8; 3],
    dash: Dash,
) {
    let mut path = PathBuilder::new();
    let mut start = 0.0;
    while start < h {
        let len = match dash {
            Dash::Solid => h,
            Dash::Dashed => 3.0_f32.min(h - start),
        };
        path.move_to(x, y + start);
        path.line_to(x + 0.83, y + start);
        path.line_to(x + 0.83, y + start + len);
        path.line_to(x, y + start + len);
        path.close();
        if matches!(dash, Dash::Solid) {
            break;
        }
        start += 6.0;
    }
    if let Some(path) = path.finish() {
        set_pdf_fill(surface, color);
        surface.set_stroke(None);
        surface.draw_path(&path);
    }
}
struct SharedPngBuffer(Rc<RefCell<Vec<u8>>>);
impl std::io::Write for SharedPngBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut output = self.0.borrow_mut();
        if output.len().saturating_add(bytes.len()) > MAX_RENDERED_BYTES {
            return Err(std::io::Error::other("PNG output too large"));
        }
        output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn encode_png(plan: &Plan) -> Result<(Vec<u8>, u32, u32), RenderError> {
    encode_png_positioned(plan)
}
macro_rules! draw_png_image_scaled {
    ($raw:expr, $width:expr, $base:expr, $scale:expr, $x:expr, $y:expr, $w:expr, $h:expr, $image:expr $(,)?) => {
        draw_png_image_scaled_impl(
            &mut PngCanvas {
                raw: $raw,
                width: $width,
                base: $base,
                scale: $scale,
            },
            $x,
            $y,
            $w,
            $h,
            $image,
        )
    };
}
macro_rules! draw_png_rect_scaled {
    ($raw:expr, $width:expr, $base:expr, $scale:expr, $x:expr, $y:expr, $w:expr, $h:expr, $color:expr $(,)?) => {
        draw_png_rect_scaled_impl(
            &mut PngCanvas {
                raw: $raw,
                width: $width,
                base: $base,
                scale: $scale,
            },
            $x,
            $y,
            $w,
            $h,
            $color,
        )
    };
}
macro_rules! draw_png_rule_scaled {
    ($raw:expr, $width:expr, $base:expr, $scale:expr, $x:expr, $y:expr, $w:expr, $color:expr, $dash:expr, $reverse:expr $(,)?) => {
        draw_png_rule_scaled_impl(
            &mut PngCanvas {
                raw: $raw,
                width: $width,
                base: $base,
                scale: $scale,
            },
            $x,
            $y,
            $w,
            $color,
            $dash,
            $reverse,
        )
    };
}
macro_rules! draw_png_vertical_scaled {
    ($raw:expr, $width:expr, $base:expr, $scale:expr, $x:expr, $y:expr, $h:expr, $color:expr, $dash:expr, $reverse:expr $(,)?) => {
        draw_png_vertical_scaled_impl(
            &mut PngCanvas {
                raw: $raw,
                width: $width,
                base: $base,
                scale: $scale,
            },
            $x,
            $y,
            $h,
            $color,
            $dash,
            $reverse,
        )
    };
}
macro_rules! draw_glyphs_png_face_scaled {
    ($raw:expr, $width:expr, $base:expr, $scale:expr, $text:expr, $x:expr, $baseline:expr, $size:expr, $r:expr, $semibold:expr, $color:expr, $slant:expr, $tracking:expr $(,)?) => {
        draw_glyphs_png_face_scaled_impl(
            &mut PngCanvas {
                raw: $raw,
                width: $width,
                base: $base,
                scale: $scale,
            },
            $text,
            $x,
            $baseline,
            $size,
            $r,
            GlyphStyle {
                semibold: $semibold,
                color: $color,
                slant: $slant,
                tracking: $tracking,
            },
        )
    };
}
fn encode_png_positioned(plan: &Plan) -> Result<(Vec<u8>, u32, u32), RenderError> {
    let scale = u32::from(plan.resolved.png_scale);
    let width = PAGE_WIDTH
        .checked_mul(scale)
        .ok_or(RenderError::OutputTooLarge {
            limit: MAX_RENDERED_BYTES,
        })?;
    let height = PAGE_HEIGHT
        .checked_mul(scale)
        .and_then(|h| h.checked_mul(plan.positioned.len() as u32))
        .ok_or(RenderError::OutputTooLarge {
            limit: MAX_RENDERED_BYTES,
        })?;
    let pixels = width as usize * (PAGE_HEIGHT * scale) as usize;
    if pixels > MAX_PNG_PIXELS {
        return Err(RenderError::OutputTooLarge {
            limit: MAX_PNG_PIXELS,
        });
    }
    let mut raw = vec![0u8; pixels * 4];
    for px in raw.chunks_exact_mut(4) {
        px.copy_from_slice(&[
            plan.resolved.tokens.paper[0],
            plan.resolved.tokens.paper[1],
            plan.resolved.tokens.paper[2],
            255,
        ]);
    }
    let output = Rc::new(RefCell::new(Vec::new()));
    let mut encoder = PngEncoder::new(SharedPngBuffer(output.clone()), width, height);
    let dpi = 72u32 * scale;
    let pixels_per_meter = ((dpi as f32 * 39.370_08).round()) as u32;
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    encoder.set_pixel_dims(Some(PixelDimensions {
        xppu: pixels_per_meter,
        yppu: pixels_per_meter,
        unit: Unit::Meter,
    }));
    let mut writer = encoder
        .write_header()
        .map_err(|e| RenderError::Encoding(e.to_string()))?
        .into_stream_writer()
        .map_err(|e| RenderError::Encoding(e.to_string()))?;
    for page in &plan.positioned {
        let base = 0;
        for placed in &page.items {
            match &placed.primitive {
                Primitive::Rect { x, y, w, h, fill } => {
                    draw_png_rect_scaled!(&mut raw, width, base, scale, *x, *y, *w, *h, *fill);
                }
                Primitive::VRule {
                    x,
                    y,
                    h,
                    color,
                    dash,
                } => {
                    draw_png_vertical_scaled!(
                        &mut raw, width, base, scale, *x, *y, *h, *color, *dash, false,
                    );
                }
                Primitive::Stroke {
                    x,
                    y,
                    w,
                    h,
                    color,
                    dash,
                    ..
                } => {
                    draw_png_rule_scaled!(
                        &mut raw, width, base, scale, *x, *y, *w, *color, *dash, false,
                    );
                    draw_png_rule_scaled!(
                        &mut raw,
                        width,
                        base,
                        scale,
                        *x,
                        *y + *h - 0.83,
                        *w,
                        *color,
                        *dash,
                        false,
                    );
                    draw_png_vertical_scaled!(
                        &mut raw, width, base, scale, *x, *y, *h, *color, *dash, false,
                    );
                    draw_png_vertical_scaled!(
                        &mut raw,
                        width,
                        base,
                        scale,
                        *x + *w - 0.83,
                        *y,
                        *h,
                        *color,
                        *dash,
                        false,
                    );
                }
                Primitive::Rule {
                    x,
                    y,
                    w,
                    color,
                    dash,
                } => {
                    draw_png_rule_scaled!(
                        &mut raw, width, base, scale, *x, *y, *w, *color, *dash, false,
                    );
                }
                Primitive::Text {
                    x,
                    baseline,
                    size,
                    advance,
                    tracking,
                    spans,
                    ..
                } => {
                    let mut pen = *x;
                    for span in spans {
                        let semibold = span.face == Face::Semibold;
                        draw_glyphs_png_face_scaled!(
                            &mut raw,
                            width,
                            base,
                            scale,
                            &span.text,
                            pen,
                            *baseline,
                            *size,
                            &plan.resolved,
                            semibold,
                            span.color,
                            span.slant,
                            *tracking,
                        );
                        let run_width =
                            span.text.chars().count() as f32 * (*advance + *tracking) - *tracking;
                        if span.underline {
                            draw_png_rule_scaled!(
                                &mut raw,
                                width,
                                base,
                                scale,
                                pen,
                                *baseline + 1.65,
                                run_width,
                                span.color,
                                Dash::Solid,
                                false,
                            );
                        }
                        pen += run_width;
                    }
                }
                Primitive::Image {
                    x, y, w, h, index, ..
                } => {
                    if let Some(image) = plan.images.get(*index) {
                        draw_png_image_scaled!(&mut raw, width, base, scale, *x, *y, *w, *h, image);
                    }
                }
            }
        }
        std::io::Write::write_all(&mut writer, &raw)
            .map_err(|e| RenderError::Encoding(e.to_string()))?;
        for px in raw.chunks_exact_mut(4) {
            px.copy_from_slice(&[
                plan.resolved.tokens.paper[0],
                plan.resolved.tokens.paper[1],
                plan.resolved.tokens.paper[2],
                255,
            ]);
        }
    }
    writer
        .finish()
        .map_err(|e| RenderError::Encoding(e.to_string()))?;
    let bytes = Rc::try_unwrap(output)
        .map_err(|_| RenderError::Encoding("PNG output still borrowed".into()))?
        .into_inner();
    Ok((bytes, width, height))
}
fn blend_rgba(dst: &mut [u8], src: &[u8], alpha: u8) {
    if dst.len() < 4 || src.len() < 3 || alpha == 0 {
        return;
    }
    if alpha == u8::MAX {
        dst[..3].copy_from_slice(&src[..3]);
        dst[3] = u8::MAX;
        return;
    }
    let source_alpha = u32::from(alpha);
    let destination_alpha = u32::from(dst[3]);
    let inverse_source = u32::from(u8::MAX - alpha);
    let output_alpha =
        source_alpha + (destination_alpha * inverse_source + 127) / u32::from(u8::MAX);
    if output_alpha == 0 {
        dst[..4].fill(0);
        return;
    }
    for channel in 0..3 {
        let source = u32::from(src[channel]) * source_alpha;
        let destination =
            u32::from(dst[channel]) * destination_alpha * inverse_source / u32::from(u8::MAX);
        dst[channel] = ((source + destination + output_alpha / 2) / output_alpha)
            .min(u32::from(u8::MAX)) as u8;
    }
    dst[3] = output_alpha.min(u32::from(u8::MAX)) as u8;
}
struct PngCanvas<'a> {
    raw: &'a mut [u8],
    width: u32,
    base: u32,
    scale: u32,
}
#[derive(Clone, Copy)]
struct GlyphStyle {
    semibold: bool,
    color: [u8; 3],
    slant: Slant,
    tracking: f32,
}
fn draw_png_image_scaled_impl(
    canvas: &mut PngCanvas<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    image: &ImageItem,
) {
    let raw = &mut *canvas.raw;
    let width = canvas.width;
    let base = canvas.base;
    let scale = canvas.scale;
    let dw = (w * scale as f32).round().max(1.0) as u32;
    let dh = (h * scale as f32).round().max(1.0) as u32;
    let x0 = (x * scale as f32).round().max(0.0) as u32;
    let y0 = base + (y * scale as f32).round().max(0.0) as u32;
    for dy in 0..dh {
        for dx in 0..dw {
            let sx = (u64::from(dx) * u64::from(image.width) / u64::from(dw))
                .min(u64::from(image.width.saturating_sub(1))) as u32;
            let sy = (u64::from(dy) * u64::from(image.height) / u64::from(dh))
                .min(u64::from(image.height.saturating_sub(1))) as u32;
            let xx = x0 + dx;
            let yy = y0 + dy;
            if xx >= width || yy >= raw.len() as u32 / (width * 4) {
                continue;
            }
            let src = ((sy * image.width + sx) * 4) as usize;
            let dst = ((yy * width + xx) * 4) as usize;
            blend_rgba(
                &mut raw[dst..dst + 4],
                &image.rgba[src..src + 4],
                image.rgba[src + 3],
            );
        }
    }
}
fn draw_png_rect_scaled_impl(
    canvas: &mut PngCanvas<'_>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 3],
) {
    let raw = &mut *canvas.raw;
    let width = canvas.width;
    let base = canvas.base;
    let scale = canvas.scale;
    let x0 = (x * scale as f32).round().max(0.0) as u32;
    let x1 = ((x + w) * scale as f32).round().max(0.0) as u32;
    let y0 = base + (y * scale as f32).round().max(0.0) as u32;
    let y1 = base + ((y + h) * scale as f32).round().max(0.0) as u32;
    for yy in y0..y1 {
        for xx in x0.min(width)..x1.min(width) {
            let idx = ((yy as usize * width as usize) + xx as usize) * 4;
            if idx + 4 <= raw.len() {
                raw[idx..idx + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
            }
        }
    }
}
fn draw_png_rule_scaled_impl(
    canvas: &mut PngCanvas<'_>,
    x: f32,
    y: f32,
    w: f32,
    color: [u8; 3],
    dash: Dash,
    reverse: bool,
) {
    let raw = &mut *canvas.raw;
    let width = canvas.width;
    let base = canvas.base;
    let scale = canvas.scale;
    let x0 = (x * scale as f32).round().max(0.0) as u32;
    let x1 = ((x + w) * scale as f32).round().max(0.0) as u32;
    let y0 = base + (y * scale as f32).round().max(0.0) as u32;
    let thickness = ((0.83 * scale as f32).round() as u32).max(1);
    for yy in y0..y0.saturating_add(thickness) {
        for xx in x0.min(width)..x1.min(width) {
            let offset = if reverse {
                x1.saturating_sub(xx + 1).saturating_sub(x0)
            } else {
                xx - x0
            };
            let on = match dash {
                Dash::Solid => true,
                Dash::Dashed => (offset / scale.max(1)) % 6 < 3,
            };
            if on {
                let idx = ((yy as usize * width as usize) + xx as usize) * 4;
                if idx + 4 <= raw.len() {
                    raw[idx..idx + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
                }
            }
        }
    }
}
fn draw_png_vertical_scaled_impl(
    canvas: &mut PngCanvas<'_>,
    x: f32,
    y: f32,
    h: f32,
    color: [u8; 3],
    dash: Dash,
    reverse: bool,
) {
    let raw = &mut *canvas.raw;
    let width = canvas.width;
    let base = canvas.base;
    let scale = canvas.scale;
    let x0 = (x * scale as f32).round().max(0.0) as u32;
    let y0 = base + (y * scale as f32).round().max(0.0) as u32;
    let y1 = base + ((y + h) * scale as f32).round().max(0.0) as u32;
    let thickness = ((0.83 * scale as f32).round() as u32).max(1);
    for yy in y0..y1 {
        let on = match dash {
            Dash::Solid => true,
            Dash::Dashed => {
                let offset = if reverse {
                    y1.saturating_sub(yy + 1).saturating_sub(y0)
                } else {
                    yy - y0
                };
                (offset / scale.max(1)) % 6 < 3
            }
        };
        if on {
            for dx in 0..thickness {
                let xx = x0.saturating_add(dx);
                if xx >= width {
                    continue;
                }
                let idx = ((yy as usize * width as usize) + xx as usize) * 4;
                if idx + 4 <= raw.len() {
                    raw[idx..idx + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
                }
            }
        }
    }
}
fn draw_glyphs_png_face_scaled_impl(
    canvas: &mut PngCanvas<'_>,
    text: &str,
    x: f32,
    baseline: f32,
    size: f32,
    r: &Resolved,
    style: GlyphStyle,
) {
    let raw = &mut *canvas.raw;
    let width = canvas.width;
    let base = canvas.base;
    let scale = canvas.scale;
    let semibold = style.semibold;
    let color = style.color;
    let slant = style.slant;
    let tracking = style.tracking;
    let raster = if semibold {
        &r.semibold_raster
    } else {
        &r.raster
    };
    let face = if semibold {
        &r.semibold_shaping
    } else {
        &r.shaping
    };
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    let features = [rustybuzz::Feature::new(
        rustybuzz::ttf_parser::Tag::from_bytes(b"liga"),
        0,
        ..,
    )];
    let shaped = rustybuzz::shape(face.as_ref(), &features, buffer);
    let upem = face.units_per_em() as f32;
    let mut pen = x;
    let px = size * scale as f32;
    let shear = if slant == Slant::Oblique { 0.2036 } else { 0.0 };
    for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        let (metrics, bitmap) = raster.rasterize_indexed(info.glyph_id as u16, px);
        let gx = (pen * scale as f32 + metrics.xmin as f32).round() as i32;
        let gy =
            (base as f32 + baseline * scale as f32 - metrics.ymin as f32 - metrics.height as f32)
                .round() as i32;
        for by in 0..metrics.height {
            for bx in 0..metrics.width {
                let row_shear = shear * (metrics.height.saturating_sub(by)) as f32;
                let xx = gx + bx as i32 + row_shear.round() as i32;
                let yy = gy + by as i32;
                if xx >= 0
                    && yy >= 0
                    && xx < width as i32
                    && yy < raw.len() as i32 / (width as i32 * 4)
                {
                    let idx = ((yy as u32 * width + xx as u32) * 4) as usize;
                    blend_rgba(
                        &mut raw[idx..idx + 4],
                        &color,
                        bitmap[by * metrics.width + bx],
                    );
                }
            }
        }
        pen += pos.x_advance as f32 / upem * size + tracking;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{execute, CommandOutcome, EditOperationInput, InvoiceCommand, Source};
    use std::{borrow::Cow, collections::BTreeSet};
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
        }
    }
    #[test]
    fn all_themes_resolve() {
        for id in supported_themes() {
            let result = render(
                SOURCE,
                RenderOptions {
                    theme: Some((*id).into()),
                    ..Default::default()
                },
            );
            assert!(result.is_ok(), "{id}");
        }
    }
    #[test]
    fn capability_registries_match_resolvers() {
        assert_eq!(supported_densities(), &["comfortable", "compact"]);
        assert_eq!(generated::FONT_ASSETS.len(), font_capabilities().count());
        for id in supported_themes() {
            assert!(theme_tokens(id).is_some(), "{id}");
        }
    }
    #[test]
    fn amount_formats() {
        let d = Decimal::new(123456, 2);
        assert_eq!(
            format_money(d, "EUR", "code-comma-dot"),
            "EUR\u{a0}1,234.56"
        );
        assert_eq!(
            format_money(d, "EUR", "code-dot-comma"),
            "EUR\u{a0}1.234,56"
        );
        assert_eq!(format_money(d, "JPY", "code-plain"), "JPY\u{a0}1235");
        assert_eq!(format_money(d, "KRW", "code-comma-dot"), "KRW\u{a0}1,235");
        assert_eq!(
            format_money(Decimal::new(123456, 2), "OMR", "code-plain"),
            "OMR\u{a0}1234.560"
        );
    }
    #[test]
    fn foreign_currency_columns_remain_authored() {
        let source = include_str!("../../../render-compat/18-foreign-currency-columns.md");
        let html = String::from_utf8(
            render(
                source,
                RenderOptions {
                    format: RenderFormat::Html,
                    ..Default::default()
                },
            )
            .unwrap()
            .bytes,
        )
        .unwrap();
        assert!(html.contains("7100.00"));
        assert!(!html.contains("EUR\u{a0}7,100.00"));
        assert!(html.contains("EUR\u{a0}12.70"));
        assert!(html.contains("EUR\u{a0}5,200.00"));
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
        assert_eq!(plan.resolved.line_advance, 12.06);
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
        let png = render(
            &styled_source,
            RenderOptions {
                format: RenderFormat::Png,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!png.bytes.is_empty());
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
                link: None,
                edit_path: None,
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
                link: None,
                edit_path: None,
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
        assert!(html.contains("Five hundred line stress document"));
        assert!(html.contains(".invoice-frame"));
        assert!(html.contains("font-variant-ligatures:none"));
        assert!(html.contains("invoice-sheet"));
        assert!(html.contains("invoice-sheet"));
    }
    #[test]
    fn html_semantics_are_populated_and_page_local() {
        let result = render(
            SOURCE,
            RenderOptions {
                format: RenderFormat::Html,
                ..Default::default()
            },
        )
        .unwrap();
        let html = String::from_utf8(result.bytes).unwrap();
        assert_eq!(html.matches("<article class=\"page").count(), result.pages);
        assert!(!html.contains("gap-"));
        assert!(!html.contains("TOTAL:"));
        assert!(!html.contains("display:none"));
        assert!(html.contains("<th scope=\"col\">Description</th>"));
        assert!(html.contains("<td>8</td>"));
        assert!(html.contains("Fictional Studio"));
    }
    #[test]
    fn html_line_height_preserves_plan_baseline_relationship() {
        let doc = document(SOURCE).unwrap();
        let resolved = resolve(&doc.config, RenderOptions::default()).unwrap();
        let plan = layout(&doc, resolved.clone()).unwrap();
        let (baseline, size) = plan
            .positioned
            .iter()
            .flat_map(|page| page.items.iter())
            .find_map(|item| match &item.primitive {
                Primitive::Text { baseline, size, .. } => Some((*baseline, *size)),
                _ => None,
            })
            .expect("simple fixture has text");
        let top = baseline - html_baseline_offset(&resolved, size, resolved.line_advance);
        assert!(
            (baseline_from_top(&resolved, top, size, resolved.line_advance) - baseline).abs()
                < f32::EPSILON
        );
        let html = String::from_utf8(
            render(
                SOURCE,
                RenderOptions {
                    format: RenderFormat::Html,
                    ..Default::default()
                },
            )
            .unwrap()
            .bytes,
        )
        .unwrap();
        assert!(html.contains(".primitive{position:absolute;white-space:pre;line-height:12.06px}"));
    }

    #[test]
    fn html_exposes_visible_contact_links_and_split_rects() {
        let result = render(
            SOURCE,
            RenderOptions {
                format: RenderFormat::Html,
                ..Default::default()
            },
        )
        .unwrap();
        let html = String::from_utf8(result.bytes).unwrap();
        assert_eq!(html.matches("class=\"link-overlay\"").count(), 3);
        assert!(html.contains("href=\"mailto:billing@example.com\""));
        assert!(html.matches("href=\"https://studio.example\"").count() >= 2);

        let doc = document(SOURCE).unwrap();
        let plan = layout(
            &doc,
            resolve(&doc.config, RenderOptions::default()).unwrap(),
        )
        .unwrap();
        let url_links: Vec<&LinkBox> = plan.positioned[0]
            .links
            .iter()
            .filter(|link| link.href == "https://studio.example")
            .collect();
        assert!(url_links.len() >= 2);
        assert!(url_links.windows(2).any(|links| links[0].y != links[1].y));
    }

    #[test]
    fn unresolved_assets_are_reported_as_plan_warnings() {
        let source = include_str!("../../../render-compat/09-signature-image.md");
        let result = render(
            source,
            RenderOptions {
                format: RenderFormat::Html,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(result.warnings.iter().any(|warning| {
            warning.code == "ASSET_UNRESOLVED"
                && warning
                    .message
                    .contains("https://assets.example/signature.svg")
        }));
    }
    #[test]
    fn prepared_render_round_trip_preserves_digest() {
        let doc = document(SOURCE).unwrap();
        let prepared = prepare_render(
            &doc,
            RenderOptions {
                format: RenderFormat::Html,
                ..Default::default()
            },
        )
        .unwrap();
        let encoded = serde_json::to_vec(&prepared).unwrap();
        let decoded: PreparedRender = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(prepared.plan_digest, decoded.plan_digest);
        assert_eq!(prepared.pages.len(), decoded.pages.len());
        assert!(decoded.pages.iter().flat_map(|page| page.items.iter()).any(
            |item| matches!(&item.primitive, PreparedPrimitive::Text { spans, .. }
                if spans.iter().any(|span| span.text.contains("EUR")))
        ));
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
    #[test]
    fn presentation_exposes_authoritative_geometry_and_accent() {
        let p = presentation(PresentationConfig {
            theme: "midnight".into(),
            accent: Some("#0f766e".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(p.tokens.paper, "#121214");
        assert_eq!(p.tokens.canvas, "#27272a");
        assert_eq!(p.accent.resolved, "#3c8f89");
        assert_eq!(p.content.left, 65.57);
        assert_eq!(p.geometry["hairline"], 0.83);
    }
    #[test]
    fn renderer_float_wire_values_are_exact_promoted_binary32() {
        let presentation = presentation(PresentationConfig::default()).unwrap();
        let encoded = serde_json::to_string(&presentation).unwrap();
        assert!(encoded.contains("\"bottom\":770.6500244140625"));
        assert!(encoded.contains("\"space\":1"));

        let primitive = PreparedPrimitive::Rect {
            x: 770.65_f32,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            fill: [0, 0, 0],
        };
        assert_eq!(
            serde_json::to_string(&primitive).unwrap(),
            r#"{"kind":"rect","x":770.6500244140625,"y":0,"w":1,"h":1,"fill":[0,0,0]}"#
        );
    }
    #[test]
    fn png_scale_changes_device_dimensions_only() {
        let source = include_str!("../../../render-compat/01-simple.md");
        let result = render(
            source,
            RenderOptions {
                format: RenderFormat::Png,
                png_scale: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((result.width, result.height), (1190, 1684));
        let stress = include_str!("../../../render-compat/10-multi-page-500.md");
        let reduced = render(
            stress,
            RenderOptions {
                format: RenderFormat::Png,
                png_scale: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(reduced.width, PAGE_WIDTH);
        assert_eq!(reduced.height, PAGE_HEIGHT * reduced.pages as u32);
        assert!(reduced
            .warnings
            .iter()
            .any(|warning| warning.code == "PNG_SCALE_REDUCED"));
    }
    #[test]
    fn positioned_wrap_preserves_inline_styles_and_links() {
        let doc = document(SOURCE).unwrap();
        let r = resolve(&doc.config, RenderOptions::default()).unwrap();
        let runs = inline_runs("prefix **bold** and [link](https://example.com)");
        let rows = wrap_runs(&r, &runs, r.geometry.font_body * 8.0, r.geometry.font_body);
        assert!(rows.len() > 1);
        assert!(rows
            .iter()
            .flatten()
            .any(|run| run.kind == InlineKind::Strong));
        assert!(rows
            .iter()
            .flatten()
            .any(|run| matches!(run.kind, InlineKind::Link(_))));
    }

    #[test]
    fn prose_geometry_emits_list_and_quote_primitives() {
        let runs = inline_runs("- one\n- two\n\n> quoted");
        assert!(runs.iter().any(|run| run.kind == InlineKind::ListMarker));
        assert!(runs.iter().any(|run| run.kind == InlineKind::QuoteMarker));
    }

    #[test]
    fn semantic_tree_contains_fixed_and_dynamic_roles() {
        let doc = document(SOURCE).unwrap();
        let plan = layout(
            &doc,
            resolve(&doc.config, RenderOptions::default()).unwrap(),
        )
        .unwrap();
        let roles: Vec<&str> = plan.tree.iter().map(|node| node.role).collect();
        for role in [
            "sheet",
            "header",
            "metadata",
            "party",
            "section",
            "caption",
            "thead",
            "tbody",
            "tr",
            "th",
            "td",
            "footer",
            "settlements",
            "payment",
            "signature",
        ] {
            assert!(roles.contains(&role), "missing semantic role {role}");
        }
        assert!(plan
            .positioned
            .iter()
            .flat_map(|page| page.items.iter())
            .any(|item| item.node != 0));
    }
    #[test]
    fn corpus_geometry_and_semantics_cover_last_mile_blocks() {
        let long = include_str!("../../../render-compat/02-long-title-address.md");
        let long_result = render(
            long,
            RenderOptions {
                format: RenderFormat::Html,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(long_result
            .bytes
            .windows(7)
            .any(|window| window == b"Invoice"));
        assert!(long_result.pages >= 1);

        let multi = include_str!("../../../render-compat/10-multi-page-500.md");
        let multi_result = render(
            multi,
            RenderOptions {
                format: RenderFormat::Html,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(multi_result.pages > 1);
        let multi_html = String::from_utf8(multi_result.bytes).unwrap();
        assert!(multi_html.matches("<thead>").count() >= multi_result.pages);

        let gaps = include_str!("../../../render-compat/07-gap-levels.md");
        let comfortable = render(
            gaps,
            RenderOptions {
                format: RenderFormat::Html,
                density: Some("comfortable".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let compact = render(
            gaps,
            RenderOptions {
                format: RenderFormat::Html,
                density: Some("compact".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_ne!(comfortable.bytes, compact.bytes);
        let footer = include_str!("../../../render-compat/08-settlements-payment.md");
        for format in [RenderFormat::Html, RenderFormat::Pdf, RenderFormat::Png] {
            let footer_result = render(
                footer,
                RenderOptions {
                    format,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(!footer_result.bytes.is_empty());
            if format == RenderFormat::Html {
                let footer_html = String::from_utf8(footer_result.bytes).unwrap();
                assert!(footer_html.contains("Payment"));
                assert!(footer_html.contains("Settlements"));
                assert!(footer_html.contains("<td>1000.00</td>"));
                assert!(!footer_html.contains("EUR\u{a0}1,000.00"));
            }
        }
        let signature_source = include_str!("../../../render-compat/09-signature-image.md");
        let signature_result = render(
            signature_source,
            RenderOptions {
                format: RenderFormat::Html,
                ..Default::default()
            },
        )
        .unwrap();
        let signature_html = String::from_utf8(signature_result.bytes).unwrap();
        assert!(signature_html.contains("Signature"));

        let options = include_str!("../../../render-compat/17-themes-density-fonts.md");
        for font in generated::FONT_ASSETS {
            let result = render(
                options,
                RenderOptions {
                    format: RenderFormat::Html,
                    font: Some(font.id.into()),
                    ..Default::default()
                },
            );
            assert!(result.is_ok(), "font {}", font.id);
        }
        for theme in supported_themes() {
            let result = render(
                options,
                RenderOptions {
                    format: RenderFormat::Html,
                    theme: Some((*theme).into()),
                    ..Default::default()
                },
            );
            assert!(result.is_ok(), "theme {theme}");
        }
    }

    #[test]
    fn tagged_pdf_contains_structure_metadata() {
        let result = render(
            SOURCE,
            RenderOptions {
                format: RenderFormat::Pdf,
                ..Default::default()
            },
        )
        .unwrap();
        let pdf = String::from_utf8_lossy(&result.bytes);
        assert!(pdf.contains("/StructTreeRoot"));
        assert!(pdf.contains("/Lang") || pdf.contains("/Language"));
    }
    #[test]
    fn prepared_plan_is_source_and_output_free() {
        let doc = document(SOURCE).unwrap();
        let prepared = prepare_render(&doc, RenderOptions::default()).unwrap();
        let json = serde_json::to_string(&prepared).unwrap();
        assert!(!json.contains("\"encoded\""));
        assert!(!json.contains("\"source\""));
        assert!(!json.contains("\"document\""));
        assert!(json.contains("\"kind\":\"rect\"") || json.contains("\"kind\":\"text\""));
        assert!(!json.contains("\"kind\":\"Rect\""));
        assert!(!json.contains("\"dash\":\"Dashed\""));
        assert!(!json.contains("\"face\":\"Regular\""));
        assert!(!json.contains("\"slant\":\"Oblique\""));
        assert!(!json.contains("\"align\":\"Left\""));
        assert!(json.contains("\"dash\":\"dashed\"") || !json.contains("\"dash\":"));
        assert!(json.contains("\"semantic\""));
        assert!(json.len() < MAX_RENDERED_BYTES);
    }

    #[test]
    fn prepared_plan_encodes_identically_for_every_format() {
        let doc = document(SOURCE).unwrap();
        for format in [RenderFormat::Html, RenderFormat::Pdf, RenderFormat::Png] {
            let options = RenderOptions {
                format,
                ..Default::default()
            };
            let prepared = prepare_render(&doc, options.clone()).unwrap();
            let from_source = render_document(&doc, options).unwrap();
            let from_prepared = render_prepared(&prepared).unwrap();
            assert_eq!(from_source.bytes, from_prepared.bytes);
            assert_eq!(from_source.width, from_prepared.width);
            assert_eq!(from_source.height, from_prepared.height);
        }
    }

    #[test]
    fn prepared_plan_rejects_primitive_semantic_image_and_format_tampering() {
        let doc = document(SOURCE).unwrap();
        let prepared = prepare_render(&doc, RenderOptions::default()).unwrap();

        let mut primitive = prepared.clone();
        match &mut primitive.pages[0].items[0].primitive {
            PreparedPrimitive::Rect { x, .. }
            | PreparedPrimitive::Stroke { x, .. }
            | PreparedPrimitive::Rule { x, .. }
            | PreparedPrimitive::VRule { x, .. }
            | PreparedPrimitive::Text { x, .. }
            | PreparedPrimitive::Image { x, .. } => *x += 1.0,
        }
        assert!(render_prepared(&primitive).is_err());

        let mut semantic = prepared.clone();
        semantic.semantic.title.push('!');
        assert!(render_prepared(&semantic).is_err());

        let mut image = prepared.clone();
        image.images.push(PreparedImage {
            alt: "tampered".into(),
            mime: "png".into(),
            bytes: vec![0],
            width: 1,
            height: 1,
            display_width: 1.0,
            display_height: 1.0,
        });
        assert!(render_prepared(&image).is_err());

        let mut format = prepared;
        format.format = RenderFormat::Pdf;
        assert!(render_prepared(&format).is_err());
    }
    #[test]
    fn prepared_items_expose_editable_scalar_paths_and_real_nodes() {
        let doc = document(SOURCE).unwrap();
        let prepared = prepare_render(&doc, RenderOptions::default()).unwrap();
        let paths: BTreeSet<String> = prepared
            .pages
            .iter()
            .flat_map(|page| page.items.iter())
            .filter_map(|item| item.edit_path.clone())
            .collect();
        let expected: BTreeSet<String> = [
            "title",
            "metadata.number",
            "metadata.kind",
            "metadata.issued",
            "metadata.due",
            "metadata.terms",
            "metadata.currency",
            "from.name",
            "from.address[0]",
            "from.address[1]",
            "from.email",
            "from.website",
            "from.identifiers.VAT",
            "bill_to.name",
            "bill_to.address[0]",
            "bill_to.address[1]",
            "sections[0].title",
            "sections[0].table.headings[0]",
            "sections[0].table.headings[1]",
            "sections[0].table.headings[2]",
            "sections[0].table.headings[3]",
            "sections[0].table.rows[0].cells[0]",
            "sections[0].table.rows[0].cells[1]",
            "sections[0].table.rows[0].cells[2]",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        assert_eq!(paths, expected);
        for item in prepared.pages.iter().flat_map(|page| page.items.iter()) {
            if item.edit_path.is_some() {
                assert_ne!(item.node, 0);
            }
            if matches!(
                item.primitive,
                PreparedPrimitive::Rect { .. }
                    | PreparedPrimitive::Stroke { .. }
                    | PreparedPrimitive::Rule { .. }
                    | PreparedPrimitive::VRule { .. }
            ) {
                assert!(item.edit_path.is_none());
            }
        }
        let revision = match execute(InvoiceCommand::Validate {
            source: Source::Markdown(Cow::Borrowed(SOURCE)),
        })
        .unwrap()
        {
            CommandOutcome::Validated { revision, .. } => revision,
            _ => panic!("validation returned the wrong outcome"),
        };
        for path in &paths {
            let value = match path.as_str() {
                "title" => "Simple consulting",
                "metadata.number" => "INV-2026-101",
                "metadata.kind" => "standard",
                "metadata.issued" => "2026-01-15",
                "metadata.due" => "2026-01-29",
                "metadata.terms" => "Net 14",
                "metadata.currency" => "EUR",
                "from.name" => "Fictional Studio",
                "from.address[0]" => "1 Example Street",
                "from.address[1]" => "Example City",
                "from.email" => "billing@example.com",
                "from.website" => "https://studio.example",
                "from.identifiers.VAT" => "EX000000000",
                "bill_to.name" => "Example Client Ltd",
                "bill_to.address[0]" => "2 Sample Road",
                "bill_to.address[1]" => "Sample City",
                "sections[0].title" => "Consulting fees",
                "sections[0].table.headings[0]" => "Description",
                "sections[0].table.headings[1]" => "Days",
                "sections[0].table.headings[2]" => "Rate",
                "sections[0].table.headings[3]" => "Amount (EUR)",
                "sections[0].table.rows[0].cells[0]" => "Systems review",
                "sections[0].table.rows[0].cells[1]" => "8",
                "sections[0].table.rows[0].cells[2]" => "650.00",
                _ => unreachable!("{path}"),
            };
            let edited = execute(InvoiceCommand::Edit {
                source: Source::Markdown(Cow::Borrowed(SOURCE)),
                base_revision: Cow::Borrowed(&revision),
                operation: EditOperationInput::SetScalar {
                    path: Cow::Borrowed(path),
                    value: Cow::Borrowed(value),
                },
            });
            assert!(
                matches!(edited, Ok(CommandOutcome::Edited { .. })),
                "{path}"
            );
        }
    }
}
