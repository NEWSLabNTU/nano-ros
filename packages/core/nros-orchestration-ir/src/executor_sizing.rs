//! Issue 0257 — model-derived executor callback-table sizing.
//!
//! The executor's callback table is `NROS_EXECUTOR_MAX_CBS` (nros-node
//! `build.rs`, default 4). Before this module the knob was discovered at
//! RUNTIME: a bringup that registers more entities than the table holds boots
//! and dies on the first over-capacity `create_*` with `code=-6 Full`.
//!
//! The SystemModel already names every entity the bake will register, so both
//! entry bakes (the `nros::main!` proc-macro and the CLI's `codegen-system`)
//! can count them and (a) size the executor and (b) refuse to emit an image
//! whose capacity is known-too-small. The counting + derivation live here so
//! the two bakes cannot drift (same rationale as [`crate::board_path_for`]).
//!
//! What the model can and cannot see:
//!
//! - CAN: subscriptions, service servers, service clients, action servers,
//!   action clients — every one of which occupies exactly one callback slot.
//! - CANNOT: timers and guard conditions (launch wiring has no such entity),
//!   so [`count_callbacks`] is a LOWER BOUND. That is why the derivation adds
//!   headroom and why the bake-time check only fires on a count that already
//!   exceeds capacity (never a false positive).
//!
//! Where the exact count comes from (phase-307). The 172.E
//! `source-metadata.json` DOES carry `timers`, and its recorder walks the same
//! `Component::register` the runtime does. Phase-307 W1/W2 made it real: every
//! Rust Node pkg is now a producer candidate, and `nros sync` refreshes stale
//! sidecars automatically with a content-addressed provenance stamp, so a bake
//! can tell a current sidecar from museum data.
//!
//! The consuming rule is `max(model_wiring_count, recorded_count)` PER NODE,
//! and the max is necessary, not merely safe — neither source is complete:
//!
//! - the model has no timer entity, so a node that publishes on a timer counts
//!   one too few here;
//! - the recorder does not record service/action CLIENTS as node entities,
//!   while the model's wiring names them.
//!
//! The rule lives in the CLI bake (`nros-cli-core`
//! `model_ingest::count_callbacks_with_metadata`), not here, because reading
//! sidecars needs `std` + the CLI's schema types. This module stays the shared
//! COUNT + derivation so the two bakes cannot drift on the part they share.
//!
//! The `nros::main!` proc-macro keeps the model bound alone: a macro expansion
//! has no ordering guarantee that `nros sync` already ran, and shelling a
//! nested cargo build during expansion is the trap that killed the naive 0257
//! approach. The macro therefore under-counts where a node has timers — which
//! is why the derivation below adds headroom.
//!
//! Publishers are deliberately not counted: `create_publisher` allocates no
//! callback entry.
//!
//! Parameter / lifecycle services are not counted either: both register their
//! server sets outside the callback arena (see
//! `Executor::register_{parameter,lifecycle}_services`).

use ros_launch_manifest_model::SystemModel;

/// The `nros-node` build-time default for `NROS_EXECUTOR_MAX_CBS`.
///
/// Mirrored here because both bakes are HOST code: the proc-macro and the CLI
/// run before/outside the `nros-node` build script that materialises
/// `config::MAX_CBS`, so neither can read the real const. Only used to decide
/// whether a derived sizing is worth emitting (below the default we emit
/// nothing, keeping today's small entries byte-identical) and for the CLI's
/// advisory capacity check; the proc-macro's hard check compares against the
/// REAL `nros::__macro_support::EXECUTOR_MAX_CBS` in the generated code.
/// Keep in sync with `packages/core/nros-node/build.rs`.
pub const DEFAULT_MAX_CBS: usize = 4;

/// Node FQN owning an endpoint ref (`"/ns/node/endpoint"` → `"/ns/node"`).
fn endpoint_node(ep: &str) -> &str {
    ep.rsplit_once('/').map(|(node, _)| node).unwrap_or(ep)
}

/// Callback-slot-consuming entities the model declares for the nodes `keep`
/// selects (by node FQN). Lower bound — see the module docs.
pub fn count_callbacks<F: FnMut(&str) -> bool>(model: &SystemModel, mut keep: F) -> usize {
    let mut n = 0usize;
    for w in model.structure.topics.values() {
        n += w
            .subscribers
            .iter()
            .filter(|ep| keep(endpoint_node(ep)))
            .count();
    }
    for w in model
        .structure
        .services
        .values()
        .chain(model.structure.actions.values())
    {
        n += w
            .server
            .iter()
            .chain(w.client.iter())
            .filter(|ep| keep(endpoint_node(ep)))
            .count();
    }
    n
}

