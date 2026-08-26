use serde::Serialize;
use ttyinv_core::{
    Diagnostic, EditRequest, EditResponse, MAX_SOURCE_BYTES, Severity, StructureManifest,
    apply_edit as apply_edit_engine, revision as revision_engine,
};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;
const MAX_DECODED_REQUEST_BYTES: usize = 256 * 1024;
#[derive(Debug, Serialize)]
struct ValidationResult<'a> {
    valid: bool,
    diagnostics: &'a [Diagnostic],
    structure_manifest: Option<StructureManifest>,
}
#[wasm_bindgen]
pub fn validate(source: &str) -> Result<JsValue, JsValue> {
    if source.len() > MAX_SOURCE_BYTES {
        return serde_wasm_bindgen::to_value(&ValidationResult {
            valid: false,
            diagnostics: &[Diagnostic {
                severity: Severity::Error,
                code: "INPUT002".into(),
                message: "input exceeds source size limit".into(),
                path: None,
                field_path: None,
                line: None,
                column: None,
                hint: None,
                section: None,
                section_index: None,
                row: None,
                column_name: None,
            }],
            structure_manifest: None,
        })
        .map_err(|e| JsValue::from_str(&e.to_string()));
    }
    let report = ttyinv_core::validate(source);
    let manifest = ttyinv_core::structure_manifest(source).ok();
    serde_wasm_bindgen::to_value(&ValidationResult {
        valid: report.is_valid(),
        diagnostics: report.diagnostics(),
        structure_manifest: manifest,
    })
    .map_err(|e| JsValue::from_str(&e.to_string()))
}
#[wasm_bindgen]
pub fn structure_manifest(source: &str) -> Result<JsValue, JsValue> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(JsValue::from_str("input exceeds source size limit"));
    }
    let m = ttyinv_core::structure_manifest(source)
        .map_err(|_| JsValue::from_str("source has no valid document structure"))?;
    serde_wasm_bindgen::to_value(&m).map_err(|e| JsValue::from_str(&e.to_string()))
}
#[wasm_bindgen]
pub fn revision(source: &str) -> String {
    revision_engine(source)
}
#[wasm_bindgen]
pub fn apply_edit(request: JsValue) -> Result<JsValue, JsValue> {
    let request: EditRequest =
        serde_wasm_bindgen::from_value(request).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let operation_size = match &request.operation {
        ttyinv_core::EditOperation::SetScalar { path, value } => {
            path.len().checked_add(value.len())
        }
        ttyinv_core::EditOperation::MoveSection { .. }
        | ttyinv_core::EditOperation::SetSectionGap { .. } => Some(0),
    }
    .ok_or_else(|| JsValue::from_str("request size overflow"))?;
    let size = request
        .source
        .len()
        .checked_add(request.base_revision.len())
        .and_then(|x| x.checked_add(operation_size))
        .and_then(|x| x.checked_add(64))
        .ok_or_else(|| JsValue::from_str("request size overflow"))?;
    if size > MAX_DECODED_REQUEST_BYTES {
        return Err(JsValue::from_str("request exceeds adapter size limit"));
    }
    let response: EditResponse = apply_edit_engine(request);
    serde_wasm_bindgen::to_value(&response).map_err(|e| JsValue::from_str(&e.to_string()))
}
