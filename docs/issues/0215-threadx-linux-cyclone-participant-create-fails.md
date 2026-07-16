---
id: 215
title: "threadx-linux Cyclone lane: freshly built talker exits silently — dds_create_participant returns -1 (museum binary hid it)"
status: open
type: bug
severity: medium
area: platform-threadx
related: [issue-0205, issue-0214, issue-0195]
---

## Summary (found 2026-07-16, during #206 validation)

`test_threadx_linux_cyclonedds_talker_to_native_listener` fails (0 samples)
on a **freshly rebuilt** `just threadx_linux build-fixtures` talker. The
long-green history was a museum binary (the #164 LKG trap): the lane last
truly passed in the 2026-07-08 sweep; every rebuild since some commit in the
2026-07-08..07-16 window produces a broken image.

## Evidence

- Fresh `threadx_c_talker` (cyclonedds) exits within ~1 ms of app start,
  printing nothing:
  ```
  [app_thread] Calling app_main (weak)...
  [app_thread] app_main returned
  ```
- gdb: `nros_support_init` returns **-1** (`NROS_RET_ERROR`);
  `dds_create_participant` returns **-1**; `rtps_init` returns **0** (OK) —
  so participant creation fails *after* RTPS stack init, inside the
  domain/participant entity path.
- **Not #206**: reproduced identically with the #206 resolver changes
  stashed (pristine `main`, same fixture builder) — pre-existing on `main`.
- Still reproduces at `b2a0810e8` (post-#214).
- The native cyclone listener (host fixture) is healthy — every other
  native↔native cyclonedds pairing in `native_api` passes.

## Secondary defect (hides the primary one)

`NROS_CHECK_LOG`'s `printf` output never reaches fd 1 under threadx-linux
(strace shows no write), despite the talker's `setvbuf(stdout, NULL,
_IOLBF, 0)` — the `[nros] main.c:102 nros_support_init(...) -> -1` line
that would have made this loud is silently swallowed (likely the tx_linux
signal-scheduler + buffered stdio interaction; `nros_board_log` writes
reach the fd fine). Fix the visibility along with the root cause, else the
next regression in this lane is silent again.

## Suspect window

Commits touching this path between the 2026-07-08 green sweep and 07-16:
- `e04eedd8e` — #205 steps 2-4: board macro `app_main` + board-owned
  anchors + CMake seam (reworked exactly the boot glue in play).
- `d7f9e43b6` — #195: threadx-riscv64 cyclone `.init_array` walk + stack
  rebuild (ctor handling changed for threadx cyclone images).
- `a9c6b1f88` — 290-W1/W2 platform-config relocation (less likely; zenoh
  build blocks only).

A worktree bisect at `a9c6b1f88` was attempted but the standalone cmake
configure path can't reproduce the fixture builder's self-provisioned
static Cyclone (falls back to the ROS-Humble shared lib, which doesn't
export `ddsrt_*`), so bisection needs the full fixture-builder route per
commit (~minutes each) — left for the fix session.

## Direction

Bisect with `just threadx_linux build-fixtures` + the 1-second manual
talker repro above (much cheaper than the e2e test); fix; make
`NROS_CHECK_LOG` visible on threadx-linux; rerun
`test_threadx_linux_cyclonedds_talker_to_native_listener`.
