//! phase-446 W6 -- the parameters a node's contract DECLARES, checked when the
//! code declares one.
//!
//! The launch contract states each node's parameters by name and type, and the
//! build sizes the parameter store from that statement (phase-446 W4). A store
//! sized from a declaration is only right while the code declares what the
//! contract says, so a declaration the contract does not make -- an unknown
//! name, or a known name with another type -- refuses the boot instead of
//! taking a slot nobody counted.
//!
//! The table is `'static` data an entry bakes from the SystemModel
//! (`nros_orchestration_ir::declared_params`); this module only looks names up
//! in it. A node with no row has no `params:` in its contract and is NOT
//! checked, which is the behaviour before the contract could say anything.
//!
//! The C++ `ComponentNode` does the same lookup in `nros/declared_params.hpp`,
//! over a header rendered from the same model by the same function.

use crate::ParameterType;

/// Names no contract has to declare: `use_sim_time` and
/// `start_type_description_service`, which rclcpp declares on every node, and
/// `qos_overrides.*`, which is derived from the contract itself. These are the
/// names play_launch exempts when it holds a launch file to the same contract,
/// so the launch check and this one agree about what a node may carry.
pub fn is_exempt(name: &str) -> bool {
    name == "use_sim_time"
        || name == "start_type_description_service"
        || name.starts_with("qos_overrides.")
}

/// The declared parameters of every node in an image whose contract has
/// `params:`.
///
/// `nodes` is `(node FQN, contract)`, where the contract is what a refusal
/// names; `params` is `(node FQN, parameter name, rcl_interfaces type code)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeclaredParams {
    nodes: &'static [(&'static str, &'static str)],
    params: &'static [(&'static str, &'static str, u8)],
}

/// A declaration the contract does not make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclaredParamMismatch {
    /// The node's FQN, as the contract keys it.
    pub node: &'static str,
    /// The contract the node's parameters are declared in.
    pub contract: &'static str,
    /// What the contract declares the name as; `None` when it does not
    /// declare the name at all.
    pub declared: Option<ParameterType>,
}

impl DeclaredParams {
    /// No table: nothing is checked.
    pub const EMPTY: Self = Self {
        nodes: &[],
        params: &[],
    };

