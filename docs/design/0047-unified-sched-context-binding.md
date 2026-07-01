---
rfc: 0047
title: "Unified config-driven sched-context binding via a node-name → tier table at the one node_builder site"
status: Draft
since: 2026-07
last-reviewed: 2026-07
implements-tracked-by: [phase-272]
supersedes: []
superseded-by: null
---

# RFC-0047 — Unified config-driven sched-context binding

## Summary

Tier scheduling is written in config (`system.toml [tiers.*]` + per-node `callback_groups`) and
resolves to a `node → sched_context` assignment in one shared place (`nros-orchestration-ir`). But
the **binding** of that assignment — attaching a node's callbacks to their scheduling context — is
today done four different ways across languages and component shapes, and one of them (rclcpp-shape
C++, issue #124) can't be done at all. This RFC replaces all four with **one** mechanism: the entry
seeds a `node_name → sched_context_id` table on the executor at boot, and the single
`Executor::node_builder(name).build()` site every node in every language funnels through
(RFC-0046) looks the node's tier up in that table and sets its `default_sched` automatically. No
`.sched()` calls, no `NodeHandle` sched field, no per-shape emit branches. It is the same
seed-a-table-look-up-by-name pattern already used for launch params (phase-269 W1) and node identity
(RFC-0046).

## Motivation / problem

`[tiers]` binding is fragmented (phase-269 W4 / #119 landed the C/C++ side but per-shape):

| Path | Binding mechanism |
| --- | --- |
| Rust `nros::main!` | `run_tiers` (spin structure) |
| Rust codegen | post-hoc `bind_handle_to_sched_context(callback, sc)` per callback |
| C / C++ configure-shape | `NodeBuilder::sched(sc).build()` → the node's `default_sched` |
| C++ rclcpp-shape (IS-A-node, RFC-0044) | **none** — the node is built inside the component ctor from a `NodeHandle` that carries no sched id (#124) |

Underneath it is all one thing: a callback inherits its **node's** `default_sched` at registration
(`Executor::apply_node_default_sched`). So the tier is a node-level default that must be set **before
the node's callbacks register**. The four paths differ only in *how/when* they set that default, and
the rclcpp path has no seam to set it (the entry never touches the node — the component builds it in
its ctor).

Two structural facts make a single mechanism possible:
- **Config already resolves `node → sched_context`** uniformly (`nros-orchestration-ir`, phase-269 W4).
- **Every node funnels through `Executor::node_builder(name).build()`** — Rust `create_node`, the C
  FFI `nros_cpp_node_create`, C++ configure-shape, AND rclcpp-shape (`ComponentNode` ctor →
  `Node::create` → `nros_cpp_node_create` → `node_builder(name).build()`, since phase-269 W2c made
  the simple create register through the builder). This is the RFC-0046 single-site invariant.

If the node's tier is keyed by **name** and looked up at that single site, every path is covered by
construction — including the one that has no other seam.

## Design

### The table
Add a bounded `node_name → sched_context_id` map to the `Executor` (a small fixed array, sized like
the other name-keyed tables; `no_std`, zero-alloc). Default: empty ⇒ every node keeps
`SchedContextId(0)` (the single-tier degenerate case — byte-identical to today).

### Seed at boot (before any node is built)
The entry, in its boot setup **before** components are constructed/configured, (1) creates the
sched-contexts for the resolved tiers (already emitted by phase-269 W4) and (2) seeds the table:
one entry `(node_name, sched_context_id)` per node with a non-default tier, from the config-resolved
`PlanNode.sched_context`. A single new executor FFI — `nros_*_bind_node_name_sched(executor, name,
sc_id)` (C/C++) + the equivalent `Executor` method (Rust) — mirroring how W1 seeds params by name.
Ordering matters: seed **before** the first `node_builder(name).build()`, so the lookup is populated.

### Look up at `node_builder`
`Executor::node_builder(name).build()` (and the FFI `create_node` paths that reach it) consult the
table: if `name` is present, set `NodeRecord.default_sched` to the mapped SC (unless an explicit
`.sched()` override was given — see precedence). The node's callbacks then inherit it at registration
exactly as today. No caller passes a sched id; the builder resolves it from the seeded config.

### Precedence
Explicit `.sched(id)` on a `NodeBuilder` (the current API, still valid for direct/programmatic use)
wins over the table; the table wins over the default `SchedContextId(0)`. Analogous to how
launch-injected node identity overrides the `NodeOptions` default (RFC-0046).

### What this deletes
- The per-shape emit branches in `emit_c`/`emit_cpp` that call `NodeBuilder::sched()` /
  `nros_cpp_node_create_ex` purely to bind a tier — replaced by the one table-seed emit + the
  builder lookup.
- The `NodeHandle` sched-field workaround that #124 would otherwise need — unnecessary, since the
  rclcpp node's own `node_builder(name)` call hits the table.

### Scope boundary — binding vs execution
This RFC unifies **binding** (which sched-context a node's callbacks use). The **execution** side —
how tiers are actually spun (`run_tiers`, per-RTOS task priorities, RFC-0016) — is orthogonal and
unchanged: the executor still schedules callbacks by their bound `sched_context` at dispatch. On
platforms/entries that need the multi-tier spin structure, `run_tiers` stays; the table only changes
*how the binding is established*, uniformly, ahead of that.

## Alternatives considered

- **Per-shape fixes (status quo + a narrow #124 patch: add `sc_id` to `NodeHandle`).** Rejected as
  the primary design — it keeps four binding paths and the special-casing the config-driven goal
  wants gone; it also spreads the `sc_id` across the `NodeHandle` ABI for one shape. (This RFC makes
  that patch unnecessary.)
- **Bind by callback_group at dispatch (per-entity group id, ROS-executor style).** The most
  ROS-faithful (each callback carries its group; executor maps group → SC at dispatch), but the
  biggest change: entity-creation FFI + every register call grows a group argument, and a
  group → SC table is consulted per dispatch. Deferred — the node-name table gets the config-driven
  unification at a fraction of the blast radius, and node-granularity matches how `[tiers]` +
  `callback_groups` resolve today (a node maps to one tier). Revisit if per-callback-group tiers
  within a single node become a requirement.
- **Post-hoc rebind by node id after construction.** Would require the executor to enumerate a
  node's already-registered callbacks and rewrite their bindings — more moving parts than seeding
  the default before registration, and it fights the "set the node default before its callbacks
  register" grain of the executor.

## Open questions

1. Table capacity — reuse the executor node cap (`NROS_EXECUTOR_MAX_NODES`) since there is at most
   one tier per node. Proposed: yes.
2. Should the Rust `nros::main!` path also drop its per-callback `bind_handle_to_sched_context` in
   favor of the table for consistency, or keep it (it already works + carries the `run_tiers` spin)?
   Proposed: migrate the *binding* to the table for a single mechanism; keep `run_tiers` for the spin
   structure. Settle in phase-272.
3. Namespaced nodes — key by fully-qualified name (`namespace + name`) to disambiguate same-named
   nodes in different namespaces. Proposed: yes, match the liveliness keyexpr's identity.

## Cross-references
- RFC-0015 (rtos-orchestration) + RFC-0016 (rtos-scheduling-features) — the tier model this binds.
- RFC-0046 (launch-authoritative node identity) — the `node_builder(name)` single-site invariant this
  relies on; same seed-a-table-by-name shape.
- RFC-0032 (entry-codegen-pipeline) — the tier resolver + `run_tiers` emission this simplifies.
- #119 (C/C++ tiers, phase-269 W4) — the per-shape binding this unifies.
- #124 (rclcpp-shape not sched-bound) — dissolved by this design.

## Changelog
- 2026-07 — created (Draft). Records the unified node-name → sched-context table at `node_builder`,
  replacing the four per-shape binding paths; grounded in the phase-269 W4 resolver +
  `apply_node_default_sched` + the RFC-0046 single-site funnel. Tracked by phase-272.
