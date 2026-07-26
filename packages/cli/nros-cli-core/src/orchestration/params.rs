//! Parameter precedence helpers.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ParameterInputs<'a> {
    pub source_metadata: Option<&'a Value>,
    pub launch_params: &'a [(String, String)],
    pub param_files: &'a [String],
}

pub fn effective_parameters(inputs: ParameterInputs<'_>) -> Value {
    let mut out = Map::new();
    if let Some(metadata) = inputs.source_metadata {
        merge_object(&mut out, metadata.get("parameter_defaults"));
        merge_object(&mut out, metadata.pointer("/parameters/defaults"));
        merge_source_parameter_array(&mut out, metadata.get("parameters"));
        merge_object(&mut out, metadata.get("parameters"));
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

/// Issue 0276 — a ROS 2 launch `<param from="…param.yaml"/>` file, resolved to
/// the flat `(name, value)` pairs that apply to ONE node.
///
/// The file shape is ROS's own:
///
/// ```yaml
/// /**:                       # wildcard — applies to every node
///   ros__parameters:
///     use_sim_time: false
/// /ns/my_node:               # FQN, or a bare `my_node`
///   ros__parameters:
///     limits:
///       max_accel: 1.5       # flattens to `limits.max_accel`
/// ```
///
/// Precedence inside the file follows ROS: wildcard blocks first, node-specific
/// blocks after (so the specific value wins). Inline `<param name= value=>` from
/// the launch file outranks BOTH and is applied by the caller, last.
///
/// Values are stringified because `PlanNode.params` is stringly typed all the
/// way to the generated entry (`nros_cpp_declare_param`), matching the inline
/// launch-param path.
pub fn param_file_values(
    path: &Path,
    node_name: &str,
    namespace: &str,
) -> eyre::Result<Vec<(String, String)>> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| eyre::eyre!("read param file `{}`: {e}", path.display()))?;
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&raw)
        .map_err(|e| eyre::eyre!("parse param file `{}`: {e}", path.display()))?;
    let serde_yaml_ng::Value::Mapping(top) = doc else {
        // An empty file parses to Null — legal, contributes nothing.
        return Ok(Vec::new());
    };

    let fqn = node_fqn(namespace, node_name);
    let mut wildcard: Vec<(String, String)> = Vec::new();
    let mut specific: Vec<(String, String)> = Vec::new();

    for (key, block) in top {
        let Some(key) = key.as_str() else { continue };
        let key = key.trim();
        let is_wildcard = key == "/**" || key == "**";
        let matches = is_wildcard
            || key == fqn
            || key == node_name
            || key.trim_start_matches('/') == node_name
            || key
                .strip_prefix("/**/")
                .is_some_and(|suffix| suffix == node_name);
        if !matches {
            continue;
        }
        let Some(params) = block.get("ros__parameters") else {
            continue;
        };
        let out = if is_wildcard {
            &mut wildcard
        } else {
            &mut specific
        };
        flatten_yaml_params("", params, out);
    }

    wildcard.extend(specific);
    Ok(wildcard)
}

/// `/ns` + `name` → `/ns/name`; an empty or `/` namespace gives `/name`.
fn node_fqn(namespace: &str, node_name: &str) -> String {
    let ns = namespace.trim().trim_end_matches('/');
    if ns.is_empty() {
        format!("/{node_name}")
    } else if ns.starts_with('/') {
        format!("{ns}/{node_name}")
    } else {
        format!("/{ns}/{node_name}")
    }
}

/// Flatten a `ros__parameters` subtree into dotted keys, ROS-style
/// (`limits: {max_accel: 1.5}` → `limits.max_accel`).
fn flatten_yaml_params(
    prefix: &str,
    value: &serde_yaml_ng::Value,
    out: &mut Vec<(String, String)>,
) {
    match value {
        serde_yaml_ng::Value::Mapping(map) => {
            for (k, v) in map {
                let Some(k) = k.as_str() else { continue };
                let key = if prefix.is_empty() {
                    k.to_string()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_yaml_params(&key, v, out);
            }
        }
        other if !prefix.is_empty() => out.push((prefix.to_string(), yaml_scalar_to_string(other))),
        _ => {}
    }
}

fn yaml_scalar_to_string(value: &serde_yaml_ng::Value) -> String {
    match value {
        serde_yaml_ng::Value::Null => String::new(),
        serde_yaml_ng::Value::Bool(b) => b.to_string(),
        serde_yaml_ng::Value::Number(n) => n.to_string(),
        serde_yaml_ng::Value::String(s) => s.clone(),
        serde_yaml_ng::Value::Sequence(items) => {
            let inner: Vec<String> = items.iter().map(yaml_scalar_to_string).collect();
            format!("[{}]", inner.join(", "))
        }
        // Mappings are handled by `flatten_yaml_params`; a tagged value keeps
        // its inner scalar.
        serde_yaml_ng::Value::Tagged(t) => yaml_scalar_to_string(&t.value),
        serde_yaml_ng::Value::Mapping(_) => String::new(),
    }
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
            launch_params: &launch,
            param_files: &[],
        });
        assert_eq!(value["rate"], 20);
        assert_eq!(value["frame"], "map");
    }

    /// Issue 0276 — key matching: `/**` wildcard, the FQN `/ns/node`, and a bare
    /// `node` key all apply; a block for a different node does not. Sequences
    /// stringify to `[a, b]`; an empty file is legal and contributes nothing.
    #[test]
    fn param_file_values_matches_wildcard_fqn_and_bare_keys() {
        let dir = tempfile::tempdir().unwrap();

        let f = dir.path().join("p.yaml");
        std::fs::write(
            &f,
            "/**:\n  ros__parameters:\n    a: 1\n\
             /ns/node:\n  ros__parameters:\n    b: [1, 2, 3]\n\
             elsewhere:\n  ros__parameters:\n    c: nope\n",
        )
        .unwrap();
        let got = param_file_values(&f, "node", "/ns").unwrap();
        assert_eq!(
            got,
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "[1, 2, 3]".to_string()),
            ]
        );

        // Bare key form, and a node at the root namespace.
        let g = dir.path().join("bare.yaml");
        std::fs::write(&g, "node:\n  ros__parameters:\n    d: true\n").unwrap();
        assert_eq!(
            param_file_values(&g, "node", "/").unwrap(),
            vec![("d".to_string(), "true".to_string())]
        );
        // Same file, different node — nothing applies.
        assert!(param_file_values(&g, "other", "/").unwrap().is_empty());

        // Empty file parses to Null: legal, no params.
        let empty = dir.path().join("empty.yaml");
        std::fs::write(&empty, "").unwrap();
        assert!(param_file_values(&empty, "node", "/").unwrap().is_empty());
    }

    /// Issue 0276 — inside one file the node-specific block outranks the
    /// wildcard, regardless of the order they appear in.
    #[test]
    fn param_file_values_node_block_outranks_wildcard() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("p.yaml");
        // node block written FIRST, wildcard second — specificity must still win.
        std::fs::write(
            &f,
            "node:\n  ros__parameters:\n    rate: 25\n\
             /**:\n  ros__parameters:\n    rate: 10\n    shared: yes\n",
        )
        .unwrap();
        let got: std::collections::BTreeMap<_, _> = param_file_values(&f, "node", "/")
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(got.get("rate").map(String::as_str), Some("25"));
        assert_eq!(got.get("shared").map(String::as_str), Some("yes"));
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
