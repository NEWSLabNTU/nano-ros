---
id: 1960
title: "28 excluded cargo roots that are built (as a fixture, or as a locked root's resolve) are linted by no lane: clippy reaches a path dependency only when it is a workspace member"
status: open
type: tech-debt
area: [ci, build]
severity: medium
found: 2026-10-18
related: [1309, 1472, 0196, phase-505]
---

**Home: [phase-505](../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** W1 — filed while closing issue 1309.

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
