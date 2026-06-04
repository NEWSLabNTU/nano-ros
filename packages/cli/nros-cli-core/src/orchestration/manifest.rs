//! ROS launch manifest adapter.

use eyre::{Context, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ManifestArtifact {
    pub path: PathBuf,
    pub value: Value,
}

pub fn load_manifest(path: &Path) -> Result<ManifestArtifact> {
    let manifest = ros_launch_manifest_types::parse_manifest(path)
        .wrap_err_with(|| format!("failed to parse ROS launch manifest {}", path.display()))?;
    let value = serde_json::to_value(manifest)?;
    Ok(ManifestArtifact {
        path: path.to_path_buf(),
        value,
    })
}

pub fn endpoint_requirements(manifests: &[ManifestArtifact]) -> Vec<Value> {
    let mut out = Vec::new();
    for artifact in manifests {
        collect_topics(&artifact.path, &artifact.value, &mut out);
        collect_services(&artifact.path, &artifact.value, &mut out);
        collect_actions(&artifact.path, &artifact.value, &mut out);
    }
    out
}

fn collect_topics(path: &Path, value: &Value, out: &mut Vec<Value>) {
    let Some(topics) = value.get("topics").and_then(Value::as_object) else {
        return;
    };
    for (name, decl) in topics {
        let msg_type = decl.get("type").and_then(Value::as_str);
        push_role_list(path, out, "publisher", name, msg_type, decl.get("pub"));
        push_role_list(path, out, "subscriber", name, msg_type, decl.get("sub"));
    }
}

fn collect_services(path: &Path, value: &Value, out: &mut Vec<Value>) {
    let Some(services) = value.get("services").and_then(Value::as_object) else {
        return;
    };
    for (name, decl) in services {
        let srv_type = decl.get("type").and_then(Value::as_str);
        push_role_list(
            path,
            out,
            "service_server",
            name,
            srv_type,
            decl.get("server"),
        );
        push_role_list(
            path,
            out,
            "service_client",
            name,
            srv_type,
            decl.get("client"),
        );
    }
}

fn collect_actions(path: &Path, value: &Value, out: &mut Vec<Value>) {
    let Some(actions) = value.get("actions").and_then(Value::as_object) else {
        return;
    };
    for (name, decl) in actions {
        let action_type = decl.get("type").and_then(Value::as_str);
        push_role_list(
            path,
            out,
            "action_server",
            name,
            action_type,
            decl.get("server"),
        );
        push_role_list(
            path,
            out,
            "action_client",
            name,
            action_type,
            decl.get("client"),
        );
    }
}

fn push_role_list(
    path: &Path,
    out: &mut Vec<Value>,
    role: &str,
    name: &str,
    interface_type: Option<&str>,
    nodes: Option<&Value>,
) {
    let Some(nodes) = nodes.and_then(Value::as_array) else {
        return;
    };
    for node in nodes {
        out.push(json!({
            "source_artifact": path,
            "role": role,
            "name": name,
            "type": interface_type,
            "node": node,
        }));
    }
}
