---
id: 2937
title: "The nightly stack-floor drift lane verified no Zephyr row: a pinned SDK outside the store read as unpinned, and a family with nothing verified still passed"
status: open
type: bug
severity: high
area: [ci, stack-floor, zephyr]
related: [phase-591, issue-2688, issue-2772, issue-1043]
found: 2026-11-24
---

## What happens

Since 3d92dabcd3 (issue 2688, 2026-11-21), `scripts/stack-floor-drift.py`
reports a walk as NOT VERIFIED when its build ran under another toolchain
than the tree pins. It identified a toolchain only by its SDK-store path
(`<store>/<tool>/<version>/...`). The nightly runner image installs the Zephyr
SDK at `/opt/zephyr-sdks/zephyr-sdk-0.16.9`, outside the store, so every
Zephyr row read:

    NOT VERIFIED — CMAKE_C_COMPILER is /opt/zephyr-sdks/zephyr-sdk-0.16.9/arm-zephyr-eabi/bin/arm-zephyr-eabi-gcc,
    outside the SDK store, so no pin speaks for it

That SDK IS the pinned 0.16.9. NOT VERIFIED is a reported skip (the
`nros_check_skip` ledger), so the job stayed green. In nightly runs
45115997343 and 45331150752, all five Zephyr rows of the drift table were
skipped:
- `action-server/rust/zenoh` and `talker/rust/zenoh` on nucleo-f767zi and
  mps2-an385;
- `talker/c/zenoh` on nucleo-f767zi.

So for three nights the lane checked no Zephyr image's stack floor while
reporting success. That covers the boards issue 2772's overflow was on.

## Two defects

1. **Identity.** A pinned SDK outside the store is not a different
   toolchain. The SDK states its own version in `<sdk>/sdk_version`, so the
   check should ask it.
2. **A family with nothing verified passed.** One family wholly skipped is
   indistinguishable from a family that was checked, unless the lane says
   so. This is issue 1043's third outcome ("NOT VERIFIED") read the wrong way
   round: a skip per ROW is honest, while a whole family skipped and called
   green is not.

## Fix

- `_sdk_version_of`: a compiler or SDK dir outside the store is identified
  by the `sdk_version` of the Zephyr SDK it sits in, and compared with the
  pins as a store path would be. A path in no SDK is still NOT VERIFIED, as
  before.
- Each row has a family, the last word of its board (`zephyr`, `freertos`,
  `baremetal`). The lane prints, per family, how many rows were verified,
  not verified and not walked. A family with rows and none verified FAILS
  the lane, naming the family.

## Acceptance

- Self-tests, each failing when its rule is removed (both mutations
  measured):
  - the pinned SDK outside the store is verified;
  - an unpinned one is NOT VERIFIED, naming its version;
  - a path in no SDK still is;
  - a family whose every row is NOT VERIFIED, or not walked, fails, and only
    that family does;
  - one verified row keeps a family passing, with its skips counted.
- The next nightly's drift job reports its Zephyr rows as verified (PASS,
  NOTE or FAIL), with the per-family table in its log.
