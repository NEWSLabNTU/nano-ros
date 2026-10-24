---
id: 2025
title: "Zephyr native_sim XRCE C/C++ leaves fail to link: `nros-platform-posix`'s platform.c is in both libnros.a and libnros_cpp.a"
status: open
type: bug
area: [zephyr, build, rmw-xrce, platform]
severity: high
found: 2026-10-24
related: [issue-0282, issue-1824]
---

**Home: none.** Seen on the scheduled tier-2 run of 2026-10-24.

## What failed

Scheduled `run-matrix` run **40070127626** (head `62b661ac1c`). Both jobs that
build Zephyr fixtures went red in the build stage, so they reported no cell
verdicts:

| job | id | failing leaves |
| --- | --- | --- |
| `tier 1 (cells)` | 120804235709 | `build-c-service-server-xrce` |
| `tier 2 (1-wise matrix)` | 120801569218 | `build-c-listener-xrce`, `build-c-talker-xrce`, `build-c-service-client-xrce` |

Every failure is a link error. The linker finds the POSIX platform port twice,
once in the Zephyr module archive and once inside the Rust `nros-cpp`
staticlib:

```
/usr/bin/ld.bfd: nros-rust/x86_64-unknown-linux-gnu/nros-relwithdebinfo/libnros_cpp.a(10fc8707dfe1bebe-platform.o): in function `nros_platform_wake_drop':
.../packages/platform/nros-platform-posix/src/platform.c:776: multiple definition of `nros_platform_wake_drop'; modules/nros/libnros.a(platform.c.obj):...
collect2: error: ld returned 1 exit status
```

16 distinct `nros_platform_*` symbols are duplicated, among them
`critical_section_acquire/release`, `log_write/flush`,
`task_storage_size/align`, `task_stack_unused_bytes[_upto]` and
`wake_park_granularity_us`. In tier 1 that leaves 0 of 6 Zephyr images built.

**Second sighting, the 05:12 nightly.** Run **40065220667** (head
`3c99fb35f0`), job **120786927499** (`tier 2 nightly (pairwise cover)`), the
only red job of 31 (25 success, 5 skipped). It shows the same 16-symbol
`multiple definition` in the Zephyr fixture build, and it reaches C++ as well
as C: `build-{c,cpp}-listener-xrce`, `build-c-talker-xrce`,
`build-c-service-{client,server}-xrce` and
`build-{c,cpp}-action-{client,server}-xrce`. So the class is "every Zephyr
native_sim XRCE C/C++ leaf", not only the four C leaves the tier-2 run built.

## What it is NOT

- **Not yesterday's Zephyr red.** Run 39911647821 (head `6e00553ede`) failed in
  the same jobs on `zephyr_run_tiers.c: unknown type name 'nros_tier_spec_t'`.
  That was phase-509 P7, fixed by #2511 (4ddf7c744f). The duplicate-symbol
  shape first appears in today's run, so it entered between `6e00553ede` and
  `62b661ac1c`.
- **Not issue 1353 (disk) or 1757.** There is no disk message, and these are
  C/XRCE leaves, not the Kconfig overlay case 1757 covered.

## Candidates (unmeasured)

These commits in that window touch the Zephyr or nros-c/nros-cpp build roads:

- a555b45da5 #1824 D1 (the Corrosion road places config headers from `$OUT_DIR`)
- 03e06fd961 and 53218f904e phase-478 (`zephyr/cmake/nros_cargo_build.cmake`,
  `zephyr/CMakeLists.txt`)
- bf30f24fca phase-478 D4 (`nros-platform-zephyr/src/platform.c`, `zephyr/Kconfig`)
- the phase-509 P9b/P10 executor and level-runner commits

None was bisected; each needs a Zephyr build. The question that decides it is
why a native_sim C image's `libnros_cpp.a` carries a compiled `platform.o` from
`nros-platform-posix`. Either the cargo invocation now selects a platform
feature it did not before, or the module now also compiles the POSIX port.

## What would close it

Build one of the four leaves (for example
`zephyr-workspace-builds/3.7/build-c-listener-xrce`) at `62b661ac1c`. Find which
archive should not carry `platform.o`, fix the road that put it there, and see
a green Zephyr build in the next scheduled `run-matrix`.
