---
id: 2218
title: "uORB sessions state no readiness form, so they are refused at open until phase-521 W8 wires the push-wake glue to the executor"
status: open
type: limitation
severity: medium
area: [rmw, uorb, px4]
related: [phase-521]
found: 2026-11-02
---

## What

Since phase-521 W6 a session that states no readiness form is refused at open
(RFC-0113 D5, `nros_rmw_cffi::refuse_without_readiness`). The uORB backend
fills neither `set_wake_callback` nor `readiness_source`, so every uORB
session states `None`: the polling build, as RFC-0113 intends, and also the
PX4 build whose `px4_callback_glue.cpp` registers a
`SubscriptionCallbackWorkItem` per subscription. That glue only sets the
subscription's `ready` flag; nothing posts the executor's wake object.

No CI lane opens a uORB session through the runtime (PX4 SITL is the manual
`just px4 test-sitl`), so no gate went red. A PX4 SITL image is refused at
open until this is fixed.

## Why it is not fixed here

RFC-0113 D9 / phase-521 W8 makes PX4 a Tier 1 framework whose tiers are work
items, and the maintainer ruling of 2026-11-02 holds W8's code pending a
ruling on its next-deadline hook. The readiness half (the glue's callback
calling the session's wake callback, and a `readiness_source` slot answering
`Notify` when the glue is linked and every subscription registered) is W8's
"arrivals through `SubscriptionCallback`" item.
