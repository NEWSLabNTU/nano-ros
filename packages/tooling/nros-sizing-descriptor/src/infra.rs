//! The runtime's own service servers, priced from the two facts every road
//! carries.
//!
//! Issue 1743. A ROS parameter or REP-2002 lifecycle service is a served
//! endpoint like any other, so EVERY backend that caps its service servers owes
//! these slots: zenoh's queryable table, XRCE's service-server slots. The
//! application's own servers reach a build script through the descriptor's
//! `[image] service_server_queryables`, which deliberately leaves these out
//! (the producer sees the user's entities, never the runtime's). The runtime
//! half arrives as two FACTS:
//!
//! * the descriptor's `[image] infra_families` — which families are compiled
//!   in: `none`, or a `+`-joined subset of `param`, `lifecycle` and (since
//!   phase-556 W13) `rosout`, in that order (`all` names all three). It arrived
//!   as ~~`NROS_DECLARED_INFRA_QUERYABLES`~~ until phase-486; the build scripts
//!   pass its tag here.
//! * the descriptor's `[image] model_nodes` — how many nodes; the parameter
//!   family registers once PER NODE (phase-426 W3), the lifecycle family once
//!   per executor. It arrived as ~~`NROS_DECLARED_NODES`~~ until phase-486 W9; the
//!   build scripts pass it here in that carrier's spelling, a decimal count.
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

/// phase-556 W13 -- the `/rosout` publisher the `rosout` family claims, once
/// per image. VOLATILE (a documented divergence from rclcpp's
/// TRANSIENT_LOCAL), so a publisher slot and a liveliness token and nothing
/// else. Mirrors `nros_node::rosout::ROSOUT_PUBLISHERS`, held the same way.
pub const ROSOUT_PUBLISHERS: usize = 1;

/// Issue 1429 — the ONE predicate for "did this carrier state anything".
///
/// A carrier can arrive EMPTY from a CI `env:` block or a `cmake -E env VAR=`,
/// and an empty string states nothing. Every rule below goes through this, so
/// none of them can panic on `Some("")` while its sibling tolerates it.
pub fn stated(v: Option<&str>) -> Option<&str> {
    v.filter(|s| !s.trim().is_empty())
}

/// How many nodes carry the parameter family: `[image] model_nodes`, floored at
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
                "the model's node count {v:?} is not a count. It is the number of \
                 nodes the entry's model declares, and the ROS parameter \
                 services are registered once PER NODE (phase-426 W3)."
            ),
        },
        None => 1,
    }
}

/// The runtime's node-interface families an image carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Families {
    /// The ROS parameter services (and `/parameter_events`).
    pub param: bool,
    /// The REP-2002 lifecycle services (and `~/transition_event`).
    pub lifecycle: bool,
    /// The `/rosout` publisher (phase-556 W13).
    pub rosout: bool,
}

impl Families {
    /// Every family: what an UNDECLARED image is priced as.
    pub const ALL: Families = Families {
        param: true,
        lifecycle: true,
        rosout: true,
    };
}

/// Which families an `[image] infra_families` tag names, or `None` when it
/// states nothing.
///
/// # Panics
/// On a spelling that is not `none`, `all` or a `+`-joined subset of
/// `param`, `lifecycle`, `rosout`. The producer writes exactly those
/// (`InfraFamilies::tag`), so anything else is a broken carrier, and sizing
/// from a guess about it is how a pool comes out short.
pub fn declared_families(infra: Option<&str>) -> Option<Families> {
    let tag = stated(infra)?.trim();
    let bad = || -> ! {
        panic!(
            "infra_families = {tag:?} is not one of `none`, `all` or a `+`-joined \
             subset of param, lifecycle, rosout."
        )
    };
    match tag {
        "none" => {
            return Some(Families {
                param: false,
                lifecycle: false,
                rosout: false,
            });
        }
        "all" => return Some(Families::ALL),
        _ => {}
    }
    let mut f = Families {
        param: false,
        lifecycle: false,
        rosout: false,
    };
    for word in tag.split('+') {
        let slot = match word {
            "param" => &mut f.param,
            "lifecycle" => &mut f.lifecycle,
            "rosout" => &mut f.rosout,
            _ => bad(),
        };
        if *slot {
            bad();
        }
        *slot = true;
    }
    Some(f)
}

