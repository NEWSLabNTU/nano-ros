---
id: 77
title: check-no-std fails — serde_core pulls std on thumbv7em-none-eabihf
status: open
type: bug
area: build
related: [phase-253]
---

## Problem

`just check-no-std` (workspace clippy on the bare embedded target
`thumbv7em-none-eabihf`, `--no-default-features`) fails to compile:

```
error[E0463]: can't find crate for `std`
  = note: the `thumbv7em-none-eabihf` target may not support the standard library
  = note: `std` is required by `serde_core` because it does not declare `#![no_std]`
```

A workspace crate that IS built for the embedded target depends on `serde`
(→ `serde_core`) without disabling default features / selecting serde's
`no_std`-compatible configuration, so on a `#![no_std]` target the build pulls
`std` and fails. The cascade of follow-on `E0463 can't find crate for std` +
`E0432 unresolved import self::core::*` errors (origin: the `crate_root` macro)
all stem from this one root: once `std` can't be found the embedded build
unwinds across every dependent.

## Evidence

- CI: `pr-checks` workflow, `check` job, `just check-build + no_std` step,
  run 27662735498 (2026-06-17). The exact clippy invocation:
  `CARGO_TARGET_DIR=target-embedded cargo clippy --quiet --workspace
  --no-default-features --target thumbv7em-none-eabihf --exclude …`.
- Pre-existing on `main` before the phase-253 CI file-merge — the old `check`
  lane was already red here; the merge only surfaced it under the new name.
  Introduced by a parallel wave (RMW/config-SSoT work, 254/255 era) that added
  a serde dependency to a no_std-built crate.

## Root cause (investigated 2026-06-18)

Not serde itself — a **runtime dependency edge missing `default-features =
false`**. Chain:

```
nros-board-{nuttx, nuttx-qemu-arm, nuttx-qemu-riscv}  [dependencies]
  → nros-board-common (DEFAULT features)
  → default = ["build-helpers"]  →  dep:serde + dep:toml + dep:cc
  → serde → serde_core (no `#![no_std]`)  →  requires std  →  fails on thumbv7em
```

`nros-board-common` is correctly layered: the `BoardInit`/`BoardPrint`/
`BoardExit` traits are no_std + zero-dep and always available; the `manifest`
parser + `policy` + `nuttx_platform_build` helpers (serde/toml/cc) sit behind
the `build-helpers` feature, intended for `build.rs` only; `default =
["build-helpers"]`.

The 3 nuttx board crates depend on board-common in `[dependencies]` WITHOUT
`default-features = false`, so the default `build-helpers` is on for the
RUNTIME (target) build. Their source uses only the no_std traits
(`impl nros_board_common::BoardInit …`); `build-helpers` is needed only by
`build.rs` (`nuttx_platform_build::run_platform()`). Under
`cargo clippy --workspace --no-default-features --target thumbv7em-none-eabihf`
the runtime edge's default re-enables `build-helpers` via feature unification →
serde/std into the no_std build.

The threadx siblings already do it right (the reference pattern):

```toml
[dependencies]
nros-board-common = { path = "…/nros-board-common", default-features = false }  # BoardInit only
[build-dependencies]
nros-board-common = { path = "…/nros-board-common" }                            # build-helpers for build.rs
```

Workspace `resolver = "2"` keeps the build-dep's `build-helpers` out of the
target build, so the split is safe.

## Direction (chosen: no_std-by-default)

board-common is `categories = ["no-std"]` yet `default = ["build-helpers"]`
(the std-pulling parser) — backwards. Flip the default so the no_std path is
the default and the std/build path is explicit, so a future omission fails
TOWARD no_std instead of silently pulling std:

1. `nros-board-common`: `default = ["build-helpers"]` → `default = []`.
2. Every consumer that actually uses a `build-helpers`-gated module
   (`manifest`, `policy`, `nuttx_platform_build`, `nuttx_ffi_build`,
   `threadx_sources`, `threadx_qemu_riscv64_build`) opts in explicitly via
   `features = ["build-helpers"]`. All such use is from `build.rs`
   (`[build-dependencies]` edge) except `nros-zpico-build`, a host build-lib
   that uses `manifest`/`policy` from `[dependencies]`. Audited set:
   threadx, threadx-linux, threadx-qemu-riscv64, nuttx-qemu-arm (+ its
   nros-nuttx-ffi), nuttx-qemu-riscv (+ its nros-nuttx-ffi), zpico-build.
3. Every other consumer uses only the always-on no_std `BoardInit` traits, so
   `default = []` makes them clean with no edit.

Manifest-only. Re-run `just check-no-std` to confirm `thumbv7em-none-eabihf`
resolves clean.
