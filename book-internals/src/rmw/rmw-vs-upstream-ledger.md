# RMW vs upstream: the decision ledger

The user page [RMW API: Differences from upstream `rmw.h`](rmw-vs-upstream.md)
states what is true of the vtable today. This page keeps the trail behind
those statements: which phase, issue or RFC made each decision, and what the
page used to say before the decision landed. Read it before changing a
divergence, so the reason it exists is not lost.

Issue files live under `docs/issues/archived/` unless noted; phase docs under
`docs/roadmap/archived/`.

## Vocabulary and slot shape

- **rmw names.** The vtable once used its own vocabulary (`subscriber`,
  `service_server`, `service_client`, `open`/`close`). Phase-301
  (`phase-301-rmw-shape-alignment.md`, issues 0240 and 0241) aligned it with
  upstream: `rmw_subscription_t` / `rmw_service_t` / `rmw_client_t`,
  `create_session` / `destroy_session`, transport hints moved out of
  `rmw_qos_profile_t` into per-create options structs, and the deprecated
  blocking `call_raw` slot was deleted. The Rust traits were renamed in the
  same phase (`Subscriber` → `Subscription`, `ServiceServerTrait` →
  `ServiceTrait`, `ServiceClientTrait` → `ClientTrait`).
- **`can_loan_messages` is derived** (issue 0814). The flag and the
  `borrow_loaned_message` slot had drifted in both directions: nothing outside
  a test ever wrote `true` (so a zenoh build whose loans worked advertised
  that it could not loan), while a backend writing `true` with a NULL slot was
  believed. The page also once claimed the runtime "dispatches the publish
  path with no per-call branch" from the flag; it never did.
- **Return codes** use upstream rmw's values at the ABI (phase-376 W3.d,
  `phase-376-rmw-api-parity.md`).
- **Graph slots** were added as visitor callbacks in phase-376 W4; node
  entities (`rmw_node_t`) arrived in phase-376 W5/B1.
- **Heap is a backend property** (issue 0777, page
  [RMW API Design](rmw-api-design.md)). That page said "no heap" flatly
  for years; `scripts/rmw-alloc-sites.py` now counts the allocation sites
  per backend.
- **zenoh version reading** (issue 0609): the ROS package version of
  `rmw_zenoh_cpp` moved its vendored zenoh 1.2.0 → 1.8.0 in a patch bump, and
  reading the package version instead of `zenoh_configure.h` produced a wrong
  version claim in that issue's first filing. The router pairing itself is
  phase-362 / RFC-0075.

## QoS

- **`_SYSTEM_DEFAULT` names no concrete policy** (issue 0829: the two
  system-default presets disagreed on depth).
- **The QoS mask default is EMPTY** (phase-428 W9,
  `phase-428-api-porting-principle-sweep.md`). It was `QoSPolicyMask::CORE`
  — four policies granted to any implementation that had written no code —
  until measurement found the zenoh shim honouring none of the four. The
  `check-qos-mask-derivation` gate now requires each bit to be backed by a
  claim sited on the code that reads the profile field.
- **The C ABI answers per backend** (issue 1329). The cffi route used to
  answer the UNION of what any supported RMW honours, which over-claimed for
  each of them.
- **Manual liveliness answers `UNSUPPORTED` where there is no per-topic
  lease** (phase-467 RMW gap-closure study, Row 7,
  `phase-467-rmw-gap-closure-design-study.md`). The unsupported default used
  to be `Ok(())` whatever the kind, which reported an assertion that never
  happened.

## Status events

- **Matched and incompatible-type events** (issue 1972). `MATCHED` was
  skipped until issue 1972 ruled it ADOPT; `INCOMPATIBLE_TYPE` came with it.

## XRCE-DDS NULL slots (section 9b of the user page)

`just check rmw-slot-producers` prints how many slots each backend fills.
The rows marked **filled** on the user page were filled by:

| slot family | issue |
| --- | --- |
| graph: node names + change edge | 2106 (`2106-xrce-node-enumeration-from-ros-discovery-info.md`) |
| granted QoS (`*_get_actual_qos`) | 2107; requester/replier created from XML since issue 2120 |
| status events: deadline | 2108 |
| per-entity new-data callbacks | 2109 |
| take with info (receive time) | 2135 |
| request info | 2081, measured live in issue 2088 |
| response info (receive time) | 2136 |
| identity (`get_gid_for_*`) | 2110; GID derivation from issue 1292 |

None of the gaps was trivial enough to land with the rulings: the closest,
`*_get_actual_qos`, needed a measurement first, which issue 2107 made (an
Agent-side profile cannot override a BIN-created entity's QoS) and which
found that a requester/replier ignores its requested QoS altogether.

## Publisher GID, message-info and service info

The section records issue 0242 (parity gaps: publisher GID, message info),
updated 2026-07-27.

- C `rclc_executor_add_subscription_with_info` registrations became
  removable in issue 1668.
- `take_with_info` as the C/C++ backend channel, and one GID from one source
  for `get_gid()` and `MessageInfo::publisher_gid`: issue 1495.
- `received_timestamp` stamping: issue 2135.
- Service info on the server side: issues 2081 (C ABI request info) and
  1942 (rclrs `ServiceInfo` counterpart, zenoh side table).
- Client side: `take_response_with_info` from issue 2136; sequence-id
  correlation from issue 0778 (and 2009 for `RequestCorrelator`); the
  callback client and C++ `PollClient::take_response` header from issue 2148.
