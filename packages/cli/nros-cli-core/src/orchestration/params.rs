//! Parameter precedence helpers.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

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

/// Phase 256 Wave 0 — a parsed `nros.toml` overlay paired with the file it came
/// from. The provenance primitive the config-SSoT endgame needs: it lets the
/// per-block deprecation warnings (Waves 1-5) NAME the offending file, and powers
/// `nros config show` provenance + `nros check`'s legacy-overlay flag (Waves 6-7).
#[derive(Debug, Clone)]
pub struct SourcedToml {
    pub path: PathBuf,
    pub value: Value,
}

/// Parse each `nros.toml` path into a [`SourcedToml`], preserving file
/// attribution. Same parse as [`load_toml_values`] — that fn is now a thin
/// projection of this (drops the path).
pub fn load_sourced_toml_values(paths: &[PathBuf]) -> eyre::Result<Vec<SourcedToml>> {
    paths
        .iter()
        .map(|path| {
            let raw = std::fs::read_to_string(path)?;
            let value: toml::Value = toml::from_str(&raw)?;
            Ok(SourcedToml {
                path: path.clone(),
                value: serde_json::to_value(value)?,
            })
        })
        .collect()
}

pub fn load_toml_values(paths: &[PathBuf]) -> eyre::Result<Vec<Value>> {
    Ok(load_sourced_toml_values(paths)?
        .into_iter()
        .map(|s| s.value)
        .collect())
}

/// The file that **last** declared top-level `block` across the sourced overlays,
/// or `None` if no overlay carries it. Last-wins matches the overlay merge
/// semantics (`schema_build_json` / `collect_*` all let later overlays override
/// earlier), so this is the file whose value actually reached the plan — the one
/// a deprecation warning or provenance column should name.
pub fn last_block_source<'a>(sourced: &'a [SourcedToml], block: &str) -> Option<&'a Path> {
    sourced
        .iter()
        .rev()
        .find(|s| s.value.get(block).is_some())
        .map(|s| s.path.as_path())
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

    /// Phase 256 Wave 0 — the sourced loader keeps file attribution, and
    /// `last_block_source` returns the LAST overlay declaring a block (last-wins,
    /// matching the merge), or `None` when no overlay carries it.
    #[test]
    fn sourced_toml_tracks_provenance_per_block() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.toml");
        let b = dir.path().join("b.toml");
        std::fs::write(
            &a,
            "[lifecycle]\nautostart=\"all\"\n[build]\nrmw=\"zenoh\"\n",
        )
        .unwrap();
        std::fs::write(&b, "[build]\nrmw=\"cyclonedds\"\n").unwrap();

        let sourced = load_sourced_toml_values(&[a.clone(), b.clone()]).unwrap();
        assert_eq!(sourced.len(), 2);
        assert_eq!(sourced[0].path, a);
        assert_eq!(sourced[0].value["lifecycle"]["autostart"], "all");

        // `[build]` last-declared in b; `[lifecycle]` only in a.
        assert_eq!(last_block_source(&sourced, "build"), Some(b.as_path()));
        assert_eq!(last_block_source(&sourced, "lifecycle"), Some(a.as_path()));
        assert_eq!(last_block_source(&sourced, "nonexistent"), None);

        // `load_toml_values` is the path-dropping projection of the same parse.
        let plain = load_toml_values(&[a, b]).unwrap();
        assert_eq!(
            plain,
            sourced.into_iter().map(|s| s.value).collect::<Vec<_>>()
        );
    }
}
