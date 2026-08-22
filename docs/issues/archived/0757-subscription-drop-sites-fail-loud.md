---
id: 757
title: "C++ arena dispatch swallows non-OK takes — BUFFER_TOO_SMALL drops
  samples with zero diagnostics (0749's open half, now tracked)"
status: resolved
type: bug
area: rmw, memory
related: [issue-0749, rfc-0052]
resolved: 2026-08-22
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


## Fixed 2026-08-22 — propagated the way #0737 already fixed the C copy

**The sweep changed the fix.** The direction above asks for a throttled
fail-loud log, and the first cut did exactly that. Then the mandated audit of
sibling sites found `drain_into_buffer_raw_c` carrying this comment:

> Issue 0737 — a transport ERROR is not "no data", and conflating them destroys
> the sample without a trace. … Now it propagates: `spin_once` maps an `Err`
> from `try_process` to `subscription_errors`, so the count stops lying.

So the remedy already existed **in the same file**, one copy over. A log-only
fix would have been a second spelling of it — the shape CLAUDE.md names. The
landed fix follows 0737: the error PROPAGATES to `spin_once`'s
`subscription_errors`, and the throttled log rides along to carry the actionable
size.

### Four copies of one drain, not one

The trajectory path hit the typed arena drain. The audit found **four** copies
of the same `if let Ok(Some(..))` swallow, all now fixed together:

| site | shape |
| --- | --- |
| `drain_into_buffer` (typed) | Triple + Ring — the reported one |
| `drain_into_buffer_raw` | Triple + Ring — identical code |
| borrowed/zero-copy dispatch | Triple only (registration rejects depth > 1) |
| `drain_into_buffer_raw_c` | already fixed by #0737 — the precedent |

The Ring arms were worse than the Triple ones: `else { break }` made a DROPPED
sample indistinguishable from an empty queue.

### What the report can and cannot say

Deliberately limited, rather than promising the issue's full wish:

* **buffer capacity — yes.** Known at the site, and the actionable half: it
  names the knob (`NROS_SUBSCRIPTION_BUFFER_SIZE`, or
  `ZPICO_SUBSCRIBER_BUFFER_SIZE` / `_LARGE_SIZE` on zenoh).
* **sample size — NO.** The C ABI contract is "non-negative = bytes produced,
  negative = error code" (`rmw_vtable.h`) with no required-length out-param, so
  the backend cannot report how big the sample was. Adding one is an ABI change,
  worth doing on its own merits rather than smuggled behind a log line.
* **topic — NO.** `SubBufferedEntry` carries no name, and the arena's entry
  structs are sized by knob (`EXECUTOR_OPAQUE_U64S`), so adding a field would
  move every image's executor footprint to buy a log line. The throttle counter
  is a module-level static for the same reason.

Throttled first-then-every-64th: a 40-participant graph must not turn one
misconfigured subscription into a flood (issue 0371's shape). `nros_log`, never
stdio — the site is reached on `no_std` and inside Zephyr `native_sim`, where a
Rust std stdio call is fatal (issue 0589).

## Follow-up: eight action/client sites audited, NOT changed

The sweep also covers "service/action takes". Eight sites carry the same
`if let Ok(Some(..))` shape:

```
action_server_raw_try_process        1361, 1386
action_client_raw_try_process        1477, 1496, 1534
action_client_callback_try_process   1834, 1850, 1884
```

All three enclosing functions return `Result<bool, TransportError>`, so
propagation is mechanically available. They are left alone deliberately: unlike
a subscription drain, these paths can return `WouldBlock` as a NORMAL condition
(no request pending), so propagating every `Err` would convert routine emptiness
into a counted error and could redden lanes. Distinguishing the lossy variants
(`BufferTooSmall` / `MessageTooLarge`) from the benign ones needs per-site
judgement and runtime verification, which is a separate change with its own
risk — not a tail on this one.

**This is an audit result, not an oversight.** If they are fixed later, the
filter is the design question, not the propagation.
