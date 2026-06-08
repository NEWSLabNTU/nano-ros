# Phase 227 — Unified config + RMW selection convergence

**Goal:** Bring code, examples, and the book into line with the unified
configuration + RMW-selection design (RFC-0004, RFC-0031). The design is locked;
this phase closes the gaps where the implementation still uses the older
per-language / Phase-172.K shapes.

**Status:** Planned

**Priority:** Medium — design-of-record is settled (RFC-0004/0031); these are
convergence + doc-sync items, not new design.

**Depends on:** Phase 212 (cargo-native + file consolidation, landed), RFC-0004
(unified config), RFC-0031 (RMW selection & lowering).

## Overview

RFC-0004 makes `system.toml` the universal, single-node-optional system
descriptor and narrows `nros.toml` to the embedded direct-mode runtime file.
RFC-0031 makes RMW a declared, language-agnostic, per-deploy selection that the
toolchain lowers to a cargo feature / `-DNANO_ROS_RMW`. Several pieces are
already true (workspace `system.toml` `rmw`, root-`nros.toml` rejection,
`-DNANO_ROS_RMW`, the cffi vtable runtime). The remaining work is below.

## Architecture

- **Single-node system synthesis:** when a single-node project has no
  `system.toml`, the orchestration layer synthesizes an implicit 1-component
  system from the node manifest + defaults; when present, it reads it the same
  way as a multi-node bringup.
- **RMW resolution:** one resolver applies the RFC-0031 precedence ladder
  (CLI/flag → `[deploy.<t>].rmw` → `[system].rmw` → default `zenoh`) and emits
  the per-language lowering (Rust feature / CMake cache).
- **C/C++ single-node parity:** a `nano_ros_application()` CMake function carries
  the same declared fields (deploy, rmw, domain) Rust expresses in
  `[package.metadata.nros.application]`.

## Work items

### 227.1 — Single-node `system.toml` synthesis + read path  ✅ DONE
Already implemented: `synthesise_self_bringup()` in `nros_config.rs` synthesizes
the implicit 1-component system from node metadata (rmw/domain_id/locator from the
first `[deploy.<t>]` block, default `zenoh`), tagged `BringupSource::SelfBringup`;
covered by `discovers_self_bringup_{component,application}_pkg` tests. No further
work — Wave 1 only adds RMW validation on this path (227.2).
**Files:** `packages/cli/nros-cli-core/src/orchestration/nros_config.rs`.

### 227.2 — RMW resolver + per-language lowering  ✅ DONE
`cargo-nano-ros::rmw_resolver::resolve_rmw` (the shared lower crate, re-exported
from `nros-cli-core::orchestration`) lowers a declared rmw → `{cargo_feature,
cmake_value, c_define_token}` + rejects unknowns. **(a)** load-time validation in
`nros_config.rs` (`InvalidSystemRmw`); **(b)** consumed by `codegen_system.rs`
(`NROS_SYSTEM_RMW_<token>` from the one mapping) and the scaffolder (227.4). 9
unit tests (5 resolver + 1 loader + 4 scaffold... see 227.4).
**Files:** `cargo-nano-ros/src/rmw_resolver.rs`, `nros-cli-core/src/orchestration/{mod,nros_config}.rs`, `cmd/codegen_system.rs`.

