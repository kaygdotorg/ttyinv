use rust_decimal::Decimal;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Accent(String);

impl Accent {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.len() == 7
            && value.as_bytes()[0] == b'#'
            && value.as_bytes()[1..]
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        {
            Ok(Self(value))
        } else {
            Err("accent must be lowercase #rrggbb".into())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Accent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

impl fmt::Display for Accent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct FontScale(u8);

impl FontScale {
    pub const MIN: u8 = 100;
    pub const MAX: u8 = 140;

    pub fn new(value: u8) -> Result<Self, String> {
        (Self::MIN..=Self::MAX)
            .contains(&value)
            .then_some(Self(value))
            .ok_or_else(|| "font-scale must be an integer from 100 to 140".into())
    }

    pub const fn value(self) -> u8 {
        self.0
    }
}

impl Default for FontScale {
    fn default() -> Self {
        Self(100)
    }
}

impl<'de> Deserialize<'de> for FontScale {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u8::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl fmt::Display for FontScale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct FrameInset(u8);

impl FrameInset {
    pub const MIN: u8 = 30;
    pub const MAX: u8 = 60;

    pub fn new(value: u8) -> Result<Self, String> {
        (Self::MIN..=Self::MAX)
            .contains(&value)
            .then_some(Self(value))
            .ok_or_else(|| "frame-inset must be an integer from 30 to 60".into())
    }

    pub const fn value(self) -> u8 {
        self.0
    }
}

impl Default for FrameInset {
    fn default() -> Self {
        Self(54)
    }
}

impl<'de> Deserialize<'de> for FrameInset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u8::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl fmt::Display for FrameInset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SourceSpan {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default)]
    pub font_weight: FontWeight,
    #[serde(default = "default_density")]
    pub density: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<Accent>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub amount_in_words: bool,
    #[serde(default)]
    pub font_scale: FontScale,
    #[serde(default)]
    pub frame_inset: FrameInset,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FontWeight {
    #[default]
    Regular,
    Semibold,
}
impl fmt::Display for FontWeight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Regular => f.write_str("regular"),
            Self::Semibold => f.write_str("semibold"),
        }
    }
}
pub(crate) fn default_format() -> String {
    "code-comma-dot".into()
}
pub(crate) fn default_theme() -> String {
    "printable".into()
}
pub(crate) fn default_font() -> String {
    "geist-mono".into()
}
pub(crate) fn default_density() -> String {
    "comfortable".into()
}
fn is_false(value: &bool) -> bool {
    !*value
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub number: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub issued: Date,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<Date>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terms: Option<String>,
    pub currency: String,
}
fn default_kind() -> String {
    "standard".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Date(pub String);
impl Date {
    pub fn parse(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        if b.len() != 10
            || b[4] != b'-'
            || b[7] != b'-'
            || !b
                .iter()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        {
            return None;
        }
        let y = s[0..4].parse::<i32>().ok()?;
        let m = s[5..7].parse::<u32>().ok()?;
        let d = s[8..10].parse::<u32>().ok()?;
        let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
        let md = match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        };
        (y > 0 && d > 0 && d <= md).then(|| Self(s.into()))
    }
}
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub alt: String,
    pub src: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identifier {
    pub key: String,
    pub value: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Party {
    pub name: String,
    #[serde(default)]
    pub address: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default)]
    pub identifiers: Vec<Identifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<Image>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Money {
    pub amount: Decimal,
    pub currency: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    pub date: Date,
    pub paid: Money,
    pub received: Money,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentMethod {
    pub title: String,
    pub fields: Vec<LabelValue>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payment {
    pub methods: Vec<PaymentMethod>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<Image>,
    pub name: String,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelValue {
    pub label: String,
    pub value: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TableAlignment {
    #[default]
    None,
    Left,
    Center,
    Right,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub headings: Vec<String>,
    #[serde(default)]
    pub alignments: Vec<TableAlignment>,
    pub rows: Vec<Vec<String>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum SectionBody {
    Table(Table),
    Prose(String),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Gap {
    None,
    Tight,
    #[default]
    Standard,
    Roomy,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionDirectives {
    pub gap: Gap,
    pub page_break_before: bool,
    pub summary_only: bool,
}
impl Default for SectionDirectives {
    fn default() -> Self {
        Self {
            gap: Gap::Standard,
            page_break_before: false,
            summary_only: false,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub title: String,
    pub body: SectionBody,
    pub directives: SectionDirectives,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<Decimal>,
    #[serde(skip)]
    pub span: SourceSpan,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub config: Config,
    pub title: String,
    pub metadata: Metadata,
    pub from: Party,
    pub bill_to: Party,
    pub ordinary_sections: Vec<Section>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlements: Option<Table>,
    #[serde(default)]
    pub settlements_page_break_before: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment: Option<Payment>,
    #[serde(default)]
    pub payment_page_break_before: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
    #[serde(default)]
    pub signature_page_break_before: bool,
    #[serde(default)]
    pub grand_total: Decimal,
    #[serde(skip)]
    pub source: String,
}
