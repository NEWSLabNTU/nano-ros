//! Issue 2670: a parameter Rust code declares through the executor's own
//! routes reaches the census.
//!
//! The store's slot count comes from the contract's `params:`, and the guard
//! against a contract that lists too few is the census's
//! `param-missing-in-contract`. The census learns what the code declares from
//! the metadata recorder. The C, C++ and declarative Rust routes record
//! through `census_hooks::on_param_declare`; the four imperative Rust routes
//! (`Node::declare_parameter::<T>`, `Node::declare_parameter_with_descriptor`,
//! and the deprecated `Executor::declare_parameter` /
//! `Executor::declare_parameter_with_descriptor`) live in `nros-node`, below
//! that hook, and recorded nothing. So a contract that omitted their
//! parameters sized the store short with no census error.
//!
//! This drives those four routes on an executor opened over the
//! transport-free `metadata` backend, with nodes built the imperative way
//! (`node_builder(..).build()`, which no hook opens either), and reads the
//! sidecar the census is built from.

#![allow(deprecated)]

use nros::{ParameterDescriptor, ParameterType, ParameterValue};

// The posix C port DEFINES the `nros_platform_*` symbols the executor calls.
extern crate nros_platform_cffi as _;

/// Every recorded parameter as `(node, name, type)`, read from the sidecar's
/// top-level `"parameters"` array, whose entries open with their `"node"`.
fn recorded_parameters(json: &str) -> Vec<(String, String, String)> {
    let field = |entry: &str, key: &str| -> String {
        let tag = format!("\"{key}\":\"");
        let at = entry
            .find(&tag)
            .map(|i| i + tag.len())
            .unwrap_or(entry.len());
        entry[at..].split('"').next().unwrap_or("").to_owned()
    };
    let start = json
        .find("\"parameters\":[")
        .unwrap_or_else(|| panic!("a parameters array: {json}"));
    json[start..]
        .split("{\"node\":")
        .skip(1)
        .map(|rest| {
            let entry = format!("\"node\":{rest}");
            (
                field(&entry, "node"),
                field(&entry, "name"),
                field(&entry, "type"),
            )
        })
        .collect()
}

#[test]
fn imperative_rust_declarations_reach_the_census() {
    let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    nros::metadata_mode::reset();
    // What `begin_node` does on a census run's first node; this test has no
    // hooked node, so it arms the observers itself.
    nros::metadata_mode::listen_for_registrations();

    let config = nros::ExecutorConfig::new("").rmw("metadata");
    let mut executor = nros::Executor::open(&config).expect("the metadata backend must open");

    let alpha = executor
        .node_builder("alpha")
        .build()
        .expect("node alpha registers");
    let beta = executor
        .node_builder("beta")
        .namespace("/robot")
        .build()
        .expect("node beta registers");

    let _rate = executor
        .node_mut(alpha)
        .declare_parameter::<i64>("rate")
        .expect("the store opens")
        .default(5)
        .mandatory()
        .expect("rate declares");
    assert!(executor.node_mut(beta).declare_parameter_with_descriptor(
        "gain",
        ParameterValue::Double(1.5),
        ParameterDescriptor::new("gain", ParameterType::Double).unwrap(),
    ));
    assert!(executor.declare_parameter("primary_flag", ParameterValue::Bool(true)));
    assert!(executor.declare_parameter_with_descriptor(
        "primary_count",
        ParameterValue::Integer(7),
        ParameterDescriptor::new("primary_count", ParameterType::Integer).unwrap(),
    ));

    let export = nros::node_metadata::SourceMetadataExport::new("fixture_pkg", "imperative")
        .executable("imperative")
        .language("rust");
    let json = nros::metadata_mode::to_json(&export).expect("serialize");

    let params = recorded_parameters(&json);
    let has = |node: &str, name: &str, ty: &str| {
        params
            .iter()
            .any(|(n, p, t)| n == node && p == name && t == ty)
    };
    assert!(
        has("alpha", "rate", "integer"),
        "the typed builder's parameter, on the node it named: {params:?}"
    );
    assert!(
        has("beta", "gain", "double"),
        "the descriptor route's parameter, on the node it named: {params:?}"
    );
    assert!(
        params
            .iter()
            .any(|(_, p, t)| p == "primary_flag" && t == "bool"),
        "the deprecated executor route: {params:?}"
    );
    assert!(
        params
            .iter()
            .any(|(_, p, t)| p == "primary_count" && t == "integer"),
        "the deprecated descriptor route: {params:?}"
    );
    assert_eq!(
        params.iter().filter(|(_, p, _)| p == "rate").count(),
        1,
        "each declaration is recorded once: {params:?}"
    );
}
