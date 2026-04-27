# Phase 94 — RTOS Orchestration via Launch Tree + Manifest Codegen

**Goal:** Replace hand-written RTOS `main.rs` orchestration with build-time codegen that consumes ROS 2 launch trees + per-package `nros.toml` manifests. Preserves ROS 2 mental model on resource-constrained MCUs.

**Status:** Not Started

**Priority:** Medium

**Depends on:** Phase 86 (lifecycle, parameter services), Phase 87 (size headers), Phase 79 (`PlatformYield`); coordinates with Phase 84.F4 (`PlatformThreading` trait migration).

**Design:** [docs/design/rtos-orchestration.md](../design/rtos-orchestration.md)

---

## Overview

Stock ROS 2 launch on Linux: each node = process, kernel scheduler arbitrates, runtime evaluates Python at launch time. RTOS targets work the opposite way: one binary, limited tasks, no runtime Python. Today nano-ros users hand-write `main.rs` (autoware-nano-ros sentinel = 1472 lines wiring 11 algos / 51 pubs / 22 services / 7 subs / 1×30 Hz timer) — loses the launch-file mental model, brittle, doesn't scale.

This phase adds codegen pipeline:

1. `play_launch_parser` evaluates launch tree at build time → `record.json` (frozen graph).
2. Codegen merges `record.json` + per-package `nros.toml` manifests → emits orchestration crate (main, per-tier task fns, shared-context C-ABI headers, lifecycle wiring, runtime arg plumbing).
3. Generated binary spawns one RTOS task per priority tier; each tier owns one `Executor`; all tiers share one zenoh-pico session.

See design doc for decisions, manifest schema, callback-group mapping, RTOS execution model translation, shared-state model.

---

## Work Items

### v1 (Phases 94.A–94.G — required)

- [ ] 94.A — IR + manifest schema crate
- [ ] 94.B — Single-tier codegen (sentinel parity oracle)
- [ ] 94.C — Cross-language shared state (C-ABI accessors)
- [ ] 94.D — Multi-tier codegen + cross-tier sync
- [ ] 94.E — Lifecycle + startup order
- [ ] 94.F — Composable node container parity
- [ ] 94.G — Runtime args (`BoardArgsSource`)

### Post-v1 (Phase 94.H)

- [ ] 94.H — MultiThreadedExecutor (Reentrant groups, MT=1 platforms only)

---

### 94.A — IR & manifest schema

Lock `RecordJson` consumption (no parser changes; reuse upstream as-is). Define `nros-orchestration-manifest` crate (TOML schema, serde, validation rules). Per-package discovery extension to `cargo-nano-ros::package_discovery`.

**Files:**

- `~/repos/play_launch/src/nros-orchestration-manifest/` — new crate (sibling of `ros-launch-manifest-types`).
- `~/repos/play_launch/src/ros-launch-manifest/types/src/types.rs` — extend `Manifest` with tier/callback_group/shared_state fields (or fork to keep Linux-side untouched; decision in 94.A).
- `packages/codegen/cargo-nano-ros/src/package_discovery.rs` — pick up `nros.toml` in workspace walk.
- `packages/codegen/cargo-nano-ros/src/manifest.rs` — new module: parse + merge node + system manifests.

### 94.B — Single-tier codegen (parity oracle)

New `cargo nano-ros generate-main` subcommand. Reuses `rosidl-codegen` Askama engine w/ new templates. Emits today's hand-written shape (single tier, single executor). Validate against autoware-sentinel: replace its `main.rs` w/ codegen output, verify behavior parity (sentinel retained as oracle until tests green).

**Files:**

- `packages/codegen/cargo-nano-ros/src/main.rs` — add `GenerateMain` subcommand.
- `packages/codegen/cargo-nano-ros/src/generate_main/` — new module.
- `packages/codegen/rosidl-codegen/templates/orchestration/main.rs.jinja` — new template.
- `packages/codegen/rosidl-codegen/templates/orchestration/tier_task.rs.jinja`
- `packages/codegen/rosidl-codegen/templates/orchestration/cargo.toml.jinja`
- `~/repos/autoware-nano-ros/manifests/sentinel.nros.toml` — author the manifest matching current hand-written sentinel.
- Test: parity-oracle integration test in nros-tests comparing hand-written vs generated binary behavior.

### 94.C — Cross-language shared state

Manifest `[[shared_state]]` → C-ABI struct + Rust + C++ accessors. Tier-aware sync stub (single tier = no lock).

**Files:**

- `packages/codegen/cargo-nano-ros/src/generate_main/shared_context.rs` — emitter logic.
- `packages/codegen/rosidl-codegen/templates/orchestration/shared_context.h.jinja`
- `packages/codegen/rosidl-codegen/templates/orchestration/shared_context.rs.jinja`
- `packages/codegen/rosidl-codegen/templates/orchestration/shared_context.hpp.jinja`
- Generated crate: `nros_generated_context` (per build target).
- CMake glue: emit `find_package(NanoRosSharedContext)` config.

### 94.D — Multi-tier codegen + cross-tier sync

Tier table → multi-task spawn. `Executor::open_with_session` + per-tier executor. Shared zenoh session w/ `ffi-sync` on MT=0 platforms (already wired). Cross-tier sync via `PlatformThreading::mutex_*` (or new `PlatformMutex` if needed).

**Files:**

