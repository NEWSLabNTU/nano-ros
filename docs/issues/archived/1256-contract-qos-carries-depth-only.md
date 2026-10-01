---
id: 1256
title: "Contract QoS reaches the build as DEPTH only -- reliability, history
  and durability are declared, delivered, and dropped"
status: resolved
resolved_in: 2026-10-01
type: enhancement
area: orchestration, build
severity: low
related: [issue-1227, issue-1255, phase-412]
---

## What is true today

The launch contract already carries the whole profile. `QosDecl` in the pinned
`ros-launch-manifest` (`v0.1.31`, `types/src/types.rs:432`) has
`reliability`, `durability`, `depth`, `history` and `lifespan`, and play_launch's
`effective_qos` resolves them per endpoint into the SystemModel.

nano-ros reads one field of it. The entity inventory's `depth_of`
(`entity_inventory.rs:872`) takes `sub_endpoints[*].qos.depth` and nothing
else, and the declaration grammar it emits is documented as "leaving room for
`@reliability=`, `@history=` and `@durability=` without another grammar
change" (`entity_inventory.rs:350`). The room was left; nothing moved into it.

So a contract that says

```yaml
sub:
  control_cmd: { qos: { depth: 1, reliability: best_effort } }
```

sizes the arena from `depth: 1` and silently ignores `best_effort`.

## Why the other three matter for memory, not just for correctness

- **history.** `KEEP_ALL` has no depth bound at all. Today it is priced as
  whatever `depth` says (or the ROS default 10), which under-sizes a
  KEEP_ALL subscription rather than refusing it. The build should refuse or
  demand an explicit cap, on the same "absence is not zero" rule issue 1227
  applied to depth.
- **reliability.** On XRCE a reliable stream needs history buffers the
  best-effort one does not, and on Cyclone a RELIABLE writer keeps a history
  cache sized from depth. Neither is priced from the contract.
