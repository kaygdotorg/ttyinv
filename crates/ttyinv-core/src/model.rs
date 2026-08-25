// These fields preserve the complete typed schema for strict deserialization.
#![allow(dead_code)]
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Frontmatter {
    pub(crate) schema: String,
    pub(crate) invoice: Invoice,
    pub(crate) from: Party,
    pub(crate) to: Party,
    pub(crate) payment: Option<Payment>,
    pub(crate) settlements: Option<Vec<Settlement>>,
    pub(crate) signature: Option<Signature>,
    pub(crate) appearance: Option<Appearance>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Invoice {
    pub(crate) number: String,
    pub(crate) title: Option<String>,
    pub(crate) issued: String,
    pub(crate) due: Option<String>,
    pub(crate) currency: String,
    pub(crate) locale: Option<String>,
    pub(crate) terms: Option<String>,
    pub(crate) kind: Option<InvoiceKind>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Party {
    pub(crate) name: String,
    pub(crate) address: Option<Vec<Scalar>>,
    pub(crate) identifiers: Option<HashMap<String, Scalar>>,
    pub(crate) email: Option<String>,
    pub(crate) logo: Option<String>,
    pub(crate) website: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Scalar {
    String(String),
    Number(serde_yaml::Number),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Payment {
    pub(crate) title: Option<String>,
    pub(crate) methods: Option<Vec<PaymentMethod>>,
    #[serde(rename = "pageBreakBefore")]
    pub(crate) page_break_before: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PaymentMethod {
    pub(crate) title: String,
    pub(crate) fields: HashMap<String, Scalar>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settlement {
    pub(crate) date: String,
    pub(crate) paid: Money,
    pub(crate) received: Option<Money>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Money {
    pub(crate) amount: Scalar,
    pub(crate) currency: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Signature {
    pub(crate) image: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Appearance {
    pub(crate) accent: Option<String>,
    pub(crate) density: Option<Density>,
    pub(crate) font: Option<Font>,
    pub(crate) ink: Option<String>,
    pub(crate) muted: Option<String>,
    pub(crate) paper: Option<String>,
    pub(crate) rule: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Font {
    pub(crate) bold: Option<String>,
    pub(crate) family: Option<String>,
    pub(crate) regular: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) enum Density {
    #[serde(rename = "comfortable")]
    Comfortable,
    #[serde(rename = "compact")]
    Compact,
}

#[derive(Debug, Deserialize)]
pub(crate) enum InvoiceKind {
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "gst")]
    Gst,
}
