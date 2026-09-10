//! phase-446 W6 -- the parameters each node's contract DECLARES, in the one
//! shape both code generators render from.
//!
//! A C++ image gets them as a generated header
//! (`nros ws entity-inventory --output-params-header`, which
//! `Node::declare_parameter` consults); a Rust image gets them from
//! the `nros::main!` expansion, which hands them to the executor before any
//! node registers. Both read THIS function, so the two languages cannot come
//! to disagree about which nodes are checked, what a name is declared as, or
//! which contract a refusal names.
//!
//! Presence in `contracts.node_params` is the declaration: a node with no
//! entry declared nothing, and its code is not checked -- today's behaviour.

use ros_launch_manifest_model::{ParamType, SystemModel};

/// One node whose contract has a `params:` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredNodeParams {
    /// The node FQN, the key `contracts.node_params` uses.
    pub fqn: String,
    /// What a refusal names as the contract (see [`contract_label`]).
    pub contract: String,
    /// `(name, type)`, sorted by name.
    pub params: Vec<(String, ParamType)>,
}

/// The `rcl_interfaces/msg/ParameterType` code for a contract type.
///
/// The ROS numbering, not a private one: it is what `nros_params::ParameterType`
/// is `repr`'d as and what a generated C++ table carries, so a code crossing
/// either boundary means the same thing on both sides by construction.
pub fn ros_type_code(t: ParamType) -> u8 {
    match t {
        ParamType::Bool => 1,
        ParamType::Integer => 2,
        ParamType::Double => 3,
        ParamType::String => 4,
        ParamType::ByteArray => 5,
        ParamType::BoolArray => 6,
        ParamType::IntegerArray => 7,
        ParamType::DoubleArray => 8,
        ParamType::StringArray => 9,
    }
}

/// The contract a node's parameters come from, as a refusal names it.
///
/// The model records the node's launch scope, and the scope records the
/// contract it was loaded from (`manifest`) when it had one -- the file a user
/// edits. Only the file name is kept: an absolute host path would be baked
/// into firmware. Without a manifest, the sidecar beside the launch file is
/// named by the convention RFC-0060 fixes (`<stem>.contract.yaml`).
pub fn contract_label(model: &SystemModel, fqn: &str) -> String {
    let scope = model.structure.nodes.get(fqn).map(|n| n.scope.as_str());
    let info = scope.and_then(|s| model.structure.scopes.get(s));
    if let Some(m) = info.and_then(|i| i.manifest.as_deref()) {
        return m.rsplit('/').next().unwrap_or(m).to_string();
    }
    if let Some(f) = info.and_then(|i| i.file.as_deref()) {
        let base = f.rsplit('/').next().unwrap_or(f);
        let stem = base.split(".launch").next().unwrap_or(base);
        return format!("{stem}.contract.yaml");
    }
    match scope {
        Some(s) if !s.is_empty() => format!("the contract sidecar of launch scope `{s}`"),
        _ => "the contract sidecar beside its launch file".to_string(),
    }
}

/// Every node the model declares parameters for, sorted by FQN, restricted to
/// the nodes `keep` accepts (an entry renders only the nodes it deploys).
pub fn declared_params(
    model: &SystemModel,
    keep: impl Fn(&str) -> bool,
) -> Vec<DeclaredNodeParams> {
    model
        .contracts
        .node_params
        .iter()
        .filter(|(fqn, _)| keep(fqn))
        .map(|(fqn, ps)| DeclaredNodeParams {
            fqn: fqn.clone(),
            contract: contract_label(model, fqn),
            params: ps.iter().map(|(n, c)| (n.clone(), c.ty)).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(y: &str) -> SystemModel {
        SystemModel::from_yaml_str(y).expect("fixture parses")
    }

    const Y: &str = r#"
meta: { version: 1 }
structure:
  scopes:
    island_bringup/island.launch.xml:
      file: island.launch.xml
      manifest: src/island_bringup/launch/island.contract.yaml
    bare.launch.xml:
      file: bare.launch.xml
  nodes:
    /system/mrm_handler:
      { scope: island_bringup/island.launch.xml, pkg: p, exec: mrm_handler, node_name: mrm_handler }
    /talker:
      { scope: bare.launch.xml, pkg: p, exec: talker, node_name: talker }
    /listener:
      { scope: bare.launch.xml, pkg: p, exec: listener, node_name: listener }
contracts:
  node_params:
    /system/mrm_handler:
      update_rate: { type: integer }
      use_emergency_holding: { type: bool }
    /talker:
      greeting: { type: string }
"#;

    /// A declaring node carries its names and types; a node with no
    /// `params:` is absent -- not checked, rather than "declares nothing".
    #[test]
    fn only_declaring_nodes_appear_with_their_types() {
        let m = model(Y);
        let d = declared_params(&m, |_| true);
        let fqns: Vec<&str> = d.iter().map(|n| n.fqn.as_str()).collect();
        assert_eq!(fqns, ["/system/mrm_handler", "/talker"]);
        assert_eq!(
            d[0].params,
            vec![
                ("update_rate".to_string(), ParamType::Integer),
                ("use_emergency_holding".to_string(), ParamType::Bool),
            ]
        );
        let only_talker = declared_params(&m, |f| f == "/talker");
        assert_eq!(only_talker.len(), 1);
    }

    /// The refusal names the file a user edits, without the host path.
    #[test]
    fn the_contract_is_named_by_file_and_never_by_host_path() {
        let m = model(Y);
        assert_eq!(
            contract_label(&m, "/system/mrm_handler"),
            "island.contract.yaml"
        );
        assert_eq!(contract_label(&m, "/talker"), "bare.contract.yaml");
    }

    /// The codes are ROS's `rcl_interfaces/msg/ParameterType`, one per type.
    #[test]
    fn type_codes_are_the_ros_numbering() {
        let all = [
            ParamType::Bool,
            ParamType::Integer,
            ParamType::Double,
            ParamType::String,
            ParamType::ByteArray,
            ParamType::BoolArray,
            ParamType::IntegerArray,
            ParamType::DoubleArray,
            ParamType::StringArray,
        ];
        let codes: Vec<u8> = all.iter().map(|t| ros_type_code(*t)).collect();
        assert_eq!(codes, (1..=9).collect::<Vec<u8>>());
    }
}
