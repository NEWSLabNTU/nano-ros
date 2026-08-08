---
id: 483
title: "All 16 emulator tests skip on a missing libzenohpico.a, and the skip
  message names a recipe that does not exist"
status: open
type: bug
area: testing
related: [issue-0481, phase-342, issue-0196]
---

## Symptom

Every test in `packages/testing/nros-tests/tests/emulator.rs` — 16 of them,
covering the QEMU bare-metal RTIC pubsub / service / action / serial / XRCE
lanes — skips in ~0.06 s:

```
[SKIPPED] zenoh-pico arm build not available
```

The suite reports success. Nothing has run.

## Why it is not self-correcting

The guard is `qemu::is_zenoh_pico_arm_available()`
(`packages/testing/nros-tests/src/qemu.rs:1158`), which checks for
`build/qemu-zenoh-pico/libzenohpico.a`. Three things then compound:

1. **The remedy it prints does not exist.** The doc comment above it says the
   library "is built with `just build-zenoh-pico-arm`". There is no such recipe:

   ```
   $ just build-zenoh-pico-arm
   error: justfile does not contain recipe `build-zenoh-pico-arm`
   ```

   The nearest name, `just build-zenoh-pico`, is described as "standalone, for
   debugging" and does not target ARM.

2. **The obvious command does not produce it.** `just qemu build-fixtures`
   completes successfully and does NOT build this library, so a developer who
   builds fixtures and runs the suite sees 16 skips and a green result.

3. **The real producer is a script, not a recipe.**
   `scripts/qemu/build-zenoh-pico.sh` builds it (125 sources → 3.2 MiB), and
   `just qemu doctor` knows the path — it prints
   `[MISSING] qemu-zenoh-pico (run: just qemu setup)`. So the knowledge exists in
   `doctor` and in the script, and is absent from the place that stops the tests.

## Evidence

Found while verifying a phase-342 W8b change to those tests: the conversion could
not be validated because the tests never reached the changed code. Building the
library by hand and re-running:

```
before   16 tests run:  0 passed, 16 skipped     (~1 s)
after    16 tests run: 16 passed, 0 skipped      (116 s)
```

**116 seconds of real QEMU coverage was being reported as a pass in one second.**

The conversion under test also found a genuine defect once it could run (a
readiness marker keyed to the wrong role), which is the direct cost of this
gap: the suite could not have caught it either.

## Class

Issue 0196's rule — a gate must cover the class it enforces — and this session's
recurring shape: **a lane that reports success while testing nothing.** Siblings
already recorded: `check-fast` failing in 0.77 s having checked nothing (0466),
`cargo check` replaying a 0.18 s cache, `wait_for_output_pattern` returning `Ok`
on timeout (0471). Same signature every time — the green means "did not run",
not "passed".

## Fix

1. **Make the printed remedy real.** Either add `just build-zenoh-pico-arm` as a
   thin wrapper over `scripts/qemu/build-zenoh-pico.sh`, or change the message to
   name `just qemu setup`, which `just qemu doctor` already prints. The two must
   agree — they are two spellings of one fact, which is the drift class that
   produced issue 0483 in the first place.
2. **Decide whether `just qemu build-fixtures` should build it.** A fixture build
   that leaves the suite unable to run is surprising; if the separation is
   deliberate (the library is a prerequisite, not a fixture), the recipe should
   say so and point at the prerequisite.
3. **Consider whether this skip should be a skip at all.** On a machine with the
   ARM toolchain present, "library not built" is a SETUP gap, not an
   unsupported-platform gap. `_require-fixtures` already models exactly this
   distinction for fixtures and fails with a remedy instead of skipping.
