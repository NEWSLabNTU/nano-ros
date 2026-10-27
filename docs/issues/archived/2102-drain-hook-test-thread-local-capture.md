---
id: 2102
title: "`t4_an_overrun_after_arming_is_the_one_stored_violation` read an empty capture under load: its log sink buffered per THREAD, and nros-log delivers a held record on whichever thread holds the dispatch guard"
status: resolved
type: bug
area: [testing, log, executor]
severity: low
found: 2026-10-27
related: [2000, 1939, phase-474]
resolved: 2026-10-27
resolved_in: "the PR that archives this file"
---

## What

`just ci gate`'s `node-std-tests` lane failed once in `nros-node`'s lib tests:

```
executor::tests::t4_an_overrun_after_arming_is_the_one_stored_violation --- FAILED
assertion `left == right` failed: the drain hook printed it once: []
  left: 0
 right: 1
```

The same run's `violations_total() == 1` assertion passed, so the executor did
store the violation and its drain hook did report it. The test then read an
empty capture. The test passed 5/5 run alone and in its full lane. It did not
fail in 30 runs of the lib suite at `--test-threads=64`, or in 40 runs of four
concurrent copies at 128 threads each. Only the 80-way gate showed it.

## Cause

The test captured log lines in a `thread_local!` buffer, from a sink installed
process-wide. `nros-log` does not promise that a record reaches the sinks on
the thread that raised it. A record raised while another thread is delivering
is HELD, and the thread that holds the dispatch guard delivers it
(`drain_and_release`, issue 2000). When that was another test's thread, the
drain hook's line went into that thread's buffer, and this test's buffer stayed
empty.

The other `thread_local!` captures in `executor/tests.rs` are trace sinks. The
executor calls those inline on the spinning thread, so they are correct. This
test was the only LOG capture of that shape. `nros-tests`' log captures were
already process-wide.

## Fix

The capture is a process-wide `Mutex<Vec<String>>`, filtered to this test's
endpoint (`/t4/handler/status`, which no other test uses), so a sibling's
record cannot match. Before reading, the test calls `nros_log::flush()` in a
bounded wait. A record dropped from the hold queue is lost and counted, and no
wait brings it back. If that happens, the assertion names how many records
`nros_log::early::overflowed()` counted during the test, so the failure can be
told apart from a broken hook.

## Measured

Not reproducible on demand (0/70 under the contention above), so the fix rests
on the mechanism, which the dispatch code states directly. The test passes, and
the lib suite passes (819).
