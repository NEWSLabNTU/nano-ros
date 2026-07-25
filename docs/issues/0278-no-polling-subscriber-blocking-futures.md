---
id: 278
title: "No polling subscriber / blocking service futures — mrm_handler-class nodes must weaken to cache-latest + send-and-poll"
status: open
type: enhancement
area: nros-cpp
related: [0254]
---

## Finding (autoware-safety-island-example ports, 2026-07-24 — porting-notes 14)

Two upstream idioms have no nano-ros equivalent:

- `autoware_utils::InterProcessPollingSubscriber` (pull-based take) — ported
  as callback subs caching latest + `has_` flag. Adequate, but a take-style
  API would keep ports verbatim.
- Blocking service futures inside callbacks: `requestMrmBehavior` does a
  10 ms `future.wait_for` inside a timer callback. A blocking wait would
  need nested executor spin; the port weakened to send-and-poll (replies
  drained on later ticks, "success" = request sent). Behavior delta
  documented in-source, but it IS a semantic weakening of the safety path.

## Direction

- take()-style subscription accessor (RMW already caches latest per sub).
- Bounded-wait service call usable inside executor context, or an async
  callback-continuation form the compat layer can express.
