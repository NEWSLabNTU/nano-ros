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

## Direction

Find the embedded-built workspace crate that newly depends on `serde`/
`serde_core` and gate it: `serde = { version = "…", default-features = false }`
(+ `features = ["derive"]`/`["alloc"]` as needed), or feature-gate the serde
dependency behind a `std`/host-only feature so the `--no-default-features`
embedded build excludes it. Re-run `just check-no-std` to confirm
`thumbv7em-none-eabihf` resolves clean.
