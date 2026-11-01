---
id: 1960
title: "28 excluded cargo roots that are built (as a fixture, or as a locked root's resolve) are linted by no lane: clippy reaches a path dependency only when it is a workspace member"
status: resolved
type: tech-debt
area: [ci, build]
severity: medium
found: 2026-10-18
resolved: 2026-10-19
related: [1309, 1472, 0196, 1966, phase-505]
resolved_in: "phase-505 W1: the feature sweep lints every excluded standalone root"
---

**Home: [phase-505](../../roadmap/archived/phase-505-ci-lanes-reach-and-infra-robustness.md)** W1 — filed while closing issue 1309.

## What was measured

Issue 1309 asked which root-workspace `exclude` entries no lane builds, and
exempted two shapes as "built standalone by a recipe": a directory with its
own `[workspace]` table, and one with a tracked `Cargo.lock`. Being BUILT is
true of both shapes. Being LINTED is not. Clippy lints only the members of the
root it runs in (`RUSTC_WORKSPACE_WRAPPER`), so when one of these crates is
compiled as a path dependency of an example leaf or a fixture, it gets
`rustc`'s warnings and none of clippy's.

`scripts/check-feature-sweep.py`'s `STANDALONE` half now lints issue 1309's
own set (four crates, plus `qemu-smoltcp-bridge`, which that derivation also
reaches). Every one of them carried latent `-D warnings` errors: 13, 12, 19,
5, plus one in a dormant board. Nothing suggests the wider set is cleaner.

On 2026-10-18 the wider set has **28 more roots**. Each is an `exclude` entry
under `packages/`, is its own cargo root (not inside a nested workspace), and
declares a Rust target:

| shape | roots |
| --- | --- |
| own `[workspace]`, fixture bins | `nros-bench/{executor-fairness,stress-zenoh,wcet-cycles-qemu,port-metric,large-msg-baremetal,large-msg-xrce,stress-xrce}`, `nros-tests/bins/{cdr-roundtrip-qemu,lan9118-qemu}` |
| own `[workspace]` + tracked lock | `nros-smoke/stm32f4-smoltcp-echo`, `nros-smoke/esp32-hello-world`, `boards/nros-board-nuttx-qemu` |
| own `[workspace]` | `boards/nros-board-zephyr` |
| tracked lock (a `locked_roots` entry) | `drivers/net/{nros-smoltcp,openeth-smoltcp}`, `drivers/serial/{cmsdk-uart,stm32f4-usart}`, `rmw/zenoh/zpico-serial`, `boards/{mps2-an385-pac,nros-board-mps2-an385,nros-board-mps2-an385-freertos,nros-board-s32z270-freertos,nros-board-mps3-an536-freertos,nros-board-threadx-linux,nros-board-threadx-qemu-riscv64}`, `reference/stm32f4-porting/{polling,rtic}`, `verification/nros-verification` |

The only standalone clippy runs in the tree are `packages/cli`'s
(`check-cli-clippy`), the example leaves' (`scripts/build/lint-example-leaf.sh`)
and the compile-check fixtures'. Measured with `git grep` over `just/` and
`scripts/` for `clippy` beside `--manifest-path` or a `cd`.

## Direction

Widen `standalone_roots()` in `check-feature-sweep.py` to this set and give
each root a `STANDALONE` run or a `STANDALONE_SKIP` reason. The coverage check
refuses a derived root with neither, so widening the derivation is the step
that makes the list complete. Expect most rows to need the board's
environment, not just a triple. That is the `CROSS` half's `cross_shape`, read
from `nros-board.toml`, and a `STANDALONE` row should name a board the same
way rather than restate its environment.

Formatting has the same reach gap. `check-workspace-fmt` reaches workspace
members only, and `qemu-smoltcp-bridge`, which this sweep now lints, still has
`rustfmt --check` diffs in `src/bridge.rs` and `src/lib.rs`. The same command
over issue 1309's other three crates is clean. A standalone run could hold
both, lint and format, from one table.

A root that some other lane lints as a WORKSPACE (a fixture's own
`[workspace]` linted by its fixture build, if any is) belongs in
`STANDALONE_SKIP`, with that lane named as the reason.

