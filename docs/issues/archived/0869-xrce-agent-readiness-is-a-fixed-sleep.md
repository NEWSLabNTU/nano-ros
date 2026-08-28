---
id: 869
title: "`XrceAgent::start` slept 500 ms instead of checking the port was bound,
  so under sweep load the client talked to nobody and the test failed as a
  missing result"
status: resolved
type: bug
area: testing
related: [issue-0868, issue-0470]
---

## Problem

```rust
// The Agent starts quickly — give it a short delay to bind the port
std::thread::sleep(Duration::from_millis(500));
```

True on an idle host, a guess on a loaded one, and nothing reported when the
guess was wrong. The client then sent its session-open to a port nobody was
listening on, got no reply, and the test failed much later as a missing RESULT
— naming neither the agent nor the timing.

Observed as an intermittent, migrating failure among the XRCE cells of
`native_example_reqresp_e2e`: `case_18_cpp_xrce_action` in one full sweep,
`case_09_cpp_xrce_service` in the next, never the same cell twice, and never at
all outside a full sweep.

It compounds with **#0868**: the C++ action client prints any non-OK `send_goal`
as `Goal was rejected by server`, so this fixture timing bug arrives dressed as
a deliberate server decision. Between the two, the evidence points at the
backend's inbox path — which is where I spent the first hour.

## Measured

On this host, tree at `79fedd32f`:

| condition | result |
| --- | --- |
| full `just ci` sweep (1541 tests) | FAIL — `case_18`, `ret=-2` (Timeout) |
| full `test-all` sweep (1573 tests) | PASS |
| full `just ci` sweep, third | FAIL — `case_09`, no client output |
| XRCE cells only, 6 consecutive runs | 36/36 PASS |
| whole `native_example_reqresp_e2e` solo | PASS |

Two failures in three full sweeps; zero in 36 isolated runs. Load-dependent,
cell-independent — which is what a startup race looks like and what a payload
defect does not.

## Fix

`wait_until_listening` replaces the sleep: poll until a second UDP bind to the
port FAILS, which means the agent owns it. That works precisely because the port
LEASE is a lockfile and never holds the socket (#0470) — so "bind failed" is
exactly "the agent is listening", with no sleep and no dependence on host load.
A child that has exited is reported as exited rather than waited out.

### Verified — negative controls, not just the happy path

Both in `readiness_tests`, run on the normal `cargo test` path:

* `a_process_that_never_binds_times_out` — a live stand-in child that never
  binds must produce the timeout error, not `Ok`. Without this the fix is a
  claim: a probe that returns `Ok` proves nothing unless it can return `Err`.
* `a_child_that_exited_is_reported_as_exited` — a dead agent is named as dead.

Both pass in 0.32 s total. The XRCE cells pass 3/3 after the change and are
measurably FASTER (5.8 s vs 6.4 s per run), because the probe returns when the
agent binds instead of always sleeping 500 ms — which is independent evidence
the probe is doing what it claims.

## NOT established

That this was the only cause of the migrating failure. The mechanism explains
every observation and the negative controls show the probe works, but the flake
was intermittent, so its absence after the fix is not yet a measurement — it
needs several more full sweeps to be one. The failure mode is now loud either
way: an agent that does not bind fails at the fixture, naming the port.

## Sibling worth checking

`start(num_ports)` for the serial/pty agent, further down the same file, spawns
socat and the agent the same way. It was not examined here.
