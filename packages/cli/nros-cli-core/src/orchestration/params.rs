//! Parameter precedence helpers.

use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ParameterInputs<'a> {
    pub source_metadata: Option<&'a Value>,
    pub package_nros: Option<&'a Value>,
    pub launch_params: &'a [(String, String)],
    pub param_files: &'a [String],
    pub overlays: &'a [Value],
}

pub fn effective_parameters(inputs: ParameterInputs<'_>) -> Value {
    let mut out = Map::new();
    if let Some(metadata) = inputs.source_metadata {
        merge_object(&mut out, metadata.get("parameter_defaults"));
        merge_object(&mut out, metadata.pointer("/parameters/defaults"));
        merge_source_parameter_array(&mut out, metadata.get("parameters"));
        merge_object(&mut out, metadata.get("parameters"));
    }
    if let Some(package_nros) = inputs.package_nros {
        merge_object(&mut out, package_nros.get("parameters"));
    }
    if !inputs.param_files.is_empty() {
        out.insert(
            "parameter_files".to_string(),
            Value::Array(
                inputs
                    .param_files
                    .iter()
                    .map(|path| Value::String(path.clone()))
                    .collect(),
            ),
        );
    }
    for (key, value) in inputs.launch_params {
        out.insert(key.clone(), parse_scalar(value));
    }
    for overlay in inputs.overlays {
        merge_object(&mut out, overlay.get("parameters"));
        merge_object(&mut out, overlay.pointer("/overlays/parameters"));
    }
    Value::Object(out)
}

pub fn load_toml_values(paths: &[PathBuf]) -> eyre::Result<Vec<Value>> {
    paths
        .iter()
        .map(|path| {
            let raw = std::fs::read_to_string(path)?;
            let value: toml::Value = toml::from_str(&raw)?;
            Ok(serde_json::to_value(value)?)
        })
        .collect()
}

fn merge_source_parameter_array(out: &mut Map<String, Value>, value: Option<&Value>) {
    let Some(Value::Array(parameters)) = value else {
        return;
    };
    for parameter in parameters {
        let Some(name) = parameter.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(default) = parameter.get("default") else {
            continue;
        };
        out.insert(name.to_string(), default.clone());
    }
}

fn merge_object(out: &mut Map<String, Value>, value: Option<&Value>) {
    let Some(Value::Object(map)) = value else {
        return;
    };
    for (key, value) in map {
        out.insert(key.clone(), value.clone());
    }
}

fn parse_scalar(value: &str) -> Value {
    if let Ok(parsed) = value.parse::<bool>() {
        return Value::Bool(parsed);
    }
    if let Ok(parsed) = value.parse::<i64>() {
        return Value::Number(parsed.into());
    }
    if let Ok(parsed) = value.parse::<f64>()
        && let Some(number) = serde_json::Number::from_f64(parsed)
    {
        return Value::Number(number);
    }
    Value::String(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn launch_params_override_source_defaults() {
        let source = json!({"parameter_defaults": {"rate": 10, "frame": "map"}});
        let launch = vec![("rate".to_string(), "20".to_string())];
        let value = effective_parameters(ParameterInputs {
            source_metadata: Some(&source),
            package_nros: None,
            launch_params: &launch,
            param_files: &[],
            overlays: &[],
        });
        assert_eq!(value["rate"], 20);
        assert_eq!(value["frame"], "map");
    }
}
