---
id: 1972
title: "Iron/Jazzy matched and incompatible-type events (RMW_EVENT_*_MATCHED, *_INCOMPATIBLE_TYPE) have no event kind here"
status: resolved
type: enhancement
area: rmw
severity: low
found: 2026-10-19
related: [issue-1978]
resolved_in: 2026-10-20
---

## What

Iron added four `rmw_event_type_t` values that Jazzy ships and Humble does
not (`docs/reference/editions/jazzy/rmw-type-surface.txt` vs the Humble one):

- `RMW_EVENT_PUBLICATION_MATCHED` / `RMW_EVENT_SUBSCRIPTION_MATCHED`, payload
  `rmw_matched_status_t {total_count, total_count_change, current_count,
  current_count_change}`;
- `RMW_EVENT_PUBLISHER_INCOMPATIBLE_TYPE` /
  `RMW_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE`, payload
  `rmw_incompatible_type_status_t`.

rclcpp exposes them as `MatchedInfo`, `PublisherMatchedCallbackType`,
`SubscriptionMatchedCallbackType`, `IncompatibleTypeInfo` and
`IncompatibleTypeCallbackType` (ledger rows `"editions": ["jazzy"]`).

## Ruling: ADOPT

The matched pair is the event an embedded node most often wants ("start
publishing once somebody listens") and today has to POLL
`publisher_count_matched_subscriptions` for. The payload is four integers;
no allocation, no new slot — it is two more `rmw_event_type_t` values through
the existing `publisher_event_init` / `subscription_event_init` /
`take_event` path, so the cost is the backend producer and two payload arms.

Incompatible-type is ruled with it because it is the same machinery: a graph
backend sees a remote endpoint on the same topic with a different type name
(Cyclone: builtin topic data; zenoh: the liveliness token's type segment).

## Work

1. Extend nano-ros's `rmw_event_type_t` + `rmw_event_payload_t` with the four
   kinds and two payloads, keeping upstream's ordering so the enum stays
   value-compatible (Jazzy inserted them before `RMW_EVENT_INVALID`).
2. Producers: Cyclone (`DDS_PUBLICATION_MATCHED_STATUS` /
   `DDS_SUBSCRIPTION_MATCHED_STATUS`), zenoh (graph cache match counter).
3. C/C++ event callbacks for the four, alongside the existing QoS events.
4. Delete the `gap` rows; the rmw type-delta rows move to `same`.

## Resolution

**ABI (header SSoT, append-only).** `rmw_event.h` gains
`NROS_RMW_EVENT_PUBLICATION_MATCHED` = 5, `_SUBSCRIPTION_MATCHED` = 6,
`_PUBLISHER_INCOMPATIBLE_TYPE` = 7, `_SUBSCRIPTION_INCOMPATIBLE_TYPE` = 8 —
APPENDED, not inserted at upstream's positions. Work item 1 asked for
upstream's ordering; that was never available, because our enum was never
value-compatible with upstream's (LIVELINESS_LOST is 3 here, 6 there), and
inserting would have renumbered every compiled backend's switch arms.
`rmw_matched_status_t {total_count, total_count_change, current_count,
current_count_change}` and `rmw_incompatible_type_status_t {total_count,
total_count_change}` are new, `uint32_t` counts (upstream: `size_t` /
`int32_t`) so the by-value payload has one layout on 32- and 64-bit targets;
`rmw_event_payload_t` grows from 8 to 16 bytes (the caller owns the union, so
an older backend writes within it). No new slot. Bindings regenerated
(`scripts/gen-abi-bindings.sh`), C/C++ headers regenerated
(`just regen-c-headers`).

**Producers.**

| Backend | Matched | Incompatible type |
|---|---|---|
| Cyclone DDS | `dds_get_{publication,subscription}_matched_status`, polled through `*_take_event`. Changes are computed against what the event last reported, not read from Cyclone's `*_change` counters — the publisher's create path reads the same status and would swallow a change | endpoints on the same DDS topic name with another type name, read from the builtin `DCPSPublication` / `DCPSSubscription` topics. **Not** `DDS_INCONSISTENT_TOPIC_STATUS` as `rmw_cyclonedds_cpp` maps it: in the pinned 0.10.5 its raiser is under `#if 0` (`dds_topic.c`), so that mapping would report a supported event that cannot fire |
| zenoh | the session's graph cache (`shim::matched`): liveliness tokens of the opposite kind with the same mangled topic name and DDS type, as `rmw_zenoh_cpp` derives it. Hashes, not string copies, so an entity pays ~100 B rather than half a KiB | same walk, same name, other type |
| XRCE-DDS, uORB | unsupported (no event slot) | unsupported |

Both are POLLED from the entity's data path, like every status event of
these backends: a subscription on `has_data` / take, a publisher on
`publish` / `assert_liveliness` (the latter is the poll point for a
publisher waiting for a listener). Stated bounds: zenoh's counts are
snapshots, so `total_count` counts increases seen between polls; QoS is not
consulted on zenoh; local endpoints count on both; zenoh refuses the
registration when graph discovery is compiled out.

**Faces.**

| Layer | API |
|---|---|
| Rust (`nros-rmw`) | `EventKind::{PublicationMatched, SubscriptionMatched, PublisherIncompatibleType, SubscriptionIncompatibleType}`, `MatchedStatus`, `IncompatibleTypeStatus`, `EventPayload` arms, and `MatchedCounter` / `IncompatibleTypeCounter` (snapshot -> report, no_std) |
| Rust (`nros-node`) | `on_matched(FnMut(MatchedStatus))` / `on_incompatible_type(FnMut(IncompatibleTypeStatus))` on the publisher and both subscription handles |
| C | `rclc_{publisher,subscription}_set_{matched,incompatible_type}_callback`, `rclc_matched_status_t`, `rclc_incompatible_type_status_t` |
| C++ | `rclcpp::MatchedInfo`, `IncompatibleTypeInfo`, `PublisherMatchedCallbackType`, `SubscriptionMatchedCallbackType`, `IncompatibleTypeCallbackType`; `Publisher::on_matched` / `on_incompatible_type` and the same on `PollSubscription` |

Divergences: callbacks are a function pointer plus `user_context`, not
`std::function` (no heap closure); one setter per event rather than the
`PublisherEventCallbacks` struct. The C and C++ setters REACH the backend —
unlike the five Tier-1 C/C++ setters, which still answer UNSUPPORTED at that
layer (not this issue's scope). Their `(callback, context, entity)` triple
lives in nros-c's static `status_event_slots` table (16 registrations), and
the backend holds a generation-tagged slot reference, so a registration that
outlives its entity — an arena-owned C subscription survives
`rcl_subscription_fini` — is a no-op rather than another entity's callback;
a C++ move re-keys the slot. A C subscription must be added to an executor
before it can register (`RCL_RET_NOT_INIT` otherwise).

**Ledgers.** `rmw-api-map.toml`'s ten `not-implemented` type-delta rows: the
two `type:` rows are now DERIVED `same` (rows deleted), the four enum rows and
six field rows `re-mapped` (`NROS_` prefix / struct tag / width). The five
`gap` rows (`cpp:MatchedInfo`, `IncompatibleTypeInfo`,
`IncompatibleTypeCallbackType`, `PublisherMatchedCallbackType`,
`SubscriptionMatchedCallbackType`) are gone — `same` under Jazzy, a
Humble-scoped `divergence` row under Humble — and the new C/C++/Rust items
are ledgered. Feature matrix: new row "Matched / incompatible-type events
(Iron+)", wired for zenoh and Cyclone (a C backend counts only with a
`case` arm with a body for each matched kind).

**Tests.** `nros-rmw` counter + payload units; `nros-rmw-cffi`
`matched_event_poll.rs` (polled path, side checks, the two fields past the old
8-byte union arrive intact); nros-c `status_event_slots` unit (routing,
relocation, stale-tag refusal, bound); `nros-node`
`matched_and_incompatible_type_callbacks_reach_the_application` (mock
backend); C and C++ declaration probes in `just check c` / `just check cpp`;
Cyclone `nros_rmw_cyclonedds_matched_events` (live reader/writer: 0 -> 1 -> 0,
then a second type raises incompatible-type); zenoh `matched_events_live`
(`just linux test-matched-events`: a peer PROCESS's publisher appears and is
killed, current_count 0 -> 1 -> 0; a different-type peer raises
incompatible-type; a publisher sees a remote subscription). No interop cell
against a stock `ros2` peer was added.

