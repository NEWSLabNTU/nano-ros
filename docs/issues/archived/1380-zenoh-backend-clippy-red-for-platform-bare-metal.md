---
id: 1380
title: "`nros-rmw-zenoh` fails `clippy -D warnings` under `platform-bare-metal`
  and has for some time — no lane runs clippy on that feature combination"
status: resolved
type: bug
area: [ci, build]
severity: medium
found: 2026-09-17
related: [issue-1040, issue-1226, issue-0196, phase-505]
resolved_in: "PR #2290"
---

**Home: [phase-505](../../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What happens

```
cargo clippy -p nros-rmw-zenoh --no-default-features \
  --features "platform-bare-metal,ros-humble" \
  --lib --target thumbv7m-none-eabi -- -D warnings
```

```
error: unneeded `return` statement
    --> packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs:1220:13
error: could not compile `nros-rmw-zenoh` (lib) due to 1 previous error
```

The site is the bare-metal arm of `check_liveliness_and_fire`:

```rust
#[cfg(feature = "platform-bare-metal")]
{
    return;
}
```

The `return` is the LAST statement of the function under that cfg, so clippy's
`needless_return` fires. Under every other platform feature the arm is
compiled out and the lint never sees it.

## Why nothing caught it

`check-workspace-all` sweeps feature combinations, and `check` runs the
embedded clippy lanes, but neither reaches `-p nros-rmw-zenoh
--features platform-bare-metal` on a `thumbv7m-none-eabi` target. This is the
0196 shape again — a gate whose REACH is narrower than the rule it enforces —
and the 2026-07-28 audit's finding restated on a new lane: "clippy is clean" is
a claim about the feature combinations somebody enumerated, not about the
crate.

Found while adding the phase-392 amendment B instrument, whose own
`platform-bare-metal` clippy run surfaced this one beside it. Confirmed
PRE-EXISTING by running the same command with the new feature absent.

## Direction

Two separate fixes, and the second is the one that matters:

1. The lint itself — `{}` instead of `{ return; }`, or hoist the cfg to the
   function. One line.
2. **The reach.** A backend that ships on five platform features should have
   all five in a clippy lane, or the four that are not there will drift the
   same way. Whether that belongs in `check-workspace-all` (which already
   enumerates) or in the embedded clippy lane is the design question; adding
   only combination five by hand repeats the defect.

## Resolution (2026-10-16, PR #2290; recorded 2026-10-18)

Both halves landed in PR #2290, which added the cross half of
`scripts/check-feature-sweep.py`. The issue stayed open only because its status
was not updated.

1. **The lint.** The bare-metal arm of `check_liveliness_and_fire` is gone.
   The function's body is now a single `#[cfg(not(feature = "platform-bare-metal"))]`
   block, so bare metal compiles an empty body and `needless_return` has nothing
   to fire on.
2. **The reach.** The sweep's `CROSS` table holds a run for every zpico
   platform feature of `nros-rmw-zenoh`: `platform-freertos`, `-threadx`,
   `-zephyr`, `-bare-metal` and `-nuttx`. Each run uses the triple and
   environment from the board that ships the feature. The table is checked
   both ways against the host `SKIP` list, so a cross-only feature with no
   cross run is a script error rather than a silent gap. That is the "all
   five, not combination five by hand" shape this issue asked for.

**Measured.** The scheduled `host-tests.yml` run 39093104537 (2026-10-18), job
`per-feature clippy (host + cross)`, succeeded with:

```
ok  nros-rmw-zenoh [alloc,platform-freertos] --target thumbv7m-none-eabi (board nros-board-mps2-an385-freertos)
ok  nros-rmw-zenoh [alloc,platform-threadx] --target riscv64gc-unknown-none-elf (board nros-board-threadx-qemu-riscv64)
ok  nros-rmw-zenoh [alloc,platform-zephyr] --target thumbv7m-none-eabi
ok  nros-rmw-zenoh [alloc,platform-bare-metal] --target thumbv7m-none-eabi
ok  nros-rmw-zenoh [alloc,platform-nuttx] --target armv7a-nuttx-eabihf (board nros-board-nuttx-qemu)
```

The sweep is scheduled, not merge-gating (about 330 clippy runs). So a new
red in one of these combinations is reported the next morning rather than at
the pull request. That is the trade the sweep's docstring records.
