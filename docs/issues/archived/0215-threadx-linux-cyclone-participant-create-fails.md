---
id: 215
title: "threadx-linux Cyclone lane: freshly built talker exits silently — dds_create_participant returns -1 (museum binary hid it)"
status: resolved
resolved_in: "2026-07-16 — test defect: 287-W6 renamed the binary (threadx_c_talker -> c_talker); the test kept the old path and executed an orphaned museum binary. One-line test fix; the fresh c_talker publishes fine."
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

## RESOLVED (2026-07-16) — test defect, not a product bug

The entire filed diagnosis was measured against a GHOST. Root cause chain:

1. **287-W6** (ament-shape migration) renamed the example output binary
   `threadx_c_talker` → `c_talker`. The fixture resolver
   (`fixtures/binaries/threadx_linux.rs`) was updated; the one direct
   path in `native_api.rs` (`test_threadx_linux_cyclonedds_talker_to_
   native_listener`) was NOT.
2. Example build dirs are incremental and never remove old artifacts, so
   the **pre-W6 orphan binary** stayed in `build-cyclonedds/`, satisfied
   the test's `exists()` check, and got executed — while every rebuild
   (mine included, repeatedly) produced a fresh, working `c_talker`
   right next to it.
3. A clean `just threadx_linux build-fixtures` at HEAD +
   `ROS_DOMAIN_ID=61 ./build-cyclonedds/c_talker` publishes
   `Hello World: N` immediately; `NROS_RMW_TRACE_OPEN=1` shows
   `open: ret=0`. There is no participant-creation failure in current
   code.

The filed evidence is retracted: the `dds_create_participant -> -1` /
`nros_support_init -> -1` readings came from gdb `finish` on the orphan
under the tx-linux SIGUSR1/2 scheduler storm — `$eax`-after-`finish` is
GARBAGE there (a later `NROS_RMW_TRACE_OPEN` run on the same orphan showed
`open ret=0`). The "CHECK_LOG printf swallowed" secondary claim was also
orphan behavior; the fresh binary's stdio works.

Fix: `native_api.rs` now points at `c_talker` (with a comment pinning the
name to the CMake target). Verified: the e2e passes on a fresh fixture
sweep.

### Lessons (the durable part)

- **Target-rename migrations must grep the test tree for hardcoded binary
  paths.** The W6 rename updated the resolver family but a direct
  `project_root().join(...)` path slipped through — and the orphan made
  the miss INVISIBLE (test kept passing on the museum binary until the
  orphan rotted).
- **`exists()` is not a fixture check.** The #182 staleness guard exists
  for exactly this; direct-path consumers bypass it. Follow-up candidate:
  route this lane through a `require_prebuilt_binary`-style resolver.
- **gdb `finish` return values are unreliable under threadx-linux** (the
  scheduler's SIGUSR1/2 storm corrupts the read); use
  `NROS_RMW_TRACE_OPEN=1` or source-level breakpoints instead.
