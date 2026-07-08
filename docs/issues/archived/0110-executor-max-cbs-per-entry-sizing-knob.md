---
id: 110
title: "No per-entry way to size the executor callback table (`NROS_EXECUTOR_MAX_CBS`) to a declared topology"
status: resolved
type: enhancement
area: core
related: [phase-263, phase-264, phase-271]
split-from: 95
---

> **Resolved by phase-271** (2026-07-01). The executor's sized tables are
> externalised into caller-owned storage (`Executor<'s>` borrowing carved
> slices), so per-entry sizing no longer needs the workspace-global build const.
> Both fix ideas below shipped: the **declarative codegen** derives the size from
> the plan's `CALLBACK_COUNT` automatically (`build_executor` → `open_in`), and
> the **`nros::main!` macro** path reads a per-entry
> `[package.metadata.nros.entry] max_callbacks` knob → `open_sized`. The C/C++ FFI
> stays a concrete wrapper (heap-free inline carve, ABI unchanged). See
> [phase-271](../../roadmap/phase-271-executor-per-entry-sizing.md).

## Summary

`MAX_CBS` (executor fixed callback-entry table size) and the derived
`ARENA_SIZE ≈ MAX_CBS × ~15 KB` are a **build-time env const** (`NROS_EXECUTOR_MAX_CBS`,
default **4**) baked into `nros-node` at *its* compile by `nros-node/build.rs`. The only
lever to raise it is cargo `[env]` in `.cargo/config.toml`, which is **workspace-global**:
raising it for one fat native entry also bloats every lean RAM-bound embedded entry in the
same workspace (an esp32 entry with `MAX_CBS = 16` derives a ~240 KB arena — over budget).

So a workspace mixing a fat native entry and lean embedded entries has **no ergonomic,
per-entry way** to size the executor to its declared topology; it must set the env per-build
out-of-band. The `nros::main!` macro *knows* the topology's entity count at expansion, but
cannot resize a dependency's fixed-size array after the fact.

Split from **#95** (gap B). #95 resolved the opaque-diagnostic half (gap A): a topology that
declares more callbacks than `MAX_CBS` now fails with an actionable
`RuntimeError::ExecutorFull(<pkg>)` naming `NROS_EXECUTOR_MAX_CBS`, instead of an opaque
`NodeRegister("<pkg>")`. This issue is the remaining ergonomics half — making the size itself
per-entry-settable so the actionable knob is reachable without the global `[env]` hammer.

## Current workaround

phase-263 A1 Track-D fixture row `workspace-rust-native-showcase`
(`examples/fixtures.toml`) sets `env = { NROS_EXECUTOR_MAX_CBS = "8" }` and builds into its
own `target-fixtures-showcase` dir so the larger `MAX_CBS` doesn't churn the `nros-node`
fingerprint shared with the default rows. The workspace's embedded entries boot only the
minimal talker+listener launch, so they keep the default 4.

## Fix ideas

- **Topology-derived const:** have the codegen emit a `const` for the entry's declared entity
  count and pass it to a **const-generic `Executor<const MAX_CBS: usize>`**, so each entry
  sizes its own table/arena from its own baked topology — no env at all.
- **Per-entry build knob:** failing the const-generic route, carry a `MAX_CBS` override in the
  entry's `Cargo.toml` package metadata that the build orchestration translates into a
  per-entry (not workspace-global) env, so cargo `[env]` global-ness stops being the only lever.

## Acceptance

- A workspace with one fat native entry (≥5 callbacks) and ≥1 lean embedded entry builds with
  each entry sized to its own topology — the embedded entries keep the small default arena, the
  native entry fits its 5+ callbacks — **without** a workspace-global `NROS_EXECUTOR_MAX_CBS`.
- The `workspace-rust-native-showcase` fixture drops its `NROS_EXECUTOR_MAX_CBS = "8"` override
  and still boots its 4-node / 5-callback launch.