/// The service servers the runtime registers on the image's behalf, from the
/// two facts.
///
/// UNDECLARED families are assumed PRESENT: over-reserving costs RAM, while
/// under-reserving is a registration failure at boot (issue 0460). A consumer
/// that must not take that direction (a FLOOR it refuses on) checks
/// [`stated`] itself before calling.
pub fn infra_service_servers(infra: Option<&str>, nodes: Option<&str>) -> usize {
    let Families {
        param, lifecycle, ..
    } = declared_families(infra).unwrap_or(Families::ALL);
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

/// Issue 2482 -- is the PER-NODE term of [`infra_service_servers`] a guess?
///
/// True when the parameter family is stated ON and the node count is not
/// stated. [`declared_nodes`] then prices ONE node, which is the small
/// direction: an owned entry (its own `main`, no launch) states its families
/// in `system.toml` but creates its nodes in its own code, so nothing before
/// the build counts them. Measured: a two-node owned image priced as one lost
/// `/beta`'s parameter services on both zenoh (liveliness) and XRCE (service
/// slots). A consumer seeing `true` must not size below its own UNDECLARED
/// budget -- the number it would use had the families not been stated at all.
pub fn per_node_term_is_unknown(infra: Option<&str>, nodes: Option<&str>) -> bool {
    declared_families(infra).is_some_and(|f| f.param) && stated(nodes).is_none()
}

/// Service clients an action CLIENT opens: `send_goal`, `cancel_goal` and
/// `get_result`.
///
/// Mirrors `nros_node::executor::action::ACTION_CLIENT_SERVICE_CLIENTS`, held
/// the same way as the two above.
pub const ACTION_CLIENT_SERVICE_CLIENTS: usize = 3;

/// The service clients the image opens, from what it declares: one per
/// declared service client plus [`ACTION_CLIENT_SERVICE_CLIENTS`] per action
/// client. `None` unless BOTH counts are stated.
///
/// Issue 1033 left XRCE's client slots the one stated cap of its three because
/// "the raw count would under-size an action client". The multiplier is that
/// count's missing term. It is the same sum the entity inventory prices the
/// zenoh liveliness pool from, so the two backends cannot count one image's
/// clients differently (issue 1025's rule). The runtime opens no service client
/// of its own, so there is no runtime half to add.
///
/// UNFLOORED (D7): an image with no clients needs 0.
pub fn service_client_slots(image: &crate::Image) -> Option<usize> {
    let clients = *image.service_client_entities().stated()?;
    let actions = *image.action_client_entities().stated()?;
    Some(clients + actions * ACTION_CLIENT_SERVICE_CLIENTS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_client_costs_three_service_clients() {
        let mut img = crate::Image::default();
        assert_eq!(service_client_slots(&img), None, "nothing stated");
        img.set_service_client_entities(Some(2));
        assert_eq!(service_client_slots(&img), None, "half stated");
        img.set_action_client_entities(Some(1));
        assert_eq!(service_client_slots(&img), Some(5));
        img.set_service_client_entities(Some(0));
        img.set_action_client_entities(Some(0));
        assert_eq!(service_client_slots(&img), Some(0), "unfloored");
    }

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
    fn the_per_node_term_is_unknown_only_for_a_stated_param_family_without_nodes() {
        assert!(per_node_term_is_unknown(Some("param"), None));
        assert!(per_node_term_is_unknown(Some("param+lifecycle"), Some(" ")));
        assert!(!per_node_term_is_unknown(Some("param"), Some("2")));
        // Lifecycle is once per executor: no per-node term to guess.
        assert!(!per_node_term_is_unknown(Some("lifecycle"), None));
        assert!(!per_node_term_is_unknown(Some("none"), None));
        // Nothing stated is the UNDECLARED road, which each consumer already
        // prices at its own budget.
        assert!(!per_node_term_is_unknown(None, None));
    }

    /// phase-556 W13 -- `rosout` joins the families: any `+`-joined subset
    /// parses, a repeated or unknown word refuses, and `rosout` claims no
    /// service server.
    #[test]
    fn rosout_is_a_family_and_claims_no_server() {
        let f = declared_families(Some("param+lifecycle+rosout")).unwrap();
        assert_eq!(f, Families::ALL);
        let r = declared_families(Some("rosout")).unwrap();
        assert!(r.rosout && !r.param && !r.lifecycle);
        assert_eq!(declared_families(Some("all")), Some(Families::ALL));
        assert_eq!(infra_service_servers(Some("rosout"), None), 0);
        assert_eq!(infra_service_servers(Some("param+rosout"), Some("2")), 12);
        assert!(!per_node_term_is_unknown(Some("rosout"), None));
    }

    #[test]
    #[should_panic(expected = "infra_families")]
    fn a_repeated_family_word_refuses() {
        declared_families(Some("param+param"));
    }

    #[test]
    #[should_panic(expected = "infra_families")]
    fn an_unknown_family_word_refuses() {
        infra_service_servers(Some("params"), None);
    }

    #[test]
    #[should_panic(expected = "node count")]
    fn a_node_count_that_is_not_a_count_refuses() {
        declared_nodes(Some("several"));
    }
}
