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

**Consequence: phase-250 ⊇ the declarative-node migration** (the withdrawn P3.5b payload,
re-scoped). The lowering (Layer 1) is the easy RMW-mirror; the real cost is migrating
talker/listener off imperative `#[cfg]`-forked mains onto `nros::main!()` + the `Node`
trait so behavior is a spec, not forked source.

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
   the greenfield-conversion framing above.
2. **Declarative-node migration (prereq).** Migrate `examples/native/rust/{talker,listener}`
   to `nros::main!()` + the `Node` trait so callbacks/params are a spec, not `#[cfg]`-forked
   `main()`s. (Re-scopes the withdrawn P3.5b — see RFC-0024.)
3. **Declared axis (Layer 1).** Add a `[package.metadata.nros.*]` / `system.toml` knob for
   `params` (+ default parameter set) and `safety` (CRC/seq integrity), language-agnostic.
4. **Lowering (Layer 1).** `nros_capability_features()` maps the declared axis → the
   entry/component `nros` dep feature(s), mirroring `board_rmw_features()`.
5. **Codegen wiring (Layer 2).** Generate `declare_parameter` (from the param set) +
   `.safety()` wiring (from the flag + a `Node`-trait hook) into the declarative node.
6. **Fixtures.** Replace the per-feature variant rows (`fixtures.toml:370-412`) with
   **on/off** variants of the ONE declarative example.
7. **Tests.** Repoint `params.rs` / `safety_e2e.rs` (and the zero-copy test if folded) at
   the on/off fixtures. (They are already active — this is a repoint, not an un-skip.)

## Acceptance

- A user enables params / safety via config (no source edit); the build lowers it to the
  `nros` feature; the declarative example gains the behavior.
- `fixtures.toml` builds the example with the dimension on + off; `params.rs` /
  `safety_e2e.rs` pass against the respective fixtures.
- Embedded targets pay the safety/param code-size cost **only** when the axis is selected
  (verified: the capability stays a compile feature, not a runtime always-on path).

## Risks

- **Coupling to the declarative migration.** Layer 2 cannot land before talker/listener are
  declarative; if that migration slips, Layer 1 (the lowering) can still land standalone but
  delivers only "capability compiled in", not "no source edit".
- **Safety callback is not pure config.** The `on_integrity` hook keeps app logic in source;
  config only toggles whether it is wired. Don't over-promise "fully declarative safety".
