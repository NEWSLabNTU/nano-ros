---
id: 2816
title: "A process wait dropped the output it read past its match, so a second wait on the same process missed a line printed in the same write"
status: resolved
type: bug
severity: medium
area: [testing, flake]
related: [issue-2286, issue-0471]
found: 2026-11-22
resolved: 2026-11-22
---

## Symptom

Tier-2 run 44916540238 on main (cbf8c22e6f):
`out_of_tree_rmw_e2e a_workspace_rmw_backend_publishes_through_nros_build`
failed with `acme_talker did not print 'acme rmw: pool_slots = 4' within 5s`.
The output it quoted held only `Publishing: 'Hello World: N'` lines. Run on
its own, the same image prints the publish marker and the pool marker 4 µs
apart, and the test passes. The knob reached the build script; the test
never saw the line that said so.

## Cause

`ManagedProcess::wait_until_pattern`, `wait_for_output_count`,
`wait_for_output`, `wait_for_all_output` and `QemuProcess::collect_until_pred`
(with every wait built on it) each read the child into a buffer local to the
call. A wait returns once the chunk it just read satisfies it, and whatever
that chunk held past the matching line went with the buffer. The next wait
started from the pipe, so a line written in the same `write` as the previous
marker was gone. Whether two lines share a read depends on scheduling, which
is why this is red under load and green alone. Any test that waits twice on
one process has the shape; 42 test files do.

## Fix

Each process type keeps one `carry` buffer. Every reader starts from it, and
a satisfied wait keeps the output past the line that satisfied it
(`satisfied_prefix_len`, a binary search over line ends with the wait's own
predicate) and returns only the output up to that line, so each line is
returned by exactly one wait: three tests concatenate or count across
successive waits, and a tail both returned and replayed would reach them
twice. When a
pattern's line was ever seen open, nothing is kept, so a cut never hands a
wait half its line (issue 2286).

Tests in `process.rs`: two markers in one write; a marker split across two
writes; a count wait leaving the rest; each fails on the old reader. A
fourth concatenates three successive waits and expects each line once. A source
check refuses a reader in either impl that starts from an empty buffer.

Three tests assert an ABSENCE on a wait's returned string, and a wait that
returns only through its match can no longer show them text printed after
it. `sched_dims_applied_e2e`'s "no executor-side budget skip" now appends
`read_pending()` before stopping the guest, so it reads the whole run.
`param_store_carve` keeps its window, with the reason in a comment: the store
is built once, carved or refused. `contract_monitor_parity` waits on the very
marker it asserts absent, so its wait never cuts.
