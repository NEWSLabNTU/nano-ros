//! phase-307 — the ONE definition of "how many callback slots does a recorded
//! node consume?".
//!
//! Two bakes read source-metadata sidecars and must agree on this number: the
//! CLI's `codegen-system` (which REFUSES an over-capacity system) and the
//! `nros::main!` proc-macro (which SIZES the executor). A disagreement between
//! them is an image that passes the bake's check and dies at boot anyway —
//! precisely the issue-0257 failure the sidecars exist to prevent. It landed
//! duplicated in both consumers first; this module is the correction.
//!
//! Deliberately `serde_json::Value`-based rather than typed. The macro cannot
//! depend on `nros-cli-core` (that crate pulls the whole planner), and a
//! hand-mirrored schema struct in the macro would be the mirror-drift failure
//! this repo already pays for elsewhere. Counting array lengths off `Value`
//! needs no schema at all, so a sidecar that grows fields keeps counting
//! correctly. The CLI still parses the typed `SourceMetadata` first — that
//! parse is the schema gate; this is only the arithmetic.

use serde_json::Value;

/// Entity kinds that occupy one executor callback slot each.
///
/// Mirrors `ExecutorSink::create_entity`. Two absences are deliberate:
///
/// * **publishers** — `create_publisher` allocates no callback entry;
/// * **the sidecar's `callbacks` array** — a recorded ACTION carries three
///   callbacks (goal / cancel / result) but occupies ONE arena slot, so
///   entities are the unit, never callbacks.
pub const SLOT_ENTITY_KEYS: [&str; 4] = ["subscribers", "timers", "services", "actions"];

/// Callback slots one recorded node consumes.
pub fn slots_of_node(node: &Value) -> usize {
    SLOT_ENTITY_KEYS
        .iter()
        .map(|key| node.get(key).and_then(Value::as_array).map_or(0, Vec::len))
        .sum()
}

/// `(package, executable)` + total slots for one sidecar document.
///
/// `None` when the document does not carry the identity keys the
/// `SystemModel`'s node instances are matched on — an unusable sidecar, which
/// callers skip rather than fail on (the fallback is the model bound, which is
/// merely less precise; a bake that died because a stale sidecar existed would
/// be worse than the bug this all fixes).
pub fn slots_of_component(sidecar: &Value) -> Option<((String, String), usize)> {
    let package = sidecar.get("package")?.as_str()?.to_string();
    let executable = sidecar.get("executable")?.as_str()?.to_string();
    let slots = sidecar
        .get("nodes")?
        .as_array()?
        .iter()
        .map(slots_of_node)
        .sum();
    Some(((package, executable), slots))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sidecar(subs: usize, timers: usize, actions: usize, publishers: usize) -> Value {
        let arr = |n: usize| vec![serde_json::json!({}); n];
        serde_json::json!({
            "package": "talker_pkg",
            "executable": "talker",
            "nodes": [{
                "subscribers": arr(subs),
                "timers": arr(timers),
                "services": [],
                "actions": arr(actions),
                "publishers": arr(publishers),
            }],
            // Three callbacks for one action — must NOT be counted.
            "callbacks": arr(3),
        })
    }

    /// The shape the whole phase exists for: timers the SystemModel cannot see.
    #[test]
    fn counts_subs_and_timers() {
        let ((pkg, exec), slots) = slots_of_component(&sidecar(1, 5, 0, 0)).expect("identified");
        assert_eq!((pkg.as_str(), exec.as_str()), ("talker_pkg", "talker"));
        assert_eq!(slots, 6);
    }

    /// `create_publisher` allocates no callback entry — counting publishers
    /// would over-size every entry in the tree.
    #[test]
    fn publishers_take_no_slot() {
        assert_eq!(slots_of_component(&sidecar(1, 0, 0, 9)).unwrap().1, 1);
    }

    /// An action's goal/cancel/result callbacks share ONE arena slot, so the
    /// `callbacks` array is not the unit of accounting.
    #[test]
    fn an_action_is_one_slot_not_three() {
        assert_eq!(slots_of_component(&sidecar(0, 0, 1, 0)).unwrap().1, 1);
    }

    #[test]
    fn a_sidecar_without_identity_keys_is_unusable() {
        assert!(slots_of_component(&serde_json::json!({ "nodes": [] })).is_none());
    }

    /// phase-308/312 — a REAL C++ sidecar, produced by the CMake probe from
    /// `examples/workspaces/cpp/src/cpp_fib_server_pkg`. Trimmed to the fields
    /// this rule reads; every key and shape is verbatim.
    ///
    /// The point of this test is that there is NO language branch: the same
    /// counter that handles Rust sidecars handles this one, because it keys on
    /// `(package, executable)` and counts entity arrays. If C++ support had
    /// needed a special case here, the "one mechanism, three front-ends"
    /// property phase-308 is built on would have been false.
    #[test]
    fn counts_a_real_cpp_sidecar_with_no_language_branch() {
        let doc = serde_json::json!({
            "version": 1,
            "package": "cpp_fib_server_pkg",
            "component": "fib_server",
            "language": "cpp",
            "executable": "fib_server",
            "nodes": [{
                "id": "fib_server",
                "declaration_slot": 0,
                "publishers": [
                    { "id": "/fibonacci/_action/feedback#4" },
                    { "id": "/fibonacci/_action/status#5" }
                ],
                "subscribers": [],
                // Recorded by the nros-cpp executor hook — timers never reach
                // the RMW, and are exactly what the SystemModel cannot see.
                "timers": [{ "id": "timer0#6", "period_ms": 500, "callback": "timer0" }],
                // The action server's goal/cancel/result trio, recorded through
                // the RMW service path.
                "services": [
                    { "id": "/fibonacci/_action/send_goal#1" },
                    { "id": "/fibonacci/_action/cancel_goal#2" },
                    { "id": "/fibonacci/_action/get_result#3" }
                ],
                "actions": []
            }]
        });
        let ((pkg, exec), slots) = slots_of_component(&doc).expect("identified");
        assert_eq!((pkg.as_str(), exec.as_str()), ("cpp_fib_server_pkg", "fib_server"));
        // 1 timer + 3 services; the 2 publishers take no callback slot.
        assert_eq!(slots, 4, "publishers must not count");
    }

    /// Multi-node components contribute every node's slots.
    #[test]
    fn nodes_accumulate() {
        let doc = serde_json::json!({
            "package": "p", "executable": "e",
            "nodes": [
                { "subscribers": [{}], "timers": [{}, {}], "services": [], "actions": [] },
                { "subscribers": [], "timers": [{}], "services": [{}], "actions": [] },
            ],
        });
        assert_eq!(slots_of_component(&doc).unwrap().1, 5);
    }
}
