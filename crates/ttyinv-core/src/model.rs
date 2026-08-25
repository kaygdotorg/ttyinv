use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;

/// A source position in the invoice input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourcePosition {
    pub line: usize,
    pub column: usize,
}

/// The inclusive source range occupied by a parsed node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

fn deserialize_decimal<'de, D>(deserializer: D) -> Result<Decimal, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    let source = match value {
        serde_yaml::Value::String(value) => value,
        serde_yaml::Value::Number(value) => value.to_string(),
        _ => return Err(serde::de::Error::custom("money amount must be a decimal")),
    };
    if !is_decimal_syntax(&source) {
        return Err(serde::de::Error::custom("money amount must be a decimal"));
    }
    Decimal::from_str_exact(&source)
        .map_err(|_| serde::de::Error::custom("money amount must be a decimal"))
}

fn is_decimal_syntax(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(a, b)| (a, Some(b)));
    !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction
            .is_none_or(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frontmatter {
    pub schema: String,
    pub invoice: Invoice,
    pub from: Party,
    pub to: Party,
    pub payment: Option<Payment>,
    pub settlements: Option<Vec<Settlement>>,
    pub signature: Option<Signature>,
    pub appearance: Option<Appearance>,
    #[serde(skip)]
    pub span: SourceSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invoice {
    pub number: String,
    pub title: Option<String>,
    pub issued: String,
    pub due: Option<String>,
    pub currency: String,
    pub locale: Option<String>,
    pub terms: Option<String>,
    pub kind: Option<InvoiceKind>,
    #[serde(skip)]
    pub span: SourceSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Party {
    pub name: String,
    pub address: Option<Vec<Scalar>>,
    pub identifiers: Option<HashMap<String, Scalar>>,
    pub email: Option<String>,
    pub logo: Option<String>,
    pub website: Option<String>,
    #[serde(skip)]
    pub span: SourceSpan,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    String(String),
    Number(serde_yaml::Number),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payment {
    pub title: Option<String>,
    pub methods: Option<Vec<PaymentMethod>>,
    #[serde(rename = "pageBreakBefore")]
    pub page_break_before: Option<bool>,
    #[serde(skip)]
    pub span: SourceSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentMethod {
    pub title: String,
    pub fields: HashMap<String, Scalar>,
    #[serde(skip)]
    pub span: SourceSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    pub date: String,
    pub paid: Money,
    pub received: Option<Money>,
    #[serde(skip)]
    pub span: SourceSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Money {
    #[serde(deserialize_with = "deserialize_decimal")]
    pub amount: Decimal,
    pub currency: String,
    #[serde(skip)]
    pub span: SourceSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub image: Option<String>,
    pub label: Option<String>,
    pub name: Option<String>,
    #[serde(skip)]
    pub span: SourceSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Appearance {
    pub accent: Option<String>,
    pub density: Option<Density>,
    pub font: Option<Font>,
    pub ink: Option<String>,
    pub muted: Option<String>,
    pub paper: Option<String>,
    pub rule: Option<String>,
    #[serde(skip)]
    pub span: SourceSpan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Font {
    pub bold: Option<String>,
    pub family: Option<String>,
    pub regular: Option<String>,
    #[serde(skip)]
    pub span: SourceSpan,
}

#[derive(Debug, Deserialize)]
pub enum Density {
    #[serde(rename = "comfortable")]
    Comfortable,
    #[serde(rename = "compact")]
    Compact,
}

#[derive(Debug, Deserialize)]
pub enum InvoiceKind {
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "gst")]
    Gst,
}
