---
id: 2715
title: "The IVC and RPMsg point-to-point peer tests fail when they share one process"
status: open
type: tech-debt
area: [rmw, zenoh, testing]
severity: low
found: 2026-11-18
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
