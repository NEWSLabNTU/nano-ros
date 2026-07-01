# Phase 272 — Unified config-driven sched-context binding

Implements **[RFC-0047](../design/0047-unified-sched-context-binding.md)**. Resolves
**[#121](../issues/0121-rclcpp-shape-cpp-nodes-not-sched-bound.md)** and refactors the per-shape
tier binding landed in phase-269 W4 (#119) into one mechanism.

## Why

`[tiers]` binding is fragmented across four language/shape paths (Rust `run_tiers`, Rust codegen
per-callback bind, C/C++ configure `NodeBuilder::sched`, rclcpp-shape = nothing → #121). All four are
the same underlying act — set a node's `default_sched` before its callbacks register
(`apply_node_default_sched`). Since tier resolves per-node in config (`nros-orchestration-ir`) and
**every** node funnels through `Executor::node_builder(name).build()` (RFC-0046 — incl. rclcpp via
`ComponentNode`→`Node::create`→`nros_cpp_node_create`→`node_builder`, phase-269 W2c), a single
config-seeded `node_name → sched_context` table looked up at that one site covers every path by
construction. See RFC-0047 for the full rationale.

## Waves

### W1 — the table + the `node_builder` lookup (core, language-agnostic)
**Files:** `packages/core/nros-node/src/executor/spin.rs` (+ `node_record.rs`): add a bounded
`node_name → SchedContextId` table to `Executor` (sized by `NROS_EXECUTOR_MAX_NODES`, `no_std`,
zero-alloc; empty ⇒ everything stays `SchedContextId(0)`); a `bind_node_name_sched(name, sc_id)`
seeder; and make `NodeBuilder::build()` consult the table by the node's (namespaced) name, setting
`default_sched` when present — **unless** an explicit `.sched()` override was given (precedence:
explicit `.sched` > table > `SchedContextId(0)`, mirroring RFC-0046 identity precedence).

- **Acceptance:** unit tests on `Executor` — seed `("talker", 2)`, `node_builder("talker").build()` →
  the node's `default_sched == 2` and a callback registered under it binds to SC 2; an unseeded name →
  SC 0; an explicit `.sched(1)` beats a table entry of 2. `cargo test -p nros-node` green.
  Namespaced key: `("/ns", "talker")` disambiguates.

### W2 — FFI seeder + emit the seed (drop the per-shape binding)
**Files:** the nros-cpp sched shim (`sched_shim.rs`) + `nros-c`: `nros_cpp_bind_node_name_sched(executor,
name, sc_id)` over the shared `Executor` (mirror the W0 lifecycle/param shims). `emit_c.rs`/`emit_cpp.rs`:
in the boot setup, AFTER creating the sched-contexts and BEFORE constructing/configuring components,
emit one `bind_node_name_sched(__exec, "<name>", __nros_sc_ids[idx])` per node with a resolved
`sched_context`; and **remove** the per-shape `NodeBuilder::sched()` / `nros_cpp_node_create_ex`
tier-binding branches (the builder now resolves the tier itself). Guard so a single-tier (no
`[tiers]`) entry emits nothing new + stays byte-identical.

- **Acceptance:** emit unit test — a multi-tier `Plan` → the generated TU seeds the table before
  configure and NO LONGER emits `NodeBuilder::sched`/`node_create_ex` for tiering; single-tier plan
  byte-identical. `just check` green.

### W3 — rclcpp-shape realtime fixture + e2e (proves #121 dissolved)
**Files:** an rclcpp-shape variant in a realtime workspace — extend `ws-realtime-cpp` (or a new
`ws-realtime-cpp-rclcpp`) with an IS-A-node (`: ComponentNode(h, "ctrl")`) component on a tier + a
`fixtures.toml` entry + an e2e asserting the rclcpp-shape node schedules on its tier (mirror
`realtime_tiers_cpp_e2e`). This is the case #121 flagged as broken; it must now pass with NO
`NodeHandle` change.

- **Acceptance:** the rclcpp-shape realtime e2e schedules the node on its high/low tier (built + run
  locally, or compile + `skip!` cleanly if unprovisioned — a skip is not a pass). Existing
  `realtime_tiers_{c,cpp}_e2e` still pass (the binding moved but the observable scheduling is
  unchanged).

### W4 (optional / decide in-phase) — migrate the Rust binding to the table
Per RFC-0047 open-question 2: have `nros::main!` / the Rust codegen seed the same table instead of
per-callback `bind_handle_to_sched_context`, so all languages share one binding mechanism. Keep
`run_tiers` for the spin structure. Gate on whether it simplifies without regressing the Rust
realtime e2e; if it adds risk for little gain, defer + note it.

## Sequencing
W1 (core table + lookup) → W2 (FFI seed + drop per-shape emit) → W3 (rclcpp e2e proof). W4 optional,
last.

## Acceptance (phase)
- One binding mechanism: a config-seeded `node_name → sched_context` table resolved at the single
  `node_builder(name)` site; no per-shape emit; no `NodeHandle` sched field.
- #121 resolved — rclcpp-shape nodes schedule on their tier, proven by an rclcpp realtime e2e, with
  no `NodeHandle` ABI change.
- Existing C/C++/Rust realtime e2e unchanged (observable scheduling identical); single-tier entries
  byte-identical.

## Risks / decisions
- **Seed ordering:** the table MUST be seeded before the first `node_builder(name).build()`. In the
  entry that means seeding right after sched-context creation, before configure/construct — verify
  the emit order in W2.
- **Namespaced identity:** key by fully-qualified name to avoid collisions (RFC-0047 OQ3).
- **Precedence with explicit `.sched()`:** keep the programmatic override winning (direct-API users);
  the table is the config-driven default.
- **Execution unchanged:** this is binding only; `run_tiers` / per-RTOS priority (RFC-0016) is
  untouched — do not conflate.
