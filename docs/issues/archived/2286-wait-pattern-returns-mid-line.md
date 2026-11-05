---
id: 2286
title: "`wait_for_output_pattern` returns as soon as a prefix appears, so a test that then kills the process can assert on half a line"
status: resolved
type: bug
area: [testing, ci]
severity: medium
found: 2026-11-05
resolved: 2026-11-05
related: [phase-466]
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**: a
scheduled lane went red because the harness stopped reading in the middle of a line.
The code under test was correct.

## Measured

`run-matrix` run 41979554190 (2026-11-05 06:18 UTC, main `1d287ef1`), job
127122834628 `tier 1 (cells)`. `Real failures: 1 / 1 total failures`; the only
real failure is `nros-tests::roundtrip_xprocess_e2e roundtrip_xprocess`:

    roundtrip_xprocess: 1 of 8 cell(s) FAILED:
      cpp/action: [cpp action] client output missing `Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]`
      — server-side compute or round-trip wrong (phase-535 ...)

The captured client output in `tier1-junit` (`junit-real.xml`) ends like this:

    Next number in sequence received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
    Result received: [0, 1, 1, 2, 3, 5, 8

The last feedback line holds the full order-10 sequence, so the server computed it
correctly. The result line stops after seven elements, with no closing `]`.

## Why

- `examples/workspaces/cpp/src/action_client_pkg/src/FibClient.cpp` sets
  `setvbuf(stdout, nullptr, _IONBF, 0)` and prints the result one element per
  `printf`. Each element is therefore its own `write(2)`.
- `roundtrip_xprocess_e2e.rs` waits with
  `cli.wait_for_output_pattern(ACTION_RESULT_PREFIX, 60 s)`, then calls `cli.kill()`
  and asserts that the output contains the FULL sequence.
- `ManagedProcess::wait_until_pattern` (`packages/testing/nros-tests/src/process.rs`)
  returns on the first read where `output.contains(pattern)` is true. That read can
  end in the middle of the line the pattern opens.

So the outcome depends on how many element writes arrive before the 4096-byte
non-blocking read that sees `Result received:`. This run got seven of them.

## What it is NOT

- Not a wrong result or a lost message. The feedback line one line earlier carries
  the full sequence, and the server's goal handling is unchanged.
- Not a stale fixture. The other 7 cells of the same test passed in the same run,
  from the same fixture build.
- Not the out-of-lane `[SKIPPED:lane]` panics, which make up the rest of the run's
  ~150 raw `FAIL` lines. The junit rewrite reclassifies those, and they are not
  counted in `Real failures`.

## The class

Many tests wait on `ACTION_RESULT_PREFIX` (`Result received:`) and some then assert
on the content after it:
- `roundtrip_xprocess_e2e.rs` asserts on the whole sequence.
- `linux_api.rs:719` asserts on `"[0, 1, 1, 2"`.

33 example sources set `_IONBF`. Every pairing of a prefix wait with an assertion on
the rest of the line has this race, and a reviewer cannot see it from either side.

## What would close it

Fix it in the shared engine, not at the site that failed:
- `wait_until_pattern` reports a match only once the line containing the pattern is
  complete (a `\n` after the match), or the process has exited, or the deadline has
  passed.
- A unit test in `process.rs` feeds `Result received: [0, 1` and then `, 2]\n` in two
  writes, and asserts that the returned output holds the whole line.
- `roundtrip_xprocess` cpp/action then needs no change of its own.

## Resolution (2026-11-05)

Fixed in the shared engines, as proposed above.

- `ManagedProcess::wait_until_pattern` returns a match only when the line holding
  it is complete: a `\n` follows the match in the stream that printed it. A line
  that never ends still matches after `MATCH_LINE_GRACE` (2 s), so a marker printed
  without a newline costs 2 s, not the caller's whole timeout. One rule,
  `process::pattern_line_ready`.
- The same rule now applies to the two sibling engines with this shape: the QEMU
  `collect_until`/`wait_for_output_pattern` (`qemu.rs`) and the Zephyr
  `wait_for_pattern` (`zephyr.rs`). The line-based reader in `lib.rs` already
  returned whole lines. The COUNT waits (`wait_for_output_count`) are left alone:
  no caller of theirs asserts on the rest of a counted line.
- Unit tests in `process.rs`: the prefix and the rest of the line arrive 300 ms
  apart, and the whole line comes back. This test fails with the grace set to zero,
  which is the old behaviour. A line that never ends matches after the grace and
  well before the timeout.
- `roundtrip_xprocess` needed no change of its own, and neither did any other
  site, because the fix is in the engine every one of them reaches. The sites
  that wait on a `*_PREFIX` constant are `wait_for_output_pattern`,
  `collect_until`, the QEMU waits and the Zephyr waits. Sweep:
  `git grep -n -A1 "wait_for_output_pattern($" packages/testing/nros-tests/tests | grep PREFIX`,
  plus `git grep -n "collect_until(.*PREFIX" packages/testing/nros-tests/tests`.
