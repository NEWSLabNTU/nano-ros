# Phase 273 — Callback-group sched binding (code-declared groups, config-assigned tiers)

Implements the revised **[RFC-0047](../design/0047-unified-sched-context-binding.md)** (per-callback-
group binding). Builds on phase-272 (the per-**node** `node_name → sched_context` table, landed) and
generalizes it to per-**callback-group**: a first-class group API in code (rclcpp/rclrs shape), the
group→tier assignment moved out of the package manifest into `system.toml` (by-name), and a
`group → sched_context` table the executor binds at registration — uniform across Rust, C, C++.

## Why

phase-272 unified tier binding at the node level (one tier per node). But the Rust path is
per-callback (a node can split a fast control loop and slow telemetry across tiers), and the current
group→tier binding lives in the **package manifest** hardcoding workspace tier names — non-portable
(RFC-0026: packages are copy-out). ROS 2 declares groups **in code** (structure/concurrency) and
assigns them to executors/priorities at **composition/deployment** — never in the package. This phase
adopts that split: groups in code, tiers in `system.toml`. See RFC-0047 (Motivation + Design).

## Waves

### W1 — executor `group → sched_context` table + bind-by-group (core, language-agnostic)
**Files:** `packages/core/nros-node/src/executor/{spin.rs,node_record.rs,node.rs}`.
- Add a bounded `group_sched_table` keyed by `(fully-qualified node name, group name)` →
  `SchedContextId` (sized like the node-name table; `no_std`, zero-alloc; empty ⇒ no per-group
  binding). A `bind_group_sched(node, group, sc)` seeder (mirror phase-272 `bind_node_name_sched`).
- Entity/callback registration grows an optional **group name**; `apply_node_default_sched` gains a
  group override: precedence **group table > node-name table / node default > `SchedContextId(0)`**
  (RFC-0047 Precedence).
- **Acceptance:** unit tests — seed `(("node","/"),"ctrl") → SC2`; a callback registered under node
  "node" with group "ctrl" binds to SC 2; a callback with no group / unmapped group → the node
  default (phase-272 behavior, unchanged); a second group "telem"→SC3 on the SAME node splits its
  callbacks (SC2 + SC3) — the sub-node capability. `cargo test -p nros-node` + `--no-default-features`
  green.

### W2 — `system.toml` group→tier schema + resolver + entry seed (config side)
**Files:** `packages/cli/nros-cli-core/src/orchestration/{cargo_metadata_schema.rs,tier_resolver.rs}`,
the entry `Plan` IR + emit (`codegen/entry/`), `nros-orchestration-ir`.
- **Schema:** add `group_tiers: BTreeMap<String,String>` (group → tier name) to the `[[component]]`
  entry in `system.toml` (RFC-0047 OQ2). **Remove** the package-manifest `callback_groups` group→tier
  binding as the source of truth (the manifest no longer carries tiers — groups come from code).
- **Resolver:** `(component, group) → tier → sched_context` from `group_tiers` + `[tiers.*]` (extend
  the shared `nros-orchestration-ir` resolver the macro + C/C++ emitters already consume; unmapped
  group → default tier).
- **Entry emit:** seed the `group → sched_context` table (`bind_group_sched(node, group, sc)` per
  resolved binding) in boot setup **before** entities register — the per-group analog of the phase-272
  node-name seed. FFI seeder `nros_cpp_bind_group_sched(...)` in the nros-cpp shim.
