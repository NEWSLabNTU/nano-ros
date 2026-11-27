---
id: 2866
title: "The `board` wall-clock source is asked once, so a board clock that drifts or later corrects itself never reaches the image"
status: resolved
type: enhancement
severity: low
area: [clock, freertos, zephyr]
related: [rfc-0141, phase-582, issue-2865, phase-478]
found: 2026-11-23
resolved: 2026-11-24
resolved_in: "`clock.board_resync_interval_ms` + `clock.board_resync_stack_bytes`; the shared re-ask helper `nros/clock_resync.h`, started by the POSIX and FreeRTOS ports"
---

## What happens

Once `board` answers and latches (RFC-0141 D2, phase-582 W1d), the image
never asks `nros_board_wall_clock_ns` again. If the board's own clock drifts,
or its SNTP/GNSS task later disciplines it, the image's wall clock does not
follow: the offset installed at the first answer stays for the rest of the run.
`sntp` does not have this limit, because `clock.sntp_resync_interval_ms`
(phase-582 W3) re-queries on an interval.

A consumer raised this while porting its island's lwIP SNTP task to the
board hook (ASI P5 on mps3-an536 / s32z270).

## Ruling (manager, 2026-11-23)

This is a follow-up, not part of the 2865 fix. If it is done, `board` gets the
SAME knob shape as SNTP, not a second mechanism:

- a `clock.board_resync_interval_ms` knob, default 0 (one-shot, today's
  behaviour, stays the default);
- above 0, the hook is re-asked on that interval, and each answer is installed
  through the one setter (`nros_clock_set_epoch_offset_us`), so the existing
  upgrade/step policy (`clock.upgrade`, the step counter) decides what a
  re-answer may change;
- the interval is the image's monotonic time (guest time on a simulator), as
  documented for `sntp_resync_interval_ms`.

## Acceptance

- On the ports that resolve `board` (POSIX, FreeRTOS), with the interval above
  0, a stub board whose answer steps mid-run has its later stamps follow the
  step within one interval. With the default 0, the step never reaches them.
- The re-ask never blocks the task that reads the clock: it runs on its own
  thread or task, as the SNTP re-sync does.
- The knob is in the registry with the SNTP interval's wording, and RFC-0141 D2
  records it.

## Resolution (2026-11-24)

Done as ruled, plus the manager's two additions of 2026-11-24: the stack is a
knob, and the pacing stays monotonic and says why.

- **Knobs.**
  - `clock.board_resync_interval_ms`: image scope, default 0, range 0 to one
    day.
  - `clock.board_resync_stack_bytes`: default 8,192 B, range 1 KiB to 1 MiB.
    It is a floor the port raises.
  - Both are in the knob registry. The image's clock selection carries them
    only beside `board`, on both roads (`clock_selection.rs` and
    `NanoRosClockSelection.cmake`), and the shared case table holds the two
    readers to one answer.
- **One helper.** `nros/clock_resync.h` (`nros_clock_resync_*`) is a re-ask
  loop on a platform-ABI task. Its stack is the knob, given as
  `stack_bytes`, a floor. Each ask's outcome is reported once when it
  changes. SNTP's re-sync (issue 2846) can move onto it rather than keep a
  second copy.
- **Board.** The selection TU's
  `nros_clock_board_resync_start__set_by__knobs_clock_source` re-asks the
  hook and installs each answer through the one setter, so `clock.upgrade`
  decides. A refusal there is policy, not a board that went quiet, so it is
  not reported as one. The POSIX port starts it after its once-resolution,
  and FreeRTOS after its boot point.
- **Pacing.** Monotonic, on purpose, as RFC-0141 D2 now says. The hook is
  local, so there is no remote rate to keep real (unlike D5's SNTP).
- **The task's priority.** It is the creator's. While writing this, the POSIX
  port turned out to map any band priority to `SCHED_FIFO`, and an
  unprivileged `pthread_create` then fails. A "least urgent" request would
  have stopped the task from starting at all, which the POSIX test caught.

Acceptance, on the host, against the real ports:

- **POSIX** (`tests/clock-board-posix-tests.sh` cases j and k): a hook that
  answers the instant and then the instant + 1 h.
  - With a 100 ms interval, the reading has followed the step 600 ms later.
  - With 0, the hook is asked once and the step never arrives.
  - A port that never starts the re-ask fails j.
- **FreeRTOS** (`tests/clock-board-freertos-tests.sh` board-resync and
  board-once): the same, on the real kernel's POSIX port through the CMake
  road, with the same negative control.
- **The helper** (`tests/clock-resolve-tests.sh` case 12):
  - one ask per interval, never sooner;
  - a change of outcome reported once;
  - an interval of 0 starts no task.

