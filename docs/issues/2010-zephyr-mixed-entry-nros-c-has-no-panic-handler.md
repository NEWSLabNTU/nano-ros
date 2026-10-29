---
id: 2010
title: "The Zephyr mixed workspace entry's `nros-c` staticlib has no `#[panic_handler]` since nros-c lost `std`"
status: open
type: bug
area: [zephyr, ci, build, api]
severity: medium
found: 2026-10-23
related: [1742, 1858, phase-513]
---

**Home: [phase-513](../roadmap/phase-513-nightly-reds-with-no-home.md)** — R1.

## Measured

- **Run:** nightly 39916704136 (scheduled 07:16 UTC, head `4ddf7c744`).
- **Job:** 120291717660, `tier 2 nightly (pairwise cover)`, failing in step
  `just build tier2-nightly`.
- **Failure:** the only family that failed was `zephyr`. Its first-error
  excerpt names a single build dir,
  `build/zephyr-workspace-builds/3.7/build-ws-mixed-entry-zenoh` (the
  `workspace-zephyr-mixed` fixture row, west id
  `zephyr/native_sim/native/64/workspace-entry-mixed`):

```
error: `#[panic_handler]` function required, but not found
error: could not compile `nros-c` (lib) due to 1 previous error
FATAL ERROR: command exited with status 101: /usr/bin/cmake --build .../build/zephyr-workspace-builds/3.7/build-ws-mixed-entry-zenoh
```

- **Everything else passed:** the same night's other 12 jobs, which include
  the eight Zephyr 3.7/4.4 C/C++ talker/listener cells (single-language
  images), built and passed on the same head.

## Probable cause (not yet confirmed by a build)

phase-359 W10 stage C (e5f29e1c8e, PR #2485, merged 2026-10-23) deleted
`std` from `nros-c` / `nros-cpp`. Before that, a staticlib built with `std`
got its panic handler from libstd. Now the handler has to come from
`nros-c`'s `panic-platform` (or `panic-halt`) feature, selected by the
image's panic policy (`nros_panic_policy_feature`, `NanoRosFeatureSet.cmake`).
The stage C notes cover the posix, threadx and freertos-posix roads.

This Zephyr workspace road, which builds a MIXED entry's `nros-c` through
west, reaches `nros-c` with neither feature selected. So the crate is
`#![no_std]` with no handler, and rustc refuses the staticlib.

Which of these is the gap is not measured yet:

- the Zephyr workspace lane never calls the panic-policy apply;
- it calls it after the `nros-c` target is configured;
- the mixed entry's cargo key misses `panic-platform`, which is issue 1858's
  shape.

## What it is NOT

- **Not the P7 `nros_tier_spec_t` compile break** (fixed by #2511,
  `4ddf7c744`). This run is ON that fix, and the Zephyr cells that break took
  down now pass.
- **Not issue 1742.** That was E0152, a DUPLICATE `panic_impl` with `std`
  present. This is the opposite: no handler at all.
- **Not disk exhaustion (1353), nor a runner fault.** The build reports a
  rustc error, and the lane ran to its recipe failure.

## What would close it

1. Reproduce: `just build-test-fixtures lane=tier2-nightly` narrowed to the
   `workspace-zephyr-mixed` row. Then read the generated cargo invocation for
   `nros-c` in that build dir and see which features it got.
2. Route the panic policy to the Zephyr workspace road, the way the posix,
   threadx and freertos arms already do, so the mixed entry's `nros-c` gets
   `panic-platform` (ending in `nros_platform_panic`). Do this in the shared
   helper, not in this one entry.
3. Acceptance: a tier-2 nightly where `build-ws-mixed-entry-zenoh` builds,
   and `entry_e2e`'s zephyr mixed cell runs.

## Acceptance is blocked, 2026-10-29

The tier-2 nightly cannot run this issue's cell yet. The cell skips because
its shared observer `workspace-c-native-robot2` (`linux,c,zenoh`) is out of
lane, before its own fixture is resolved. See
[issue 2139](2139-entry-cells-shared-peer-out-of-lane.md).