- **Acceptance:** resolver unit test (`group_tiers` + `[tiers]` → expected group→SC); emit test (a
  multi-tier `Plan` with `group_tiers` → the TU seeds `bind_group_sched` before construction);
  `just check` green. Migrate the `ws-realtime-*` fixtures' group→tier from the package manifest into
  `system.toml [[component]].group_tiers` (portability: a node's manifest no longer names a tier).

### W3 — first-class callback-group API (code side, all languages)
**Files:** `nros-node` (Rust API), `nros-cpp`/`nros-c` (C/C++ API + the entity register FFI).
- **Rust:** `node.create_callback_group("ctrl") -> CallbackGroup` + `create_*_in(&group, …)` (or
  entity-builder `.callback_group(&g)`), rclrs-shaped. The entity registration carries the group name
  to the executor (W1). Replaces the ad-hoc `node.callback_group("ctrl")` labeling.
- **C++:** `create_callback_group("ctrl")` on `ComponentNode`/`Node` + a group arg/option on
  `create_timer`/`create_subscription`/… (rclcpp-shaped). Thread the group name across the
  `nros_cpp_*_register` FFI.
- **C:** a group handle + group-scoped create (the register FFI carries the group name).
- Default group when omitted (backward-compatible — existing entities keep the node default).
- **Acceptance:** the register FFI carries the group; unit/compile tests per language; `just check`
  green. No manifest group list anywhere.

### W4 — wire end-to-end + migrate Rust + sub-node e2e (the proof)
**Files:** the Rust `nros::main!`/codegen path; a sub-node realtime fixture + e2e.
- **Rust migration:** route the Rust binding through the group table (seed `bind_group_sched` from
  config; entities created in named groups carry them; drop the bespoke per-callback
  `bind_handle_to_sched_context` loop). Keep `run_tiers` for the spin structure.
- **Sub-node fixture + e2e:** a workspace where ONE node has TWO groups on TWO tiers (e.g. a node with
  a 10 ms `ctrl` timer on high + a 100 ms `telem` timer on low), across Rust + C++; assert the two
  callbacks schedule at their distinct cadences (the capability the per-node table couldn't express).
- **Portability check:** copy a group-using node package into a second workspace whose `system.toml`
  names tiers differently and binds the group there → it schedules correctly with **no package
  change** (the RFC-0047 coupling removed). Assert via the fixture build + e2e.
- **Acceptance:** the sub-node e2e passes (built + run) across Rust + C++; the portability fixture
  builds + binds; existing `realtime_tiers_{c,cpp,cpp_rclcpp}_e2e` still pass (node-level case
  unchanged). `just check` green.

## Sequencing
W1 (executor table + bind-by-group) → W2 (config schema + resolver + seed) → W3 (first-class group
API + FFI threading) → W4 (Rust migration + sub-node e2e + portability). Each wave independently
green + landable; the observable node-level scheduling stays identical throughout (phase-272 e2e as
the regression guard).

## Acceptance (phase)
- Callback groups are first-class, code-declared (rclcpp/rclrs shape), uniform across Rust/C/C++;
  entities are created in a group; the default group applies when omitted.
- Group→tier is owned by `system.toml` (by-name), NOT the package manifest — a group-using package is
  portable across workspaces (proven by the portability fixture).
- One binding mechanism: the config-seeded `group → sched_context` table, bound at registration,
  subsuming the phase-272 node-name table as the default-group case.
- Sub-node tiering (a node with callbacks on different tiers) works across languages; node-level
  scheduling unchanged (phase-272 e2e green).

## Risks / decisions
- **Migration compatibility:** removing the manifest group→tier binding changes the config surface —
  migrate the in-tree `ws-realtime-*` fixtures in W2; document the `system.toml group_tiers` move
  (book/AGENTS as needed). Manifest `callback_groups` (id-only, if kept for discovery) vs nothing —
  prefer nothing (groups from code), decide in W2.
- **FFI group threading:** the entity register FFI gains a group-name arg — an append to the C/C++
  entity-create surface; keep it optional (NULL/empty ⇒ default group) so existing callers are
  byte-compatible.
- **Concurrency type deferred:** MutuallyExclusive/Reentrant is a follow-up (RFC-0047 OQ1) — the group
  object carries only name + tier binding this phase; don't build the concurrency semantics yet.
- **Node-name table stays:** phase-272's node-name table remains as the default-group/degenerate path
  — do NOT remove it; the group table layers on top.
