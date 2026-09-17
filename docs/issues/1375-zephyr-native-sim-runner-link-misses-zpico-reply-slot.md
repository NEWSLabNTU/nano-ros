---
id: 1375
title: "The native_sim runner link cannot see the zpico reply-slot symbols the
  Rust side calls — every Zephyr fixture fails to link, and tier 2 is the only
  lane that would have said so"
status: open
type: bug
area: [zephyr, rmw-zenoh, build]
severity: high
found: 2026-09-17
related: [issue-0902, issue-1332, issue-1365, issue-0056, issue-0475]
---

## What happens

`just build tier2-nightly` cannot build a single Zephyr fixture. The Zephyr
image itself links; the **native_sim runner** link that follows does not:

```
[9/12] Linking C executable zephyr/zephyr.elf
[10/12] Building native simulator runner, and linking final executable
FAILED: [code=2] zephyr/CMakeFiles/native_runner_executable zephyr/zephyr.exe
/usr/bin/ld: …/zephyr.elf.loc_cpusw.o: in function
  `<nros_rmw_zenoh::zpico::Context>::take_reply_slot_announcement':
  packages/rmw/zenoh/nros-rmw-zenoh/src/zpico.rs:909:
  undefined reference to `zpico_reply_slot_take_announcement'
/usr/bin/ld: …/zephyr.elf.loc_cpusw.o: in function
  `<nros_rmw_zenoh::zpico::Context>::reply_slot_stats':
  packages/rmw/zenoh/nros-rmw-zenoh/src/zpico.rs:879:
  undefined reference to `zpico_reply_slot_stats'
collect2: error: ld returned 1 exit status
```

Evidence: nightly 35184836117 (schedule, 2026-09-17 05:12 UTC, `fa6cf68d`), job
105084553092 `tier 2 nightly (pairwise cover)`, step `just build tier2-nightly`,
building `build-rust-talker-zenoh` on zephyr 3.7 / native_sim. The build stops
at the first leaf, so the count of affected fixtures is a floor, not a measure.

## Both symbols exist, and are compiled for this target

They are not feature-gated and not missing:

- declared `packages/rmw/zenoh/zpico-sys/c/include/zpico.h:809` and `:826`
- defined, under no enclosing `#if`, `packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c:3252`
  and `:3330`
- and `zpico.c` IS in the Zephyr module's source list —
  `zephyr/cmake/nros_rmw_zenoh.cmake` adds it with `zephyr_library_sources`,
  which is why `[8/12] Linking C static library modules/nros/libnros.a` and the
  `zephyr.elf` link both succeed.

So the failing question is not "was the C compiled" but **what the native_sim
second link stage has on its line**. `zephyr.elf` links with `libnros.a`
available; the runner link consumes `zephyr.elf.loc_cpusw.o` and does not
resolve these two — a member of `libnros.a` is only pulled in for a symbol
undefined *at the moment the archive is scanned*, which is the ordering failure
[[issue-0056]] fixed for the message-FFI staticlibs and [[issue-0475]] fixed for
a backend reached through a raw `-Wl,` flag. This site was never covered by
either.

## Why nobody saw it for five days

The callers landed in `022e94157` (2026-09-12, phase-455 W1, issue [[issue-0902]]
/ [[issue-1332]]): `zpico.rs:879` and `:909` call the two C functions, both added
in the same commit. Nothing merge-gating builds a Zephyr fixture — the nightly's
own `zephyr *` cells die earlier in `_setup-common` ([[issue-1359]]), and tier 2
is the only lane that reaches a Zephyr link.

Tier 2 produced **no verdict at all from 2026-09-14 to 2026-09-17** because the
self-hosted runner was unregistered ([[issue-1365]]). The run above is the first
tier-2 job to start since the runner came back, and it failed on its first leaf.
The lane was not quiet because the tree was healthy; it was quiet because
nothing ran.

## What this is NOT

- **Not [[issue-1360]].** That is the persistent `~/.nros/workspaces/zephyr/3.7`
  codegen-version `#error`. This build gets past codegen — the only
  codegen-version lines in the log are the `NROS_SKIP_VERSION_CHECK=1` bypass
  warnings — and fails at `ld`.
- **Not [[issue-1158]].** That is tier 2 stopping at provisioning or build
  before the cells. This one reaches a per-leaf link, which is further than 1158
  ever got, and names two symbols.
- **Not a missing definition or a feature gate.** Both functions are defined
  unconditionally in a TU this build compiles; `nm` on `libnros.a` is the check
  that makes that concrete.
- **Not the `zpico-sys/src/ffi.rs` stubs.** Those `#[unsafe(no_mangle)] pub
  extern "C"` definitions are the pure-Rust no-op build's providers; on Zephyr
  the C TU is meant to win, and whichever is intended here, the runner link sees
  neither.

## What would close it

1. The native_sim runner link resolving both symbols — whichever way the seam is
   meant to work: whole-archiving `libnros.a` into the runner link (the 0056
   shape), or making the referencing objects and the archive land in an order
   ld can resolve. Verify with `ninja -C <build-dir> -t query zephyr.exe` and a
   `touch` of `zpico.c`, not with a wipe.
2. A **merge-gating** reason to notice next time. Today the only lane that links
   a Zephyr fixture is tier 2, which runs nightly on one self-hosted machine; a
   five-day outage of that machine is also a five-day outage of every Zephyr
   link check. That is the same argument [[issue-1040]] makes for a gate that
   runs nowhere.
