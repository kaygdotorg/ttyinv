use serde::Serialize;
use ttyinv_core::{
    apply_edit as apply_edit_engine, render as render_engine, revision as revision_engine,
    Diagnostic, EditRequest, EditResponse, RenderError, RenderOptions, StructureManifest,
    MAX_SOURCE_BYTES,
};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};

const MAX_DECODED_REQUEST_BYTES: usize = 256 * 1024;
const MAX_RENDER_OPTION_KEYS: u32 = 32;
const MAX_RENDER_OPTION_KEY_UTF16: u32 = 32;
#[derive(Serialize)]
struct ValidationResult<'a> {
    valid: bool,
    diagnostics: &'a [Diagnostic],
    structure_manifest: Option<StructureManifest>,
}
fn bounded_string(value: &JsValue, field: &str, limit: usize) -> Result<String, JsValue> {
    let length = value
        .dyn_ref::<js_sys::JsString>()
        .map(js_sys::JsString::length)
        .ok_or_else(|| adapter_error("invalid_request", format!("{field} must be a string")))?;
    if (length as usize) > limit {
        return Err(adapter_error(
            "request_too_large",
            format!("{field} exceeds adapter size limit ({limit} UTF-16 code units)"),
        ));
    }
    let value = value.as_string().ok_or_else(|| {
        adapter_error("invalid_request", format!("{field} is not a valid string"))
    })?;
    if value.len() > limit {
        return Err(adapter_error(
            "request_too_large",
            format!("{field} exceeds adapter size limit ({limit} bytes)"),
        ));
    }
    Ok(value)
}

fn bounded_source(value: JsValue) -> Result<String, JsValue> {
    if !value.is_string() {
        return Err(adapter_error("invalid_request", "source must be a string"));
    }
    bounded_string(&value, "source", MAX_SOURCE_BYTES)
}

#[wasm_bindgen]
pub fn validate(source: JsValue) -> Result<JsValue, JsValue> {
    let source = bounded_source(source)?;
    let report = ttyinv_core::validate(&source);
    let manifest = ttyinv_core::structure_manifest(&source).ok();
    serde_wasm_bindgen::to_value(&ValidationResult {
        valid: report.is_valid(),
        diagnostics: report.diagnostics(),
        structure_manifest: manifest,
    })
    .map_err(|e| JsValue::from_str(&e.to_string()))
}
#[wasm_bindgen]
pub fn structure_manifest(source: JsValue) -> Result<JsValue, JsValue> {
    let source = bounded_source(source)?;
    let m = ttyinv_core::structure_manifest(&source)
        .map_err(|_| JsValue::from_str("source has no valid document structure"))?;
    serde_wasm_bindgen::to_value(&m).map_err(|e| JsValue::from_str(&e.to_string()))
}
#[wasm_bindgen]
pub fn revision(source: JsValue) -> Result<String, JsValue> {
    Ok(revision_engine(&bounded_source(source)?))
}

