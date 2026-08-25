use serde::Serialize;
use ttyinv_core::{Diagnostic, MAX_SOURCE_BYTES, Severity, validate as validate_source};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Debug, Serialize)]
struct ValidationResult<'a> {
    valid: bool,
    diagnostics: &'a [Diagnostic],
}

/// Validate one invoice source in the browser.
#[wasm_bindgen]
pub fn validate(source: &str) -> Result<JsValue, JsValue> {
    if source.len() > MAX_SOURCE_BYTES {
        return serialize_result(false, &[input_limit_diagnostic()]);
    }

    let report = validate_source(source);
    serialize_result(report.is_valid(), report.diagnostics())
}

fn input_limit_diagnostic() -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "INPUT002".to_owned(),
        message: "input exceeds the source size limit".to_owned(),
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

fn serialize_result(valid: bool, diagnostics: &[Diagnostic]) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&ValidationResult { valid, diagnostics })
        .map_err(|error| JsValue::from_str(&error.to_string()))
}
