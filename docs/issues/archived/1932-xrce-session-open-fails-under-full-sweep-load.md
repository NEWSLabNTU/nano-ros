---
id: 1932
title: "Two XRCE clients failed to open their Agent session in one tier-1 sweep on newslab-server-241, and pass 20 of 20 solo"
status: resolved
type: bug
area: [rmw-xrce, testing, ci]
severity: low
found: 2026-10-17
resolved: 2026-10-19
related: [0741, 1365, phase-507]
---

**Home: [phase-507](../../roadmap/archived/phase-507-xrce-and-action-client-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## Seen

run-matrix **38818350050**, job **116843072656** `tier 1 (cells)` on
`nano-ros-runner-newslab-server-241`, 2026-10-16 ~09:44Z. The junit rewrite
counted 2 real failures, both XRCE clients that never got a session from the
Agent the test started for them:

- `c_xrce_api::test_c_xrce_listener_starts` (14.3 s):
  `rclc_support_init(&app.support, argc, argv, &allocator) -> -1`, before
  `Support initialized`.
- `bridge_mixed_rmw::test_zenoh_to_xrce_bridge_e2e` (16.9 s): the zenoh half
  opened; the XRCE half panicked
  `egress Node (XRCE session open): Transport(Backend("rmw_ret error"))`.

Both runs used the SDK-pinned Agent
(`~/.nros/sdk/xrce-agent/2.4.3-nros1/bin/MicroXRCEAgent`). In the same sweep
39 other XRCE tests passed, including `test_c_xrce_talker_starts`, the
listener's twin, which ran beside it.

## Not reproduced

- The next tier-1 run, **38861323075** on `newslab-118-server`, passed both.
  Its only real failure was `rmw_coordinate_truth`, fixed by #2317.
- On newslab-118, on fixtures rebuilt at `c43a903140`: both tests 10 times
  each, run solo, 20 of 20 passed.

So the failure needs the full sweep, at the lane's `-P32`, or that host. That
is the shape CLAUDE.md warns about for QEMU lanes ("flake under load, retest
SOLO"), here on the host XRCE path.

## Not established

- Whether the Agent was not yet accepting when each client sent its
  CREATE_CLIENT. The `XrceAgent` fixture waits for the UDP bind; the
  `bridge_mixed_rmw` comment measured a further settle the Agent needs.
- Whether the client's session-open retry budget is shorter than an Agent's
  start-up on a loaded host.
- Whether it is specific to 241. That runner had just been brought back after
  issue 1365, and this was its first tier-1 sweep.

## Next

Watch tier-1 runs for a repeat. If one appears, keep the Agent's own log for
that test (the fixture captures it), and compare the client's session-open
timeline with when the Agent printed its bind line.

## 2026-10-18 — load alone does not reproduce it; the two failures now say which they are

Measured on newslab-118 with a scratch probe calling `xrce_session_create`
against a fresh `MicroXRCEAgent` per run (the shape every XRCE test has):

| condition | runs | result | open time |
| --- | --- | --- | --- |
| idle | 5 | 5 OK | 105–106 ms |
| 32 busy loops on 32 cores | 8 | 8 OK | 112–134 ms |

So CPU contention by itself adds ~30 ms against a budget of seconds. That
fits the issue's reading (the sweep, or that host) and rules out the simplest
explanation.

The open path has two waits, and both returned the same
`NROS_RMW_RET_ERROR`, which is why this issue could not say which one ran out:

* the session open: `XRCE_SESSION_CREATION_RETRIES` (3) attempts of
  `UXR_CONFIG_MIN_SESSION_CONNECTION_INTERVAL` (1000 ms);
* the participant create: one `XRCE_ENTITY_CREATION_TIMEOUT_MS` (1000 ms) wait.

Each now logs at the failure site, naming the step, its budget and, for the
participant, whether the Agent answered at all:

```
[ERROR] nros_rmw_xrce: XRCE session open: the Agent sent no CREATE_CLIENT status in 3 attempt(s) x 1000 ms (issue 1932)
```

The next sighting's test log therefore names the step. If it is the
participant with "no answer", raise that one wait; if it is the session, the
Agent was not answering at all, and its own log is the next thing to read.

## Resolution (2026-10-19) — the fixture handed the test an Agent that was not there

The fixture's readiness probe was wrong in a way that only a full sweep can
trigger. `XrceAgent::wait_until_listening` read "a second UDP bind on the port
fails" as "the agent holds it". The port LEASE (issue 0470) makes the port
exclusive among OUR fixtures and nothing else, as its own docs say. In a sweep,
every other socket that binds port 0 draws from the same ephemeral range
without consulting the lease: each XRCE client's, each DNS lookup's. One can
land on a leased port between the lease and the Agent's bind. Measured with a
stranger socket on the Agent's port:

| step | result |
| --- | --- |
| the fixture's probe (`bind 127.0.0.1:<port>`) | fails (`Address already in use`) → reported READY |
| the Agent | dead within 1.5 s: `Error while starting IPvX agent!` |
| the client | sends CREATE_CLIENT to the stranger, gets nothing → `rmw_ret error` after its 3 × 1000 ms budget |

That last row is this issue's signature, and it is a CLEAN session-open
failure: no Agent answers at all. That fits what the earlier measurement ruled
out (CPU load adds ~30 ms) and what it could not explain (sweep-only, and
never solo, because only a sweep has hundreds of concurrent ephemeral binds).
The two failures landed within one second of each other (09:44:11–12Z), which
fits one transient collision rather than a slow host.

**What is measured and what is inferred.** The mechanism is measured, end to
end, against the real Agent. That it is what happened on 241 on 2026-10-16 is
inferred: the fixture sent the Agent's output to `/dev/null`, so that run kept
no `Error while starting IPvX agent!` line to confirm it.

**Fix** (`nros-tests/src/fixtures/xrce_agent.rs`):

- The probe now asks WHO holds the port. On Linux it reads the port's socket
  inodes from `/proc/net/udp{,6}` and requires one of them among the Agent's
  own `/proc/<pid>/fd`. Elsewhere it keeps the bind answer.
- A port held by anyone else is `StartError::PortTaken`, and `start_unique`
  answers it with a fresh lease, up to 5 times, saying so on stderr. A caller
  that named its own port (`start(port)`) gets the error, naming issue 1932.
- **Unit test:** `a_port_held_by_a_stranger_is_not_the_agent_listening` is the
  negative control. The old probe returned `Ok` there.
- **Real Agent:** 20 of 20 `start_unique` agents were confirmed as owners, and
  `start(port)` on a stranger's port is refused with the new message.

**Class sweep.** This was the only bind-probe readiness check in
`nros-tests`. The router fixtures use TCP `connect`, which a non-listening
client socket cannot satisfy, so a stranger there times out loudly instead of
reading as ready.

The session-open retry budget was left as it was: with an Agent actually
listening, opens take 105–134 ms even under 32-way load.
