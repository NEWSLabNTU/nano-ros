---
id: 650
title: "A fixture lane that skips every step still prints `<platform> test fixtures built.` and exits 0"
status: resolved
type: bug
severity: high
area: build/ci
related: [issue-0599, issue-0445, issue-0196, phase-366]
---

## Symptom

On a host without the RISC-V bare-metal toolchain:

```
$ just threadx_riscv64 build-fixtures
ThreadX-RV64 skip: riscv64-unknown-elf-gcc not found
ThreadX-RV64 test fixtures built.
$ echo $?
0
```

Nothing was built. The lane says it built its fixtures, in its own words, and
exits 0. `build-test-fixtures` then records `== threadx-riscv64 == OK`.

## Why it matters, measured

This is how a real defect reached main. phase-366 W5.c moved a
`#[panic_handler]` into six `qemu-riscv64-threadx` example `lib.rs` files, which
diverged them from their `native` copies and broke `example_portability` — a
gate whose divergence ratchet phase-338 had drained to empty. The lane that
would have compiled those images reported OK on the author's host and on mine,
because neither had the toolchain, so the only signal left was a source-level
gate in a different lane, one run later, attributed to whatever else changed.

A skip that reports success does not merely hide itself; it removes the
evidence that anything is unverified, which is issue 0445's rule
("a STALE verdict is ABSORBING — read what did NOT run") applied to the build
side.

## Cause

Issue 0599 already named this exact defect and built the fix: `nros_lane_skip`
prints an `NROS_LANE_SKIP:` marker and exits 78, which the driver renders as
SKIPPED. It was applied to three lanes (px4, qemu-baremetal, zephyr-ci) and its
own commentary says why — *"six sites across three lanes had the same `exit 0`
and fixing one would have left five."*

Twenty-one sites in five other lanes were not converted, in two spellings the
first sweep's grep would not have matched together:

```sh
echo "FreeRTOS skip: arm-none-eabi-gcc not found"; exit 0    # one line
echo "Zephyr/FVP-ws-entry skip: … toolchain missing"          # two lines
exit 0
```

`nros_lane_skip` alone does not fit most of them, and that is why they were left:
these are STEPS, not whole lanes. nuttx builds arm and riscv; a host with one
toolchain and not the other should still get the half it can build, and exiting
78 from the first step would abort the rest.

## Fix

A third shape beside 0599's, in the same file, for the partial case:

* `nros_lane_skip_note <lane> "<reason>"` — a step records why it did nothing
  and returns; the lane keeps going and builds what it can.
* `nros_lane_skip_flush <lane> "<success line>"` — the **only** place a lane
  claims it built its fixtures. Clean lane: prints the line, exits 0. Any step
  skipped: prints every reason and exits 78, so the driver says SKIPPED.
* `nros_lane_skip_reset <lane>` — at the lane's first step, so a lane that is
  complete this time is not reported SKIPPED from last run's notes.

A file under `build/lane-skips/` is the channel because each step runs as its
own `just` invocation — no shell state survives between them.

Whole-recipe preconditions (the four Zephyr/FVP recipes) take 0599's original
`nros_lane_skip` instead: nothing partial is possible there.

`nuttx build-fixtures` had **no body at all** — it inherited exit 0 from its two
steps — so it never had a success claim to make honest. It has one now.

## Verification

* `just threadx_riscv64 build-fixtures` → **rc 78**, both skipped steps named,
  and no "fixtures built" line:
  ```
  lane threadx-riscv64 INCOMPLETE — 2 step(s) skipped, so its fixtures are NOT built:
    - Skipping ThreadX QEMU RISC-V examples (riscv64-unknown-elf-gcc not found)
    - riscv64-unknown-elf-gcc not found
  ```
* `just threadx_linux build-fixtures` (a lane this host CAN build) → rc 0,
  `ThreadX-Linux test fixtures built.` — the success path is unchanged.
* `scripts/check-lane-skip-protocol.py` fails on either spelling of a raw
  `exit 0` skip in a lane recipe, with self-tests in both directions.

## Adjacent, not fixed here

The RISC-V toolchain mismatch that exposed this: `nros setup` provisions
`riscv-none-elf-gcc` (xPack) while the ThreadX lane requires
`riscv64-unknown-elf-gcc` with picolibc specs. Shimming the names gets as far as
netxduo and fails on an unrelated libc difference (implicit declaration of
`rand`). So the lane cannot run on a host provisioned entirely by `nros setup` —
which is issue 0625's territory (a provisioned tool is SEARCHED for, not
constructed), and is now at least VISIBLE rather than reported as OK.
