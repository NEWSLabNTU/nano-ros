---
id: 3000
title: "The Zephyr connect-budget cells assert a line they may not have read yet"
status: resolved
type: bug
severity: low
area: [tests, zephyr, zenoh]
related: [2437, 2950]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix-connect-budget-no-listener-race"
---

## What happened

On host 241, at the head of B46's pull request (#4943),
`zephyr_connect_budget::zephyr_zenoh_open_against_no_listener_ends_at_its_budget`
failed with "the open must end in `RMW session open failed` ...
`ConnectionFailed`". The same built image, run by hand against a refused port,
printed the budget line and then `RMW session open failed — ConnectionFailed`
in 6 runs of 6, both stamped 5.002 s. So the image behaves, and the test reads
too early.

## Cause

`boot_until_given_up` returns as soon as `wait_for_pattern` has a complete
budget line (`zenoh open: no session with ...`). `assert_gave_up_naming_the_budget`
then requires the session-open failure line, which the image prints just
after, and the reader thread may not have taken it yet. On a loaded host it
had not. The helper has been unchanged since 2026-11-11, so the race predates
B46. All three cells (no listener, dropped SYN, silent listener) go through
the helper.

## Fix

The helper takes its time at the budget line, as before, so the timing
assertions are unchanged. It then waits, up to 10 s, until the failure line
is read too, and returns that output. The output it returns is cumulative, so
the budget line is still in it.

## Acceptance (2026-11-27)

On host 241 (a Zephyr host), with the fix, the fixture build
`zephyr-rust-talker-connect-budget-zenoh` returned 0. Then `cargo nextest
run -p nros-tests --test zephyr_connect_budget` passed 4 of 4, three runs in
a row (6.2 s, 5.6 s, 5.6 s), the no-listener cell included.