    pub const fn new(
        nodes: &'static [(&'static str, &'static str)],
        params: &'static [(&'static str, &'static str, u8)],
    ) -> Self {
        Self { nodes, params }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Check one declaration by the node `namespace` + `node_name`.
    ///
    /// `Ok` when the node's contract has no `params:`, when the name is
    /// [exempt](is_exempt), or when the contract declares the name with this
    /// type.
    pub fn check(
        &self,
        namespace: &str,
        node_name: &str,
        param: &str,
        ty: ParameterType,
    ) -> Result<(), DeclaredParamMismatch> {
        let Some(&(node, contract)) = self
            .nodes
            .iter()
            .find(|(fqn, _)| fqn_matches(fqn, namespace, node_name))
        else {
            return Ok(());
        };
        if is_exempt(param) {
            return Ok(());
        }
        let declared = self
            .params
            .iter()
            .find(|(fqn, name, _)| *fqn == node && *name == param)
            .map(|&(_, _, code)| ros_type(code));
        if declared == Some(ty) {
            return Ok(());
        }
        Err(DeclaredParamMismatch {
            node,
            contract,
            declared,
        })
    }
}

/// `fqn == namespace + "/" + name`, where a namespace of `/` or `` is the root.
fn fqn_matches(fqn: &str, namespace: &str, name: &str) -> bool {
    let ns = namespace.trim_end_matches('/');
    fqn.strip_prefix(ns).and_then(|rest| rest.strip_prefix('/')) == Some(name)
}

/// The [`ParameterType`] for an `rcl_interfaces/msg/ParameterType` code.
pub fn ros_type(code: u8) -> ParameterType {
    match code {
        1 => ParameterType::Bool,
        2 => ParameterType::Integer,
        3 => ParameterType::Double,
        4 => ParameterType::String,
        5 => ParameterType::ByteArray,
        6 => ParameterType::BoolArray,
        7 => ParameterType::IntegerArray,
        8 => ParameterType::DoubleArray,
        9 => ParameterType::StringArray,
        _ => ParameterType::NotSet,
    }
}

/// The contract's spelling of a type, for a diagnostic.
pub fn type_name(ty: ParameterType) -> &'static str {
    match ty {
        ParameterType::NotSet => "not_set",
        ParameterType::Bool => "bool",
        ParameterType::Integer => "integer",
        ParameterType::Double => "double",
        ParameterType::String => "string",
        ParameterType::ByteArray => "byte_array",
        ParameterType::BoolArray => "bool_array",
        ParameterType::IntegerArray => "integer_array",
        ParameterType::DoubleArray => "double_array",
        ParameterType::StringArray => "string_array",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static NODES: &[(&str, &str)] = &[("/system/mrm_handler", "island.contract.yaml")];
    static PARAMS: &[(&str, &str, u8)] = &[
        ("/system/mrm_handler", "update_rate", 2),
        ("/system/mrm_handler", "use_emergency_holding", 1),
    ];
    const TABLE: DeclaredParams = DeclaredParams::new(NODES, PARAMS);

    #[test]
    fn a_declaration_that_matches_the_contract_is_accepted() {
        assert_eq!(
            TABLE.check(
                "/system",
                "mrm_handler",
                "update_rate",
                ParameterType::Integer
            ),
            Ok(())
        );
        // A trailing `/` on the namespace is the same namespace.
        assert_eq!(
            TABLE.check(
                "/system/",
                "mrm_handler",
                "use_emergency_holding",
                ParameterType::Bool
            ),
            Ok(())
        );
    }

    #[test]
    fn a_name_the_contract_does_not_declare_is_refused_naming_the_contract() {
        let e = TABLE
            .check("/system", "mrm_handler", "timeout", ParameterType::Double)
            .unwrap_err();
        assert_eq!(e.node, "/system/mrm_handler");
        assert_eq!(e.contract, "island.contract.yaml");
        assert_eq!(e.declared, None);
    }

    #[test]
    fn a_type_that_differs_is_refused_with_the_declared_type() {
        let e = TABLE
            .check(
                "/system",
                "mrm_handler",
                "update_rate",
                ParameterType::Double,
            )
            .unwrap_err();
        assert_eq!(e.declared, Some(ParameterType::Integer));
        assert_eq!(type_name(ParameterType::Integer), "integer");
    }

    /// A node with no `params:` is not checked, and neither is an image with
    /// no table: absence is not a declaration of nothing.
    #[test]
    fn an_undeclaring_node_and_an_empty_table_check_nothing() {
        assert_eq!(
            TABLE.check("/", "talker", "anything", ParameterType::String),
            Ok(())
        );
        assert_eq!(
            DeclaredParams::EMPTY.check("/system", "mrm_handler", "x", ParameterType::Bool),
            Ok(())
        );
        // A node in ANOTHER namespace with the same name is another node.
        assert_eq!(
            TABLE.check("/other", "mrm_handler", "x", ParameterType::Bool),
            Ok(())
        );
    }

    /// The names every node carries are never refused, whatever their type.
    #[test]
    fn the_names_play_launch_exempts_are_exempt_here_too() {
        for name in [
            "use_sim_time",
            "start_type_description_service",
            "qos_overrides./chatter.subscription.depth",
        ] {
            assert_eq!(
                TABLE.check("/system", "mrm_handler", name, ParameterType::Bool),
                Ok(()),
                "{name}"
            );
        }
    }

    #[test]
    fn every_ros_type_code_round_trips() {
        for code in 1..=9u8 {
            assert_eq!(ros_type(code) as u8, code);
        }
    }
}
