---
id: 791
title: "We are visible in the ROS graph and cannot read it — 12 rmw vtable graph
  slots exist, all `None`, while both backends already run the discovery machinery"
status: resolved
type: bug
area: api, rmw
related: [rfc-0035, rfc-0036, phase-376, phase-379, phase-381, phase-407, 0903, 0927]
---

## Problem

RFC-0036 says nano-ros has "no dynamic discovery — peers static via `nros.toml` /
Kconfig", and phase 379 began by declining the whole graph-query family on that
basis. The `graph` stage did not survive contact with the code: **37 of its 68
rows are gaps, not declines.**

Three facts, each checkable:

**The vtable already carries the family.** `nros/rmw_vtable.h` has held these as
optional slots since phase-376 W4 — `get_node_names`, the four `*_by_node` forms,
`get_topic_names_and_types`, `get_service_names_and_types`,
`get_publishers_info_by_topic`, `get_subscriptions_info_by_topic`,
`count_publishers`, `count_subscribers`, `node_get_graph_guard_condition` — 12
slots for upstream's 15 names, plus `rmw_topic_endpoint_info_t` in
`rmw_entity.h`.

**Every one is `None`.** `packages/rmw/cffi/src/lib.rs` fills none of them, no
runtime wrapper exists above them, and no user-facing entry point exists in C,
C++ or Rust.

**Both real backends already run the machinery.** The zenoh shim *declares and
queries* `@ros2_lv` liveliness tokens for nodes, publishers, subscriptions and
services (`Ros2Liveliness::*_keyexpr`, wildcard GETs in
`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs`) — the same mechanism
`rmw_zenoh_cpp` builds its graph cache from. `nros-rmw-cyclonedds/src/graph.cpp`
publishes `ros_discovery_info` so `ros2 node list` can see us.

So the position is not "we have no discovery". It is: **we are visible in the
graph, we already speak the protocol that carries it, and the reading half was
never wired up.**

## Why it matters

The asymmetry is the problem. A nano-ros node appears in `ros2 node list` and
`ros2 topic info`, so an operator reasonably expects it to behave like a
participant — but the node itself cannot answer "is anyone subscribed to this
topic", "did the peer I need come up", or "what is on this topic" in any of the
three languages. Code that would branch on the graph has to be written as if
blind, on a system where it is not.

It also makes RFC-0036's blanket sentence misleading in a way that has already
cost work: the first pass of this campaign declined six rows across two stages
citing it, and those had to be re-verdicted once the vtable was read.

## Two smaller findings from the same stage

* **`get_transition_graph`**: `nros-node/src/lifecycle_services.rs` serves our
  full `ALL_TRANSITIONS` table over `~/get_transition_graph`, so a remote peer
  can read the lifecycle state machine over the wire while the node's own code
  cannot read it in-process in any language — only `nros_lifecycle_get_current_state`
  exists. The table is already `const`.
* **`subscription` vs `subscriber`**: rclrs says
  `get_subscription_names_and_types_by_node`, rcl says `subscriber`. Whichever
  we pick, one lane is not a drop-in. Related to issue 0788.

## Evidence

`scripts/api-parity.py --topic graph`, and
`docs/reference/api-parity-ledger/graph.json` — 37 `gap`, 15 `declined`,
8 `divergence`, 1 `rename`. The declines that survive are the rclcpp
`Event`/`wait_for_graph_change` shape (allocator, listener thread, and a
blocking wait that does not drive the executor) and rclrs's
`notify_on_graph_change` (future + runtime).

## Scope, measured 2026-08-25 — this is a PHASE, not a W3 item

Phase 379 W3 was going to take this alongside the other coverage gaps. It is
several times their size, and the reason is specific: **the zenoh backend has a
boolean liveliness CHECK, not an enumeration.**

`packages/rmw/zenoh/nros-rmw-zenoh/src/zpico.rs:920-940` gives
`liveliness_get_start(keyexpr, timeout_ms)` / `liveliness_get_check(handle)` —
"does anything match this keyexpr", used for peer-alive detection. It collects
no replies and there is no keyexpr parser anywhere in the crate. So the earlier
reading that "zenoh already queries the tokens, only the reading half is
missing" is right about the protocol and wrong about the distance: what exists
answers a yes/no question, and the graph needs the reply set.

