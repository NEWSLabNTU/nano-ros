---
id: 1972
title: "Iron/Jazzy matched and incompatible-type events (RMW_EVENT_*_MATCHED, *_INCOMPATIBLE_TYPE) have no event kind here"
status: open
type: enhancement
area: rmw
severity: low
found: 2026-10-19
related: [issue-1971]
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
