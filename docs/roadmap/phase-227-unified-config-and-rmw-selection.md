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

### 227.1 — Single-node `system.toml` synthesis + read path
Implement implicit-1-component-system synthesis when no `system.toml` is present,
and the optional single-node `system.toml` read path in the orchestration layer.
**Files:** `packages/cli/nros-cli-core/src/orchestration/nros_config.rs`,
`packages/cli/nros-cli-core/src/cmd/{plan,check,codegen_system}.rs`.

### 227.2 — RMW resolver + per-language lowering
Add the RFC-0031 precedence resolver; read declared `rmw` for single-node
(`system.toml` / flag) and lower it to the `nros` cargo feature (Rust) or
`-DNANO_ROS_RMW` (C/C++). Remove the per-language asymmetry where the user must
set the feature directly.
**Files:** `packages/cli/nros-cli-core/src/cmd/setup.rs`, orchestration RMW
resolution, `cmake/*`.

### 227.3 — Converge examples onto uniform RMW lowering
Make every example (zenoh / xrce / cyclonedds) lower RMW the same way from the
declared value; remove the inconsistent mix of project-level backend deps vs
`nros/rmw-*` features. cyclonedds keeps its CMake/Corrosion link path.
**Files:** `examples/**/Cargo.toml`, `examples/**/CMakeLists.txt`.

### 227.4 — `nros new --rmw` templating
Make `--rmw <x>` actually template the scaffold (today it only prints a
"next steps" banner).
**Files:** `packages/cli/nros-cli-core/src/cmd/new.rs`,
`packages/cli/cargo-nano-ros/src/scaffold.rs`.

### 227.5 — `nano_ros_application()` CMake function
Add a C/C++ single-node descriptor function so single-node C/C++ has parity with
Rust's `[package.metadata.nros.application]`.
**Files:** `cmake/*.cmake`.

### 227.6 — Multi-node RT/scheduling exposure (schema + impl)
The *shape* is decided (2026-06, RFC-0015 Phase 212 reconciliation): node declares
callback groups (`[package.metadata.nros.node]` / `nano_ros_node_register`);
`system.toml` owns `[tiers.<name>.<rtos>]` (priority/stack) + a per-`[[component]]`
group→tier map + `[[shared_state]]`. Implement the `system.toml` schema + loader;
the per-tier task/Executor codegen is **Phase 94**. (Today only board defaults
apply.)
**Files:** `system.toml` schema, `packages/cli/nros-cli-core/src/orchestration/`,
cross-ref Phase 94.

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
