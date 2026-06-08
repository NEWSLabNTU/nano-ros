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

### 227.3 — Converge examples onto uniform RMW lowering  ⏳ DEFERRED
Build-risk (changing link wiring per example) — needs per-`just <plat>` build
verification, so it lands as its own verified pass, not folded into a CLI wave.
Make every example (zenoh / xrce / cyclonedds) lower RMW the same way from the
declared value. cyclonedds keeps its CMake/Corrosion link path.
**Files:** `examples/**/Cargo.toml`, `examples/**/CMakeLists.txt`.

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

### 227.6 — Multi-node RT/scheduling exposure (schema + impl)
The *shape* is decided (2026-06, RFC-0015 Phase 212 reconciliation): node declares
callback groups (`[package.metadata.nros.node]` / `nano_ros_node_register`);
`system.toml` owns `[tiers.<name>.<rtos>]` (priority/stack) + a per-`[[component]]`
group→tier map + `[[shared_state]]`. Implement the `system.toml` schema + loader;
the per-tier task/Executor codegen is **Phase 228**. (Today only board defaults
apply.)
**Files:** `system.toml` schema, `packages/cli/nros-cli-core/src/orchestration/`,
cross-ref Phase 228.

### 227.8 — Codegen-timing contract (ahead-of-vendor + hook convenience)
Make `nros deploy` always run `nros codegen system` ahead of the vendor tool
(the contract, RFC-0003 §7), keeping the configure-time hook as an idempotent
convenience that yields the same baked tree. Ensure both triggers are
byte-identical so raw `west build` / `idf.py build` stay valid in dev.
**Files:** `packages/cli/nros-cli-core/src/cmd/{deploy,codegen_system}.rs`,
`cmake/NanoRosEntry.cmake`, `integrations/<rtos>/`.

### 227.7 — Book sync
- `book/src/user-guide/configuration.md` — replace the Phase-172.K
  single-`nros.toml`-owns-all model with the RFC-0004 matrix.
- `book/src/user-guide/rmw-backends.md` + `internals/rmw-backends.md` — replace
  "not by features on `nros`" with the RFC-0031 declared-and-lowered model.
- `book/src/reference/build-commands.md` + `porting/custom-platform.md` — present
  RMW as declared config, with the cargo feature noted as the lowering detail.
**Files:** the listed book pages.

### 227.9 — Deploy-model doc (no `nros deploy` verb)
Phase 222 removed `nros build`/`run`/`deploy`/`monitor`/`launch`; deployment is
`nros codegen-system` + the native tool. Document the per-RTOS native deploy
command-map (the `-D` args derived from `[deploy.<board>]` for west / idf.py /
cmake / cargo) so embedded deploy is a clear documented multi-step (RFC-0003 §4).
Scrub any lingering `nros deploy`/`build`/`run` from the book the way the RFCs
were scrubbed.
**Files:** `book/src/getting-started/workspace-*.md`, embedded chapters, the
per-`just <plat>` recipe docs.

## Acceptance

- A single-node project with no `system.toml` builds via the synthesized system;
  adding a `system.toml` with `rmw = "<x>"` selects the backend without touching
  any cargo feature.
- All three backends select uniformly from the declared value across Rust and
  C/C++ examples.
- `nros new foo --rmw xrce` scaffolds an xrce-wired project.
- The four book pages describe one consistent config + RMW story.
- `just ci` green.

## Notes

Design-of-record: RFC-0004 (config), RFC-0031 (RMW selection). This phase is
convergence only — any *design* change discovered here updates those RFCs first
(per the design→RFC rule in AGENTS.md).
