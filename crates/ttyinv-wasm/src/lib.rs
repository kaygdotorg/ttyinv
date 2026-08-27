use serde::Serialize;
use ttyinv_core::{
    execute as execute_core, invalid_command_message, CommandError, CommandErrorCode,
    CommandOutcome, Diagnostic, InvoiceCommand, RetryClass, SOURCE_SIZE_LIMIT_MESSAGE,
};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};

const MAX_DECODED_REQUEST_BYTES: usize = 256 * 1024;
const MAX_WASM_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
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
        let length = bytes.length();
        if length as usize > MAX_DECODED_REQUEST_BYTES {
            return Err(preflight_error(
                CommandErrorCode::Limit,
                format!("{label} exceeds adapter size limit"),
            ));
        }
        budget.charge(length as usize)?;
        let copy = js_sys::Uint8Array::new_with_length(length);
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
            "png_scale",
            "assets",
        ],
        budget,
    )?;
    let mut entries = Vec::with_capacity(fields.len());
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
                            if asset_value.dyn_ref::<js_sys::Uint8Array>().is_none() {
                                return Err(preflight_error(
                                    CommandErrorCode::InvalidAsset,
                                    "asset.bytes must be Uint8Array",
                                ));
                            }
                            scalar_or_tree(&asset_value, "asset.bytes", budget, 0)?
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
        if bytes.len() > MAX_WASM_OUTPUT_BYTES {
            return Err(preflight_error(
                CommandErrorCode::Limit,
                format!("rendered output exceeds adapter limit ({MAX_WASM_OUTPUT_BYTES} bytes)"),
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

    fn diagnostic_message(error: &JsValue) -> String {
        let diagnostics = js_sys::Reflect::get(error, &JsValue::from_str("diagnostics")).unwrap();
        let first = js_sys::Reflect::get(&diagnostics, &JsValue::from_f64(0.0)).unwrap();
        js_sys::Reflect::get(&first, &JsValue::from_str("message"))
            .unwrap()
            .as_string()
            .unwrap()
    }
    #[test]
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
    #[test]
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

    #[test]
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

    #[test]
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
                "{INVALID_COMMAND_MESSAGE_PREFIX}unknown variant `gif`, expected one of `html`, `pdf`, `png`"
            )
        );
    }

    #[test]
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

    #[test]
    fn assets_must_be_uint8_arrays_and_are_copied() {
        let options = js_sys::Object::new();
        let assets = js_sys::Array::new();
        let asset = js_sys::Object::new();
        js_sys::Reflect::set(
            &asset,
            &JsValue::from_str("source"),
            &JsValue::from_str("logo.png"),
        )
        .unwrap();
        js_sys::Reflect::set(&asset, &JsValue::from_str("bytes"), &js_sys::Array::new()).unwrap();
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

    #[test]
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
