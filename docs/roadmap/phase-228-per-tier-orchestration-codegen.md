# Phase 228 — Per-tier scheduling orchestration codegen

**Goal.** Deliver the multi-tier orchestration codegen described in RFC-0015 — emit
one RTOS task per priority tier (each owning one `Executor`), with callback groups
assigned to tiers from `system.toml`, plus shared-state accessor codegen. Phases
94 → 126 (both archived) shipped only the **single-tier degenerate case** (all
nodes in one task / one Executor — today's `nros codegen-system` output). This
phase closes the gap to the full RFC-0015 execution model.

**Status.** Proposed (2026-06-08).

**Priority.** P2 — the single-tier path works today and covers most cases;
multi-tier is the differentiator for hard-RT embedded (mixed-criticality on one
MCU) but not blocking the common deployment.

**Depends on.** Phase 227 (`system.toml` `[tiers.*]` + group→tier schema + loader —
227.6), Phase 126 (orchestration codegen foundation, archived), RFC-0015
(execution model), RFC-0016 (per-RTOS priority mapping), RFC-0017 (`PlatformTimer`
for the `Sporadic` class).

## Overview

RFC-0015 (Phase 212-reconciled) fixes the design:

- The **node** declares its callback *groups* (`[package.metadata.nros.node]` /
  `nano_ros_node_register`).
- `system.toml` owns **tier definitions + group→tier assignment**
  (`[tiers.<name>.<rtos>]` priority/stack + per-`[[component]]` group→tier map) and
  `[[shared_state]]`.
- Codegen emits **one RTOS task per tier**, each opening an `Executor` on the one
  shared session, with the tier's callback groups pre-registered; all-default-tier
  collapses to the single-task shape that ships today.

The schema + loader land in Phase 227.6; this phase is the **code emission** on
top of it.

## Architecture

```
system.toml ([tiers.*], group→tier, [[shared_state]])
  + node callback-group metadata
        │  nros codegen-system  (ahead-of-vendor, RFC-0003 §4)
        ▼
  tier resolver ─► per-tier task entry fns ─► toplevel main (per platform)
                 ├► shared_context C ABI + Rust/C++/C accessors
                 └► per-RTOS spawn (xTaskCreate / tx_thread_create / k_thread / pthread)
```

One shared session per binary; one `Executor::open_with_session(shared)` per tier
task; cross-tier shared state guarded by a `nros-platform` mutex (single-tier =
no lock).

## Work Items

### 228.A — Tier resolver
Resolve `system.toml` `[tiers.<name>.<rtos>]` + per-component group→tier map +
node callback-group metadata into an ordered tier table (tier → priority, stack,
{component.group}). Degenerate (all default) → one tier.
**Files:** `packages/cli/nros-cli-core/src/orchestration/{planner,tier}.rs`.

### 228.B — Per-tier task + executor emission
Emit one task entry fn per tier (`Executor::open_with_session(shared)` + pre-register
the tier's callback groups + spin loop) and a platform-specific `main()` that opens
the shared session and spawns the tasks. Wire `Executor::open_with_session` if not
already present (RFC-0015 §11.3).
**Files:** `packages/cli/nros-cli-core/src/cmd/codegen_system.rs`,
`packages/core/nros-node/src/executor/`.

### 228.C — Callback-group → tier registration
Generated per-tier task pre-registers exactly the callbacks whose group maps to
that tier (v1: all groups effectively MutuallyExclusive within their tier-task,
per RFC-0015 §5.3).
**Files:** codegen + `nros-node` registration.

### 228.D — Shared-state accessor codegen
Emit the `nros_shared_context` C-ABI struct + accessors from `system.toml
[[shared_state]]`, plus Rust/C++/C wrappers; single-tier → no lock, cross-tier →
platform mutex.
**Files:** codegen, `nros-cpp`/`nros-c` shared-context wrappers.

### 228.E — Per-RTOS spawn + priority lowering
Map the normalized 0–31 tier priority to each RTOS (RFC-0016) and emit the native
spawn (`xTaskCreate` / `tx_thread_create` / `k_thread_create` / `pthread_create`).
Use `PlatformTimer` (RFC-0017) for the `Sporadic` class budget refill.
**Files:** per-platform codegen templates, `nros-platform-*`.

### 228.F — Multi-tier fixture + acceptance test
A 2-tier fixture (e.g. a `high` control loop + a `low` telemetry group) building on
≥2 platforms; assert distinct tasks/priorities at runtime. Single-tier parity test
confirms the degenerate output is byte-equivalent to today's.
**Files:** `packages/testing/nros-tests/fixtures/orchestration_tiers/*`,
`packages/testing/nros-tests/tests/orchestration_tiers.rs`.

## Acceptance

- A `system.toml` with two `[tiers.*]` + a group→tier map produces a binary with
  two RTOS tasks at the declared priorities, each running its tier's callbacks.
- All-default-tier `system.toml` produces the same single-task output that ships
  today (no regression).
- Shared state declared in `[[shared_state]]` is reachable from both tiers with
  the correct lock behavior.
- `just ci` green; multi-tier fixture passes on ≥2 platforms.

## Notes

Design-of-record: RFC-0015 (execution model, reconciled to Phase 212). The
scheduling *config home* is decided (RFC-0015 banner / RFC-0004 §7 / Phase 227.6);
this phase is the codegen + runtime that consumes it. Per the design→RFC rule, any
design change discovered here updates RFC-0015 first. RT acceptance harness +
hardware gates are Phase 162; this phase is the codegen, not the test rig.
