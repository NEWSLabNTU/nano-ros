---
id: 2560
title: "NuttX C++ and Zephyr images fail in the scheduled lanes: the Corrosion-built Rust FFI glue resolves std's dependencies offline and finds no `addr2line`"
status: resolved
type: bug
severity: medium
area: [build, ci, cargo, nuttx, cmake]
found: 2026-11-15
related: [2444, 2561, phase-466]
---

## What

Every NuttX C++ fixture row fails in the nightly's per-board jobs. First
seen in run 43383470982 (2026-11-13 07:07Z, the first run with per-board
jobs), and again in 43559931051 (2026-11-14): `NuttxArm` and `NuttxRiscv`,
rows `workspace-cpp-nuttx-*`, `workspace-cpp-nuttx-realtime` and
`nuttx-cpp-zenoh-talker`. It happens in the cmake build, at
`Building Rust FFI glue for <pkg> C++ bindings`:

```
Locking 5 packages to latest Rust 1.96.0-nightly compatible versions
error: no matching package named `addr2line` found
location searched: crates.io index
required by package `std v0.0.0 (…/nightly-2026-04-11-…/lib/rustlib/src/rust/library/std)`
    ... which satisfies path dependency `std` (locked to 0.0.0) of package `sysroot v0.0.0 (…)`
note: offline mode (via `--offline`) can sometimes cause surprising resolution failures
```

The family-level `nuttx` job of 2026-11-12 07:07Z (run 43172962836, at
`e403555654`) printed no such error.

## Known

- The glue build is a `-Zbuild-std` cargo run inside cmake. It has no lock of
  its own, so it resolves, and that resolution takes in std's `sysroot`
  workspace. Under `--offline` that needs every std dependency in the cached
  crates.io index. A CARGO_HOME that has never resolved std has none of them.
- `nros build --offline` sets `CARGO_NET_OFFLINE=true` for the whole process
  since issue 2444 (`e403555654`), so a cargo that cmake drives is offline
  too. Before that, this cargo was online.
- `scripts/build/leaf-cache.sh` warms only cargo-driven images. For a
  cmake-driven image it fetches nothing, by design ("no cargo root to warm").

## Measured (2026-11-15)

A zero-dependency `no_std` staticlib was built with no lock and
`CARGO_NET_OFFLINE=true`:

| cargo | build-std | CARGO_HOME | result |
| --- | --- | --- | --- |
| `+nightly-2026-04-11` | `core,alloc`, `armv7a-nuttx-eabihf` | empty | `no matching package named rustc-literal-escaper` |
| the same | the same | after a fetch of std's `library/` workspace | builds |
| `+1.99.0` under `RUSTC_BOOTSTRAP=1` | `core,alloc,compiler_builtins`, `thumbv7m-none-eabi` | empty | `no matching package named rustc-literal-escaper` |
| the same | the same | after the fetch | builds |

So the failure needs only an offline build-std over a CARGO_HOME that never
resolved std. The second row's crate (`rustc-literal-escaper`) is the one
live-peer's Zephyr C++ glue stopped on (run 43555266126, `build-cortex-m-c-talker-zenoh`),
so live-peer's red is this issue too. On stable, the fetch needs
`RUSTC_BOOTSTRAP=1`, because std's manifest names a nightly cargo feature
(`profile-rustflags`).

## Fix (2026-11-15)

`scripts/build/warm-offline-cargo.sh` fetches two graphs:
1. **The repo-root workspace.** live-peer's same leaf also stopped on
   `failed to download const-default v1.0.0`, a root-lock crate. That cargo is
   cmake-driven, so it had no root of its own to warm. This step was
   `leaf-cache.sh`'s root fetch, which now calls the script instead, so the
   step has one spelling.
2. **Each `rust-src` channel's std sources.** For every channel in
   `config/rust-toolchains.txt` that has `rust-src`, it runs `cargo fetch` over
   that channel's `<sysroot>/lib/rustlib/src/rust/library` workspace. This is the one
network step, like the cache warm in `leaf-cache.sh`. The build that follows
stays offline.

- **Cheap to repeat:** it is stamped per CARGO_HOME and std lock. A
  re-fetch measured 0.3 s.
- **Never fatal:** a failure is reported, not raised.
- **Skipped** when `NROS_SKIP_LEAF_FETCH` is set.

It runs at every fixture entry that runs `nros build --offline`:
- the leaf-cache root warm, which covers `workspace-fixtures-build.sh` and
  `census-prepass.sh`;
- `west-fixtures.sh`;
- `zephyr-fixture-run-one.sh`;
- `compile-check-fixtures.sh`;
- `build-all-jobserver.sh`.

Sweep (every offline build entry):

    git grep -n -- '--offline' -- scripts just justfile

## Not known (unchanged)

- Why the 2026-11-12 family job did not fail, given that it already ran at
  `e403555654`. Two possibilities, neither checked: the per-board split
  changed what was built first, or which CARGO_HOME state the job starts from.

## Directions

- Warm std's dependencies once per toolchain before an offline build:
  `cargo fetch --manifest-path "$(rustc +<nightly> --print sysroot)/lib/rustlib/src/rust/library/Cargo.toml"`.
  This is the one network step, the same kind of step as issue 0967's.
- Or give the glue build a lock to resolve from, so that it reads std from
  `library/Cargo.lock` instead of from the index.