## Resolution (2026-10-19, phase-505 W1)

`standalone_roots()` in `scripts/check-feature-sweep.py` no longer drops a root
for having its own `[workspace]` or a tracked `Cargo.lock`. It keeps every
`exclude` entry under `packages/` that has a `[package]`, declares a Rust
target and has no `[workspace]` ancestor inside the checkout. Those two
properties now decide HOW a root is linted (`standalone_shape`), never
WHETHER:

- a tracked lock is linted `--locked` against that lock; an unlocked root is
  seeded from the pin set and held to it with `nros ws check-lock`, as before;
- a manifest that path-depends on `generated/` or `build/` is rendered with
  `nros sync` first, as its fixture build does;
- an explicit `[workspace] members` list is linted `--workspace`; an implicit
  one is the root package alone, because its other members are the generated
  message crates sync wrote (none of the 13 own-`[workspace]` roots lists
  members);
- the lint covers the default targets (library and binaries), then
  `cargo +<pinned nightly> fmt --check` runs on the same packages.

Each row names where its target comes from, the CROSS half's way: a BOARD
(through `cross_shape`, so the triple, environment and build-std are read
from `nros-board.toml`), a TRIPLE, `LEAF` (the root's tracked
`.cargo/config.toml` `[build] target` or its `system.toml` image) or `HOST`.
`check_standalone_where` refuses a `LEAF` row for a root that states no target,
and a board or triple row that overrides a target the root states. The
self-test has negative controls for both, and for a
`STANDALONE_NO_DEFAULT` entry that names no run.

Measured on 2026-10-19: the derivation finds 33 roots, the 5 of issue 1309 and
this issue's 28. 31 are runs and 2 are skips, and all 31 runs are green.

- **20 clippy `-D warnings` errors in 8 roots**, all fixed:
  `nros-smoltcp` 7, `nros-board-zephyr` 5, `nros-board-threadx-qemu-riscv64` 2,
  `zpico-serial` 2, and one each in `mps2-an385-pac`, `nros-board-mps2-an385`,
  `nros-board-nuttx-qemu` and `openeth-smoltcp`. One is an `allow`, with its
  reason beside it: `nros-smoltcp`'s platform macro writes `crate::$plat` on
  purpose, because the platform type lives in the caller's crate. Two fixes
  changed behaviour. `ZephyrBoard::wait_link_up` became `pub`: the crate doc
  and the README call it, but the inherent-method conversion had left it
  private, so nothing could. `nros-board-mps2-an385`'s log bridge now compiles
  only with the `board-entry` or `rtic` boot path that calls it.
- **rustfmt diffs in 5 roots**, all fixed: `qemu-smoltcp-bridge` (the two
  files this issue named), `zpico-serial`, `openeth-smoltcp`, and the
  `build.rs` of `stm32f4-porting/{polling,rtic}`.
- `port-metric` had no `.gitignore`, so a lint's seeded lock showed as an
  untracked file. It now ignores `/Cargo.lock` and `/target/`, like its
  siblings.

Exceptions, each with its reason in the script:

- `STANDALONE_SKIP`: `nros-verification` is a Verus crate. `verus!` erases its
  spec items unless Verus builds it, so plain rustc cannot resolve the spec
  functions its modules import. `just verify-verus` checks it. The dormant
  `nros-board-esp32-qemu` keeps the skip it had.
- `STANDALONE_NO_DEFAULT`: `nros-board-{s32z270,mps3-an536}-freertos` are
  linted `--no-default-features`. On the cargo road, their default
  `rmw-zenoh` builds zpico-sys's C for `armv8r-none-eabihf` without `-mfpu`,
  which gcc refuses. Filed as
  [issue 1966](1966-armv8r-cargo-road-zpico-c-lacks-fpu-flags.md); drop
  the entries when it is fixed.

The NuttX board's run needs a configured NuttX tree, which the scheduled
`feature-sweep` job builds. Locally it was verified against an existing
configured tree. No root is linted as a workspace by any other lane, so no
skip names another lane.