- **durability.** `TRANSIENT_LOCAL` publishers keep the last `depth` samples
  for late joiners. That is publisher-side memory, and publishers are exactly
  the endpoints issue 1227 left undeclared ("sizes no receive buffer, so
  nothing reads it yet").

## Also: code vs contract agreement covers depth only

`nros_declared_qos_generated.h` and the boot-time `check_declared_depth`
(phase-403) catch a node whose code passes a depth the contract does not state
(`DECLARED_DEPTH_MISMATCH`). A code/contract disagreement on reliability or
durability is not checked at all, and that disagreement is an interop failure
(an incompatible-QoS match never delivers) rather than a sizing one.

## Where the ROS defaults live

When the contract is silent the build uses ROS 2's defaults, and those are
still literals in code: `PUBSUB_QOS_DEPTH = 10` (`nros-node/build.rs:27`), the
`qos_profiles!` table (phase-428 W10, `58612b5b4`), and copies in
`nros-c/src/qos.rs` and the cffi layer, held equal by gates against
`docs/reference/rmw-qos-profiles.txt`. The Zephyr Kconfig
`CONFIG_NROS_PUBSUB_QOS_DEPTH` (`7ff68c777`) is the only config-side copy. A
fix that adds three more defaulted fields should move the defaults to one
config source first, rather than adding three more literals.

## Acceptance

- **DONE** (phase-454 W1) — ROS default values are read from one config source,
  not from code literals. `QoSProfile`'s `qos_profiles!` table is the one
  authored site; every site that can read Rust reads it, and the six that cannot
  (a Kconfig `default`, two C headers, a C++ header, a C source) are compared
  field by field by `check-qos-profile-ssot.py`.
- **DONE** (phase-454 W3) — the inventory carries all four fields per endpoint.
  `EntityDecl` gained `reliability` / `durability` / `history` beside `depth`,
  `EntityInventory::from_model` reads all four out of both endpoint maps, and
  they reach a build through `NROS_ENTITY_DECLARED_{RELIABILITY,DURABILITY,
  HISTORY}[_PUBLISHER]` with a per-policy, per-kind undeclared count.
- **DONE** (phase-454 W3) — an unknown value is an error, never a skip.
  `reject_unknown_qos_values` refuses at the two places a model reaches
  `from_model`, naming the endpoint, what was written and what is accepted.
  Both roads, because a check on one of two is issue 1199's shape.
- **DONE** (phase-454 W3) — KEEP_ALL refuses at build time, naming the endpoint.
  Stronger than this line asked: it refuses **whether or not** a cap is stated
  beside it, because a `depth:` written next to KEEP_ALL is not a cap on the
  queue (RFC-0100 D6). The refusal is per fact — the depth-derived numbers go,
  the entity counts, the type sets and the policy table itself stay.
- **DONE** (2026-10-01, see Resolution) — the declared-QoS header and boot check
  cover reliability and durability. This one is NOT W3's. `NROS_ASSERT_DECLARED_DEPTH` compares a
  call site's QoS against a generated `(type, topic) -> depth` table, and
  widening that table is the same edit as widening it to the languages it does
  not reach today (`_nros_declared_qos_arm` returns early for Rust and INTERFACE
  targets, and C has no equivalent macro at all). Both halves are phase-454 W10,
  which exists for exactly that surface. Note what this bullet is and is not: a
  code/contract disagreement on reliability or durability is an INTEROP failure
  (an incompatible-QoS match never delivers), not a sizing one, so it does not
  hold up any of the derivations above.

## Resolution (2026-10-01)

The last bullet closed, in all three languages, for SUBSCRIPTIONS -- the scope
the header and the boot check have always had.

**The table.** `nros_declared_qos_generated.h`'s rows gained two columns,
`reliability` and `durability`, beside the depth
(`EntityInventory::declared_qos_header_table`, which joins the depth view with
the policy view per `(type, topic)`). A row exists when ANY column is stated,
so a subscription that declares `reliability: best_effort` and no depth gets
one, with the depth column `-1`. The policy columns are bare TOKENS
(`NROS_DQ_RELIABLE`, `NROS_DQ_BEST_EFFORT`, `NROS_DQ_VOLATILE`,
`NROS_DQ_TRANSIENT_LOCAL`, `NROS_DQ_UNDECLARED`) because C and C++ number those
enums differently (`NROS_QOS_RELIABILITY_RELIABLE` is 1, `nros::Reliable` is
0): each header pastes the token onto a prefix of its own, and
`nros/component.hpp` / `nros/subscription.h` `static_assert` the literals equal
to the enumerators, so a drift is a compile error rather than a check against
the wrong value. `system_default` is not a column value -- it says "the
middleware chooses", which no call site can disagree with. `keep_all` now
refuses the DEPTH column only (RFC-0100 D6, per fact): before, it took the
whole table with it.

**C++.** `NROS_ASSERT_DECLARED_RELIABILITY` / `_DURABILITY` beside the depth
macro, and `NROS_ASSERT_DECLARED_QOS` for all three -- what `NROS_SUBSCRIBE`'s
four-argument form now asserts. The three-argument form fills every declared
column in (`detail::qos_from_declared`). The boot-time half is
`Node::check_declared_qos` (ledger row added), which `create_subscription_in`
and `_in_group` now call; a policy disagreement goes through `set_error` as
`DECLARED_QOS_MISMATCH` (-403, the depth's code).

**C.** `NROS_DECLARED_RELIABILITY` / `NROS_DECLARED_DURABILITY` and the two
`NROS_ASSERT_DECLARED_*` macros, in `nros_qos_t` values, with the values in the
diagnostic as the sizes of a conflicting `extern char` pair (offset by one, so
0 is legal; the identifier spells the legend). A C COMPONENT's `nros_cpp_qos_t`
numbers reliability the C++ way, so it is held at registration instead.

**Rust / every FFI seam.** `DECLARED_QOS_ROWS` is now a table of
`declared_qos::DeclaredEndpoint { depth, reliability, durability }`, each
optional. The policies arrive on the SIZING DESCRIPTOR only -- no env knob was
added; the descriptor already states them (0460/0491). `check` (the C/C++ FFI
seams, which took the declaration at their call site) refuses any disagreement,
with `NodeError::DeclaredQosMismatch` for a policy. `honour` (every Rust
registration) applies the depth's rule to the policies, ordered by how much
each ASKS FOR: a weaker declared value (best_effort, volatile) is TAKEN and
reported, a stronger one REFUSED, and a call site that asked `SystemDefault`
takes the declaration silently.

**Gates, each with its negative control measured red.**
`just check declared-qos-header` grew cases F3 (C++) and F4 (C): a contract
stating `best_effort` + `transient_local`, an agreeing call site, and one
disagreeing call site per policy, each of which must be OUR rejection naming
the topic and both values (49 assertions in all). Planted mutations -- the
C++ reliability lookup always answering UNDECLARED, the C one likewise -- each
turned exactly that case red. `just check declared-qos-registration` now
requires exactly THREE declared-image tests on the descriptor carrier (the
third reads `/status`'s declared `best_effort` + `volatile` from
`tests/fixtures/declared-qos-descriptor.toml`) and two on the depth-only env
carrier; with `build.rs` dropping the descriptor's policy columns the
descriptor step went red at `2 passed`.

**Measured on a real image** (issue 1564 is the adopter): with
`examples/workspaces/cpp`'s contract changed to `{ depth: 1, reliability:
best_effort }` against `Listener.cpp`'s `nros::QoS(1)`, the native workspace
build fails in `Listener.cpp` with `declared_reliability_agrees<1, 0>` and the
topic `"/chatter"`.

**What it does not cover, stated rather than implied.**

* PUBLISHERS. The table, the boot check and the registration check are
  subscription-only, for depth as well as policies; a publisher needs a kind
  column and a C++ macro seam that does not exist. Filed as issue 1608.
* The Rust half on a MULTI-ENTRY cmake configure. Such a configure names no
  sizing descriptor to cargo (phase-457 W0.c, permanent), and the descriptor is
  the policies' only carrier, so there the Rust registration checks the depth
  and not the policies. The C/C++ compile-time table is per component and does
  check them there (issue 1564).
