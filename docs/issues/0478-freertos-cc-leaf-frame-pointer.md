---
id: 478
title: "cc-rs sends `-mno-omit-leaf-frame-pointer` to `arm-none-eabi-gcc`, killing every freertos fixture build"
status: resolved
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

## Fix (landed)

`nros_cc_flags::gcc_safe_frame_pointer` turns cc-rs's automatic pair off and
re-adds `-fno-omit-frame-pointer`, the half gcc understands — so a debug build
still gets a frame pointer instead of the policy being thrown away. clang and
MSVC are untouched: the flag is legal under clang, and a blanket disable would
give up leaf frame pointers on the toolchain that supports them.

**Where it is called matters more than what it does.** `strict_decls` is already
the one function every nano-ros C compile calls, so the new helper is invoked
from inside it — that fixed the failing site and ~20 others with zero call-site
edits, and leaves no site for the next person to miss.

Seven sites route through neither helper and had to name it directly:
`nros-board-freertos/build.rs` (4) and `nros-board-mps2-an385-freertos/build.rs`
(3). Note they also miss the issue-0383 diagnostics — a real gap, but adding
`strict_decls` to vendored FreeRTOS/lwIP would turn long-standing warnings into
errors, which belongs to 0383 and not here.

### The gate

`check-cc-build-policy` (in `just check`) requires any file constructing a
`cc::Build` to name the helper crate. Both classes that escaped — 0383's
diagnostics and this one — escaped through an unrouted call site, so the
structural half is refusing to let a new one appear undecided.

It checks presence per FILE, not per construction; a per-site check needs a Rust
parser. Under the issue-0196 rule that is the right trade: a narrow gate that
looks healthy is worse than a coarse one that makes someone look. Tripwired both
ways — it fails on an ungoverned site and passes clean, and it correctly ignores
`threadx_sources.rs`, whose three `cc::Build::new()` occurrences are all inside
doc comments.

### Verified

`just build-test-fixtures lane=tier2` — all eight modules OK, `freertos`
included, zero `-mno-omit-leaf-frame-pointer` occurrences, stamp written. Run
twice: once for the helper, once again after the seven call sites were hooked.

## Reproduce

```sh
source ./activate.sh
just build-test-fixtures lane=tier2      # freertos == FAILED (rc=101)
```

## Blocks

`just ci-matrix` — tier 2 gates on the tier-2 fixture lane, and this is now the
only module failing it. Issue 0477 (the NuttX ROM overflow) was the other gate
and is resolved; `nuttx == OK` in the same run.
