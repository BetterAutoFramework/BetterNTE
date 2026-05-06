//! JsValue <-> serde_json::Value conversion utilities.

use anyhow::Result;
use rquickjs::{Array, Ctx, Object, Value};
use serde_json::Value as JsonValue;
use tracing::warn;

/// Convert serde_json::Value to JS Value.
pub fn json_to_js<'js>(ctx: &Ctx<'js>, value: &JsonValue) -> Result<Value<'js>> {
    match value {
        JsonValue::Null => Ok(Value::new_null(ctx.clone())),
        JsonValue::Bool(b) => Ok(Value::new_bool(ctx.clone(), *b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::new_int(ctx.clone(), i as i32))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::new_float(ctx.clone(), f))
            } else {
                Ok(Value::new_float(ctx.clone(), 0.0))
            }
        }
        JsonValue::String(s) => {
            let js_str = rquickjs::String::from_str(ctx.clone(), s)?;
            Ok(js_str.into_value())
        }
        JsonValue::Array(arr) => {
            let js_arr = Array::new(ctx.clone())?;
            for (i, item) in arr.iter().enumerate() {
                js_arr.set(i, json_to_js(ctx, item)?)?;
            }
            Ok(js_arr.into_value())
        }
        JsonValue::Object(map) => {
            let js_obj = Object::new(ctx.clone())?;
            for (key, val) in map.iter() {
                js_obj.set(key.as_str(), json_to_js(ctx, val)?)?;
            }
            Ok(js_obj.into_value())
        }
    }
}

/// Convert JS Value to serde_json::Value.
pub fn js_to_json<'js>(value: &Value<'js>) -> Result<JsonValue> {
    match value.type_of() {
        rquickjs::Type::Null | rquickjs::Type::Undefined => Ok(JsonValue::Null),
        rquickjs::Type::Bool => Ok(JsonValue::Bool(value.as_bool().unwrap_or(false))),
        rquickjs::Type::Int => Ok(JsonValue::Number(value.as_int().unwrap_or(0).into())),
        rquickjs::Type::Float => {
            let f = value.as_float().unwrap_or(0.0);
            if let Some(n) = serde_json::Number::from_f64(f) {
                Ok(JsonValue::Number(n))
            } else {
                Ok(JsonValue::Null)
            }
        }
        rquickjs::Type::String => {
            let s = value
                .as_string()
                .and_then(|s| s.to_string().ok())
                .unwrap_or_default();
            Ok(JsonValue::String(s))
        }
        rquickjs::Type::Array => {
            let arr = value.as_array().unwrap();
            let mut result = Vec::new();
            for i in 0..arr.len() {
                if let Ok(val) = arr.get::<Value>(i) {
                    result.push(js_to_json(&val)?);
                }
            }
            Ok(JsonValue::Array(result))
        }
        rquickjs::Type::Object => {
            let obj = value.as_object().unwrap();
            // Date objects stringify via toISOString for deterministic output.
            if let Ok(to_iso) = obj.get::<_, rquickjs::Function>("toISOString") {
                if let Ok(iso) = to_iso.call::<_, String>(()) {
                    return Ok(JsonValue::String(iso));
                }
            }
            let mut map = serde_json::Map::new();
            let mut entries: Vec<(String, JsonValue)> = Vec::new();
            for (key, val) in obj.props().flatten() {
                entries.push((key, js_to_json(&val)?));
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, val) in entries {
                map.insert(key, val);
            }
            Ok(JsonValue::Object(map))
        }
        rquickjs::Type::BigInt => {
            warn!("js_to_json: BigInt is converted to string");
            Ok(JsonValue::String(
                value
                    .as_string()
                    .and_then(|s| s.to_string().ok())
                    .unwrap_or_default(),
            ))
        }
        rquickjs::Type::Symbol | rquickjs::Type::Function => {
            warn!("js_to_json: non-serializable JS type converted to null");
            Ok(JsonValue::Null)
        }
        _ => {
            warn!("js_to_json: unsupported JS type converted to null");
            Ok(JsonValue::Null)
        }
    }
}
