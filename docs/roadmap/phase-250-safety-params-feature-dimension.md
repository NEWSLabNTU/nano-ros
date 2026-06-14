# Phase 250 — safety + params as a config-driven, language-agnostic feature dimension

Status: **Planned (2026-06-15)** · Motivated by phase-249 P3.5b (declarative native
example migration) · Related: RFC-0031 (declared selection → lowered build feature),
RFC-0024 (declarative Node/Entry).

## Why

The native Rust examples historically carried per-example **Cargo feature gates** —
`param-services` (declare a `start_value` parameter + read it for the publish counter)
and `safety-e2e` (CRC + sequence-gap tracking on a subscription) — plus `link-tls` and
`unstable-zenoh-api` (zero-copy) variants. Fixtures built each example *with the feature
on* into an isolated `target-*/` dir, and `params.rs` / `safety_e2e.rs` ran e2e against
those variant binaries.

Phase-249 P3.5b migrated the native examples to the declarative Node + `nros::main!()`
composable-node shape, where **RMW is a config/board choice, not example source**. The
same principle should apply to safety + params: they are **application capabilities a
user toggles in config**, not reasons to fork the example source or maintain
`#[cfg(feature = …)]` variants. ROS expresses these as node configuration, not as
separate programs.

## Direction (user, 2026-06-14)

Turn `param-services` and `safety-e2e` into a **declared, language-agnostic feature
dimension**:

- The user **selects whether to enable** them in **config files** (the Entry pkg's
  `[package.metadata.nros.*]` / system bringup config), not via a per-example Cargo
  feature in user source.
- The selection **lowers** to a build feature (the RFC-0031 pattern used for RMW): one
  declared axis → the codegen/build glue enables the capability.
- **No separate examples.** One declarative example; **fixtures build it with the
  dimension on and off** (the manifest carries the on/off variants), and the e2e tests
  consume the matching fixture.

This mirrors how RMW is already declared (Entry `deploy` / `rmw`) and lowered via
`resolve_rmw()` — safety + params become sibling declared axes.

## Scope

- **Declared axis.** Add a `[package.metadata.nros.*]` (or system-config) knob for
  `params` (+ default parameter set) and `safety` (CRC/seq integrity), language-agnostic.
- **Lowering.** Map the declared axis → the build feature(s)/codegen that wire
  `declare_parameter` (the declarative Node API at `nros/src/node.rs:1412`) and the
  `.safety()` / `.message_info()` subscription builders (`nros-node/.../node.rs:1765`)
  into the generated node — without the user writing `#[cfg]`.
- **Fixtures.** `fixtures.toml` builds the migrated talker/listener with the dimension
  **on and off** (replacing the retired per-example `param-services` / `safety-e2e` /
  `link-tls` / `unstable-zenoh-api` rows).
- **Tests.** Re-enable `params.rs` + `safety_e2e.rs` (skipped in P3.5b) against the
  on/off fixtures.
- **Optional:** fold `link-tls` (transport) + `unstable-zenoh-api` (zero-copy) into the
  same declared-axis treatment, or keep them transport-config knobs.

## Interim state (set by phase-249 P3.5b)

- The migrated native examples carry **no** `param-services` / `safety-e2e` / `link-tls`
  / zero-copy source gates.
- Their `fixtures.toml` variant rows are **removed**; `params.rs` + `safety_e2e.rs` (and
  the tls / zero-copy e2e tests) are **`skip!`-gated** with a pointer here.
- This phase restores the capability as a declared dimension.

## Acceptance

- A user enables params / safety via config (no source edit); the build lowers it; the
  declarative example gains the behavior.
- `fixtures.toml` builds the example with the dimension on + off; `params.rs` /
  `safety_e2e.rs` are un-skipped and green against the respective fixtures.