Filling one slot for zenoh therefore needs, in order:

1. a new zpico C shim entry point that COLLECTS liveliness reply keyexprs
   rather than counting them (the shim is `packages/rmw/zenoh/zpico-sys/`, and
   its config is ABI-coupled to the library — see CLAUDE.md's zpico shim rule);
2. a parser for the `@ros2_lv/<domain>/<zid>/<nid>/<eid>/…/<namespace>/<node>`
   keyexpr grammar, which is `rmw_zenoh_cpp`'s and must match it exactly or the
   answers are wrong rather than absent;
3. the vtable slot itself, which is the easy part — the visitor contract is
   already designed and needs no allocator;
4. a runtime wrapper above the vtable, then user entry points in three
   languages;
5. live verification against `ros2 node list` / `ros2 topic info`, because a
   graph query that returns a plausible wrong answer is worse than one that
   returns none.

Cyclone would need a READER for `ros_discovery_info`, which it currently only
writes. XRCE has no graph at all, so whatever lands must degrade per backend —
which is what the optional slots are for.

That is a phase with its own acceptance, not a gap to close in a sitting — filed
as **phase-381**, which carries the work items and the acceptance. The 37 `gap`
rows in `graph.json` stay accurate; what changed is where the work is tracked.

## Direction

What the stage established that a planner should start from:

* The seam exists and is the right shape — this is filling slots, not designing
  an API.
* zenoh already SPEAKS the protocol but only asks it a yes/no question today —
  see the scope section above for what stands between that and an enumeration.
* The result carriers are the real design question, not the queries. rcl returns
  `rcl_names_and_types_t` and `rcl_topic_endpoint_info_array_t`, both allocated;
  `graph.json` records three visitor typedefs as the committed replacement shape.
* **RFC-0036's "no dynamic discovery" line should be narrowed** to say what is
  actually true: no discovery-driven *entity matching* (peers are static), but
  the graph is observable and we do not yet observe it.

## Resolution, 2026-08-31 — already done, and the issue text had gone stale

Re-measured before picking this up as work, and all three of the "each
checkable" facts are now false. The reading half was wired by phase-381 W3 and
made to actually return data by issue 0903 (the `@ros2_lv` liveliness
SUBSCRIBER with history, replacing the get that only ever saw a subset of the
domain's tokens).

**"Every one is `None`."** Eleven of the twelve are filled, by
`RustBackendAdapter::<R>::VTABLE`. The claim was read off `EMPTY_VTABLE`, which
is the DEFAULT a backend overrides — not what any live backend presents:

    get_node_names                          Some
    get_topic_names_and_types               Some
    get_service_names_and_types             Some
    get_publisher_names_and_types_by_node   Some
    get_subscriber_names_and_types_by_node  Some
    get_service_names_and_types_by_node     Some
    get_client_names_and_types_by_node      Some
    get_publishers_info_by_topic            Some
    get_subscriptions_info_by_topic         Some
    count_publishers                        Some
    count_subscribers                       Some
    node_get_graph_guard_condition          -- not filled, and DECIDED

The twelfth is `not-supported` as of phase-407: guard conditions are a platform
primitive the executor owns, not something a backend hands out, and nothing
consumes graph-change events.

**"No user-facing entry point exists in C, C++ or Rust."** All eleven have one
in all three. The audit that suggested otherwise was searching for
`get_subscriber_names_and_types_by_node`, which is the C spelling; Rust and C++
say `get_subscription_...`. One name, three surfaces, and a grep that found two
of them.

**"We are visible in the graph and cannot read it."** No longer: issue 0903
verified node and topic enumeration against a live `rmw_zenoh_cpp` node and got
agreement with `ros2 node list` / `ros2 topic list`, and issue 0927 did the
cyclonedds side once its probe was put on the right domain.

## What this issue should be remembered for

Not the gap, which closed, but how the gap was DESCRIBED. Two of the three
checkable facts were read off the wrong artifact — `EMPTY_VTABLE` instead of the
adapter's, and one language's spelling instead of three. Both are the same
mistake in different clothes: measuring the thing that was easy to reach and
reporting it as the thing that was asked about.
