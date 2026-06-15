# Phase 250 — safety + params as a config-driven, language-agnostic feature dimension

Status: **Planned (2026-06-15)** · Related: RFC-0031 (declared selection → lowered
build feature), RFC-0024 (declarative Node/Entry).

## Why

The native Rust examples carry per-example **Cargo feature gates** — `param-services`
(declare a `start_value` parameter + read it for the publish counter) and `safety-e2e`
(CRC + sequence-gap tracking on a subscription) — plus `link-tls` and
`unstable-zenoh-api` (zero-copy) variants. Fixtures build each example *with the feature
on* into an isolated `target-*/` dir, and `params.rs` / `safety_e2e.rs` / `zero_copy.rs`
run e2e against those variant binaries.

These are **application capabilities a user should toggle in config**, not reasons to
fork the example source or maintain `#[cfg(feature = …)]` variants. ROS expresses them
as node configuration, not as separate programs. The aim mirrors how RMW is already
declared (`system.toml [system].rmw`) and lowered (RFC-0031): make `params` and `safety`
**sibling declared axes**.

## Ground truth (2026-06-15) — corrects the prior premise

An earlier draft assumed phase-249 P3.5b had already migrated the native examples to the
declarative shape and removed these gates. **P3.5b was withdrawn.** Nothing was removed.
Current state:

- `param-services` / `safety-e2e` / `link-tls` / `unstable-zenoh-api` are **all still
  declared and used** in `examples/native/rust/{talker,listener}/Cargo.toml` +
  `src/main.rs`.
- `examples/fixtures.toml:370-412` still carries the per-feature variant rows
  (`param-services`, `link-tls` → `target-tls`, `safety-e2e` → `target-safety`, zero-copy
  → `target-zero-copy`).
- `params.rs` / `safety_e2e.rs` / `zero_copy.rs` are **active, never skip-gated**.
- Only `examples/native/rust/entry-poc` uses the declarative `nros::main!()` shape; every
  real example is still the imperative builder.

So phase-250 is a **greenfield conversion** of working feature-gated examples, not a
restore of something removed.

## Design — two separable layers

The feature gate today does **two** things, and only one is a clean config-lowering:

### Layer 1 — capability compilation (easy; mirrors RMW)

Declared `params` / `safety` → lower to a build feature, exactly as RMW lowers
`[system].rmw` → the board crate's `rmw-<x>` feature. **Simpler than RMW:** these are
RMW-agnostic `nros` capabilities, so they lower straight to the entry/component pkg's
`nros = { features = [...] }` dep — no board-crate indirection. Add a
`nros_capability_features()` beside `board_rmw_features()`
(`packages/cli/nros-cli-core/src/orchestration/generate.rs`). This is what gives embedded
its cost-gate: the safety arena entries + CRC and the param-service handlers are compiled
in **only when selected**, so it must stay a **compile** dimension (not a runtime
always-on toggle — code size matters on no_std).

### Layer 2 — application wiring (hard; needs declarative nodes)

The gate also **swaps the example's own source**. `listener/src/main.rs:54-139` is **two
whole `main()` bodies** (`#[cfg(safety-e2e)]` vs `not`) with *different callbacks*; talker
has `#[cfg]` blocks that call `register_parameter_services()` + `declare_parameter()`. A
config flag **cannot inject those calls into hand-written imperative `main()`** — it can
only pick a prewritten `#[cfg]` branch. To reach "user enables in config, **no source
edit**", the node must be **declarative**: its behavior expressed as a spec that codegen
lowers into the generated entrypoint (the `declare_parameter` API at
`packages/core/nros/src/node.rs:1412`; the `.safety()` / `.message_info()` builders at
`packages/core/nros-node/src/executor/node.rs:1748-1773`).

