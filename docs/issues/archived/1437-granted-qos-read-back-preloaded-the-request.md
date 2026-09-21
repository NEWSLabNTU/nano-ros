---
id: 1437
title: "The `*_get_actual_qos` read-back was pre-loaded with the REQUEST, so an
  unreportable policy read back as a confident grant — and the answer was
  dropped anyway"
status: resolved
type: bug
area: rmw
severity: medium
related: [1327, 1329, 0823, phase-417]
---

## What was true

Two defects in one seam, and each hid the other.

**1. The pre-load inverted the slot's own contract.** `rmw_vtable.h` has always
said a backend writes `*_UNKNOWN` for a policy it cannot determine and returns
`NROS_RMW_RET_OK` for the rest — a PARTIAL answer is legal, and `UNKNOWN` is
how the gap is spelled. The caller, `report_granted_qos` in
`packages/rmw/cffi/src/lib.rs`, pre-loaded the out-struct with the REQUEST
instead, and cyclonedds' `qos_from_dds` overwrites exactly the fields whose
`dds_qget_*` succeeds. So every field a backend could not report read back as
*granted exactly what you asked for* — the inverse of the meaning — and the one
consumer (`report_qos_downgrade`) saw agreement precisely where nothing had been
looked at.

The `*_UNKNOWN` policy values had existed since phase-376 W5/B2 and
`NROS_RMW_QOS_PROFILE_UNKNOWN` since phase-428 W10, and **no
`*_get_actual_qos` implementation anywhere wrote one**: the slots whose
documentation defines the contract were the only ones not honouring it. The
Rust side could not have: three of the four policy enums had no `Unknown`
variant to build the profile from, which is what the
`nros-qos-absent: upstream=rmw_qos_profile_unknown` note in `traits.rs` recorded.

**2. Nobody kept the answer.** Issue 1327 landed the CONSUMER — `create_client`
and `create_service` read all four service/client directions — and
`create_publisher` had read its own since phase-393 W1. All five sites logged
the grant and dropped it:

```rust
unsafe { report_granted_qos("publisher", topic.name, &qos_struct, &v, slot) };
```

So the value existed at the seam, was computed correctly on cyclonedds, and was
unreachable from every language. `grep -rn actual_qos` over the facade, the node
handles and the C and C++ headers found nothing outside the RMW.

**And the sixth slot was never dispatched at all.** `subscription_get_actual_qos`
was filled by cyclonedds AND uORB and called by nobody — the entity whose
reliability downgrade is the textbook "why is nothing arriving" was the one that
could not answer. 1327 counted the four it was about; the fifth was already
there and the sixth was missed by both passes.

## Why it matters

A reliability or depth downgrade is silent, and silence from a QoS mismatch is
indistinguishable from a name typo (issue 0801's domain split, issue 0803's
discovery failure — each cost hours). The read-back is the one mechanism that
tells the two apart, and it was reporting "no difference" for any policy it had
not read.

## Fixed

* `QoSHistoryPolicy` / `QoSReliabilityPolicy` / `QoSDurabilityPolicy` /
  `QoSLivelinessPolicy` gain `Unknown`, and `QoSProfile::QOS_PROFILE_UNKNOWN`
  joins the `qos_profiles!` SSoT table with `rmw_qos_profile_unknown` as its
  provenance — so `check-qos-profile-ssot` compares it against the extracted
  upstream record field by field, and `rmw_entity.h`'s `UNKNOWN` row stops being
  an unmodelled mirror.
* **`UNKNOWN` is refused as a REQUEST**, once, in
  `QoSProfile::validate_against`: the way it reaches a create call is a caller
  feeding a read-back profile back in, and `required_policies` cannot express it
  because there is no capability a backend could declare that makes "I cannot
  say" servable. zenoh's `qos::admit` refuses it per policy as well, naming it.
* The pre-load is `qos_unknown_c()` — built from the Rust preset, so the two
  cannot drift. `report_qos_downgrade` skips an `UNKNOWN` field rather than
  reporting it as a change: silence is not a grant, and neither is it a
  downgrade.
* The six read-backs are RETAINED: `actual_qos` on `CffiPublisher` /
  `CffiSubscription`, and `request_qos` / `response_qos` on `CffiService` /
  `CffiClient` (one create builds two endpoints that negotiate against
  different peers; neither `rmw_service_t` nor `rmw_client_t` has a `qos`
  field, so this is the only place the per-direction answer exists).
* `nros_rmw::{Publisher, Subscription, ServiceTrait, ClientTrait}` gain the six
  upstream-named accessors, defaulting to `QOS_PROFILE_UNKNOWN` — the honest
  answer for a backend with no read-back, and deliberately neither the request
  nor `QOS_PROFILE_DEFAULT`.
* **zenoh, the default backend, now answers.** It GRANTS rather than echoes —
  `qos::admit` clamps depth to the ring it enforces, grants RELIABLE whatever
  was asked, and serves TRANSIENT_LOCAL on a publisher at the retention depth —
  and that granted profile is already what goes into the liveliness token a
  `rmw_zenoh_cpp` peer parses. It is retained on each entity and reported, so
  what a caller reads back and what the graph carries are one value.
  `rust_adapter` installs the six trampolines for every Rust backend.
* uORB writes `durability = VOLATILE` (the only value its `qos_admit` lets
  through, so an entity that exists is running it) beside the depth and history
  it already reported, and leaves reliability `UNKNOWN` — it does not retain
  the request, so it genuinely cannot say.

## Acceptance

`nros-rmw`'s `granted_qos_tests`: the trait default is an absence, `UNKNOWN` is
not `SYSTEM_DEFAULT`, a read-back profile cannot be used as a request, and
`UNKNOWN` demands no capability bit. `nros-rmw-cffi`'s 1327 tests now assert the
slot was handed `UNKNOWN` — and `assert_ne!` against the request, so the old
pre-load fails them — plus that every direction's answer is RETAINED and
readable, and that a NULL slot answers an absence rather than the request.

## Not done here

The C and C++ SURFACES. `nros_qos_t`'s four enums
(`nros_qos_reliability_t` and friends, `nros_generated.h`) carry only concrete
values — no `SYSTEM_DEFAULT`, no `UNKNOWN` — so exposing a granted profile
through them needs additive enum values and a cbindgen regeneration of two
committed headers before `rcl_publisher_get_actual_qos` and
`Publisher::get_actual_qos` can be written. Twelve `gap` rows in the
`pubsub`/`service` ledger shards stay open for it, with their `why` amended to
record that the value now EXISTS and is reachable from Rust.