### 227.3 — Converge examples onto uniform RMW lowering  🔄 REOPENED (mechanism done)
First closed as won't-do (cycle risk), then **reopened 2026-06-09** — the
"cycle" only applied to placing the force-link in `nros-node`; a **facade**
force-link is cycle-free (`nros-rmw-zenoh`/`-xrce` don't depend on `nros`).

**Done (mechanism + canonical proof):**
- `nros` re-adds the Phase-104.A `?/` forwarding (std/platform/ros/safety/tls →
  backend, inert via `?`).
- `nros` carries `#[used] __FORCE_LINK_{ZENOH,XRCE}` statics → backend
  self-registers; no `main.rs` `register()`.
- `examples/native/rust/talker` converged (zenoh + xrce via umbrella, build+run
  verified; no regression vs the explicit-register build). RFC-0031 updated.

**Propagation — DONE (all 18 native-rust examples converged):** talker, listener,
action-{client,client-async,client-rtic,server,server-rtic},
service-{client,client-async,client-rtic,server,server-rtic}, talker-rtic,
listener-rtic, custom-msg, lifecycle-node, serial-{talker,listener},
custom-transport-{talker,listener}. All route RMW through the `nros` umbrella +
force-link; no `register()`. `link-custom` forwarding added for custom-transport.
Build-verified (zenoh + xrce where applicable). cyclonedds keeps its CMake path;
bare-metal keeps explicit `register()`.

**Drift-collapse — DONE.** The RMW alias/name table is now single-sourced:
`cargo-nano-ros::rmw_resolver::canonical_rmw` is the one alias table (`zenoh` /
`rmw-zenoh` / `rmw-zenoh-cffi` → `zenoh`, …); `resolve_rmw` uses it, and the
orchestrated `generate.rs::normalize_rmw` delegates to it. Adding a backend now
touches one place. (`nros new --rmw` templating landed in 227.4.) The
orchestrated codegen still emits a `register()` in its *generated* entry — that
is a machine-emitted lowering of the declared config, not a user hardcode (like
the C/C++ auto-synthesized stub), so it is consistent with the model and left
as-is. 227.3 complete.

### 227.4 — `nros new --rmw` templating  ✅ DONE
`scaffold_package` validates `--rmw` via the resolver (clear error on a typo, no
package dir created) and templates it: Rust Cargo.toml gets the `rmw-<x>` feature
(was hardcoded `rmw-zenoh`); C/C++ CMakeLists bakes `set(NANO_ROS_RMW <x> CACHE …)`.
The "template diversification: TODO" banner is gone. 4 scaffold tests.
**Files:** `packages/cli/cargo-nano-ros/src/scaffold.rs`.

### 227.5 — C/C++ single-node descriptor  ✅ DONE (via `nano_ros_entry`)
No new `nano_ros_application()` fn needed — the scaffolder already gives single-node
C/C++ its descriptor through `nano_ros_entry(NAME … DEPLOY native …)`
(`cmake/NanoRosEntry.cmake`, Phase 212.N.6); combined with the baked
`NANO_ROS_RMW` from 227.4 this is full parity with Rust's
`[package.metadata.nros.application]`.

### 227.6 — Multi-node RT/scheduling exposure (schema)  ⏳ DEFERRED to Phase 228
The *shape* is decided (RFC-0015 reconciliation): node declares callback groups;
`system.toml` owns `[tiers.<name>.<rtos>]` + a per-`[[component]]` group→tier map +
`[[shared_state]]`. But the exact field set isn't pinned in any RFC, and the
schema is consumed only by the Phase 228 per-tier codegen — designing it in
isolation here risks rework. **Co-designed with the codegen in Phase 228** (228.A
tier resolver / 228.D shared-state). Existing `system.toml` (no tiers) is
unaffected; today only board defaults apply.

### 227.8 — Codegen-timing contract (ahead-of-vendor + hook convenience)  ✅ DONE
Already satisfied: `nros codegen-system` runs **ahead of the native build** (the
contract; there is no `nros deploy` orchestrator — RFC-0003 §4), the bake is
deterministic + **byte-identical on re-run** (test
`codegen_system_idempotent_on_unchanged_input`, 212.E.T2), and the configure-time
hook exists (`cmake/NanoRosEntry.cmake`). Contract documented in RFC-0003 §4/§7
and the book deploy page (227.9). No new code needed.

### 227.7 — Book sync  ✅ DONE
- `book/src/user-guide/configuration.md` — replaced the Phase-172.K
  single-`nros.toml`-owns-all model with the RFC-0004 config-home matrix
  (`nros.toml` narrowed to embedded direct-mode; `system.toml` universal/optional).
- `book/src/user-guide/rmw-backends.md` + `internals/rmw-backends.md` — replaced
  "not by features on `nros`" with the RFC-0031 declared-and-lowered model.
- `book/src/reference/build-commands.md` + `porting/custom-platform.md` — bare
  feature example annotated as the *lowering* of the declared RMW.

### 227.9 — Deploy-model doc (no `nros deploy` verb)  ✅ DONE
- `book/src/user-guide/deployment.md` — documents the native multi-step embedded
  deploy (`nros codegen-system` → native build → native flash, RFC-0003 §4) and
  that Phase 222 removed `nros deploy`/`build`/`run`.
- `book/src/getting-started/workspace-bringup.md` — dropped the removed `nros launch`.
- Book grep clean of stray `nros deploy`/`build`/`run` command usages.
- Remaining: the per-`just <plat>` recipe `-D`-arg command-map is a small doc
  follow-up (low urgency; the native sequence is documented).

## Acceptance

- A single-node project with no `system.toml` builds via the synthesized system;
  adding a `system.toml` with `rmw = "<x>"` selects the backend without touching
  any cargo feature.
- ~~All three backends select uniformly from the declared value across Rust and
  C/C++ examples.~~ Reframed (227.3): the *user-facing* declare→lower model is
  uniform; example-internal Cargo wiring is architecturally split (zenoh/xrce
  project-dep, cyclone umbrella) and stays so — see RFC-0031.
- `nros new foo --rmw xrce` scaffolds an xrce-wired project.
- The four book pages describe one consistent config + RMW story.
- `just ci` green.

## Notes

Design-of-record: RFC-0004 (config), RFC-0031 (RMW selection). This phase is
convergence only — any *design* change discovered here updates those RFCs first
(per the design→RFC rule in AGENTS.md).
