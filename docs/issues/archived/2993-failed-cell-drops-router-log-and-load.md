---
id: 2993
title: "A failed cell lost its zenoh router's output and the host's load, so a load-dependent failure kept only the image's side"
status: resolved
resolved: 2026-11-27
resolved_in: "the change that archives this issue"
type: bug
severity: medium
area: [testing, ci]
related: [issue-2416]
found: 2026-11-27
---

## What happened

Release blocker B46: `freertos_posix_c_entry_delivers_over_zenoh` failed in
runner 118's tier-2 nightly on 2026-11-26 (run 45868144195). It passes 10/10
solo, so the failure depends on load.

What survived the run:

- The test's stderr: `zenohd: accepting on tcp/127.0.0.1:11130 117 ms after
  spawn`, then the image's single open with `connect_timeout_ms=0` refused
  within 1 ms.
- The image's five-line console log, kept in `build/failed-run-logs`.

What did not survive:

- **The router's side.** `ZenohRouter` sent the router's stdout to
  `/dev/null` and its stderr to a pipe. That pipe was read only when the
  router died during startup, so what the router said at the moment of the
  refusal was gone with the test.
- **The host's load.** Nothing sampled it, so "load-dependent" could not be
  placed against a number.

The pipe was also a hazard of its own. Nobody drained it after startup, so a
router that writes 64 KiB to stderr blocks on its own logging.

## Fix

- **The router's output always goes to a file.** It is the opt-in log in
  `test-logs/fixtures/` when `ZENOHD_LOG` / a caller's filter /
  `NROS_TEST_LOGS` is set, at the level asked for. Otherwise it is a file in
  a temp dir the fixture owns, at zenohd's default level. A restarted router
  appends to the same file. One helper, `route_router_output`, serves the
  TCP, serial and TLS routers.
- **A router that exits during startup is explained from that file.** There
  is no pipe any more.
- **On a drop while the test panics,** `ZenohRouter` prints the router's last
  lines (into the junit, as the XRCE Agent's have been since issue 2416) and
  keeps two files in `build/failed-run-logs`, beside the image's console log:
  - `<cell>--zenohd-<port>.log`: the log's tail, capped by the rule run logs
    use;
  - `<cell>--zenohd-<port>--host-load.txt`: `/proc/loadavg`,
    `procs_running` and `procs_blocked`, sampled when the router started and
    at the failure.
- **The XRCE Agent**, the other broker a cell starts, already printed its
  tail (issue 2416) and then deleted its temp log. It now keeps the log and a
  load sample in the same place, through the same helper.
- **One rule for every kept failure log,** in `nros_tests::run`:
  - `keep_log_tail` keeps the capped tail;
  - `flat` makes the file-name component;
  - `host_load_sample` takes the load sample;
  - `keep_peer_failure` keeps a peer's side.

A passing test keeps nothing.

## Tests

- `an_early_exit_is_explained_from_the_router_log`: a stand-in that writes to
  stderr and exits is reported with its words, read from the file.
- `a_failed_cell_keeps_the_router_log_and_the_host_load`: both files, named
  for the cell, with the log's text and both load samples.
- `only_a_panicking_drop_keeps_the_router_side`: a passing drop keeps
  nothing, and a panicking one keeps the log. Mutation: with the keep call
  removed from `Drop`, this test fails (`kept: ""`).
