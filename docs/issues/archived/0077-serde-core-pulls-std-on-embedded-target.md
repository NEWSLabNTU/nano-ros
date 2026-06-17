---
id: 77
title: check-no-std fails — serde_core pulls std on thumbv7em-none-eabihf
status: resolved
type: bug
area: build
related: [phase-253]
resolved_in: "phase-253 follow-up — nros-board-common no_std-by-default"
---

## Resolution

`just check-no-std` failed: `serde_core` (no `#![no_std]`) reached the embedded
build → `error[E0463]: can't find crate for std` on `thumbv7em-none-eabihf`.

Root cause: `nros-board-common` (`categories = ["no-std"]`) had
`default = ["build-helpers"]`, and `build-helpers` pulls `serde`+`toml`+`cc`. The
3 nuttx board crates depended on it in `[dependencies]` WITHOUT
`default-features = false`, so under `cargo clippy --workspace
--no-default-features --target thumbv7em` the runtime edge's default re-enabled
`build-helpers` via feature unification → serde/std into the no_std build. The
runtime side uses only the always-on no_std `BoardInit` traits; `build-helpers`
is a `build.rs` concern.

Fix (Option B — no_std-by-default): flip `nros-board-common` to `default = []`,
and have every consumer that uses a `build-helpers`-gated module
(`manifest`/`policy`/`nuttx_platform_build`/`nuttx_ffi_build`/`threadx_sources`/
`threadx_qemu_riscv64_build`) opt in explicitly via `features = ["build-helpers"]`.
All such use is from `build.rs` (`[build-dependencies]`) except `nros-zpico-build`
(host build-lib, `[dependencies]`). Edited: nros-board-{threadx, threadx-linux,
threadx-qemu-riscv64, nuttx-qemu-arm (+nros-nuttx-ffi), nuttx-qemu-riscv
(+nros-nuttx-ffi)} build-dep edges + nros-zpico-build. Every other consumer uses
only `BoardInit` → clean with no edit. resolver 2 keeps the build-dep feature out
of the target build. Manifest-only, no source change.

Now no_std is the default and the std/parser path is explicit, so a forgotten
flag fails toward no_std instead of silently pulling std.

Verified: `just check-no-std` green (thumbv7em / thumbv7m / riscv32imc);
`cargo check -p nros-zpico-build` green.
