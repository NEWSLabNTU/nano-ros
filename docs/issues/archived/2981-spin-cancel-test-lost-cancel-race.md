---
id: 2981
title: "Six spin tests cancel from a peer thread after a fixed sleep, and hang forever when the spinning thread reaches the loop after the cancel"
status: resolved
resolved: 2026-11-26
resolved_in: "the change that adds this file"
type: bug
severity: medium
area: [testing, executor]
related: [phase-417, phase-521]
found: 2026-11-26
---

**Resolved: every peer now either waits until it sees the spin running, or
keeps cancelling until the spin returns.** Both orders are safe, because entry
clears the flag before it raises `is_spinning`.

## What happened

The full `just ci gate` for PR #4890 on host 142, under a load average around
20, timed out `build::check::node-std-tests` at its 2400 s budget. The one test
still running was
`executor::spin::cancel_tests::cancel_terminates_a_spin_and_is_spinning_tracks_it`.
The PR changed no executor code. The same lane passed on three other PRs that
day.

## Why

`spin` and `spin_period` begin with `enter_spin_loop()`, which **clears** the
cancel flag on purpose: a cancel requested before this spin is not a cancel of
this spin (phase-417 W4.c). The test's peer thread slept 50 ms and then stored
`true` once. If the spinning thread was descheduled for more than 50 ms between
`thread::spawn` and `enter_spin_loop`, the store landed first, entry erased it,
and the untimed spin never returned. A loaded host does exactly that.

Measured: the same test from `main`, with a 200 ms sleep inserted before
`spin`, still hung at a 30 s `timeout`. The fixed test with the same sleep
passes in milliseconds.

## Sweep

`git grep -n -A8 'thread::sleep(Duration::from_millis' -- packages/core/nros-node/src`
found every peer whose body stores the halt flag. A repo-wide grep for files
that spawn a thread and also cancel or spin found two more to read:

| Test | Shape | Fix |
| --- | --- | --- |
| `spin.rs` `cancel_terminates_a_spin_and_is_spinning_tracks_it` | sleep 50 ms, one store | `CancelPeer` |
| `spin.rs` `is_spinning_is_true_inside_the_loop` | poll `spinning` 1 s, then one store regardless | `CancelPeer`, polling for up to 10 s |
| `tests.rs` `test_spin_halt` | sleep 50 ms, one store | `CancelPeer` |
| `tests.rs` `spin_without_a_timeout_still_runs_with_no_clock` | sleep 50 ms, one store | `CancelPeer` |
| `tests.rs` `test_spin_period_halts` | sleep 50 ms, one store | `CancelPeer` |
| `tests.rs` `spin_period_counts_its_wakes_and_keeps_late_within_total` | sleep 60 ms, one store | `CancelPeer` |
| `nros-c/tests/run/node_guard_condition.c` `period_thread` | sleep 400 ms, one `rclc_executor_cancel` | waits for `rclc_executor_is_spinning` first |

Not affected: `nros-cpp/tests/compile/executor_cancel_runtime.cpp` already
waits for `is_spinning()` before it cancels, and its spin is bounded.
`test_guard_handle_send_across_thread` triggers a guard and does not cancel.

## Fix

`executor/cancel_peer.rs` (test-only) is the one way a Rust test cancels a
running spin from another thread. It sleeps, then re-asserts the cancel every
millisecond until `finish()` joins it. `finish()` joins before the test can
start another spin, so a late store can never cancel a later spin.

The doc example on `Executor::halt_flag` has the same single-store shape with a
5 s sleep. It is left alone, because a cancel sent before a spin starts being
dropped is the API's stated contract, not a test defect.
