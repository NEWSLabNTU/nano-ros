---
id: 1329
title: "The cffi QoS mask is the UNION of three backends, so it over-claims for
  each of them"
status: resolved
type: bug
area: rmw
severity: medium
related: [1328, 1327]
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

## Resolved 2026-09-12

The slot exists:

```c
rmw_ret_t (*supported_qos_policies)(const rmw_session_t *, uint32_t *);
```

with the bit vocabulary as `NROS_RMW_QOS_POLICY_*` macros in `rmw_entity.h`,
mirroring `QoSPolicyMask` name for name and bit for bit. `CffiSession` asks the
registered backend once at `open` and caches the answer; the union is gone and
no second copy of it survives to drift.

### NULL means the backend has NOT SAID, and the runtime reads that as NONE

The one decision here worth arguing, so: the alternatives are both worse.
"NULL means the union" reinstates this issue with the ABI as its author — a
route that cannot ask answering as though every backend honoured everything.
"NULL means do not validate" is worse still: it admits even the bits no backend
implements, and switches off the no-silent-downgrade contract at the one seam
that enforces it, invisibly.

NONE is the answer `Session::supported_qos_policies`'s trait default already
gives an implementation that has written no code — phase-428 W9 moved it from
`CORE` to nothing for exactly this reason — so the ABI and the trait give ONE
answer to ONE question. And it is loud, which is the property that makes it
safe: `IncompatibleQos` at create naming a policy, plus one WARN per session
naming the missing slot. An application cannot mistake it for working, which is
what the union allowed for as long as it stood.

The slot is deliberately NOT added to `first_missing_vtable_slot` — that set
stays equal to what the runtime `.expect()`s (issue 0349). What makes it
mandatory in practice is the gate: **a backend that carries any
`nros-qos-honours:` claim and fills no slot is a failure**, because leaving it
NULL is a declaration that it honours nothing.

### Per backend, measured

`check-qos-mask-derivation --audit`, 2026-09-12, against this issue's own table:

| policy | cyclonedds | xrce | uorb |
| --- | --- | --- | --- |
| reliability | yes | yes | **yes** (was "no") |
| durability, both values | yes | yes | VOLATILE only |
| history | yes | yes | yes |
| depth | yes | yes | yes |
| deadline | yes | **no** (was silently ignored) | no |
| lifespan | yes | **no** | no |
| liveliness kind + lease | AUTOMATIC, MANUAL_BY_TOPIC, lease | **no** | no |
| avoid_ros_namespace_conventions | **no** | **yes, services included** | no |

Three rows differ from the table above, and each is a decision rather than a
re-measurement:

* **uorb is not "nothing".** The issue predicted it would lose everything and
  take the PX4 path with it. Instead the four CORE policies are EARNED: a new
  `qos.cpp` refuses KEEP_ALL and TRANSIENT_LOCAL (the ring overwrites and keeps
  no cache for late joiners), accepts both reliabilities because a shared-memory
  ring loses nothing within its depth, and grants depth down to the topic's
  `o_queue` while REPORTING the grant through `publisher_get_actual_qos` /
  `subscription_get_actual_qos`, which this backend now fills.
  Granted-and-reported is a different thing from clamped-and-silent. The PX4
  example keeps working and its QoS now means something.
* **xrce keeps `avoid_ros_namespace_conventions` and it is now true of the
  whole backend.** It was honoured for publishers and subscriptions and not for
  services, which a per-backend mask cannot express — a bit is true of the
  backend or it is not. Refusing the policy outright would have cost the
  pub/sub path something that works, so `xrce_dds_request_topic` /
  `xrce_dds_reply_topic` drop the `rq/` + `rr/` prefixes too, those being the
  service-side counterpart of `rt/`. No shipped profile sets the flag.
* **xrce loses liveliness, and that is what the issue said would break every C
  application.** It would have: every C and C++ default profile stated
  `LIVELINESS_AUTOMATIC`, so `required_policies()` demanded a bit XRCE cannot
  serve. The fix is not to let XRCE claim the policy — it lowers no liveliness
  field at all — but to stop ASKING for one upstream leaves unset. `nros-c`'s
  three `NROS_QOS_*` statics, `nros_c_qos_default()` and `nros::QoS`'s
  constructor moved onto the sentinel in one commit, closing the
  mirror-deviation phase-428 W10 had recorded and could not move alone. Wire
  effect on all three backends: none (cyclonedds' own default IS
  `DDS_LIVELINESS_AUTOMATIC` and it only calls `dds_qset_liveliness` for a
  non-sentinel kind; zenoh's keyexpr leaves the liveliness positions empty;
  XRCE has no field).

### The gate moved with the shape

`check-qos-mask-derivation` could only ask whether every bit in the UNION was
honoured by somebody. It now asks, per backend:

* a `nros-qos-mux:` session must ROUTE, not author a mask;
* each routed backend's C mask must EQUAL its evidenced claims, in BOTH
  directions — an extra bit is the old over-claim, a missing one refuses a
  policy the backend implements;
* a backend that honours anything and fills no slot is an error;
* and the two halves of the vocabulary must agree name for name and bit for
  bit, because a backend setting one bit while the runtime reads another
  ACCEPTS the wrong profile, silently. (`nros-rmw-cffi` asserts the same pair
  at compile time; the gate is the buildless half.)

Everything stays derived: the C mask is read out of the function the vtable
installs, with comments stripped so a doc block naming the policies a backend
does NOT claim cannot be read as a claim.

Five more live-tree negative controls, on the normal path, eleven in total:
re-advertise a deadline XRCE cannot serve; drop LIFESPAN from cyclonedds' mask
while the claim stands; NULL uORB's slot while its claims stand; put the union
back in the cffi route; move a C policy bit out from under its Rust twin.

### Acceptance beyond the gate

`the_route_returns_the_registered_backends_mask` scripts a backend mask no
union in the tree ever produced, so passing it cannot be the old constant still
being returned. `a_backend_that_fills_no_mask_slot_honours_nothing` pins the
NULL-slot meaning. Both backends that can be built on a plain host were built
and their suites run: `just check rmw-xrce` and `just check rmw-uorb` green,
and `just check rmw-cyclonedds` 31/31 with the submodule initialised.
