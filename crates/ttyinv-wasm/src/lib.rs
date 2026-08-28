use serde::Serialize;
use ttyinv_core::{
    execute as execute_core, invalid_command_message, CommandError, CommandErrorCode,
    CommandOutcome, Diagnostic, InvoiceCommand, RetryClass, MAX_ASSET_BYTES, MAX_ASSET_TOTAL_BYTES,
    MAX_RENDERED_BYTES, SOURCE_SIZE_LIMIT_MESSAGE,
};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};

const MAX_DECODED_REQUEST_BYTES: usize = 256 * 1024;
const MAX_SOURCE_BYTES: usize = ttyinv_core::MAX_SOURCE_BYTES;
const MAX_KEY_UTF16: u32 = 64;
const MAX_OBJECT_KEYS: u32 = 256;
const MAX_ARRAY_ITEMS: u32 = 4096;
const MAX_SNAPSHOT_DEPTH: usize = 64;

#[derive(Default)]
struct SnapshotBudget {
    bytes: usize,
}

impl SnapshotBudget {
    fn charge(&mut self, amount: usize) -> Result<(), JsValue> {
        self.bytes = self
            .bytes
            .checked_add(amount)
            .ok_or_else(|| preflight_error(CommandErrorCode::Limit, "request size overflow"))?;
        if self.bytes > MAX_DECODED_REQUEST_BYTES {
            return Err(preflight_error(
                CommandErrorCode::Limit,
                format!("request exceeds adapter limit ({MAX_DECODED_REQUEST_BYTES} bytes)"),
            ));
        }
        Ok(())
    }
}

fn diagnostic(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        severity: ttyinv_core::Severity::Error,
        code: code.to_owned(),
        message: message.into(),
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

fn error_value(error: CommandError) -> JsValue {
    serde_wasm_bindgen::to_value(&error).unwrap_or_else(|_| {
        let object = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &object,
            &JsValue::from_str("code"),
            &JsValue::from_str("invalid_request"),
        );
        object.into()
    })
}

fn preflight_error(code: CommandErrorCode, message: impl Into<String>) -> JsValue {
    let diagnostic_code = match code {
        CommandErrorCode::Limit => "LIMIT001",
        CommandErrorCode::InvalidAsset => "ASSET001",
        _ => "REQUEST001",
    };
    error_value(CommandError {
        code,
        diagnostics: vec![diagnostic(diagnostic_code, message)],
        retry: RetryClass::AfterInputChange,
    })
}

fn invalid_request(message: impl Into<String>) -> JsValue {
    preflight_error(CommandErrorCode::InvalidRequest, message)
}

fn read_keys(value: &JsValue, label: &str) -> Result<Vec<String>, JsValue> {
    if !value.is_object()
        || value.is_null()
        || value.dyn_ref::<js_sys::Array>().is_some()
        || value.dyn_ref::<js_sys::Uint8Array>().is_some()
    {
        return Err(invalid_request(format!("{label} must be an object")));
    }
    let keys = js_sys::Object::keys(value.unchecked_ref::<js_sys::Object>());
    if keys.length() > MAX_OBJECT_KEYS {
        return Err(preflight_error(
            CommandErrorCode::Limit,
            format!("{label} contains too many fields"),
        ));
    }
    let mut result = Vec::with_capacity(keys.length() as usize);
    for index in 0..keys.length() {
        let key = keys.get(index);
        let length = key
            .dyn_ref::<js_sys::JsString>()
            .map(js_sys::JsString::length)
            .ok_or_else(|| invalid_request(format!("{label} has an invalid field name")))?;
        if length > MAX_KEY_UTF16 {
            return Err(preflight_error(
                CommandErrorCode::Limit,
                format!("{label} field name exceeds adapter size limit"),
            ));
        }
        let key = key
            .as_string()
            .ok_or_else(|| invalid_request(format!("{label} has an invalid field name")))?;
        result.push(key);
    }
    Ok(result)
}

fn field(value: &JsValue, key: &str, label: &str) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(value, &JsValue::from_str(key))
        .map_err(|_| invalid_request(format!("cannot read {label}.{key}")))
}

fn object(entries: impl IntoIterator<Item = (String, JsValue)>) -> Result<JsValue, JsValue> {
    let result = js_sys::Object::new();
    for (key, value) in entries {
        js_sys::Reflect::set(&result, &JsValue::from_str(&key), &value)
            .map_err(|_| invalid_request("cannot snapshot request"))?;
    }
    Ok(result.into())
}

