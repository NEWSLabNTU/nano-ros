# Phase 381 — read the ROS graph, which we are already visible in

**Status (2026-08-25). NOT STARTED — scoped out of phase-379 W3 on measurement.**
Split from issue 0791 because it is several times the size of the other W3
coverage gaps and needs live interop verification the others do not.

**Implements.** RFC-0035 (RMW vtable ABI). Amends RFC-0036, whose "no dynamic
discovery" line is the reason this was never attempted.

## Why

A nano-ros node appears in `ros2 node list` and `ros2 topic info` — the zenoh
shim declares `@ros2_lv` liveliness tokens, `nros-rmw-cyclonedds/src/graph.cpp`
publishes `ros_discovery_info` — and the node itself cannot answer "is anyone
subscribed to this topic", "did the peer I need come up", or "what is on this
topic" in any of the three languages.

The asymmetry is the defect. An operator who can see the node reasonably expects
it to behave like a participant, and code that would branch on the graph has to
be written blind on a system where it need not be.

Phase 379's `graph` stage recorded **37 `gap` rows** on this, against 15
`declined`. The declines that survive are the rclcpp `Event` /
`wait_for_graph_change` shape (allocator, listener thread, and a blocking wait
that does not drive the executor) and rclrs's `notify_on_graph_change`.

## What already exists — the easy half

`nros/rmw_vtable.h` has carried the family as optional slots since phase-376 W4:
`get_node_names`, the four `*_by_node` forms, `get_topic_names_and_types`,
`get_service_names_and_types`, `get_publishers_info_by_topic`,
`get_subscriptions_info_by_topic`, `count_publishers`, `count_subscribers`,
`node_get_graph_guard_condition` — 12 slots for upstream's 15 names, plus
`rmw_topic_endpoint_info_t` in `rmw_entity.h`.

**The result-carrier design is already settled and is the part that would
otherwise be hard.** Upstream returns `rcutils_string_array_t` and
`rmw_names_and_types_t`, which allocate two levels deep. There is no allocator
at this seam, and a caller-provides-the-buffer shape is worse than it looks: the
graph has no bound the CALLER can know. So enumeration is a **visitor** —
`rmw_node_visit_fn`, `rmw_names_and_types_visit_fn`,
`rmw_topic_endpoint_info_visit_fn`. Peak extra RAM is one entry, the backend
streams from state it already holds, and a caller with a bound stops early by
returning `false`. Every string is borrowed for the call only.

Every one of the 12 is `None` in `packages/rmw/cffi/src/lib.rs`. No runtime
wrapper exists above them, and no user entry point in C, C++ or Rust.

## What does not exist — the reason this is a phase

**zenoh has a boolean liveliness CHECK, not an enumeration.**
`packages/rmw/zenoh/nros-rmw-zenoh/src/zpico.rs:920-940` is
`liveliness_get_start(keyexpr, timeout_ms)` / `liveliness_get_check(handle)` —
"does anything match this keyexpr", used for peer-alive detection. It collects
no replies, and the crate has no keyexpr parser.

So "zenoh already queries the tokens, only the reading half is missing" is right
about the protocol and wrong about the distance.

## Work items

**W1 — zenoh enumeration.** A zpico shim entry point that COLLECTS liveliness
reply keyexprs rather than counting them. Note the shim's config is ABI-coupled
to the zenoh-pico library (CLAUDE.md's zpico rule, issue 0135): the generated
config must be shared or the TUs disagree silently.

**W2 — the keyexpr parser.** `@ros2_lv/<domain>/<zid>/<nid>/<eid>/…/<namespace>/<node>`
is `rmw_zenoh_cpp`'s grammar and must match it **exactly** — a graph query that
returns a plausible wrong answer is worse than one that returns none. Pin the
grammar against the `rmw_zenoh_cpp` the recorded router links (RFC-0075), and
expect it to move with the distro.

**W3 — fill the vtable slots for zenoh.** The easy part; the visitor contract is
designed and needs no allocator.

**W4 — the runtime wrapper and user entry points**, in all three languages. Note
the `subscription` / `subscriber` split found in phase-379: rclrs says
`get_subscription_names_and_types_by_node`, rcl says `subscriber`. Whichever we
pick, one lane is not a drop-in — settle it with issue 0788's sweep, not
separately.

**W5 — Cyclone.** Needs a READER for `ros_discovery_info`, which
`nros-rmw-cyclonedds/src/graph.cpp` currently only writes.

**W6 — degrade per backend.** XRCE has no graph. The slots are optional
precisely so a backend can decline; the user-facing answer for "this backend
cannot tell you" must be distinguishable from "nothing is there", or callers
will read absence as emptiness.

**W7 — amend RFC-0036.** Its "no dynamic discovery — peers static" line caused
phase-379 to decline six rows across two stages that had to be re-verdicted. The
accurate statement is narrower: no discovery-driven *entity matching*, but the
graph is observable.

## Acceptance

* `ros2 node list` and a nano-ros node's own `get_node_names()` agree, on zenoh,
  in a live interop cell.
* A backend with no graph reports "unsupported", not an empty list.
* Phase-379's `graph.json` rows flip from `gap` to `same`, and
  `just check-api-parity` stays green — which is the mechanical proof the
  surface landed rather than a hand-check.

## Adjacent, cheap, and not blocked on any of this

`get_transition_graph`: `nros-node/src/lifecycle_services.rs` serves our full
`ALL_TRANSITIONS` table over `~/get_transition_graph`, so a remote peer can read
the lifecycle state machine over the wire while the node's own code cannot read
it in-process in any language. The table is already `const`. That is a local
accessor, not a graph query, and could land any time.