**Consequence (corrected 2026-06-15 after the D7 re-examination): the declarative target
already exists; the real gap is the declarative *API surface*, not a migration.**
phase-244 **D7 blessed the board-less imperative `Executor::open` shape for the native
single-file `examples/native/rust/{talker,listener}`** as the *intended* native shape (not
a leak) and deliberately did **not** migrate them — P3.5b was withdrawn for re-migrating
them against that decision. Meanwhile declarative talker/listener **already exist**:
`examples/workspaces/rust/{talker_pkg,listener_pkg,native_entry}` (+ `demo_bringup/system.toml`)
and the esp32 single-pkg pair, across 8 platforms on `nros::main!()` + `Node`/`ExecutableNode`.
So the Layer-2 prerequisite is **not** "migrate the native examples" (forbidden by D7) — it is
**extending the declarative `Node`/`NodeContext` API** to expose params + safety, which it does
not today: `NodeContext::create_subscription_for_callback_name()` yields only basic subscriptions,
and `declare_parameter` / `.safety()` / `.message_info()` are **imperative-`Executor`-only**
(`packages/core/nros/src/node.rs:1412`, `packages/core/nros-node/src/executor/node.rs:1748-1773`).
The native imperative examples keep their `#[cfg]` Cargo features as the D7-blessed imperative
idiom — phase-250 does not touch them.

## Per-axis tractability

- **params — fully config-expressible.** A declared default-param set (e.g.
  `start_value = 42`) → codegen emits `declare_parameter` + `register_parameter_services`;
  the node body reads the value via the runtime API. Clean.
- **safety — only partially.** The `.safety()` builder wiring (enable integrity on
  subscription X) is config-expressible, but the **integrity callback carries app logic**
  (what to do on a gap/dup) — pure config can't express arbitrary bodies. Scope safety to
  "wire the builder + a `Node`-trait hook (e.g. `on_integrity(status)`)", **not** "generate
  arbitrary callbacks".
- **link-tls / unstable-zenoh-api — out of scope here.** These are **transport / RMW-backend**
  features (`nros-rmw-zenoh?/link-tls`; zero-copy `nros/unstable-zenoh-api`), not node
  capabilities. They belong with the transport/RMW declared axis. Keep them as transport-config
  knobs; do **not** fold them into params/safety.

## Scope

1. **Correct premise — DONE (this doc).** The "removed, restore" framing is replaced with
   the greenfield-conversion framing above; the D7 re-examination further replaces the
   "migrate the native examples" prerequisite with "extend the declarative API surface".
2. **Declared axis + lowering (Layer 1).** A declared `[safety]` / `[params]` knob lowers to
   the entry's `nros` dep feature, beside the existing `[param_persistence]`/`[lifecycle]`
   paths in `generated_default_features()`. (Safety half **DONE** — Wave 1.)
3. **Extend the declarative `Node`/`NodeContext` API (Layer 2 prereq).** Add params + safety
   to the declarative path that today only the imperative `Executor` carries: a
   `declare_parameter`-equivalent usable from `Node::register()`, and an integrity-status
   surface on `create_subscription_for_callback_name()` (the `.safety()` analog) feeding a
   `Node`-trait hook. This is core `nros-node` API work — **not** example migration (the
   declarative talker/listener already exist; the native imperative ones are D7-blessed).
4. **Codegen wiring (Layer 2).** Lower declared `[params]` (the default param set) +
   `[safety]` into the generated declarative node via the new API.
5. **Target a declarative example.** Add the config-driven safety/params variant on the
   **already-declarative** `examples/workspaces/rust/` (or esp32) talker/listener — never the
   D7-blessed native imperative pair.
6. **Fixtures.** Add **on/off** variants of the declarative example. The native imperative
   per-feature rows (`fixtures.toml`) stay as-is (they exercise the imperative API, which D7
   keeps); this **augments**, it does not replace them.
7. **Tests.** Add declarative on/off coverage; `params.rs` / `safety_e2e.rs` / `zero_copy.rs`
   stay pointed at the imperative fixtures (the imperative API still ships). Augment, not repoint.

## Waves

