---
id: 1930
title: "On XRCE an action client sends its goal blind, and a goal the Agent drops is never re-sent"
status: open
type: bug
area: [rmw-xrce, examples, tests]
severity: medium
found: 2026-10-16
related: [1686, 1918, phase-507]
---

**Home: [phase-507](../roadmap/phase-507-xrce-and-action-client-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## Measured

`linux_example_reqresp_e2e::native_example_reqresp::case_16_rust_xrce_action`
failed in phase-497 W1's tier-2 run (3/3 under nextest's capture, then passed
`--no-capture`, which read as capture-dependent and was not). Run by hand —
one MicroXRCEAgent, the example `action-server`, then `action-client`, each
pair on its own UDP port and domain — the pair failed **3 of 10**:

```
[INFO] Sending goal
[INFO] Goal sent, waiting for result
```

and nothing more for 30 s; the server logged `Waiting for action goals` and
never `Received goal request`.

## Cause

XRCE cannot answer "is the server there?" (`service_server_is_available` is
NULL by design: the Agent owns the DDS participant, issue 1686), so
`action_server_is_ready_for_name` returns `Err`. Every action-client example
read anything but `Ok(false)` as "ready" and sent ONCE. Inside the Agent the
client's request writer and the server's request reader match asynchronously,
and a goal written before they match is dropped. With no answer coming back and
nothing re-sending, the client waits forever.

The C and C++ clients have the same shape — `wait_for_action_server` answers
`UNSUPPORTED` at once, then one `send_goal` — and fail loudly (a goal-response
timeout) rather than hanging.

## Fix (this change)

A send is BLIND when the backend could not confirm a server. A blind send is
repeated, a confirmed one never is:

- the Rust clients (7 copies: every platform's `action-client` and
  `workspaces/rust`'s `action_client_pkg`) re-send after 5 s with no result.
  The demo server answers order 10 in milliseconds, so a repeat is only ever a
  goal that was lost;
- the C and C++ clients (12 copies) retry a blind send that timed out, up to
  three attempts.

Measured by hand after the fix: **34 of 35** (15 + 20 runs), against 7 of 10
before.

## What stays open

The one failure left was a different shape: SIX sends over 30 s, none of which
reached the server. That is not a discovery race at startup — the two
endpoints never matched for the whole run. The bundled Agent prints nothing at
`-v6` (stdout line-buffered), so there is no Agent-side evidence yet. The hand
runs did not apply the harness's loopback-only Fast-DDS profile
(`nros_tests::dds_isolation`), so the next step is to reproduce under it, and
then to get Agent logs from a build that has them.

## 2026-10-18 — five tier-2 runs since the fix, all green on this cell

`native_example_reqresp::case_16_rust_xrce_action` passed in every tier-2
(1-wise matrix) job of the last five `run-matrix` runs: 38818350050 (1.505 s),
38861323075 (1.413 s), 38933437082 (1.483 s), 38980380118 (1.567 s) and
39009801514 (3.386 s), against 3 of 3 failing in the run that filed this. The
C and C++ XRCE action cells print `FAIL` in ~0.1 s in the same logs; those are
`lane_skip!` panics outside tier 2's cover, which the junit rewrite counts as
skips (`Real failures: 0 / 0` on 39009801514), not results.

What stays open is unchanged: the one hand-run failure in 35, where the two
endpoints never matched in 30 s, was reproduced WITHOUT the harness's
loopback-only Fast-DDS profile, and no harness run has shown it. Next step as
before: reproduce under `nros_tests::dds_isolation`'s profile, with an Agent
built with its logger profile so `-v6` prints something.
