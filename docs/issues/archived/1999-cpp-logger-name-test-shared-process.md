---
id: 1999
title: "`two_cpp_nodes_emit_under_their_own_names` fails whenever another nros-cpp lib test runs first in the same process"
status: resolved
resolved: 2026-10-22
resolved_in: "PR #2490"
type: bug
area: [testing, cpp]
severity: low
found: 2026-10-22
related: [2000, phase-427]
---

## Measured

- `cargo test -p nros-cpp --lib --no-default-features --features std,rmw-cffi`
  on `origin/main` at `4b4f372522`, 2026-10-22, and the same suite with
  `alloc,env,log,rmw-cffi` on the phase-359 W10 stage C branch: **41 passed, 1
  failed**, every run, `--test-threads=1` included.

  ```
  node_logger_name_tests::two_cpp_nodes_emit_under_their_own_names
  assertion `left == right` failed: expected exactly the two records this test emitted
    left: 6
   right: 2
  ```

- The same test ALONE (`-- two_cpp`) passes.

## Why nothing reported it

The lanes run this crate's lib tests under `cargo nextest`, which gives each
test its own process. The test counts records through a process-global sink
(`SEEN_N`) that `nros_log::init(SINKS)` installs, so it is correct only when it
is the first thing in its process to log. Under `cargo test`, earlier tests in
the binary leave records (or log-emitting state) behind, and the count is
wrong by construction, not by load: `--test-threads=1` fails the same way.

## Cause (measured)

The 4 extra records are the EARLY RING. Records raised before any sink is
installed are held (`nros_log::early`, `NROS_LOG_EARLY_RECORDS`, builtin 4), and
`nros_log::init` replays them into the sinks it installs. Under `cargo test`,
sibling tests log before this test runs `init`, so its sink receives their 4
held records and then its own 2: 6, every run. A baseline taken after `init`
would also have been wrong, because the sink is process-global and keeps seeing
whatever other test threads log while it is installed.

## Resolution

The sink keeps only records whose message is this test's marker
(`w5 cpp marker`), the way the nros-node twin already filtered. Measured:
`cargo test -p nros-cpp --lib --no-default-features --features std,rmw-cffi`
was 41 passed / 1 failed before and is 42 / 0 after.

## Sibling: nros-node's twin

`nros-node`'s `executor::tests::two_nodes_on_one_executor_emit_under_their_own_names`
counts through the same kind of process-global sink. It failed ONCE under
`just ci gate`'s loaded `check::build` on 2026-10-22, at 07:01 UTC on the
phase-359 W10 stage C branch: `expected exactly the two marker records this test
emitted, got [("w5_two_names_listener", ...)]`, which is one record short where
the C++ one is four over. `just check node-std-tests` then passed 3 of 3 alone,
and it passed in the gate run before. That crate is untouched by that branch.

It already filtered on its own marker, so extra records could not fail it. A
MISSING one has exactly one cause in nros-log: the dispatch recursion guard is
one process-wide flag, so a record emitted while ANOTHER thread is inside a
sink is dropped. `node-std-tests` runs `cargo test`, whose test threads log
concurrently. That is a product defect, not a test one, and it is filed
separately as issue 2000; this test's comment now names it.
