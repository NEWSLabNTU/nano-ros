---
id: 1930
title: "On XRCE an action client sends its goal blind, and a goal the Agent drops is never re-sent"
status: resolved
type: bug
area: [rmw-xrce, examples, tests]
severity: medium
found: 2026-10-16
related: [1686, 1918, phase-507]
---

**Home: [phase-507](../../roadmap/phase-507-xrce-and-action-client-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## 2026-10-18 — resolved: a deterministic test, and the residual measured

**The fix now has a test that fails without it.** The runtime cell only met a
lost goal by luck of discovery timing, which is why it read as a flake for so
long. `linux_example_reqresp_e2e::xrce_action_goal_sent_before_its_server_is_resent`
makes the loss certain instead: the CLIENT is started first, its goal goes out
while no server exists (volatile, so the server's reader never sees it), and
only then is the server started. The result can then arrive only through the
re-send, and the test also asserts the client logged it
(`nros_tests::output::ACTION_GOAL_RESENT_MARKER`). Cases: Rust, C, C++ on the
`linux` XRCE fixtures.

| build | Rust | C | C++ |
| --- | --- | --- | --- |
| with the re-send (HEAD) | 5/5 (~5 s) | 5/5 (~18 s) | 5/5 (~32 s) |
| re-send removed (Rust `BLIND_RESEND` 5 s → 3600 s; C/C++ `attempt >= 2` → `>= 0`) | red: no re-send, no result | red: `No goal response from server: -2` | red: `No goal response from server (order=10, ret=-2)` |

The mutated sources were restored and `cmp`'d clean against `HEAD`, the
fixtures rebuilt, and the five green runs above (which also ran the three
original `*_xrce_action` cells, 15/15) taken after that.

**The residual — "the two endpoints never matched in 30 s" — did not
reproduce.** Measured with the in-tree client (re-send present):

| arm | runs | failed | re-sends taken |
| --- | --- | --- | --- |
| harness, loopback Fast-DDS profile (`dds_isolation`), all three cells | 90 | 0 | — |
| harness, `NROS_DDS_ALLOW_LAN=1` (no profile), all three cells | 90 | 0 | — |
| by hand, issue's shape, no profile, Rust | 60 | 0 | 1 (recovered) |
| by hand, loopback profile, Rust | 60 | 0 | 0 |

And with the Rust re-send REMOVED, to see how often the drop itself happens:
0 of 30 under the profile, 0 of 30 with the 1734 session lock also disabled,
0 of 30 against the ROS-paired Agent, 0 of 60 across six concurrent loops, 0
of 20 under a 20-way CPU burner — and **1 of 30 without the profile**. So the
blind-send drop is real and lives on the unpinned (LAN + shared-memory) bus;
under the harness's loopback profile it was not seen at all (0 of 170 with
the re-send removed, 0 of 150 with it), and
the one hand-run drop without the profile was recovered by the re-send, as
designed. The never-matched shape from the 35-run hand measurement was not
seen again in 150 unprofiled runs with the re-send; if it returns, it is a
new filing with an Agent log (`NROS_XRCE_AGENT_VERBOSE=1` against an Agent
built with `NROS_XRCE_AGENT_LOGGER=1`), not this issue.

Not measured here: the non-Linux copies of the clients (FreeRTOS, NuttX,
ThreadX, Zephyr, mps2 bare-metal) — the same logic, but their XRCE fixtures
were not built for this — and a full tier-1 sweep (phase-507's acceptance).
