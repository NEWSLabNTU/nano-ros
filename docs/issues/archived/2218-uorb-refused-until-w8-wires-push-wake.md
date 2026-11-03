---
id: 2218
title: "uORB sessions state no readiness form, so they are refused at open until phase-521 W8 wires the push-wake glue to the executor"
status: resolved
resolved: 2026-11-03
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

## Resolution (2026-11-03, phase-521 W8 d)

The cause was deeper than "nothing posts the wake". The glue was never
LINKED. `callback_default.cpp` defined `nros_orb_register_callback` as a WEAK
stub beside the glue's strong definition, and in the PX4 module's static
archive the linker pulls the first member that resolves the symbol. That was
the stub, because nothing else references the glue object. So every
registration returned -1 and every subscription silently polled `orb_check`.
The fix has four parts:

- **Linking.** `NrosRmwUorbSources.cmake` now names EXACTLY ONE of
  `NROS_RMW_UORB_PX4_GLUE_SOURCES` or `NROS_RMW_UORB_NO_GLUE_SOURCES` per
  build, and the stub is no longer weak.
- **Readiness.** The session fills `set_wake_callback`, `readiness_source`
  (Notify with the glue, `NROS_RMW_UORB_PX4_GLUE`; none without it, which
  RFC-0113 D5 refuses at open) and `subscription_set_on_new_message_callback`.
  With the glue linked, a subscription whose registration fails is refused
  rather than polled.
- **No lp_default hop.** The glue's callback is a `uORB::SubscriptionCallback`
  called in the publisher's context. It is no longer a work item on
  `lp_default`.
- **Tier binding.** `nros_rmw_uorb_set_arrival_target` binds each subscription
  that a PX4 tier's setup creates to that tier's `TierWorkItem::notify()`.

Measured on PX4 SITL: the generated `nros_uorb_demo` runs on a work item,
`vehicle_status` dispatches on arrival, and `px4_uorb_interop_e2e` passes.
