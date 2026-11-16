---
id: 2659
title: "The test harness's QEMU ladder falls back to `$PATH` past a missing pinned QEMU, so a test can pass on an emulator CI does not run"
status: open
type: bug
area: [testing, qemu, sdk]
severity: medium
found: 2026-11-16
related: [phase-524]
---

**Home: [phase-524](../roadmap/phase-524-emulator-twins.md)** (the emulator
side of the harness).

## What happened

Main's tier 2 of 2026-11-16 failed both QEMU cases of `killed_run_verb`. They
booted MPS2 with no semihosting, the bare-metal image faulted at its first
semihosting call, and the pinned QEMU (11.0.0-nros8) ended with `qemu: fatal:
Lockup: can't escalate 3 to HardFault`. Fixed on the test side by booting
from the board descriptor.

On host 118 the same cases PASSED, at the red commit and at main. The host's
`nros setup` store held 11.0.0-nros2/6/7 but not the pinned nros8, so the
harness ladder (`nros_tests::qemu::qemu_system_path_in`) found nothing at
rungs 1-3 and returned the bare `qemu-system-arm`. That resolved to the
distro's QEMU 6.2 on `$PATH`, which HALTS a locked-up guest instead of
exiting, so the process the test waited for stayed alive and the broken boot
read green. The pinned QEMU, once installed with `nros setup --tool qemu`,
reproduced the red at once.

So a test result was about an emulator neither the index nor CI chose. This
is the "resolves vs. what it runs" class (issue 0774's router, issue 0930's
stale project prefix).

## Where the asymmetry is

`qemu_system_path_in` already treats one wrong-QEMU case as fatal:

- `QemuBin::RefusedPrefix` (a project prefix holding another version)
  **panics**: "A test must not run on an emulator nobody chose (issue 0930)".
- `QemuBin::Missing` (no pinned QEMU at all) falls to `$PATH` **silently**,
  which runs the same test on an emulator nobody chose, without a word.

The `$PATH` fallback is documented in `nros-qemu-ladder` as the harness's
choice, "so a host that never ran setup produces the documented precondition
report rather than an exec error". That holds when `$PATH` has no QEMU. When
`$PATH` has an OLD one, there is no report at all.

The harness passes `store_targets: None`, so it cannot tell a target the
store builds (where `$PATH` is the wrong emulator) from one it does not
(where `$PATH` is the only one, `QemuBin::Host`).

## Proposed fix

1. **The harness refuses a missing pinned QEMU for a target the store
   builds.** It reads `store_targets` from the index, as the CLI does. For a
   target the store builds, `Missing` becomes `nros_tests::unmet!`, naming
   the pin and `nros setup --tool qemu`. That makes it symmetric with
   `RefusedPrefix`, and is the "missing ⇒ unmet, never a quiet pass" rule
   (issue 1758). A target the store does not build keeps `$PATH`
   (`QemuBin::Host`).
2. **`just check tier-preconditions` reports the missing pin** for a lane
   whose scope boots QEMU: it lists it as unmet with the `nros setup` line,
   so the gap shows before a run, not as a confusing green.
3. **A warning when any consumer falls past the pin**, as the
   `nros-qemu-ladder` doc already requires the CLI to "say so in the note".

**Acceptance:**
- On a host whose store lacks the pinned QEMU and whose `$PATH` has one, a
  QEMU test fails `[UNMET PRECONDITION]`, naming the pin.
- `tier-preconditions` lists it.
- A store-unbuilt target still resolves to `$PATH`.
- Unit tests cover the three `QemuBin` arms with the index's target list.
