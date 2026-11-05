---
id: 2285
title: "Tier-2 nightly: the three Zephyr native_sim workspace Rust images are refused by `nros build --dry-run` for a target and component the lane just provisioned"
status: open
type: bug
area: [ci, zephyr, cli, toolchain]
severity: medium
found: 2026-11-05
related: [2151, 1672, 2211, 1474, 1277, 2242, phase-466]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — a
scheduled lane that has produced the same red two nights running.

## Measured

`nightly` run 41972046603 (2026-11-05, main `2887760614`), job 127062311783
`tier 2 nightly (pairwise cover)` on `nano-ros-runner-newslab-118-server`.
The job ends `build-test-fixtures: FAILED stage(s): zephyr(rc=2)`. The
uploaded `tier2-nightly-junit` artifact's `zephyr.log` names three rows, all
refused before anything is built:

```
zephyr-fixture-run-one: nros build --dry-run failed for demo_bringup:zephyr (zephyr/native_sim/native/64/workspace-entry)
Error: missing prerequisites for this build:
  - Rust target `x86_64-unknown-none` (a Rust image on board `zephyr`)
      run: rustup target add x86_64-unknown-none
  - rustup component `clippy` (a Rust image on board `zephyr`)
      run: rustup component add clippy
Location:
    nros-cli-core/src/cmd/build.rs:942:77
```

The same text for `demo_bringup:zephyr_rust_params`
(`build-ws-rs-params-entry-zenoh`) and `demo_bringup:zephyr_rust_lifecycle`
(`build-ws-rs-lifecycle-entry-zenoh`). The other 59 Zephyr rows build,
including every non-workspace Rust row (`build-rust-*`,
`build-cortex-m-rust-*`).

The same three rows failed the same way in the previous night's run,
41790118101 (2026-11-04, `zephyr.log` lines 109–152 of its
`tier2-nightly-junit` artifact). That run was attributed to issue 2242
(`hal_stm32`) alone. 2242's cause is fixed, and the Nucleo row builds
`[ok]` tonight, so this is what remains.

## The contradiction

The same job's provisioning step, a few seconds earlier, reports both
prerequisites present:

```
info: component rust-std for target x86_64-unknown-none is up to date
```

and `clippy` is part of the pinned toolchain. So the preflight added by issue 2151
(`[board.zephyr] rust_target` / `rust_components`) is checking a toolchain
that is NOT the one the lane provisioned. The workspace rows run
`nros build --dry-run` from the workspace directory inside
`nros-fixture-tree` (a linked worktree; the log shows the issue-1280
re-rooting). The non-workspace rows take the west road and do not run that
preflight.

## What it is NOT

- Not 2242/2249: `hal_stm32` is fetched now (the Nucleo row builds).
- Not 1474: that was the clippy step inside `west build`, this is the
  `nros build` preflight refusing before any build.
- Not a disk or runner fault: the job ran to its end and reported a stage
  failure.
- Not caused by phase-530 W1 (#3096): the 2026-11-04 run predates its merge.

## What would close it

Find which toolchain the preflight asks, then make it ask the one the build
uses. Likely suspects: a `rust-toolchain.toml` that resolves differently from
the workspace dir, or the probe running `rustup` under a `RUSTUP_TOOLCHAIN` /
cwd that the provisioning did not target. Then confirm with a tier-2 nightly
whose `zephyr.log` builds `build-ws-rs-entry-zenoh`,
`build-ws-rs-params-entry-zenoh` and `build-ws-rs-lifecycle-entry-zenoh`. A
local reproduction is `NROS_ZEPHYR_FIXTURE_FILTER=build-ws-rs-entry-zenoh just
zephyr build-fixtures` on a host that has the target installed for the pinned
toolchain.