fn string(
    value: &JsValue,
    label: &str,
    budget: &mut SnapshotBudget,
    limit: usize,
) -> Result<JsValue, JsValue> {
    let js_string = value
        .dyn_ref::<js_sys::JsString>()
        .ok_or_else(|| invalid_request(format!("{label} must be a string")))?;
    if js_string.length() as usize > limit {
        let message = if label == "source" {
            SOURCE_SIZE_LIMIT_MESSAGE.to_owned()
        } else {
            format!("{label} exceeds adapter size limit ({limit} UTF-16 code units)")
        };
        return Err(preflight_error(CommandErrorCode::Limit, message));
    }
    let value = value
        .as_string()
        .ok_or_else(|| invalid_request(format!("{label} is not a valid string")))?;
    if value.len() > limit {
        let message = if label == "source" {
            SOURCE_SIZE_LIMIT_MESSAGE.to_owned()
        } else {
            format!("{label} exceeds adapter size limit ({limit} bytes)")
        };
        return Err(preflight_error(CommandErrorCode::Limit, message));
    }
    budget.charge(value.len())?;
    Ok(JsValue::from_str(&value))
}
fn base64_digit(value: u8) -> Option<u8> {
    match value {
        b'A'..=b'Z' => Some(value - b'A'),
        b'a'..=b'z' => Some(value - b'a' + 26),
        b'0'..=b'9' => Some(value - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decoded_base64_length(value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }
    let padding = bytes.iter().rev().take_while(|&&byte| byte == b'=').count();
    if padding > 2
        || bytes[..bytes.len() - padding]
            .iter()
            .any(|&byte| base64_digit(byte).is_none())
    {
        return None;
    }
    if padding > 0
        && (bytes.len() <= padding
            || bytes[bytes.len() - padding - 1] == b'='
            || (padding == 1
                && base64_digit(bytes[bytes.len() - 2]).is_some_and(|digit| digit & 0x03 != 0))
            || (padding == 2
                && base64_digit(bytes[bytes.len() - 3]).is_some_and(|digit| digit & 0x0f != 0)))
    {
        return None;
    }
    Some(bytes.len() / 4 * 3 - padding)
}

fn max_base64_length(decoded_bytes: usize) -> usize {
    decoded_bytes.saturating_add(2) / 3 * 4
}

fn asset_length(value: &JsValue, total: usize) -> Result<Option<usize>, JsValue> {
    if let Some(bytes) = value.dyn_ref::<js_sys::Uint8Array>() {
        return Ok(Some(bytes.length() as usize));
    }
    if let Some(array) = value.dyn_ref::<js_sys::Array>() {
        return Ok(Some(array.length() as usize));
    }
    let Some(js_string) = value.dyn_ref::<js_sys::JsString>() else {
        return Ok(None);
    };
    let length = js_string.length() as usize;
    if length > max_base64_length(MAX_ASSET_BYTES) {
        return Err(preflight_error(
            CommandErrorCode::InvalidAsset,
            "asset exceeds 1 MiB",
        ));
    }
    let remaining = MAX_ASSET_TOTAL_BYTES.saturating_sub(total);
    if length > max_base64_length(remaining) {
        return Err(preflight_error(
            CommandErrorCode::InvalidAsset,
            "aggregate asset byte budget exceeded",
        ));
    }
    Ok(value
        .as_string()
        .and_then(|value| decoded_base64_length(&value)))
}

fn validate_asset_limits(value: &JsValue, total: &mut usize) -> Result<(), JsValue> {
    let Some(length) = asset_length(value, *total)? else {
        return Ok(());
    };
    if length > MAX_ASSET_BYTES {
        return Err(preflight_error(
            CommandErrorCode::InvalidAsset,
            "asset exceeds 1 MiB",
        ));
    }
    *total = total.checked_add(length).ok_or_else(|| {
        preflight_error(CommandErrorCode::InvalidAsset, "asset byte budget overflow")
    })?;
    if *total > MAX_ASSET_TOTAL_BYTES {
        return Err(preflight_error(
            CommandErrorCode::InvalidAsset,
            "aggregate asset byte budget exceeded",
        ));
    }
    Ok(())
}

fn snapshot_asset_bytes(value: &JsValue, budget: &mut SnapshotBudget) -> Result<JsValue, JsValue> {
    budget.charge(16)?;
    if let Some(bytes) = value.dyn_ref::<js_sys::Uint8Array>() {
        let length = bytes.length() as usize;
        let copy = js_sys::Uint8Array::new_with_length(length as u32);
        copy.set(bytes, 0);
        return Ok(copy.into());
    }
    if let Some(array) = value.dyn_ref::<js_sys::Array>() {
        let copy = js_sys::Array::new_with_length(array.length());
        for index in 0..array.length() {
            copy.set(index, array.get(index));
        }
        return Ok(copy.into());
    }
    if let Some(value_string) = value.as_string() {
        if decoded_base64_length(&value_string).is_some() {
            return Ok(JsValue::from_str(&value_string));
        }
        return string(value, "asset.bytes", budget, MAX_SOURCE_BYTES);
    }
    Err(preflight_error(
        CommandErrorCode::InvalidAsset,
        "asset.bytes must be a Uint8Array, base64 string, or byte sequence",
    ))
}

fn scalar_or_tree(
    value: &JsValue,
    label: &str,
    budget: &mut SnapshotBudget,
    depth: usize,
) -> Result<JsValue, JsValue> {
    if depth > MAX_SNAPSHOT_DEPTH {
        return Err(preflight_error(
            CommandErrorCode::Limit,
            "request nesting exceeds adapter limit",
        ));
    }
    if value.is_string() {
        return string(value, label, budget, MAX_SOURCE_BYTES);
    }
    if value.is_null() || value.is_undefined() || value.as_bool().is_some() {
        budget.charge(1)?;
        return Ok(value.clone());
    }
    if let Some(number) = value.as_f64() {
        if !number.is_finite() {
            return Err(invalid_request(format!("{label} must be finite")));
        }
        budget.charge(16)?;
        return Ok(value.clone());
    }
    if let Some(bytes) = value.dyn_ref::<js_sys::Uint8Array>() {
        let length = bytes.length() as usize;
        if length > MAX_DECODED_REQUEST_BYTES {
            return Err(preflight_error(
                CommandErrorCode::Limit,
                format!("{label} exceeds adapter size limit"),
            ));
        }
        budget.charge(length)?;
        let copy = js_sys::Uint8Array::new_with_length(length as u32);
        copy.set(bytes, 0);
        return Ok(copy.into());
    }
    if let Some(array) = value.dyn_ref::<js_sys::Array>() {
        if array.length() > MAX_ARRAY_ITEMS {
            return Err(preflight_error(
                CommandErrorCode::Limit,
                format!("{label} contains too many items"),
            ));
        }
        budget.charge(array.length() as usize * 4)?;
        let copy = js_sys::Array::new_with_length(array.length());
        for index in 0..array.length() {
            let item = array.get(index);
            let item = scalar_or_tree(&item, &format!("{label}[{index}]"), budget, depth + 1)?;
            copy.set(index, item);
        }
        return Ok(copy.into());
    }
    if value.is_object() && !value.is_null() {
        let keys = read_keys(value, label)?;
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            budget.charge(key.len())?;
            let item = field(value, &key, label)?;
            entries.push((
                key.clone(),
                scalar_or_tree(&item, &format!("{label}.{key}"), budget, depth + 1)?,
            ));
        }
        return object(entries);
    }
    Err(invalid_request(format!("{label} has an unsupported value")))
}

fn strict_object(
    value: &JsValue,
    label: &str,
    allowed: &[&str],
    budget: &mut SnapshotBudget,
) -> Result<Vec<(String, JsValue)>, JsValue> {
    let keys = read_keys(value, label)?;
    let mut fields = Vec::with_capacity(keys.len());
    for key in keys {
        if !allowed.contains(&key.as_str()) {
            return Err(invalid_request(format!("unknown {label} field: {key}")));
        }
        budget.charge(key.len())?;
        fields.push((key.clone(), field(value, &key, label)?));
    }
    Ok(fields)
}

fn snapshot_source(
    value: &JsValue,
    label: &str,
    budget: &mut SnapshotBudget,
) -> Result<JsValue, JsValue> {
    let fields = strict_object(value, label, &["markdown", "json", "yaml"], budget)?;
    if fields.len() != 1 {
        return Err(invalid_request(format!(
            "{label} must contain exactly one source format"
        )));
    }
    let (key, value) = fields.into_iter().next().expect("length checked");
    object([(key, string(&value, label, budget, MAX_SOURCE_BYTES)?)])
}

fn snapshot_options(
    value: &JsValue,
    label: &str,
    budget: &mut SnapshotBudget,
) -> Result<JsValue, JsValue> {
    let fields = strict_object(
        value,
        label,
        &[
            "format",
            "theme",
            "font",
            "font_weight",
            "density",
            "accent",
            "font_scale",
            "frame_inset",
            "assets",
        ],
        budget,
    )?;
    let mut entries = Vec::with_capacity(fields.len());
    let mut asset_total = 0usize;
    for (key, value) in fields {
        if key == "assets" {
            let array = value
                .dyn_ref::<js_sys::Array>()
                .ok_or_else(|| invalid_request(format!("{label}.assets must be an array")))?;
            if array.length() > MAX_ARRAY_ITEMS {
                return Err(preflight_error(
                    CommandErrorCode::Limit,
                    "too many render assets",
                ));
            }
            budget.charge(16)?;
            let copied = js_sys::Array::new_with_length(array.length());
            for index in 0..array.length() {
                let asset = array.get(index);
                let asset_fields =
                    strict_object(&asset, "render asset", &["source", "bytes", "mime"], budget)?;
                let mut asset_entries = Vec::with_capacity(asset_fields.len());
                for (asset_key, asset_value) in asset_fields {
                    let copied_value = match asset_key.as_str() {
                        "source" => string(&asset_value, "asset.source", budget, MAX_SOURCE_BYTES)?,
                        "mime" => {
                            if asset_value.is_null() || asset_value.is_undefined() {
                                asset_value
                            } else {
                                string(&asset_value, "asset.mime", budget, 128)?
                            }
                        }
                        "bytes" => {
                            if !asset_value.is_string()
                                && asset_value.dyn_ref::<js_sys::Uint8Array>().is_none()
                                && asset_value.dyn_ref::<js_sys::Array>().is_none()
                            {
                                return Err(preflight_error(
                                    CommandErrorCode::InvalidAsset,
                                    "asset.bytes must be a Uint8Array, base64 string, or byte sequence",
                                ));
                            }
                            validate_asset_limits(&asset_value, &mut asset_total)?;
                            snapshot_asset_bytes(&asset_value, budget)?
                        }
                        _ => unreachable!(),
                    };
                    asset_entries.push((asset_key, copied_value));
                }
                copied.set(index, object(asset_entries)?);
            }
            entries.push((key, copied.into()));
        } else {
            entries.push((
                key.clone(),
                scalar_or_tree(&value, &format!("{label}.{key}"), budget, 0)?,
            ));
        }
    }
    object(entries)
}

fn snapshot_operation(value: &JsValue, budget: &mut SnapshotBudget) -> Result<JsValue, JsValue> {
    let fields = strict_object(
        value,
        "operation",
        &["kind", "path", "value", "from", "to", "section", "gap"],
        budget,
    )?;
    let mut entries = Vec::with_capacity(fields.len());
    for (key, value) in fields {
        let copied = if key == "kind" {
            string(&value, "operation.kind", budget, 64)?
        } else if matches!(key.as_str(), "path" | "value") {
            string(
                &value,
                &format!("operation.{key}"),
                budget,
                MAX_SOURCE_BYTES,
            )?
        } else {
            scalar_or_tree(&value, &format!("operation.{key}"), budget, 0)?
        };
        entries.push((key, copied));
    }
    object(entries)
}

fn snapshot_config(value: &JsValue, budget: &mut SnapshotBudget) -> Result<JsValue, JsValue> {
    let fields = strict_object(
        value,
        "presentation config",
        &[
            "theme",
            "font",
            "font_weight",
            "density",
            "accent",
            "font_scale",
            "frame_inset",
        ],
        budget,
    )?;
    let mut entries = Vec::with_capacity(fields.len());
    for (key, value) in fields {
        let copied = if matches!(key.as_str(), "theme" | "font" | "font_weight" | "density") {
            string(&value, &format!("presentation config.{key}"), budget, 64)?
        } else if key == "accent" && !value.is_null() && !value.is_undefined() {
            string(&value, "presentation config.accent", budget, 64)?
        } else {
            scalar_or_tree(&value, &format!("presentation config.{key}"), budget, 0)?
        };
        entries.push((key, copied));
    }
    object(entries)
}

fn snapshot_command(value: &JsValue) -> Result<JsValue, JsValue> {
    let mut budget = SnapshotBudget::default();
    let fields = strict_object(
        value,
        "command",
        &[
            "kind",
            "draft",
            "source",
            "mode",
            "to",
            "base_revision",
            "operation",
            "options",
            "config",
        ],
        &mut budget,
    )?;
    let kind = fields
        .iter()
        .find(|(key, _)| key == "kind")
        .map(|(_, value)| value.clone())
        .ok_or_else(|| invalid_request("command.kind is required"))?;
    let kind = kind
        .as_string()
        .ok_or_else(|| invalid_request("command.kind must be a string"))?;
    if kind.len() > 64 {
        return Err(preflight_error(
            CommandErrorCode::Limit,
            "command.kind is too long",
        ));
    }
    let allowed: &[&str] = match kind.as_str() {
        "create" => &["kind", "draft"],
        "validate" => &["kind", "source"],
        "inspect" => &["kind", "source", "mode"],
        "convert" => &["kind", "source", "to"],
        "edit" => &["kind", "source", "base_revision", "operation"],
        "prepare_render" => &["kind", "source", "options"],
        "resolve_presentation" => &["kind", "config"],
        "render" => &["kind", "source", "options"],
        "registry" => &["kind"],
        _ => return Err(invalid_request(format!("unknown command kind: {kind}"))),
    };
    let mut entries = Vec::with_capacity(fields.len());
    for (key, value) in fields {
        if !allowed.contains(&key.as_str()) {
            return Err(invalid_request(format!(
                "field {key} is not valid for command {kind}"
            )));
        }
        let copied = match key.as_str() {
            "kind" => string(&value, "command.kind", &mut budget, 64)?,
            "source" => snapshot_source(&value, "source", &mut budget)?,
            "options" => snapshot_options(&value, "render options", &mut budget)?,
            "config" => snapshot_config(&value, &mut budget)?,
            "operation" => snapshot_operation(&value, &mut budget)?,
            "base_revision" => string(&value, "base_revision", &mut budget, MAX_SOURCE_BYTES)?,
            "mode" | "to" => string(&value, &key, &mut budget, 32)?,
            // InvoiceDraft and its nested structs use deny_unknown_fields in core.
            // Copying the complete inert tree leaves exact field validation to serde.
            "draft" => scalar_or_tree(&value, "draft", &mut budget, 0)?,
            _ => unreachable!(),
        };
        entries.push((key, copied));
    }
    object(entries)
}

fn serialize_outcome(outcome: CommandOutcome) -> Result<JsValue, JsValue> {
    if let CommandOutcome::Rendered { bytes, .. } = &outcome {
        if bytes.len() > MAX_RENDERED_BYTES {
            return Err(preflight_error(
                CommandErrorCode::Limit,
                format!("rendered output exceeds core limit ({MAX_RENDERED_BYTES} bytes)"),
            ));
        }
    }
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    let value = outcome
        .serialize(&serializer)
        .map_err(|error| invalid_request(format!("cannot serialize command outcome: {error}")))?;
    if let CommandOutcome::Rendered { bytes, .. } = outcome {
        let rendered = js_sys::Reflect::get(&value, &JsValue::from_str("rendered"))
            .map_err(|_| invalid_request("cannot serialize rendered outcome"))?;
        let bytes = js_sys::Uint8Array::from(bytes.as_slice());
        js_sys::Reflect::set(&rendered, &JsValue::from_str("bytes"), &bytes)
            .map_err(|_| invalid_request("cannot serialize rendered bytes"))?;
    }
    Ok(value)
}

/// Execute one bounded, typed core command.
#[wasm_bindgen]
pub fn execute(command: JsValue) -> Result<JsValue, JsValue> {
    let snapshot = snapshot_command(&command)?;
    let command: InvoiceCommand<'static> = serde_wasm_bindgen::from_value(snapshot)
        .map_err(|error| invalid_request(invalid_command_message(error)))?;
    let outcome = execute_core(command).map_err(error_value)?;
    serialize_outcome(outcome)
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn code(error: JsValue) -> String {
        js_sys::Reflect::get(&error, &JsValue::from_str("code"))
            .unwrap()
            .as_string()
            .unwrap()
    }

    fn set(object: &js_sys::Object, key: &str, value: &JsValue) {
        js_sys::Reflect::set(object, &JsValue::from_str(key), value).unwrap();
    }

    fn source_value() -> JsValue {
        let source = js_sys::Object::new();
        set(
            &source,
            "markdown",
            &JsValue::from_str(include_str!("../../../examples/simple.md")),
        );
        source.into()
    }

    fn options_value() -> JsValue {
        let options = js_sys::Object::new();
        set(&options, "format", &JsValue::from_str("html"));
        options.into()
    }

    fn command(kind: &str) -> js_sys::Object {
        let command = js_sys::Object::new();
        set(&command, "kind", &JsValue::from_str(kind));
        command
    }

    fn assert_success(value: JsValue, key: &str) {
        assert!(js_sys::Reflect::has(&value, &JsValue::from_str(key)).unwrap());
        assert_eq!(
            js_sys::Object::keys(value.unchecked_ref::<js_sys::Object>()).length(),
            1
        );
    }

    fn assert_error_shape(error: JsValue, expected_code: &str) {
        assert_eq!(code(error.clone()), expected_code);
        let diagnostics = js_sys::Reflect::get(&error, &JsValue::from_str("diagnostics")).unwrap();
        assert!(diagnostics.dyn_ref::<js_sys::Array>().unwrap().length() > 0);
        let retry = js_sys::Reflect::get(&error, &JsValue::from_str("retry"))
            .unwrap()
            .as_string();
        assert!(retry.is_some());
    }
    fn diagnostic_message(error: &JsValue) -> String {
        let diagnostics = js_sys::Reflect::get(error, &JsValue::from_str("diagnostics")).unwrap();
        let first = js_sys::Reflect::get(&diagnostics, &JsValue::from_f64(0.0)).unwrap();
        js_sys::Reflect::get(&first, &JsValue::from_str("message"))
            .unwrap()
            .as_string()
            .unwrap()
    }

    #[wasm_bindgen_test]
    fn every_command_has_one_success_outcome_shape() {
        let source = source_value();
        let validate_command = command("validate");
        set(&validate_command, "source", &source);
        let validated = execute(validate_command.into()).expect("validate command");
        assert_success(validated.clone(), "validated");
        let validated_payload =
            js_sys::Reflect::get(&validated, &JsValue::from_str("validated")).unwrap();
        let revision =
            js_sys::Reflect::get(&validated_payload, &JsValue::from_str("revision")).unwrap();

        let inspect_command = command("inspect");
        set(&inspect_command, "source", &source);
        set(&inspect_command, "mode", &JsValue::from_str("summary"));
        assert_success(
            execute(inspect_command.into()).expect("inspect command"),
            "inspected",
        );

        let convert_command = command("convert");
        set(&convert_command, "source", &source);
        set(&convert_command, "to", &JsValue::from_str("json"));
        assert_success(
            execute(convert_command.into()).expect("convert command"),
            "converted",
        );

        let edit_command = command("edit");
        let operation = js_sys::Object::new();
        set(&operation, "kind", &JsValue::from_str("set_scalar"));
        set(&operation, "path", &JsValue::from_str("metadata.terms"));
        set(&operation, "value", &JsValue::from_str("Net 30"));
        set(&edit_command, "source", &source);
        set(&edit_command, "base_revision", &revision);
        set(&edit_command, "operation", &operation.into());
        assert_success(
            execute(edit_command.into()).expect("edit command"),
            "edited",
        );

        let prepare_command = command("prepare_render");
        set(&prepare_command, "source", &source);
        set(&prepare_command, "options", &options_value());
        assert_success(
            execute(prepare_command.into()).expect("prepare render command"),
            "prepared",
        );

        let render_command = command("render");
        set(&render_command, "source", &source);
        set(&render_command, "options", &options_value());
        assert_success(
            execute(render_command.into()).expect("render command"),
            "rendered",
        );

        let presentation_command = command("resolve_presentation");
        set(
            &presentation_command,
            "config",
            &js_sys::Object::new().into(),
        );
        assert_success(
            execute(presentation_command.into()).expect("presentation command"),
            "resolved_presentation",
        );

        let create_command = command("create");
        let draft = js_sys::JSON::parse(
            r#"{"title":"Created invoice","metadata":{"number":"INV-2026-001","issued":"2026-01-01","currency":"EUR"},"from":{"name":"Northstar Studio"},"bill_to":{"name":"Acme Research Ltd"}}"#,
        )
        .unwrap();
        set(&create_command, "draft", &draft);
        assert_success(
            execute(create_command.into()).expect("create command"),
            "created",
        );

        let registry_command = command("registry");
        assert_success(
            execute(registry_command.into()).expect("registry command"),
            "registry",
        );
    }

    #[wasm_bindgen_test]
    fn command_errors_use_the_typed_error_shape() {
        let unknown = command("not_a_command");
        assert_error_shape(execute(unknown.into()).unwrap_err(), "invalid_request");

        let malformed_source = command("validate");
        let source = js_sys::Object::new();
        set(&source, "markdown", &JsValue::from_str("# title"));
        set(&source, "json", &JsValue::from_str("{}"));
        set(&malformed_source, "source", &source.into());
        assert_error_shape(
            execute(malformed_source.into()).unwrap_err(),
            "invalid_request",
        );
    }

    #[wasm_bindgen_test]
    fn valid_assets_are_not_limited_by_request_budget() {
        let options = js_sys::Object::new();
        set(&options, "format", &JsValue::from_str("html"));
        let asset = js_sys::Object::new();
        set(&asset, "source", &JsValue::from_str("large.bin"));
        let bytes = js_sys::Uint8Array::new_with_length((MAX_DECODED_REQUEST_BYTES + 1) as u32);
        set(&asset, "bytes", &bytes.into());
        let assets = js_sys::Array::new();
        assets.push(&asset);
        set(&options, "assets", &assets.into());
        let snapshot = snapshot_options(
            &options.into(),
            "render options",
            &mut SnapshotBudget::default(),
        )
        .expect("asset bytes use the core asset budget");
        let copied_assets = js_sys::Reflect::get(&snapshot, &JsValue::from_str("assets")).unwrap();
        let copied_asset = js_sys::Reflect::get(&copied_assets, &JsValue::from_f64(0.0)).unwrap();
        let copied_bytes =
            js_sys::Reflect::get(&copied_asset, &JsValue::from_str("bytes")).unwrap();
        assert_eq!(
            copied_bytes
                .dyn_into::<js_sys::Uint8Array>()
                .unwrap()
                .length(),
            (MAX_DECODED_REQUEST_BYTES + 1) as u32
        );
    }

    #[wasm_bindgen_test]
    fn oversized_asset_strings_are_rejected_before_conversion() {
        let options = js_sys::Object::new();
        set(&options, "format", &JsValue::from_str("html"));
        let asset = js_sys::Object::new();
        set(&asset, "source", &JsValue::from_str("oversized.bin"));
        let value = "A".repeat(max_base64_length(MAX_ASSET_BYTES) + 1);
        set(&asset, "bytes", &JsValue::from_str(&value));
        let assets = js_sys::Array::new();
        assets.push(&asset);
        set(&options, "assets", &assets.into());
        let error = snapshot_options(
            &options.into(),
            "render options",
            &mut SnapshotBudget::default(),
        )
        .unwrap_err();
        assert_error_shape(error, "invalid_asset");
    }

    #[wasm_bindgen_test]
    fn asset_size_errors_are_stable_across_wire_forms() {
        let base64 = "A".repeat((MAX_ASSET_BYTES / 3 + 1) * 4);
        let values = [
            js_sys::Uint8Array::new_with_length((MAX_ASSET_BYTES + 1) as u32).into(),
            js_sys::Array::new_with_length((MAX_ASSET_BYTES + 1) as u32).into(),
            JsValue::from_str(&base64),
        ];
        for value in values {
            let options = js_sys::Object::new();
            set(&options, "format", &JsValue::from_str("html"));
            let asset = js_sys::Object::new();
            set(&asset, "source", &JsValue::from_str("asset.bin"));
            set(&asset, "bytes", &value);
            let assets = js_sys::Array::new();
            assets.push(&asset);
            set(&options, "assets", &assets.into());
            let error = snapshot_options(
                &options.into(),
                "render options",
                &mut SnapshotBudget::default(),
            )
            .unwrap_err();
            assert_error_shape(error.clone(), "invalid_asset");
            assert_eq!(diagnostic_message(&error), "asset exceeds 1 MiB");
        }

        let bytes = js_sys::Uint8Array::new_with_length(MAX_ASSET_BYTES as u32);
        let mut total = 0;
        for _ in 0..(MAX_ASSET_TOTAL_BYTES / MAX_ASSET_BYTES) {
            validate_asset_limits(&bytes.clone().into(), &mut total).unwrap();
        }
        assert_eq!(total, MAX_ASSET_TOTAL_BYTES);
        let error = validate_asset_limits(&bytes.into(), &mut total).unwrap_err();
        assert_error_shape(error.clone(), "invalid_asset");
        assert_eq!(
            diagnostic_message(&error),
            "aggregate asset byte budget exceeded"
        );
    }

    #[wasm_bindgen_test]
    fn uint8array_snapshot_charges_before_copying() {
        let bytes = js_sys::Uint8Array::new_with_length(MAX_DECODED_REQUEST_BYTES as u32);
        let mut budget = SnapshotBudget::default();
        let copied = scalar_or_tree(&bytes.clone().into(), "bytes", &mut budget, 0)
            .expect("bytes at the request limit");
        assert_eq!(
            copied.dyn_into::<js_sys::Uint8Array>().unwrap().length(),
            MAX_DECODED_REQUEST_BYTES as u32
        );
        let error = scalar_or_tree(&bytes.into(), "bytes", &mut budget, 0).unwrap_err();
        assert_error_shape(error, "limit");
    }
    #[wasm_bindgen_test]
    fn registry_crosses_the_single_executor() {
        let command = js_sys::Object::new();
        js_sys::Reflect::set(
            &command,
            &JsValue::from_str("kind"),
            &JsValue::from_str("registry"),
        )
        .unwrap();
        let result = execute(command.into()).expect("registry command");
        assert!(js_sys::Reflect::has(&result, &JsValue::from_str("registry")).unwrap());
    }
    #[wasm_bindgen_test]
    fn registry_and_presentation_are_json_compatible() {
        let registry_command = js_sys::Object::new();
        js_sys::Reflect::set(
            &registry_command,
            &JsValue::from_str("kind"),
            &JsValue::from_str("registry"),
        )
        .unwrap();
        let registry = execute(registry_command.into()).expect("registry command");
        let registry_json = js_sys::JSON::stringify(&registry)
            .unwrap()
            .as_string()
            .unwrap();
        assert!(registry_json.contains("\"capabilities\""));
        let registry_object = js_sys::JSON::parse(&registry_json).unwrap();
        let registry_snapshot =
            js_sys::Reflect::get(&registry_object, &JsValue::from_str("registry")).unwrap();
        let capabilities =
            js_sys::Reflect::get(&registry_snapshot, &JsValue::from_str("capabilities")).unwrap();
        assert!(capabilities.dyn_ref::<js_sys::Map>().is_none());
        let commands = js_sys::Reflect::get(&capabilities, &JsValue::from_str("commands")).unwrap();
        assert!(commands.dyn_ref::<js_sys::Array>().unwrap().length() > 0);
        let presentation =
            js_sys::Reflect::get(&capabilities, &JsValue::from_str("presentation")).unwrap();
        let font_scale =
            js_sys::Reflect::get(&presentation, &JsValue::from_str("font_scale")).unwrap();
        assert!(font_scale.dyn_ref::<js_sys::Map>().is_none());
        assert_eq!(
            js_sys::Reflect::get(&font_scale, &JsValue::from_str("minimum"))
                .unwrap()
                .as_f64(),
            Some(100.0)
        );

        let resolve_command = js_sys::Object::new();
        js_sys::Reflect::set(
            &resolve_command,
            &JsValue::from_str("kind"),
            &JsValue::from_str("resolve_presentation"),
        )
        .unwrap();
        js_sys::Reflect::set(
            &resolve_command,
            &JsValue::from_str("config"),
            &js_sys::Object::new(),
        )
        .unwrap();
        let resolved = execute(resolve_command.into()).expect("resolve presentation command");
        let resolved_json = js_sys::JSON::stringify(&resolved)
            .unwrap()
            .as_string()
            .unwrap();
        assert!(resolved_json.contains("\"geometry\""));
        let resolved_object = js_sys::JSON::parse(&resolved_json).unwrap();
        let resolved_presentation = js_sys::Reflect::get(
            &resolved_object,
            &JsValue::from_str("resolved_presentation"),
        )
        .unwrap();
        let geometry =
            js_sys::Reflect::get(&resolved_presentation, &JsValue::from_str("presentation"))
                .unwrap();
        let geometry = js_sys::Reflect::get(&geometry, &JsValue::from_str("geometry")).unwrap();
        assert!(geometry.dyn_ref::<js_sys::Map>().is_none());
        assert!(js_sys::Object::keys(geometry.unchecked_ref::<js_sys::Object>()).length() > 0);
    }

    #[wasm_bindgen_test]
    fn unknown_command_fields_fail_before_deserialization() {
        let command = js_sys::Object::new();
        js_sys::Reflect::set(
            &command,
            &JsValue::from_str("kind"),
            &JsValue::from_str("registry"),
        )
        .unwrap();
        js_sys::Reflect::set(
            &command,
            &JsValue::from_str("evil"),
            &JsValue::from_str("getter"),
        )
        .unwrap();
        let error = execute(command.into()).unwrap_err();
        assert_eq!(code(error.clone()), "invalid_request");
        assert_eq!(diagnostic_message(&error), "unknown command field: evil");
    }

    #[wasm_bindgen_test]
    fn invalid_option_uses_shared_invalid_command_message_prefix() {
        let command = js_sys::Object::new();
        let options = js_sys::Object::new();
        let source = js_sys::Object::new();
        js_sys::Reflect::set(
            &source,
            &JsValue::from_str("markdown"),
            &JsValue::from_str("# title"),
        )
        .unwrap();
        js_sys::Reflect::set(
            &options,
            &JsValue::from_str("format"),
            &JsValue::from_str("gif"),
        )
        .unwrap();
        js_sys::Reflect::set(
            &command,
            &JsValue::from_str("kind"),
            &JsValue::from_str("render"),
        )
        .unwrap();
        js_sys::Reflect::set(&command, &JsValue::from_str("source"), &source).unwrap();
        js_sys::Reflect::set(&command, &JsValue::from_str("options"), &options).unwrap();
        let error = execute(command.into()).unwrap_err();
        assert_eq!(code(error.clone()), "invalid_request");
        assert_eq!(
            diagnostic_message(&error),
            format!(
                "{}unknown variant `gif`, expected one of `html`, `pdf`, `png`",
                ttyinv_core::INVALID_COMMAND_MESSAGE_PREFIX
            )
        );
    }

    #[wasm_bindgen_test]
    fn source_requires_one_exact_format() {
        let source = js_sys::Object::new();
        js_sys::Reflect::set(
            &source,
            &JsValue::from_str("markdown"),
            &JsValue::from_str("# title"),
        )
        .unwrap();
        js_sys::Reflect::set(
            &source,
            &JsValue::from_str("json"),
            &JsValue::from_str("{}"),
        )
        .unwrap();
        let command = js_sys::Object::new();
        js_sys::Reflect::set(
            &command,
            &JsValue::from_str("kind"),
            &JsValue::from_str("validate"),
        )
        .unwrap();
        js_sys::Reflect::set(&command, &JsValue::from_str("source"), &source).unwrap();
        assert_eq!(
            code(execute(command.into()).unwrap_err()),
            "invalid_request"
        );
    }

    #[wasm_bindgen_test]
    fn assets_accept_wire_forms_but_reject_nonsense() {
        let options = js_sys::Object::new();
        let assets = js_sys::Array::new();
        let asset = js_sys::Object::new();
        js_sys::Reflect::set(
            &asset,
            &JsValue::from_str("source"),
            &JsValue::from_str("logo.png"),
        )
        .unwrap();
        js_sys::Reflect::set(&asset, &JsValue::from_str("bytes"), &JsValue::from_f64(7.0)).unwrap();
        assets.push(&asset);
        js_sys::Reflect::set(&options, &JsValue::from_str("assets"), &assets).unwrap();
        assert_eq!(
            code(
                snapshot_options(
                    &options.clone().into(),
                    "options",
                    &mut SnapshotBudget::default()
                )
                .unwrap_err()
            ),
            "invalid_asset"
        );
    }
    #[wasm_bindgen_test]
    fn render_accepts_uint8array_assets() {
        let source = r#"---
schema: ttyinv/v2
format: code-comma-dot
theme: printable
font: geist-mono
density: comfortable
---
# Synthetic signature

- Number: INV-WASM-SIG-001
- Kind: standard
- Issued: 2026-08-27
- Currency: EUR

## From

- Name: Fictional Studio

## Bill to

- Name: Fictional Client

## Services

| Description | Units | Rate | Amount (EUR) |
|---|---:|---:|---:|
| Synthetic work | 1 | 750.00 | auto |

## Signature

![Synthetic signature](./assets/signature.png)
- Name: Ada Example
- Label: Authorized representative
"#;
        let signature_png: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 8, 0, 0, 0, 8,
            8, 6, 0, 0, 0, 196, 15, 190, 139, 0, 0, 0, 50, 73, 68, 65, 84, 120, 218, 99, 184, 160,
            82, 240, 31, 132, 113, 1, 6, 16, 129, 79, 17, 3, 140, 129, 75, 17, 3, 50, 7, 155, 34,
            6, 116, 29, 232, 138, 80, 220, 128, 142, 9, 42, 0, 97, 134, 255, 120, 0, 72, 1, 0, 183,
            213, 216, 169, 177, 120, 110, 118, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        let command = js_sys::Object::new();
        let options = js_sys::Object::new();
        let source_value = js_sys::Object::new();
        let assets = js_sys::Array::new();
        let asset = js_sys::Object::new();
        let bytes = js_sys::Uint8Array::new_with_length(signature_png.len() as u32);
        bytes.copy_from(signature_png);
        js_sys::Reflect::set(
            &source_value,
            &JsValue::from_str("markdown"),
            &JsValue::from_str(source),
        )
        .unwrap();
        js_sys::Reflect::set(
            &asset,
            &JsValue::from_str("source"),
            &JsValue::from_str("./assets/signature.png"),
        )
        .unwrap();
        js_sys::Reflect::set(&asset, &JsValue::from_str("bytes"), &bytes).unwrap();
        js_sys::Reflect::set(
            &asset,
            &JsValue::from_str("mime"),
            &JsValue::from_str("image/png"),
        )
        .unwrap();
        assets.push(&asset);
        js_sys::Reflect::set(
            &options,
            &JsValue::from_str("format"),
            &JsValue::from_str("html"),
        )
        .unwrap();
        js_sys::Reflect::set(&options, &JsValue::from_str("assets"), &assets).unwrap();
        js_sys::Reflect::set(
            &command,
            &JsValue::from_str("kind"),
            &JsValue::from_str("render"),
        )
        .unwrap();
        js_sys::Reflect::set(&command, &JsValue::from_str("source"), &source_value).unwrap();
        js_sys::Reflect::set(&command, &JsValue::from_str("options"), &options).unwrap();
        let result = execute(command.into()).expect("Uint8Array asset render");
        let rendered =
            js_sys::Reflect::get(&result, &JsValue::from_str("rendered")).expect("rendered");
        let output =
            js_sys::Reflect::get(&rendered, &JsValue::from_str("bytes")).expect("rendered bytes");
        let output = output
            .dyn_into::<js_sys::Uint8Array>()
            .expect("Uint8Array output");
        assert!(output.length() > signature_png.len() as u32);
    }

    #[wasm_bindgen_test]
    fn oversized_source_matches_core_limit_diagnostic() {
        let command = js_sys::Object::new();
        let source = js_sys::Object::new();
        js_sys::Reflect::set(
            &source,
            &JsValue::from_str("markdown"),
            &JsValue::from_str(&"x".repeat(MAX_SOURCE_BYTES + 1)),
        )
        .unwrap();
        js_sys::Reflect::set(
            &command,
            &JsValue::from_str("kind"),
            &JsValue::from_str("validate"),
        )
        .unwrap();
        js_sys::Reflect::set(&command, &JsValue::from_str("source"), &source).unwrap();
        let error = execute(command.into()).unwrap_err();
        assert_eq!(code(error.clone()), "limit");
        assert_eq!(diagnostic_message(&error), SOURCE_SIZE_LIMIT_MESSAGE);
    }
}
