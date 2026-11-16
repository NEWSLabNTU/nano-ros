---
id: 2651
title: "Three nros-node unit tests failed intermittently because parallel tests share process-wide trace, profile and rosout state"
status: resolved
type: bug
area: [node, testing]
severity: low
found: 2026-11-16
related: []
---

## What happened

Under nros-node's feature union without `rmw-cffi` and `signal-fd-wake`
(the executor tests are `not(rmw-cffi)`), three tests failed at random. Each
passed when run alone. Measured on `main`:

| Test | Failure | Rate |
| --- | --- | --- |
| `executor::tests::i3_a_thinned_timer_traces_one_tick_in_n` | `one tick in three of nine`, left 2 or 4, right 3 | about 1 run in 4 |
| `executor::tests::rosout_writers::the_spin_pumps_a_node_record_onto_its_rosout_writer` | `the Log carries the node logger's name` | about 1 in 12 |
| `executor::profile::tests::a_timed_dispatch_records_ns_and_the_topics_it_published` | `timer0` labelled a `/chatter` subscription row | about 1 in 35 |

libtest runs each test on its own thread of one process. Each of these tests
read state that every other executor test writes from its own thread.

## Causes

- **Trace.** The callback-trace sink was one process-wide pointer. While the
  thinned-timer test had its sink installed, any other test that spun a timer
  in slot 0 went through `timer_tick_traced`, found a sink, and advanced
  `TIMER_TICKS[0]`. That shifted the test's one-in-three phase. The tests that
  SET trace state already held `trace_sink_lock()`; the writers were the
  unlocked readers, which a lock cannot reach.
- **Profile.** No test installed a `thread_key`, so the recorder's default
  key (`0`) gave every thread one shared cursor. Another test's registration
  between this test's `on_register` and `label_last` moved the cursor, and the
  label landed on that row.
- **Rosout.** The record queue is process-wide, and a record no node name
  matches goes to an executor's first node. The test checked the LAST sample
  on its writer, which was sometimes another test's record.

## Fix

- In a test build, the trace sink is thread-local (`callback_trace::with_sink`).
  Only the thread that installed a sink sees it, and every trace reader returns
  on `sink().is_none()` before touching shared counters. An image keeps the
  one process-wide sink, unchanged.
- The profile test installs a per-thread key (the address of a thread-local),
  as the hosted edge does.
- The rosout test checks only a sample carrying its own message, and logs again
  (up to twenty times) until one reaches its writer. The old test passed
  vacuously when nothing reached it; this one requires one.

Measured after the fix: 0 failures in 40 full-suite runs of the same feature
set, and the default and `--all-features` suites stay green.

## Sweep

`git grep -n 'static [A-Z_]*: \(Atomic\|Mutex\)' -- packages/core/nros-node/src`
lists the crate's process-wide state. The trace, profile and rosout tables are
the ones a test asserts exact counts or labels against.

Not explained: `a_full_store_reports_the_refusal_and_names_its_knob` failed
once, on a phase-554 branch, with its message not captured. It reads the log
sinks, also process-wide, but filters for its own probe name. It did not fail
in the 40 runs above or in 4 runs on `main`. Candidates are its
`add_sink` finding the sink table full (`MAX_ADDED_SINKS` is 4; nros-node's
tests add 3) or another test's knob line reaching its capture. Reopen with
the message if it recurs.