fn snapshot_edit_request(request: &JsValue) -> Result<JsValue, JsValue> {
    if !request.is_object() || request.is_null() {
        return Err(adapter_error(
            "invalid_request",
            "edit request must be an object",
        ));
    }
    let get = |object: &JsValue, key: &str| {
        js_sys::Reflect::get(object, &JsValue::from_str(key))
            .map_err(|_| adapter_error("invalid_request", format!("cannot read {key}")))
    };
    let source_value = get(request, "source")?;
    let source = bounded_string(&source_value, "source", MAX_SOURCE_BYTES)?;
    let revision_value = get(request, "base_revision")?;
    let base_revision = bounded_string(&revision_value, "base_revision", MAX_SOURCE_BYTES)?;
    let sequence = get(request, "sequence")?;
    if sequence.as_f64().is_none() {
        return Err(adapter_error(
            "invalid_request",
            "sequence must be a number",
        ));
    }
    let operation = get(request, "operation")?;
    if !operation.is_object() || operation.is_null() {
        return Err(adapter_error(
            "invalid_request",
            "operation must be an object",
        ));
    }
    let kind_value = get(&operation, "kind")?;
    let kind = bounded_string(&kind_value, "operation.kind", 64)?;
    let operation_snapshot = js_sys::Object::new();
    js_sys::Reflect::set(
        &operation_snapshot,
        &JsValue::from_str("kind"),
        &JsValue::from_str(&kind),
    )
    .map_err(|_| adapter_error("invalid_request", "cannot snapshot operation.kind"))?;
    let mut size = source
        .len()
        .checked_add(base_revision.len())
        .and_then(|size| size.checked_add(kind.len()))
        .and_then(|size| size.checked_add(64))
        .ok_or_else(|| adapter_error("request_too_large", "request size overflow"))?;
    for key in ["path", "value", "gap"] {
        let value = get(&operation, key)?;
        if value.is_undefined() {
            continue;
        }
        let string = bounded_string(&value, &format!("operation.{key}"), MAX_SOURCE_BYTES)?;
        size = size
            .checked_add(string.len())
            .ok_or_else(|| adapter_error("request_too_large", "request size overflow"))?;
        if size > MAX_DECODED_REQUEST_BYTES {
            return Err(adapter_error(
                "request_too_large",
                "request exceeds adapter size limit",
            ));
        }
        js_sys::Reflect::set(
            &operation_snapshot,
            &JsValue::from_str(key),
            &JsValue::from_str(&string),
        )
        .map_err(|_| adapter_error("invalid_request", "cannot snapshot edit operation"))?;
    }
    for key in ["from", "to", "section"] {
        let value = get(&operation, key)?;
        if value.is_undefined() {
            continue;
        }
        if value.as_f64().is_none() {
            return Err(adapter_error(
                "invalid_request",
                format!("operation.{key} must be a number"),
            ));
        }
        js_sys::Reflect::set(&operation_snapshot, &JsValue::from_str(key), &value)
            .map_err(|_| adapter_error("invalid_request", "cannot snapshot edit operation"))?;
    }
    let snapshot = js_sys::Object::new();
    for (key, value) in [
        ("source", JsValue::from_str(&source)),
        ("base_revision", JsValue::from_str(&base_revision)),
        ("sequence", sequence),
        ("operation", operation_snapshot.into()),
    ] {
        js_sys::Reflect::set(&snapshot, &JsValue::from_str(key), &value)
            .map_err(|_| adapter_error("invalid_request", "cannot snapshot edit request"))?;
    }
    if size > MAX_DECODED_REQUEST_BYTES {
        return Err(adapter_error(
            "request_too_large",
            "request exceeds adapter size limit",
        ));
    }
    Ok(snapshot.into())
}

#[wasm_bindgen]
pub fn apply_edit(request: JsValue) -> Result<JsValue, JsValue> {
    let snapshot = snapshot_edit_request(&request)?;
    let request: EditRequest = serde_wasm_bindgen::from_value(snapshot)
        .map_err(|e| adapter_error("invalid_request", e.to_string()))?;
    let response: EditResponse = apply_edit_engine(request);
    serde_wasm_bindgen::to_value(&response).map_err(|e| JsValue::from_str(&e.to_string()))
}
fn adapter_error(code: &str, message: impl Into<String>) -> JsValue {
    let object = js_sys::Object::new();
    let code_value = JsValue::from_str(code);
    let _ = js_sys::Reflect::set(&object, &JsValue::from_str("code"), &code_value);
    let _ = js_sys::Reflect::set(&object, &JsValue::from_str("kind"), &code_value);
    let message = message.into();
    let _ = js_sys::Reflect::set(
        &object,
        &JsValue::from_str("message"),
        &JsValue::from_str(&message),
    );
    object.into()
}

fn render_error_value(error: RenderError) -> JsValue {
    let diagnostics = match &error {
        RenderError::InvalidDocument(diagnostics) => serde_wasm_bindgen::to_value(diagnostics).ok(),
        _ => None,
    };
    let (code, message) = match error {
        RenderError::SourceTooLarge { limit } => (
            "source_too_large",
            format!("source exceeds limit ({limit} bytes)"),
        ),
        RenderError::InvalidDocument(_) => ("invalid_document", "document is invalid".to_owned()),
        RenderError::UnsupportedTheme(value) => {
            ("unsupported_theme", format!("unsupported theme: {value}"))
        }
        RenderError::UnsupportedFont(value) => {
            ("unsupported_font", format!("unsupported font: {value}"))
        }
        RenderError::UnsupportedDensity(value) => (
            "unsupported_density",
            format!("unsupported density: {value}"),
        ),
        RenderError::InvalidAccent(value) => ("invalid_accent", format!("invalid accent: {value}")),
        RenderError::InvalidOption(value) => {
            ("invalid_option", format!("invalid render option: {value}"))
        }
        RenderError::InvalidAsset(value) => ("invalid_asset", format!("invalid asset: {value}")),
        RenderError::OutputTooLarge { limit } => (
            "output_too_large",
            format!("rendered output exceeds limit ({limit} bytes)"),
        ),
        RenderError::Encoding(value) => ("encoding", format!("render encoding failed: {value}")),
        RenderError::Font(value) => ("font", format!("font error: {value}")),
        RenderError::Backend(value) => ("backend", format!("render backend failed: {value}")),
    };
    let object = adapter_error(code, message);
    if let Some(diagnostics) = diagnostics {
        let _ = js_sys::Reflect::set(&object, &JsValue::from_str("diagnostics"), &diagnostics);
    }
    object
}

