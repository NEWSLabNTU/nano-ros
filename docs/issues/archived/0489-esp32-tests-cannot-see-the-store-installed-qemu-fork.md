---
id: 489
title: "Every ESP32 test skipped \"qemu-system-riscv32 not available\" on a host
  where `nros setup --tool esp32-qemu` had just succeeded"
status: resolved
resolved_in: phase-340
type: bug
area: testing
related: [issue-0486, issue-0487, issue-0400, phase-340]
---

## Symptom

```
$ nros setup --tool esp32-qemu     # succeeds, installs the Espressif fork
$ cargo test -p nros-tests --test esp32_emulator
[SKIPPED] qemu-system-riscv32 not available
Install Espressif's QEMU fork: nros setup --tool esp32-qemu (or: just esp32 setup)
```

The skip message names the command that had just been run successfully.

## Cause

`packages/testing/nros-tests/src/esp32.rs` spelled the binary by bare name at
three sites — the availability probe and both launchers:

```rust
Command::new("qemu-system-riscv32")
```

A bare name resolves through `$PATH`, which finds the SYSTEM
`qemu-system-riscv32`. That one has no `esp32c3` machine model — which is
precisely what the probe then rejects:

```rust
text.contains("esp32c3")     // correct probe, unreachable binary
```

`nros setup` installs the fork into the SDK store
(`~/.nros/sdk/esp32-qemu/<version>/bin/`), and `activate.sh` **deliberately**
keeps qemu off the global PATH — the `build/<tool>` convention, stated in its own
comment: "Unlike qemu — which the test harness resolves via a `build/<tool>`
prefix and deliberately keeps OFF the global PATH". So PATH was never going to
bridge the gap; the harness had to resolve it, and for ESP32 it never did.

The arm family had the resolver all along
(`crate::qemu::qemu_system_arm_path` — env override → `build/qemu` →
`nros_store_bin` → PATH fallback). ESP32 was written without the equivalent, so
the store branch simply did not exist for it.

## Fix

`qemu_system_riscv32_path()` / `qemu_system_riscv32_cmd()`, the ESP32 twin of
the arm resolver, with the same order and the same reasoning for each rung:

1. `QEMU_SYSTEM_RISCV32` — developer override / CI pin.
2. `nros_store_bin("esp32-qemu", "qemu-system-riscv32")` — **the missing rung.**
3. Bare name on `$PATH` — kept, so a host that never ran setup produces the
   documented `[SKIPPED]` instead of an exec error.

All three call sites converted.

## Verified

```
SUCCESS: ESP32-C3 QEMU boots and shows platform banner
test result: ok. 1 passed
```

The suite went from 3 passing (detection probes only, everything real skipped)
to 4, with the boot test running an actual ESP32-C3 image for the first time in
this tree.

## Why it hid for so long

A skip is not a failure, and this one printed a plausible, actionable-looking
remedy. A reader who ran that remedy got a successful install and an unchanged
result, which reads as "still missing something" rather than "the harness cannot
see it" — the same class as issues 0481 (readiness greps waiting on markers the
process never prints), 0483 (sixteen tests skipping on a missing prerequisite)
and 0445 (a STALE verdict absorbing the run). **A guard that names a remedy
should be tested against a host that has applied it.**

Found only because three separate provisioning defects had to be cleared first
(0486 espflash undeclared, 0487 the single-probe system check, and QEMU's
`-Werror` against gcc 16) before anything could get far enough to expose it.

## Note for the box

`scripts/dev/ros2-box-env.sh` sets `NROS_HOME=~/.nros-box` (issue 0400's
box-private store), so a fork provisioned on the host is not visible inside the
distrobox and vice versa. The resolver honours `NROS_HOME` through
`nros_store_bin`, so both work — but each tree needs its own
`nros setup --tool esp32-qemu`. These tests need QEMU and espflash, not ROS 2,
so the host is the cheaper place to run them.
