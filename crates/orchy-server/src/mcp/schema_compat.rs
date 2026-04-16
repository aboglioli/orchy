use std::sync::Arc;

use rmcp::model::JsonObject;
use serde_json::Value;

pub(crate) fn compat_tool_input_schema(schema: Arc<JsonObject>) -> Arc<JsonObject> {
    let mut v = Value::Object((*schema).clone());
    normalize_schema(&mut v);
    if let Value::Object(mut map) = v {
        map.remove("$schema");
        Arc::new(map)
    } else {
        schema
    }
}

fn normalize_schema(val: &mut Value) {
    match val {
        Value::Object(map) => {
            flatten_nullable_type_array(map);
            for child in map.values_mut() {
                normalize_schema(child);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                normalize_schema(item);
            }
        }
        _ => {}
    }
}

fn flatten_nullable_type_array(map: &mut serde_json::Map<String, Value>) {
    let Some(Value::Array(types)) = map.get("type") else {
        return;
    };
    let has_null = types.iter().any(|t| t.as_str() == Some("null"));
    let non_null: Vec<Value> = types
        .iter()
        .filter(|t| t.as_str() != Some("null"))
        .cloned()
        .collect();
    if has_null && non_null.len() == 1 {
        map.insert("type".to_string(), non_null[0].clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn removes_schema_and_flattens_optional_string() {
        let schema: JsonObject = serde_json::from_value(json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "alias": { "type": ["string", "null"] },
                "project": { "type": "string" }
            },
            "required": ["project"]
        }))
        .unwrap();
        let out = compat_tool_input_schema(Arc::new(schema));
        let v = serde_json::Value::Object(out.as_ref().clone());
        assert!(v.get("$schema").is_none());
        assert_eq!(v["properties"]["alias"]["type"], json!("string"));
    }
}
