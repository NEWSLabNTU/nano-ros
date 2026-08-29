---
id: 902
title: "action goals complete between 20 % and 90 % of the time on the same build,
  with no session expiry and no fault to explain the difference"
status: open
type: bug
area: rmw
related: [issue-0882, issue-0879, issue-0852]
---

## Measurement

Same image, same board, same router config, `order: 6`, direct serial link:

| run | goals completing |
| --- | ---: |
| after the 0882 allocator fix | 9/10 |
| after the 0879 INIT fix | 6/10 |
| immediately after, unchanged build | 8/10 |
| after a 100 s idle soak | 2/5 |

Nothing distinguishes these runs but time. Split across two fixes it looked like
one had regressed the other; a repeat run inside the same build gave 6/10 then
8/10, so the spread is the system, not the change.

## What it is NOT

Both of the obvious explanations are excluded by direct measurement, not by
argument:

- **Not a session expiry.** Zero `Closing session because it has expired`
  messages across a 160 s session that included five goals
  ([issue 0839](archived/0839-action-image-session-expires-every-20s.md) is
  resolved on exactly this evidence).
- **Not a crash.** Zero faults; the board is alive and answering afterwards.
- **Not discovery.** `ros2 node list` and `ros2 action list` resolve before and
  after, including after 160 s of idling.

So the session stays up, the board stays alive, and a goal still fails to
complete. The failures observed earlier had a consistent shape worth
re-checking: the goal is **accepted** and the result never arrives.

## Why this matters more than the raw number

A 20–90 % spread with no observable cause is worse than a hard failure. It is
not measurable as a regression gate, and any future change to this path will be
evaluated against noise wide enough to hide it — which has already happened once
in this campaign, when 6/10 was briefly read as a regression from 9/10.

## Where to start

The instrumentation for this already exists and is proven on this board:

- the socat tap (`experiments/serial-interop/serial-tap.py`) shows whether the
  `get_result` query and its reply reach the wire, and in which direction the
  exchange stops. It does not halt the core.
- RTT shows whether the application layer saw the query — but attaching it
  perturbs the link ([issue 0881](0881-the-debugger-is-not-a-passive-instrument.md)),
  so use it after the fact, not during.

Capture one *failing* goal on the tap and establish whether the reply is never
sent or never arrives. That is one experiment and it splits the problem in half.
