---
id: 2715
title: "The IVC and RPMsg point-to-point peer tests fail when they share one process"
status: resolved
type: tech-debt
area: [rmw, zenoh, testing]
severity: low
found: 2026-11-18
resolved: 2026-11-23
related: [phase-575]
---

## What is wrong

`packages/rmw/zenoh/nros-rmw-zenoh/tests/point_to_point_peer.rs` holds two
tests:
- `two_peers_on_an_ivc_link_exchange_data_without_a_router`;
- `two_peers_on_an_rpmsg_link_exchange_data_without_a_router`.

They pass only when each runs in its own process. Under bare `cargo test`
the two run as threads of one process, and one of them fails. Which one
fails varies from run to run:

```text
the connecting peer should open with no router: InvalidConfig
listening peer did not open: ConnectionFailed
```

Measured 2026-11-18 with main's test file (features `std,alloc,platform-posix,link-ivc,link-rpmsg`):
- `cargo test --test point_to_point_peer`: 1 passed, 1 failed, on two runs;
- with `-- --test-threads=1`: 2 passed, 3 runs out of 3.

nextest runs each test in its own process, so the test's lane
(`just check required-features-tests`) does not hit this, and nothing says
the tests depend on that.

## Likely cause (not confirmed)

zenoh-pico's process-wide state: a session or link table shared by both
peers' sessions. The two errors come from opening a second point-to-point
session while another test's session is open in the same process. Not
investigated beyond reproducing it.

## Done when

Either:
- the two tests pass together in one process (the shared state is per
  session, or the tests open their sessions in turn); or
- the dependency is stated where it is relied on: a comment on the test file
  and a nextest-only marker, so a `cargo test` run fails with a message
  naming the reason instead of a transport error.

## Resolved, 2026-11-23

Did not reproduce on main at 7d14b8efb3: with the two tests in one process
and nothing serialising them, 0 failures in 59 runs. That covers
`--no-default-features` with the issue's feature set, the lane's set over
the default features, and 20 runs with every CPU of a 48-thread host
busy. The test was reworked on 2026-11-18, the day this was filed, to run
the listening peer in a child process. The run measured here was probably
of the version before that rework, but that is not proven.

The build resolves `zenoh.max_sessions` to 1 outside the lane (the lane
sets `NROS_ZENOH_MAX_SESSIONS=2`). So the tests now take turns explicitly
through `tests/support::session_turn()`, a process-wide lock held while a
test's session is open, with a comment saying why. That makes the first
"done when" hold by construction rather than by timing. The sweep found one
sibling with the same shape, `tests/in_process_graph.rs` (three tests, each
holding a live session), and it takes the same turn. The other files either
open one session (`matched_events_live.rs`, which runs its peer in a second
process) or are refused before a session exists (`session_config_keys.rs`).
