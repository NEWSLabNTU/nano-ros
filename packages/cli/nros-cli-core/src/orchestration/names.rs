//! ROS name normalization helpers used by the host planner.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedName {
    pub source: String,
    pub resolved: String,
    pub remapped_from: Option<String>,
}

pub fn normalize_namespace(namespace: Option<&str>) -> String {
    let raw = namespace.unwrap_or("/");
    if raw.is_empty() || raw == "/" {
        return "/".to_string();
    }
    normalize_absolute(raw)
}

pub fn node_fqn(namespace: Option<&str>, name: Option<&str>, fallback: &str) -> String {
    let ns = normalize_namespace(namespace);
    let node_name = name.unwrap_or(fallback).trim_matches('/');
    if node_name.is_empty() {
        ns
    } else if ns == "/" {
        format!("/{node_name}")
    } else {
        format!("{ns}/{node_name}")
    }
}

pub fn resolve_entity_name(
    namespace: &str,
    node_name: &str,
    source_name: &str,
    remaps: &[(String, String)],
) -> ResolvedName {
    let remapped = remaps
        .iter()
        .find(|(from, _)| from == source_name)
        .map(|(_, to)| to.as_str());
    let effective = remapped.unwrap_or(source_name);
    ResolvedName {
        source: source_name.to_string(),
        resolved: resolve_without_remap(namespace, node_name, effective),
        remapped_from: remapped.map(|_| source_name.to_string()),
    }
}

pub fn resolve_without_remap(namespace: &str, node_name: &str, name: &str) -> String {
    let namespace = normalize_namespace(Some(namespace));
    if let Some(rest) = name.strip_prefix("~/") {
        let node = node_name.trim_matches('/');
        return normalize_absolute(&join_ros(&join_ros(&namespace, node), rest));
    }
    if name == "~" {
        let node = node_name.trim_matches('/');
        return normalize_absolute(&join_ros(&namespace, node));
    }
    if name.starts_with('/') {
        normalize_absolute(name)
    } else {
        normalize_absolute(&join_ros(&namespace, name))
    }
}

fn join_ros(prefix: &str, suffix: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    let suffix = suffix.trim_start_matches('/');
    match (prefix.is_empty(), suffix.is_empty()) {
        (true, true) => "/".to_string(),
        (true, false) => format!("/{suffix}"),
        (false, true) => prefix.to_string(),
        (false, false) => format!("{prefix}/{suffix}"),
    }
}

fn normalize_absolute(name: &str) -> String {
    let mut out = String::from("/");
    out.push_str(
        &name
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("/"),
    );
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_private_names_under_node() {
        assert_eq!(
            resolve_without_remap("/robot", "driver", "~/cmd"),
            "/robot/driver/cmd"
        );
    }

    #[test]
    fn resolves_relative_names_under_namespace() {
        assert_eq!(
            resolve_without_remap("robot", "driver", "scan"),
            "/robot/scan"
        );
    }

    #[test]
    fn applies_exact_remap_before_resolution() {
        let remaps = vec![("~/cmd".to_string(), "/mux/cmd".to_string())];
        let resolved = resolve_entity_name("/robot", "driver", "~/cmd", &remaps);
        assert_eq!(resolved.resolved, "/mux/cmd");
        assert_eq!(resolved.remapped_from.as_deref(), Some("~/cmd"));
    }
}
