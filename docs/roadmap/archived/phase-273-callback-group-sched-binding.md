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

**Landed:**
- **NodeSpansTiers constraint lifted** (`nros-orchestration-ir`): removed the v1 rule that pinned a
  whole node to one tier; the resolver now allows `group_tiers` entries for one node across multiple
  tiers. Unit test updated (`node_spanning_tiers_is_allowed`). Issue #124 dissolved.
- **C++ sub-node fixture + e2e** (`ws-realtime-cpp-subnode`): ONE `subnode_pkg::SubNode`
  (`nros::ComponentNode` subclass) declares two callback groups in its constructor (`ctrl` at 10 ms,
  `telem` at 100 ms). `system.toml group_tiers = { ctrl = "high", telem = "low" }` maps them to the
  two tiers. Generated entry seeds `bind_group_sched` for both groups before construction.
  E2E result: ctrl=49 telem=5 ratio=9.8× (≥3× required). Test: `realtime_subnode_cpp_two_groups_on_two_tiers`.
- **Portability fixture + e2e** (`ws-realtime-cpp-subnode-portable`): identical `subnode_pkg`
  source, second workspace uses tier names `fast`/`bulk`. No package change. Test:
  `realtime_subnode_cpp_portable_two_groups_bind_renamed_tiers`.
- **Existing regression tests** (`realtime_tiers_{c,cpp,cpp_rclcpp}_e2e`) rebuilt + passing.
- **Resolver test** (`resolve_system_tiers_sub_node_two_groups_two_tiers` in `nros-cli-core`).

**Deferred to issue #125:**
- **Rust entry group-seed:** the `nros::main!` multi-tier path does not emit `bind_group_sched`
  for Rust nodes. The Rust `run_tiers` mechanism already handles multi-tier dispatch via
  `active_groups` filtering (each tier's executor admits only its groups). The `bind_group_sched`
  would add finer-grained per-group SchedContext assignment within a single-executor path —
  entangled because SchedContextId values are runtime-assigned by the board, not available at
  macro-expansion time. Design direction and workaround recorded in issue #125.

- **Acceptance:** the sub-node e2e passes (built + run) for C++; the portability fixture
  builds + binds; existing `realtime_tiers_{c,cpp,cpp_rclcpp}_e2e` still pass (node-level case
  unchanged). Rust group-seed cleanly deferred to issue #125. `just check` green.

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

## Outcome (2026-07-02) — DONE (single-Executor model; execution converges in phase-274)

| Wave | Commit | Result |
| --- | --- | --- |
| W1 core | `5b9b3bcf9` | executor `group_sched_table` + `bind_group_sched` + `apply_node_default_sched(…, group)` (precedence group > node > default); sub-node-split unit tests |
| W2 config | `9f726c1a2` | `system.toml [[component]].group_tiers` (group→tier off the package manifest); resolver + `bind_group_sched` FFI + C/C++ entry seed; `ws-realtime-*` migrated |
| W3 API | `90f9be998` | first-class `create_callback_group` + `create_*_in` (Rust/C/C++); group threaded through the register FFI |
| W4 proof | `155296769` | **sub-node e2e PASS** (`ctrl:telem = 9.8×`, one node → two tiers); **portability PASS** (same package, renamed tiers, no package change); `NodeSpansTiers` v1 guard lifted; Rust seed deferred → #125 |

Callback grouping is now the durable user surface across all languages (code-declared groups +
`system.toml group_tiers`, portable). Delivered on the **single-`Executor` + `sched_context`** backend;
the **execution model converges onto RFC-0015 Model 1** (per-tier executors + gating) for C/C++ in
**phase-274**, at which point `sched_context` is re-scoped to fallback + intra-tier (RFC-0047
reconciliation) and sub-node becomes Model-1 v2. Open: **#125** (Rust entry group-seed).

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