/// phase-307 W4 — the shared `max(model_wiring, recorded_metadata)` rule.
///
/// `keep` selects the nodes this entry registers (the CLI bake takes all of
/// them; the macro slices per board). `recorded` answers "how many callback
/// slots did the source-metadata sidecar record for this `(pkg, exec)`?", or 0
/// when there is no sidecar.
///
/// The max is PER NODE and it is necessary, not merely safe — neither source is
/// complete on its own (module docs). Per-node keeps the two sources' blind
/// spots from being added together, and monotonicity means a workspace with no
/// sidecars produces exactly today's model bound, so no existing build
/// regresses.
///
/// Both bakes call this, for the same reason they share [`count_callbacks`]:
/// the CLI refuses an over-capacity system and the `nros::main!` macro SIZES
/// the executor, and a disagreement between them is an image that passes the
/// check and dies at boot anyway.
pub fn count_callbacks_with_recorded<F, R>(
    model: &SystemModel,
    mut keep: F,
    mut recorded: R,
) -> usize
where
    F: FnMut(&str) -> bool,
    R: FnMut(&str, &str) -> usize,
{
    let mut total = 0usize;
    for (fqn, inst) in &model.structure.nodes {
        if !keep(fqn) {
            continue;
        }
        let modelled = count_node_callbacks(model, fqn);
        let slots = match (inst.pkg.as_deref(), inst.exec.as_deref()) {
            (Some(pkg), Some(exec)) => recorded(pkg, exec),
            _ => 0,
        };
        total += modelled.max(slots);
    }
    // Endpoints whose owning node the model does not list as an instance still
    // register callbacks; count them from the wiring so the total can never
    // fall below the plain `count_callbacks` bound.
    total += count_callbacks(model, |node| {
        keep(node) && !model.structure.nodes.contains_key(node)
    });
    total
}

/// Callback slots one node's modelled entities consume.
pub fn count_node_callbacks(model: &SystemModel, fqn: &str) -> usize {
    count_callbacks(model, |node| node == fqn)
}

/// Callback-table size derived from a modelled entity count.
///
/// `derived = counted + headroom`, `headroom = max(2, ceil(counted / 4))` —
/// i.e. 25 %, floored at two slots. The headroom is not cosmetic: the model
/// cannot see timers or guard conditions (module docs), so an exact fit would
/// be wrong at runtime for the very common "one sub + one timer" node. 25 %
/// keeps a large system's slack proportional without the arena (which scales
/// linearly with the slot count, `nros_node::config::arena_size_for`) growing
/// by more than a quarter.
pub fn derive_max_callbacks(counted: usize) -> usize {
    if counted == 0 {
        return 0;
    }
    counted + core::cmp::max(2, counted.div_ceil(4))
}

/// Whether the board behind a `deploy = "<key>"` honors the per-entry sizing
/// the entry bake emits (`BoardEntry::run_with_deploy_sized` →
/// `Executor::open_sized`, phase-271 / issue #110).
///
/// Only the hosted boards override it; every firmware board takes the default
/// trait body, which drops the sizing and opens at the build-time `MAX_CBS`.
/// The bakes use this to decide whether a derived sizing actually fixes an
/// over-capacity model or whether the user must raise `NROS_EXECUTOR_MAX_CBS`.
/// `None` (the macro's explicit `board = <Zst>` form, where no deploy key was
/// read) is "unknown" — reported as not honoring is a false alarm, so callers
/// skip the check instead.
pub fn board_honors_entry_sizing(deploy_key: &str) -> bool {
    matches!(deploy_key, "native" | "posix")
}

#[cfg(test)]
mod tests {
    use ros_launch_manifest_model::{ServiceWiring, TopicWiring};

    use super::*;

    fn model() -> SystemModel {
        let mut m = SystemModel::default();
        m.structure.topics.insert(
            "/chatter".into(),
            TopicWiring {
                msg_type: "std_msgs/msg/String".into(),
                publishers: vec!["/talker/chatter".into()],
                subscribers: vec!["/listener/chatter".into(), "/logger/chatter".into()],
            },
        );
        m.structure.services.insert(
            "/add".into(),
            ServiceWiring {
                srv_type: "example_interfaces/srv/AddTwoInts".into(),
                server: vec!["/adder/add".into()],
                client: vec!["/listener/add".into()],
            },
        );
        m.structure.actions.insert(
            "/fib".into(),
            ServiceWiring {
                srv_type: "example_interfaces/action/Fibonacci".into(),
                server: vec!["/adder/fib".into()],
                client: vec!["/talker/fib".into()],
            },
        );
        m
    }

    #[test]
    fn counts_subs_services_and_actions_but_not_publishers() {
        // 2 subs + 1 srv server + 1 srv client + 1 action server + 1 action
        // client = 6; the `/talker/chatter` publisher is not a callback.
        assert_eq!(count_callbacks(&model(), |_| true), 6);
    }

    #[test]
    fn counts_per_node() {
        let m = model();
        assert_eq!(count_node_callbacks(&m, "/listener"), 2); // sub + srv client
        assert_eq!(count_node_callbacks(&m, "/adder"), 2); // srv + action server
        assert_eq!(count_node_callbacks(&m, "/talker"), 1); // action client only
        assert_eq!(count_node_callbacks(&m, "/nobody"), 0);
    }

    #[test]
    fn empty_model_counts_zero_and_derives_nothing() {
        let m = SystemModel::default();
        assert_eq!(count_callbacks(&m, |_| true), 0);
        assert_eq!(derive_max_callbacks(0), 0);
    }

    #[test]
    fn headroom_is_25_percent_floored_at_two() {
        assert_eq!(derive_max_callbacks(1), 3);
        assert_eq!(derive_max_callbacks(8), 10);
        assert_eq!(derive_max_callbacks(9), 12);
        assert_eq!(derive_max_callbacks(32), 40);
    }

    #[test]
    fn only_hosted_boards_honor_entry_sizing() {
        assert!(board_honors_entry_sizing("native"));
        assert!(board_honors_entry_sizing("posix"));
        assert!(!board_honors_entry_sizing("zephyr"));
        assert!(!board_honors_entry_sizing("freertos"));
    }
}
