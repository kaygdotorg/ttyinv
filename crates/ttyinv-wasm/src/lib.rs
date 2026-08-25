use serde::Serialize;
use ttyinv_core::{
    Diagnostic, MAX_SOURCE_BYTES, ScalarEditRequest, ScalarEditResponse, Severity,
    apply_scalar as apply_scalar_engine, revision as revision_engine,
};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

const MAX_DECODED_REQUEST_BYTES: usize = 256 * 1024;

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

    let report = ttyinv_core::validate(source);
    serialize_result(report.is_valid(), report.diagnostics())
}

/// Return the deterministic digest of exact source bytes.
#[wasm_bindgen]
pub fn revision(source: &str) -> String {
    revision_engine(source)
}

/// Apply one scalar edit after enforcing the adapter request bound.
#[wasm_bindgen]
pub fn apply_scalar(request: JsValue) -> Result<JsValue, JsValue> {
    let request: ScalarEditRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let decoded_bytes = request
        .source
        .len()
        .checked_add(request.base_revision.len())
        .and_then(|size| size.checked_add(request.path.len()))
        .and_then(|size| size.checked_add(request.value.len()))
        .ok_or_else(|| JsValue::from_str("request size overflow"))?;
    if decoded_bytes > MAX_DECODED_REQUEST_BYTES {
        return Err(JsValue::from_str("request exceeds the adapter size limit"));
    }
    let response: ScalarEditResponse = apply_scalar_engine(request);
    serde_wasm_bindgen::to_value(&response).map_err(|error| JsValue::from_str(&error.to_string()))
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
