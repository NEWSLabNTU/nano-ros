---
id: 1472
title: "36 tests in three workspace-excluded crates ran nowhere — `cargo test
  --workspace` reaches members, and nothing reached the rest"
status: resolved
type: bug
area: [testing, build]
found: 2026-09-24
related: [1309, 1451, phase-451]
---

# Excluded is not tested

`cargo test --workspace` reaches workspace MEMBERS. A crate in the root
`exclude` list is reached by nothing — so its tests are not slow, not flaky and
not skipped. They never execute, and nothing reports that they did not.

[phase-451](../../roadmap/phase-451-dead-build-declarations.md) W4 found this for
one crate:

> `nros-platform-stm32f4`'s three `detect_phy_type` tests DID run while it was
> briefly a member (`3 passed`) […] reaching them permanently needs a per-crate
> test lane, not membership.

Membership is not available and the phase established why: `cortex-m`,
`esp-hal` and `nros-platform-critical-section` each select a different
`critical-section` restore-state width, and critical-section refuses more than
one outright. No workspace build can hold them.

## Measured — the phase understated it by an order of magnitude

Three excluded crates have `#[test]`s that no lane runs. All of them pass:

| crate | tests | mentioned by any lane? |
| --- | --- | --- |
| `packages/drivers/net/openeth-smoltcp` | **32** | **nothing in `just/`, `scripts/` or `.github/` names it at all** |
| `packages/platform/nros-platform-stm32f4` | 3 | only `check-platform-abi-mirror.sh` |
| `packages/drivers/net/nros-smoltcp` | 1 | two scripts, neither a test lane |

36 tests, not 3. The largest of the three is the one nothing in the repository
mentions.

## The lane

`scripts/run-excluded-crate-tests.sh`, on `check::build-serial`. It parses the
root manifest's `exclude` list, keeps the paths that exist and contain a
`#[test]`, and runs `cargo test --lib` for each. **0.6 s warm** — the crates are
small and the tests are pure functions.

Discovery finding NOTHING is a failure, not a pass: three were measured when
this was written, so an empty result means the exclude list stopped parsing or
every crate became exempt.

### Exemptions are structural, and each states why

An exempt crate is one whose tests run somewhere else, or that cannot build for
the host at all. Neither is a promise to come back later, and an **unexempted**
excluded crate with tests FAILS the lane — so the list cannot grow by neglect.

* `packages/cli` — its own sub-workspace with its own lane (`check-cli-tests`).
* `third-party/*` — vendored; the tests are upstream's.
* `packages/testing/nros-px4-sitl-test` — driven by `just px4`, which owns the
  SITL it needs.
* `packages/boards/nros-board-nuttx-qemu` — measured, not assumed: `cargo test
  --lib` on the host exits 101 with ``error: entry symbol `main` declared
  multiple times``. A board crate supplies an entry point and the test harness
  supplies another.

That last one also served as the control for the failure arm before it was
exempted: the loop propagated its 101.

### The target dir

`nros_scoped_target_dir`, never a bare `cd <crate> && cargo test`. That is
phase-340 P2's defect — a build with no coordinate gets no shared cargo group
and re-creates a per-leaf `target/`. Measured here: the first hand run of these
tests left one inside `packages/platform/nros-platform-stm32f4/`.

## What this closes in phase-451

W4's last box. Its remaining text was also stale and is corrected separately
(issue 1451): `HOST_UNCHECKABLE` is already derived from
`[package.metadata.nros] embedded-only = true`, so the "make the host lane's
exclusion derived" half was done. This is the other half.

One redundancy noticed while measuring, not fixed here:
`nros-platform-stm32f4` appears BOTH in the root `exclude` list and in the
derived `HOST_UNCHECKABLE`. `--exclude` on a non-member is a no-op, so the
second is inert.
