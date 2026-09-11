---
id: 1329
title: "The cffi QoS mask is the UNION of three backends, so it over-claims for
  each of them"
status: open
type: bug
area: rmw
severity: medium
related: [1328]
---

## What happens

`CffiSession::supported_qos_policies()` answers for whichever C backend is
registered, and it has no way to ask that backend. The vtable has no
`supported_qos_policies` slot, so the route returns the UNION of what any
nros-supported RMW honours and relies on the backend surfacing
`NROS_RMW_RET_INCOMPATIBLE_QOS` from its own create.

Two of the three backends never surface it.

Measured 2026-09-11 (phase-428 W9), per policy:

| policy | cyclonedds | xrce | uorb |
| --- | --- | --- | --- |
| reliability | yes (`qos.cpp:38`) | yes, in the CREATE submessage (`session.c`) | no |
| durability (both values) | yes (`qos.cpp:53`) | yes | no |
| history / depth | yes (`qos.cpp:84`) | yes, deferred to the Agent | no |
| deadline | yes (`qos.cpp:93`) | **dropped** (`optional_deadline_msec = false`) | no |
| lifespan | yes (`qos.cpp:96`) | **dropped** | no |
| liveliness kind + lease | yes, AUTOMATIC and MANUAL_BY_TOPIC | **dropped** — no field in `uxrQoS_t` | no |
| avoid_ros_namespace_conventions | **no read site** — mangling is env-only (`NROS_RMW_CYCLONEDDS_SKIP_PREFIX`) | pub + sub only, not services | no |

uorb binds the parameter as `const rmw_qos_profile_t * /*qos*/` at all four
create slots and returns `NROS_RMW_RET_UNSUPPORTED` for services. It honours
nothing and refuses nothing.

So an application that asks cyclonedds for `avoid_ros_namespace_conventions`,
or xrce for a deadline, is admitted by the runtime's pre-validate step and then
silently ignored downstream — the no-silent-downgrade contract broken one layer
below where it is enforced.

## Why the obvious narrowing is not the fix

Replacing the union with a per-backend table keyed on the registered name was
tried on paper and rejected, because the intersection is empty and the
per-backend rows break working builds:

* xrce would lose `LIVELINESS_AUTOMATIC`. Every default profile in the C API
  states `NROS_QOS_LIVELINESS_AUTOMATIC` (`packages/api/nros-c/src/qos.rs`), so
  `required_policies()` demands the bit and **every C application on xrce would
  fail at entity create**.
* uorb would lose everything, and any stated profile — including
  `QOS_PROFILE_DEFAULT` — would be refused, taking the PX4 path with it.

Both are real refusals of policies those backends genuinely do not implement,
which is the point; but landing them as a side effect of a mask change, on
backends whose test lanes need submodules a plain worktree does not have, is
not a safe way to discover that.

## Fix

Give the vtable the slot the TODO has asked for since phase 115: a
`supported_qos_policies` callback the route queries, with each C backend
returning what its own code honours. Then:

1. the answer is the backend's, not a guess about it;
2. the narrowings above land per backend, with each backend's own lane to
   measure them;
3. where a backend's honest mask would refuse a profile that works today
   (xrce + `LIVELINESS_AUTOMATIC`), the choice becomes explicit — implement the
   policy, or state the refusal — instead of being hidden by a union.

Until then the over-claim is TRACKED rather than silent:
`check-qos-mask-derivation` requires the cffi mask to declare
`nros-qos-mux:` with the crates it routes to, and every bit in the union to be
honoured by at least one of them. That is what caught issue 1328 — a bit in the
union that no routed backend honoured at all.