- **Wave 1 — safety lowering (Layer 1) — DONE (2026-06-15).** A declared `[safety]`
  overlay block lowers to the `nros/safety-e2e` umbrella feature on the generated entry,
  mirroring the existing `[param_persistence] → nros/param-services` and
  `[lifecycle] → nros/lifecycle-services` paths in `generated_default_features()`
  (`packages/cli/nros-cli-core/src/orchestration/generate.rs`). Wiring:
  `collect_safety()` (planner) reads the block (last-overlay-wins; `enabled = false`
  disables; `crc` defaults true) → `NrosPlan.safety: Option<PlanSafety>` (additive,
  skip-when-absent → byte-identical plans) → `generated_default_features(.., safety, ..)`
  pushes `nros/safety-e2e`. Tests: `collect_safety_reads_block_with_defaults` (planner),
  `safety_axis_lowers_to_nros_feature` (generate). `params` is **not** in Wave 1 — it
  already has a lowering path via `[param_persistence]`; a plain `[params]` (declare-only,
  no persistence) axis lands with the Layer-2 codegen wave. Layer 1 alone is not yet
  observable end-to-end (the imperative examples gate on their *own* Cargo feature, a
  different namespace) — it is the foundation the later waves consume.

  **Schema (`[safety]`, an nros.toml / `[package.metadata.nros]` overlay block):**
  ```toml
  [safety]
  enabled = true   # optional, default true; false drops the capability
  crc     = true   # optional, default true; CRC-32 check alongside seq gap/dup tracking
  ```

- **D7 re-examination — DONE (2026-06-15).** phase-244 D7's Shape-B *mechanism prose* is
  stale post-P4b (it cites the linkme `RMW_INIT_ENTRIES` section; P4b replaced it with the
  `.init_array` ctor — the `#[used] __FORCE_LINK_*` static now anchors the ctor object, same
  DCE role, still not a `register()` call). But D7's **substantive** decision — the native
  single-file talker/listener stay imperative `Executor::open`, do **not** migrate — is
  P4b-independent and **stands**. So the planned "declarative migration" wave is **dropped**:
  the declarative talker/listener already exist (`examples/workspaces/rust/`, esp32), and the
  real Layer-2 prerequisite is extending the declarative Node API (next).
- **Wave 2 (planned, revised)** — extend the declarative `Node`/`NodeContext` API: a
  `declare_parameter`-equivalent callable from `Node::register()`, and an integrity-status
  surface on `create_subscription_for_callback_name()` (the `.safety()` analog) → a
  `Node`-trait `on_integrity` hook. Core `nros-node` work, not example migration.
- **Wave 3 (planned)** — params lowering + codegen: a plain declare-only `[params]` axis
  (distinct from the existing `[param_persistence]`) → `declare_parameter` +
  `register_parameter_services` in the generated declarative node.
- **Wave 4 (planned)** — safety codegen (Layer 2): lower the Wave-1 `[safety]` flag into the
  new declarative `.safety()` wiring + the `on_integrity` hook.
- **Wave 5 (planned)** — add on/off fixtures for the declarative `examples/workspaces/rust/`
  talker/listener + declarative tests. The native imperative fixtures + `params.rs` /
  `safety_e2e.rs` / `zero_copy.rs` stay (the imperative API ships under D7). Augment, not replace.

## Acceptance

- A user enables params / safety via config (no source edit); the build lowers it to the
  `nros` feature; the declarative example gains the behavior.
- `fixtures.toml` builds the example with the dimension on + off; `params.rs` /
  `safety_e2e.rs` pass against the respective fixtures.
- Embedded targets pay the safety/param code-size cost **only** when the axis is selected
  (verified: the capability stays a compile feature, not a runtime always-on path).

## Risks

- **Declarative API surface is the real cost.** Layer 2 needs new params + safety surface on
  the declarative `Node`/`NodeContext` path (today imperative-`Executor`-only). This is core
  `nros-node` API design, not example edits — the bulk of the remaining work. Layer 1 (the
  lowering) stands alone meanwhile but only "compiles the capability in".
- **Two API shapes coexist by design.** The imperative `Executor` (D7-blessed, native) and the
  declarative `Node` path both carry params/safety after this phase; the config-driven story
  is the declarative path only. Don't conflate them or try to delete the imperative surface.
- **Safety callback is not pure config.** The `on_integrity` hook keeps app logic in source;
  config only toggles whether it is wired. Don't over-promise "fully declarative safety".
