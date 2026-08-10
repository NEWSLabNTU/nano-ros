---
id: 488
title: "Second-build-path residue: per-leaf cargo dirs outside `examples/**/target/`
  that phase-340 P2's gate does not cover"
status: open
type: tech-debt
area: build
related: [phase-340, rfc-0070, issue-0475, issue-0393]
---

## Context

phase-340 P2 closed the second build path for the population that blocks item 7 /
P4: a `cargo build` whose cwd is a leaf under `examples/` and which writes the
leaf's own `target/`. That population is now empty and gated by
`scripts/check-example-leaf-target-dirs.py`.

The sweep that found it turned up more sites in the same CLASS that the gate
deliberately does not cover, because they do not block P4 and each needs a
consumer moved with it. They are listed here so the next pass has the measurement
rather than a re-sweep.

## Residue 1 — per-leaf `target/` under `packages/testing/**`

Same defect (bare in-leaf `cargo build`, no manifest row, no coordinate, no
group), different tree. These do not block P4: the 391 per-leaf `.gitignore`
files item 7 deletes are the ones under `examples/`.

| site | leaf | consumer that must move with it |
| --- | --- | --- |
| `just/freertos.just` `build-fixture-extras` | `packages/testing/nros-bench/wake-latency-cortex-m3` (and its `wake-latency-pub` sibling) | `tests/wake_latency_cortex_m3.rs` spells `…/target/thumbv7m-none-eabi/<profile>/…` by hand |
| `just/qemu-baremetal.just` `build` / `build-fixtures` | `packages/testing/qemu-smoltcp-bridge` | resolver in `fixtures::binaries` |
| `just/qemu-baremetal.just` `build-rtic-main-e2e` | `packages/testing/nros-tests/bins/rtic-run-plan-e2e` | resolver in `fixtures::binaries` |
| `just/ros-editions.just` `build-fixture` | `packages/testing/nros-tests/bins/ros-edition-pose-pub` | the recipe's own echoed path; docker-gated lane |

The wake-latency pair is the one worth doing first: it runs inside
`build-test-fixtures` (via `just freertos build-fixtures`), on a MIGRATED
platform, so it re-creates a per-leaf tree on every full sweep. Preferred fix is
a `[[fixture]]` row (it would inherit the `freertos` group the six Entry rows now
share); the two images would then also stop being enumerated in the recipe.

## Residue 2 — authored per-leaf `target-<variant>/` on migrated platforms

These pass P2's gate by construction — they DO pass a `--target-dir` — and they
are covered by the repo-root `examples/**/target-*/` ignore, so they are not a P4
blocker either. They are still R1 duplicates: an authored dir on a migrated
platform is precisely what phase-340 W2 (work-order item 5) decided should name a
GROUP rather than a directory, and the fixture lane already does that
(`nros_fixture_strip_authored_target_dir`). These call sites are outside the
fixture lane and never got it.

- `just/freertos.just` — `build-with-tracing`, `_run-qemu` (`target-zenoh/`)
- `just/threadx-linux.just` `_run` (`target-zenoh/`)
- `just/threadx-riscv64.just` `_run-qemu` (`target-zenoh/`)
- `scripts/build/fixture-make-driver.sh` — `examples/native/rust/<role>`
  (`target-cyclonedds/`); this one IS in the native fixture lane and is the
  `native 2 (target-cyclonedds)` line phase-340 recorded as surviving wave 2
- `just/ros-editions.just` `build-e2e-fixtures` — `examples/native/rust/*`
  (`target-ros-edition-<distro>-<rmw>/`), 6 dirs × edition × rmw
- `just/px4.just` `build-examples` — `examples/px4/rust/companion/*`
  (`target-xrce/`); `px4` is NOT a migrated platform, so this one is correct
  today and becomes residue only if px4 joins `NROS_FIXTURE_SHARED_PLATFORMS`

`nuttx`'s `_run-qemu` was in this list until P2: it wrote a plain `target/` AND
hand-spelled its `-kernel` path, the exact pair phase-340 item 7 hit on esp32, so
it was fixed there rather than deferred here.

## Residue 3 — dev tool

`scripts/stack-analysis.sh` builds an arbitrary example dir into that dir's own
`target/` and then reads the ELF back out of it. It takes a directory argument,
so it has no fixed coordinate; it should probably grow a `--target-dir` of its
own rather than join a group.

## Why not fix them under P2

P2's acceptance is "the per-leaf dirs stop being recreated" for the population
that blocks P4, verified by a REBUILD, not by a gate. Every entry above needs its
consumer moved in the same commit (that is the whole lesson of #393 and of
phase-340 item 7's esp32 pack step), and each consumer is a different test. Doing
them in one change would make the rebuild that proves P2 unattributable.

## Fix sketch

Same mechanism P2 used, in preference order:

1. a `[[fixture]]` row — the build gets a coordinate, and therefore a group,
   a lane and a staleness probe, for free;
2. failing that, `nros_fixture_target_dir_flag` for the build plus its inverse
   `nros_fixture_row_artifact_dir` for the lookup, from `ONE`
   `nros_fixture_group` call so the two cannot disagree.

Never a new literal, and never a third spelling of the group key.