const MAX_WASM_OUTPUT_BYTES: usize = 32 * 1024 * 1024;

fn snapshot_render_options(options: &JsValue) -> Result<JsValue, JsValue> {
    if options.is_null() || options.is_undefined() {
        return Ok(JsValue::UNDEFINED);
    }
    if !options.is_object() {
        return Err(adapter_error(
            "invalid_options",
            "render options must be an object",
        ));
    }
    let keys = js_sys::Object::keys(options.unchecked_ref::<js_sys::Object>());
    if keys.length() > MAX_RENDER_OPTION_KEYS {
        return Err(adapter_error(
            "request_too_large",
            "render options contain too many fields",
        ));
    }
    let snapshot = js_sys::Object::new();
    let mut size = 0usize;
    for index in 0..keys.length() {
        let key_value = keys.get(index);
        let key_length = key_value
            .dyn_ref::<js_sys::JsString>()
            .map(js_sys::JsString::length)
            .ok_or_else(|| adapter_error("invalid_options", "render option key is invalid"))?;
        if key_length > MAX_RENDER_OPTION_KEY_UTF16 {
            return Err(adapter_error(
                "request_too_large",
                "render option key exceeds adapter size limit",
            ));
        }
        let key = key_value
            .as_string()
            .ok_or_else(|| adapter_error("invalid_options", "render option key is invalid"))?;
        if !matches!(
            key.as_str(),
            "format"
                | "theme"
                | "font"
                | "font_weight"
                | "density"
                | "accent"
                | "font_scale"
                | "frame_inset"
                | "assets"
        ) {
            return Err(adapter_error(
                "invalid_options",
                format!("unknown render option: {key}"),
            ));
        }
        size = size
            .checked_add(key_length as usize)
            .ok_or_else(|| adapter_error("request_too_large", "render request size overflow"))?;
        let value = js_sys::Reflect::get(options, &JsValue::from_str(&key))
            .map_err(|_| adapter_error("invalid_options", "cannot read render options"))?;
        let copied = if key == "assets" {
            let array = value
                .dyn_ref::<js_sys::Array>()
                .ok_or_else(|| adapter_error("invalid_options", "assets must be an array"))?;
            if array.length() > MAX_RENDER_OPTION_KEYS {
                return Err(adapter_error("request_too_large", "too many render assets"));
            }
            let copied = js_sys::Array::new_with_length(array.length());
            for index in 0..array.length() {
                let item = array.get(index);
                if !item.is_object() || item.is_null() {
                    return Err(adapter_error(
                        "invalid_options",
                        "render asset must be an object",
                    ));
                }
                let source = js_sys::Reflect::get(&item, &JsValue::from_str("source"))
                    .map_err(|_| adapter_error("invalid_options", "cannot read asset source"))?;
                let source = bounded_string(&source, "asset.source", MAX_SOURCE_BYTES)?;
                let bytes = js_sys::Reflect::get(&item, &JsValue::from_str("bytes"))
                    .map_err(|_| adapter_error("invalid_options", "cannot read asset bytes"))?;
                let bytes = bytes.dyn_ref::<js_sys::Uint8Array>().ok_or_else(|| {
                    adapter_error("invalid_options", "asset.bytes must be Uint8Array")
                })?;
                size = size
                    .checked_add(source.len())
                    .and_then(|size| size.checked_add(bytes.length() as usize))
                    .ok_or_else(|| {
                        adapter_error("request_too_large", "render request size overflow")
                    })?;
                if size > MAX_DECODED_REQUEST_BYTES {
                    return Err(adapter_error(
                        "request_too_large",
                        "render request exceeds adapter limit",
                    ));
                }
                let copied_bytes = js_sys::Uint8Array::new_with_length(bytes.length());
                copied_bytes.set(bytes, 0);
                let copied_item = js_sys::Object::new();
                js_sys::Reflect::set(
                    &copied_item,
                    &JsValue::from_str("source"),
                    &JsValue::from_str(&source),
                )?;
                js_sys::Reflect::set(&copied_item, &JsValue::from_str("bytes"), &copied_bytes)?;
                let mime = js_sys::Reflect::get(&item, &JsValue::from_str("mime"))
                    .map_err(|_| adapter_error("invalid_options", "cannot read asset mime"))?;
                if !mime.is_undefined() && !mime.is_null() {
                    let mime = bounded_string(&mime, "asset.mime", 128)?;
                    js_sys::Reflect::set(
                        &copied_item,
                        &JsValue::from_str("mime"),
                        &JsValue::from_str(&mime),
                    )?;
                }
                copied.set(index, copied_item.into());
            }
            copied.into()
        } else if value.is_string() {
            let value = bounded_string(&value, &format!("render option {key}"), MAX_SOURCE_BYTES)?;
            size = size.checked_add(value.len()).ok_or_else(|| {
                adapter_error("request_too_large", "render request size overflow")
            })?;
            JsValue::from_str(&value)
        } else if value.as_f64().is_some()
            || value.as_bool().is_some()
            || value.is_null()
            || value.is_undefined()
        {
            size = size.checked_add(16).ok_or_else(|| {
                adapter_error("request_too_large", "render request size overflow")
            })?;
            value
        } else {
            return Err(adapter_error(
                "invalid_options",
                format!("render option {key} must be a scalar"),
            ));
        };
        if size > MAX_DECODED_REQUEST_BYTES {
            return Err(adapter_error(
                "request_too_large",
                format!("render request exceeds adapter limit ({MAX_DECODED_REQUEST_BYTES} bytes)"),
            ));
        }
        js_sys::Reflect::set(&snapshot, &JsValue::from_str(&key), &copied)?;
    }
    Ok(snapshot.into())
}

