---
id: 240
title: "RMW vtable naming/shape diverges from rmw.h without RTOS justification: open/close verbs, session/subscriber/service_server terms, transport hints inside the QoS struct, a deprecated blocking call slot"
status: open
type: enhancement
area: rmw
related: [rfc-0035, issue-0238]
---

## Finding (RMW/platform API audit, 2026-07-21)

The RMW C ABI is a faithful, C-ABI-sound remodel of upstream `rmw.h` with
THREE well-justified structural changes — no waitset → `drive_io` +
per-entity event callbacks; `rmw_time_t` → `uint32_t` milliseconds;
positive→negative return codes for the byte-count/error dual convention.
The following divergences are NOT RTOS-driven — they are cosmetic or
organizational drift from the `rmw_` shape, collected here for a single
naming/shape cleanup pass.

### Verb / term drift (cosmetic; API + ABI rename)
- **`open`/`close`** (`rmw_vtable.h:50,53`) break the table's own
  `create_*`/`destroy_*` convention and rmw's `rmw_create_node`/
  `rmw_destroy_node`. → `create_session`/`destroy_session` (or `_node`).
- **`session`** (`nros_rmw_session_t`, `rmw_entity.h:207`) merges
  `rmw_context_t` + `rmw_node_t`; reasonable for 1-node-per-process, but
  the struct carries `node_name`/`namespace_` — it IS a node. "session" is
  a transport term. Consider `nros_rmw_node_t`.
- **`subscriber`** (`nros_rmw_subscriber_t`, `create_subscriber`) — rmw is
  `subscription` everywhere. Gratuitous rename.
- **`service_server`** / **`service_client`** — rmw is `rmw_service_t` /
  `rmw_client_t`; "service_server" is redundant.

### Shape antipattern — transport hints smuggled into the QoS struct
`tx_express` (`lib.rs:258-262`, `traits.rs:459-465`) and `rx_buffer_hint`
(`lib.rs:279-284`, `rmw_entity.h:119-124`) are TRANSPORT HINTS, not DDS QoS
policies — the code comments say so ("a transport hint, not a DDS policy —
no RxO matching"). They live in `nros_rmw_qos_t`, so they get
compared/defaulted/carried on every entity, and each new hint is an
ABI-append into the QoS struct (`rx_buffer_hint` was appended at the tail).
Upstream's home for exactly this class is `rmw_publisher_options_t` /
`rmw_subscription_options_t`. Introducing
`nros_rmw_publisher_options_t`/`_subscription_options_t` params on
`create_publisher`/`create_subscriber` would keep `nros_rmw_qos_t` a pure
policy mirror and stop the QoS-struct ABI churn.

### Deprecated blocking primitive still in the vtable
`call_raw` (`rmw_vtable.h:98`, trait `traits.rs:2008` — already
`#[deprecated]`) is a blocking RPC that duplicates the async
`send_request_raw` + `try_recv_reply_raw` pair (`:112,:123`). rmw has no
blocking call. Plan its removal with this cleanup so there is one
request/reply path.

### Minor note — return-code scheme
`nros_rmw_ret_t`'s negative-error convention is justified (byte-count dual
use), but note it also diverges numerically from `RMW_RET_*` (small
positives) — cross-referenced here so a future rmw-parity reviewer isn't
surprised; NOT proposed for change.

## Direction
These are API + ABI breaks, so batch them into one deliberate rmw-shape
alignment (an RFC amendment to RFC-0035 + a phase), renaming verbs/terms
to the `rmw_` derivable form, moving the transport hints into options
structs, and dropping the deprecated `call_raw` slot. Additions
(ping/loan/streamed/wake/etc.) stay — they are documented RTOS
enhancements, not drift.
