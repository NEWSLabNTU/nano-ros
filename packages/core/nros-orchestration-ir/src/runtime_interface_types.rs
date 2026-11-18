//! Issue 2704 -- the interface types the runtime creates entities with on its
//! own (the parameter services, `/rosout`, the lifecycle services and their
//! event publishers), per node interface axis.
//!
//! ONE table, `data/runtime-interface-types.json`, read here and by
//! `cmake/NanoRosRuntimeInterfaceTypes.cmake` (`string(JSON)`), so a new
//! runtime-owned type or axis cannot land on one build road only. The two
//! roads read different columns of it, because they register differently:
//!
//! * the cmake roads generate each enabled axis's [`Axis::packages`] WHOLE
//!   through the backend's per-message hook; the descriptor table then
//!   records every type of those packages (the cap counts them at configure);
//! * the cargo road registers, at create time, only the types the executor
//!   creates, [`Axis::registers`], and sizes its tables from that count
//!   ([`crate::type_descriptor_sizing::infra_types`]).
//!
//! The CLI's configure-time refusal (`backend_link::
//! runtime_interface_types_refusal`) reads [`Axis::packages`].
//! `check-runtime-interface-types-roads` holds [`Axis::registers`] to the
//! types nros-node's creation sites name.

use std::sync::OnceLock;

use serde::Deserialize;

const TABLE_JSON: &str = include_str!("../data/runtime-interface-types.json");

/// One interface package, with the packages its IDL includes.
#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub deps: Vec<String>,
}

/// One node interface axis (`param_services`, `rosout`, `lifecycle`).
#[derive(Debug, Deserialize)]
pub struct Axis {
    /// The `[system].features` / `node_interfaces` key.
    pub axis: String,
    /// The packages the cmake roads generate for it, whole.
    pub packages: Vec<String>,
    /// The types the executor creates entities with for it, ROS form
    /// (`pkg/msg/Name`, `pkg/srv/Name`).
    pub registers: Vec<String>,
}

/// The table.
#[derive(Debug, Deserialize)]
pub struct Table {
    /// Every package, in dependency order.
    pub packages: Vec<Package>,
    pub axes: Vec<Axis>,
}

/// The parsed table. It is compiled in, so a parse failure is a build defect
/// the unit tests below catch before anything ships.
pub fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(TABLE_JSON).expect("data/runtime-interface-types.json parses")
    })
}

/// The axis named `axis`, if the runtime owns types for it.
pub fn axis(axis: &str) -> Option<&'static Axis> {
    table().axes.iter().find(|a| a.axis == axis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn the_table_parses_and_is_consistent() {
        let t = table();
        let known: Vec<&str> = t.packages.iter().map(|p| p.name.as_str()).collect();
        // Dependency order: a package's deps are listed before it.
        for (i, p) in t.packages.iter().enumerate() {
            for d in &p.deps {
                let at = known.iter().position(|k| k == d);
                assert!(
                    at.is_some_and(|j| j < i),
                    "{}'s dependency {d} must be listed before it",
                    p.name
                );
            }
        }
        let mut axes = BTreeSet::new();
        for a in &t.axes {
            assert!(axes.insert(a.axis.as_str()), "axis {} listed twice", a.axis);
            assert!(!a.registers.is_empty(), "{}: registers nothing", a.axis);
            for p in &a.packages {
                assert!(
                    known.contains(&p.as_str()),
                    "{}: unknown package {p}",
                    a.axis
                );
                // An axis's package list is closed under dependencies, so
                // the cmake road never generates a package without its deps.
                let pkg = t.packages.iter().find(|q| &q.name == p).unwrap();
                for d in &pkg.deps {
                    assert!(a.packages.contains(d), "{}: {p} needs {d}", a.axis);
                }
            }
            // Every registered type comes from a package the axis generates,
            // so the cmake road's whole-package generation covers it.
            for ty in &a.registers {
                let (pkg, rest) = ty.split_once('/').expect("pkg/kind/Name");
                assert!(
                    a.packages.iter().any(|p| p == pkg),
                    "{}: {ty} is not from a package the axis generates",
                    a.axis
                );
                assert!(
                    rest.starts_with("msg/") || rest.starts_with("srv/"),
                    "{}: {ty} is neither a msg nor a srv",
                    a.axis
                );
            }
        }
    }
}
