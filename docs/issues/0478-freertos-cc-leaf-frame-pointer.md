---
id: 478
title: "cc-rs sends `-mno-omit-leaf-frame-pointer` to `arm-none-eabi-gcc`, killing every freertos fixture build"
status: open
type: bug
area: build
related: [issue-0477, phase-340, phase-334]
---

## Symptom

`just build-test-fixtures lane=tier2` (and `lane=all`) fails in the `freertos`
module with `rc=101`. Every other module — `native`, `qemu`, `nuttx`, `esp32`,
`threadx_linux`, `threadx_riscv64` — is OK in the same run.

```
cargo:warning=arm-none-eabi-gcc: error: unrecognized command-line option
  '-mno-omit-leaf-frame-pointer'; did you mean '-fno-omit-frame-pointer'?

error occurred in cc-rs: command did not execute successfully (status code exit status: 1)
  … arm-none-eabi-gcc -Os -ffunction-sections -fdata-sections -g
    -fno-omit-frame-pointer -mno-omit-leaf-frame-pointer -mthumb -march=armv7-m …
    -c packages/rmw/zenoh/zpico-sys/zenoh-pico/src/api/admin_space.c
```

## Cause

`-mno-omit-leaf-frame-pointer` is a **clang** flag. GCC does not have it, and
`arm-none-eabi-gcc` rejects it outright rather than warning.

Nothing in this repo passes it. cc-rs adds it itself when it decides to force a
frame pointer, which it does off the profile's debug setting — and
`nros-relwithdebinfo` carries `debug = 1`. So the flag appears on a bare-metal
GCC target purely from cc-rs defaults.

**Why now, when `debug = 1` is old:** the workspace lock that pins `cc` for this
build is `examples/workspaces/mixed/`'s, which is GENERATED per host by
`nros sync` and not tracked. A fresh resolve picked up a newer `cc` whose
frame-pointer logic emits the flag for `thumbv7m-none-eabi`. The root lock has
`cc 1.2.63`, but that is not the lock this build resolves through.

That makes it the untracked-generated-lock class: nothing changed in a commit,
and the failure still arrived. Same shape as issue 0477 — build state, not code
— which is worth noting because two in a row now have looked like regressions
and were not.

## Fix — one shared constructor, not five call sites

`cc::Build::new()` appears at ~5 places:

* `packages/rmw/zenoh/nros-zpico-build/src/runner.rs` — lines 604, 639, 802
* `packages/tooling/nros-build-helpers/src/c.rs` — line 467
* `packages/tooling/nros-build-helpers/src/shared.rs`

Sprinkling `.force_frame_pointer(false)` across them is the second-spelling
mistake CLAUDE.md names explicitly — and the next `cc::Build::new()` anyone adds
would miss it. Add ONE constructor (`nros_cc_build()`) that returns a
`cc::Build` with the bare-metal defaults already applied, route every site
through it, and gate on the raw `cc::Build::new()` spelling so a new one cannot
appear ungoverned.

Check whether it should key on "target is bare-metal" or on "compiler is GCC"
before writing it; the flag is legal under clang, so a blanket disable gives up
frame pointers where they work.

## Reproduce

```sh
source ./activate.sh
just build-test-fixtures lane=tier2      # freertos == FAILED (rc=101)
```

## Blocks

`just ci-matrix` — tier 2 gates on the tier-2 fixture lane, and this is now the
only module failing it. Issue 0477 (the NuttX ROM overflow) was the other gate
and is resolved; `nuttx == OK` in the same run.
