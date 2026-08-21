---
id: 757
title: "C++ arena dispatch swallows non-OK takes — BUFFER_TOO_SMALL drops
  samples with zero diagnostics (0749's open half, now tracked)"
status: open
type: bug
area: rmw, memory
related: [issue-0749, rfc-0052]
---

## Problem

The 0749 defect had two halves; the knob-forwarding half is fixed, this is
the other one, until now tracked only as a sentence inside the archived
issue.

`try_recv_raw` correctly returns `BUFFER_TOO_SMALL` when a reassembled
sample exceeds the subscription buffer — but the C++ arena dispatch path
(the typed `bind_subscription` trampoline over the raw subscription)
swallows EVERY non-OK take. At transport level cyclone completes and ACKs
the sample, then discards it. The result is the worst observable shape a
drop can have: the subscription looks matched and healthy from every
outside probe (`ros2 topic info -v`, tshark ACKNACK analysis) while the
app waits forever.

That is how 13.4 KiB Autoware trajectories were silently thrown away by
every Zephyr image for the whole life of the lane (0749): small degenerate
samples fit the 1 KiB default buffer, so every green marker stayed green.
Attribution took a consumer-side tshark session.

## Direction

A throttled fail-loud log at the drop site — the RFC-0052 fail-loud rule:

- On non-OK take in the dispatch trampoline, log the callback/topic, the
  error code, and for `BUFFER_TOO_SMALL` the sample size vs the buffer
  size (the actionable half: it names the knob to raise).
- Throttled (first occurrence + every Nth, or once per period) — a
  40-participant graph must not turn one misconfigured subscription into
  a log flood.
- `nros_log`, not stdio (issue 0589 class — the site is reached on
  no_std targets and inside native_sim).

Sweep rule applies: audit the OTHER dispatch/take sites for the same
swallow shape (C dispatch, service/action takes, param service), not just
the one the trajectory path hit.