fn preflight_render_request(
    source: JsValue,
    options: &JsValue,
) -> Result<(String, JsValue), JsValue> {
    let source = bounded_source(source)?;
    Ok((source, snapshot_render_options(options)?))
}

/// Render one document and return owned bytes plus metadata.
#[wasm_bindgen]
pub fn render(source: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
    let (source, options) = preflight_render_request(source, &options)?;
    let options = if options.is_undefined() {
        RenderOptions::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| adapter_error("invalid_options", format!("invalid render options: {e}")))?
    };
    let result = render_engine(&source, options).map_err(render_error_value)?;
    if result.bytes.len() > MAX_WASM_OUTPUT_BYTES {
        return Err(adapter_error(
            "output_too_large",
            format!("rendered output exceeds WASM limit ({MAX_WASM_OUTPUT_BYTES} bytes)"),
        ));
    }
    let object = js_sys::Object::new();
    let bytes = js_sys::Uint8Array::from(result.bytes.as_slice());
    let bytes_value: JsValue = bytes.into();
    js_sys::Reflect::set(&object, &JsValue::from_str("bytes"), &bytes_value)?;
    js_sys::Reflect::set(
        &object,
        &JsValue::from_str("mime"),
        &JsValue::from_str(&result.mime),
    )?;
    js_sys::Reflect::set(
        &object,
        &JsValue::from_str("extension"),
        &JsValue::from_str(&result.extension),
    )?;
    js_sys::Reflect::set(
        &object,
        &JsValue::from_str("pages"),
        &JsValue::from_f64(result.pages as f64),
    )?;
    js_sys::Reflect::set(
        &object,
        &JsValue::from_str("width"),
        &JsValue::from_f64(result.width as f64),
    )?;
    js_sys::Reflect::set(
        &object,
        &JsValue::from_str("height"),
        &JsValue::from_f64(result.height as f64),
    )?;
    let warnings = serde_wasm_bindgen::to_value(&result.warnings)
        .map_err(|e| adapter_error("metadata", e.to_string()))?;
    js_sys::Reflect::set(&object, &JsValue::from_str("warnings"), &warnings)?;
    Ok(object.into())
}
