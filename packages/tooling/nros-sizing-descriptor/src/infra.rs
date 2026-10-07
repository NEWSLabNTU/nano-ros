//! The runtime's own service servers, priced from the two `NROS_DECLARED_*`
//! facts every road carries.
//!
//! Issue 1743. A ROS parameter or REP-2002 lifecycle service is a served
//! endpoint like any other, so EVERY backend that caps its service servers owes
//! these slots: zenoh's queryable table, XRCE's service-server slots. The
//! application's own servers reach a build script through the descriptor's
//! `[image] service_server_queryables`, which deliberately leaves these out
//! (the producer sees the user's entities, never the runtime's). The runtime
//! half arrives as two FACTS:
//!
//! * `NROS_DECLARED_INFRA_QUERYABLES` — which families are compiled in:
//!   `none`, `param`, `lifecycle` or `param+lifecycle` (`all` is a synonym).
//! * `NROS_DECLARED_NODES` — how many nodes; the parameter family registers
//!   once PER NODE (phase-426 W3), the lifecycle family once per executor.
//!
//! The arithmetic lived in `nros-zpico-build` alone, and XRCE's build had no
//! copy, so on the cargo and CMake roads an XRCE image carrying both families
//! booted against the header's 4 slots and died registering its eleventh
//! server. A second copy of the arithmetic in the XRCE build script would have
//! been issue 1025's defect (one formula, its inputs derived twice), so it
//! lives here, beside the reader both build scripts already share. What each
//! backend DOES with the number — floor, headroom, refusal — stays in that
//! backend's build (RFC-0100 D5).
//!
//! UNFLOORED, like everything in this crate (D7).

/// Service servers the ROS parameter family registers on each node.
///
/// Mirrors `nros_node::parameter_services::PARAM_SERVICE_QUERYABLES`, which a
/// build-script helper cannot read: it sees neither another crate's constants
/// nor its features. Held to the creation sites by
/// `check-infra-queryable-counts`.
pub const PARAM_SERVICE_QUERYABLES: usize = 6;

/// Service servers the REP-2002 lifecycle family registers, once per executor.
/// Mirrors `nros_node::lifecycle_services::LIFECYCLE_SERVICE_QUERYABLES`, held
/// the same way.
pub const LIFECYCLE_SERVICE_QUERYABLES: usize = 5;

/// Issue 1429 — the ONE predicate for "did this carrier state anything".
///
/// A carrier can arrive EMPTY from a CI `env:` block or a `cmake -E env VAR=`,
/// and an empty string states nothing. Every rule below goes through this, so
/// none of them can panic on `Some("")` while its sibling tolerates it.
pub fn stated(v: Option<&str>) -> Option<&str> {
    v.filter(|s| !s.trim().is_empty())
}

/// How many nodes carry the parameter family: `NROS_DECLARED_NODES`, floored at
/// one (the executor's own `nodes.len().max(1)`), and one when undeclared —
/// the pre-phase-426 number.
///
/// # Panics
/// On a value that is not a count. A malformed declaration must not silently
/// become "undeclared": that is the shape issue 0827 measured, where a value
/// reads as applied and is not.
pub fn declared_nodes(nodes: Option<&str>) -> usize {
    match stated(nodes) {
        Some(v) => match v.trim().parse::<usize>() {
            Ok(n) => n.max(1),
            Err(_) => panic!(
                "NROS_DECLARED_NODES={v:?} is not a count. It is the number of \
                 nodes the entry's model declares, and the ROS parameter \
                 services are registered once PER NODE (phase-426 W3)."
            ),
        },
        None => 1,
    }
}

/// Which families `NROS_DECLARED_INFRA_QUERYABLES` names, as
/// `(param, lifecycle)`, or `None` when it states nothing.
///
/// # Panics
/// On a word that is not one of the four. The producer writes exactly those
/// (`InfraServices::token` in the CLI), so anything else is a broken carrier,
/// and sizing from a guess about it is how a pool comes out short.
pub fn declared_families(infra: Option<&str>) -> Option<(bool, bool)> {
    match stated(infra)?.trim() {
        "none" => Some((false, false)),
        "param" => Some((true, false)),
        "lifecycle" => Some((false, true)),
        "param+lifecycle" | "all" => Some((true, true)),
        other => panic!(
            "NROS_DECLARED_INFRA_QUERYABLES={other:?} is not one of \
             none|param|lifecycle|param+lifecycle (phase-392 W5)."
        ),
    }
}

/// The service servers the runtime registers on the image's behalf, from the
/// two facts.
///
/// UNDECLARED families are assumed PRESENT: over-reserving costs RAM, while
/// under-reserving is a registration failure at boot (issue 0460). A consumer
/// that must not take that direction (a FLOOR it refuses on) checks
/// [`stated`] itself before calling.
pub fn infra_service_servers(infra: Option<&str>, nodes: Option<&str>) -> usize {
    let (param, lifecycle) = declared_families(infra).unwrap_or((true, true));
    let params = if param {
        PARAM_SERVICE_QUERYABLES * declared_nodes(nodes)
    } else {
        0
    };
    params
        + if lifecycle {
            LIFECYCLE_SERVICE_QUERYABLES
        } else {
            0
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_family_costs_its_own_servers() {
        assert_eq!(infra_service_servers(Some("none"), None), 0);
        assert_eq!(infra_service_servers(Some("param"), None), 6);
        assert_eq!(infra_service_servers(Some("lifecycle"), None), 5);
        // The issue-1743 image: both families on one node -- eleven.
        assert_eq!(
            infra_service_servers(Some("param+lifecycle"), Some("1")),
            11
        );
        assert_eq!(infra_service_servers(Some("all"), None), 11);
    }

    #[test]
    fn the_parameter_family_is_per_node_and_lifecycle_is_not() {
        assert_eq!(
            infra_service_servers(Some("param+lifecycle"), Some("3")),
            18 + 5
        );
        assert_eq!(infra_service_servers(Some("lifecycle"), Some("3")), 5);
        // Zero nodes is floored at the executor's own one.
        assert_eq!(infra_service_servers(Some("param"), Some("0")), 6);
    }

    #[test]
    fn undeclared_is_the_large_direction_and_empty_is_undeclared() {
        assert_eq!(infra_service_servers(None, None), 11);
        assert_eq!(infra_service_servers(Some(""), Some(" ")), 11);
        assert_eq!(declared_families(Some("  ")), None);
    }

    #[test]
    #[should_panic(expected = "NROS_DECLARED_INFRA_QUERYABLES")]
    fn an_unknown_family_word_refuses() {
        infra_service_servers(Some("params"), None);
    }

    #[test]
    #[should_panic(expected = "NROS_DECLARED_NODES")]
    fn a_node_count_that_is_not_a_count_refuses() {
        declared_nodes(Some("several"));
    }
}