- `packages/core/nros-node/src/executor/mod.rs` — new `Executor::open_with_session` API.
- `packages/core/nros-rmw/src/traits.rs` — `Session` trait reference-sharing semantics.
- `packages/core/nros-platform-api/src/lib.rs` — confirm `PlatformThreading::mutex_*` suffices, or add `PlatformMutex` trait.
- `packages/codegen/cargo-nano-ros/src/generate_main/tier.rs` — tier resolver + spawn-call emission per `target_rtos`.
- Tests: FreeRTOS QEMU + Zephyr multi-tier E2E in nros-tests.

### 94.E — Lifecycle + startup order

Wire `startup_order` to `LifecyclePollingNode::activate()` calls w/ `STATE_ACTIVE` ack-poll + timeout (default 5 s, manifest-overridable).

**Files:**

- `packages/core/nros-node/src/lifecycle.rs` — confirm `activate()` exposes ack-poll API; extend if needed.
- `packages/codegen/cargo-nano-ros/src/generate_main/lifecycle.rs` — emit pre-spawn activation orchestrator.

### 94.F — Composable node container parity

Map `ComposableNodeContainer` / `ComposableNodeContainerMT` from `record.json::container[]` to tier (single-threaded → MutuallyExclusive group; MT → Reentrant group + post-v1 multi-worker exec). Verify rmw_zenoh interop via composable container test.

**Files:**

- `packages/codegen/cargo-nano-ros/src/generate_main/container.rs` — container → tier mapping.
- Test: composable container against rmw_zenoh on Linux + RTOS QEMU.

### 94.G — Runtime args

`BoardArgsSource` trait + per-platform default impls. Wire into ParameterServer pre-spawn pass.

**Files:**

- `packages/core/nros-c/include/nano_ros/runtime_args.h` — C ABI declaration.
- `packages/core/nros-node/src/runtime_args.rs` — Rust API + `BoardArgsSource` trait.
- `packages/core/nros-platform-posix/src/runtime_args.rs` — argv parser.
- `packages/core/nros-platform-zephyr/src/runtime_args.rs` — settings/NVS backend.
- `packages/core/nros-platform-{nuttx,threadx,freertos}/src/runtime_args.rs` — empty defaults.
- `packages/codegen/cargo-nano-ros/src/generate_main/runtime_args.rs` — emit `apply_runtime_param_overrides` call in main.

### 94.H — MultiThreadedExecutor (post-v1)

Reentrant groups w/ multi-worker executor for `MT=1` platforms (POSIX/Zephyr/ESP-IDF).

**Files:**

- `packages/core/nros-node/src/executor/multi_threaded.rs` — new executor variant.
- Manifest schema: callback group `type = "Reentrant"` semantics activated.

---

## Acceptance Criteria

- [ ] `cargo nano-ros generate-main` subcommand exists and emits a complete orchestration crate from `record.json` + workspace `nros.toml` manifests.
- [ ] Single-tier degenerate case: generated binary passes autoware-nano-ros sentinel integration tests with identical behavior to hand-written `main.rs`.
- [ ] Multi-tier case: generated binary on FreeRTOS QEMU spawns N RTOS tasks at correct priorities, each owning one `Executor`, all sharing one zenoh-pico session.
- [ ] Cross-tier shared state via `*_modify(fn)` mutator works under preemption; Kani harness models the state machine.
- [ ] Lifecycle `startup_order` activates nodes in declared order with `STATE_ACTIVE` ack + timeout.
- [ ] `ComposableNodeContainer` (single-threaded) and `ComposableNodeContainerMT` from launch tree map to tier with MutuallyExclusive vs Reentrant groups respectively.
- [ ] Runtime args: POSIX `--ros-args -p key:=val` and Zephyr settings overrides land in ParameterServer before tier tasks spawn; values readable via `~/get_parameters` post-boot.
- [ ] All 6 platforms (POSIX, Zephyr, FreeRTOS, NuttX, ThreadX, bare-metal RTIC) have at least one E2E orchestration test in nros-tests.
- [ ] Per-package `nros.toml` discovery works for 3rd-party packages outside the nano-ros workspace.
- [ ] Codegen rejects builds violating §4.3 invariants (missing manifest, unknown tier, spin period > timer period, unresolved binding name, out-of-range RTOS priority).
- [ ] Documentation in `book/src/user-guide/` covers manifest authoring; `book/src/getting-started/` updated to use codegen path; hand-written `main.rs` sentinel deprecated.

---

## Notes

- **Greenfield estimate:** ~4000 LOC across nano-ros + play_launch (see design §11.4).
- **Phase 94.B is the critical-path validation gate.** Sentinel parity proves the codegen pipeline end-to-end before adding multi-tier complexity.
- **Coordination with Phase 84.F4:** if `PlatformThreading::mutex_*` lands as part of 84.F4 trait migration before 94.D, no new `PlatformMutex` trait needed.
- **Coordination with Phase 77.22:** orchestration's tier-task spin loop uses the planned `PlatformYield` trait. If 77.22 not done, fall back to existing per-platform yield helpers.
- **No new repo.** Codegen lives in nano-ros (`packages/codegen/cargo-nano-ros/`); manifest schema crate lives in play_launch workspace next to `ros-launch-manifest-types`. Decision rationale in design doc §2.4.
- **`generate-main` gated behind a Cargo feature on `cargo-nano-ros`** until v1 ships, so codegen bugs don't block nano-ros core releases.
- **Hand-written sentinel kept as parity oracle** until all tests green against generated output; then deprecated, then removed.
