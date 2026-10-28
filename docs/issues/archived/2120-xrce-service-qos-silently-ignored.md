---
id: 2120
title: "An XRCE service or client admits a requested QoS it never applies: the client library drops it and the Agent uses a fixed profile"
status: resolved
type: bug
area: [rmw, xrce, qos]
severity: low
found: 2026-10-27
related: [2107, 1329]
resolved: 2026-10-29
---

## What

`create_service` / `create_client` on XRCE take a QoS profile, check it
against `xrce_supported_qos_policies` (reliability, durability, history, depth,
avoid-ROS), admit it, and pass it to `uxr_buffer_create_{replier,requester}_bin`.
That function discards it: micro-XRCE-DDS-Client 3.0.1's
`create_entities_bin.c` reads `(void) qos;` and serializes no QoS into
`OBJK_{Requester,Replier}_Binary`. The Agent (2.4.3) then builds both endpoints
from fixed attributes (`FastDDSMiddleware::create_{requester,replier}_by_bin`):
the writer from a default `PublisherAttributes`, the reader from a default
`SubscriberAttributes` with reliability forced RELIABLE.

Measured by issue 2107 with a stock `rmw_fastrtps_cpp` peer reading each
endpoint's discovered QoS (`get_{publishers,subscriptions}_info_by_topic`,
`no_mangle`), for a server and a client created with the services-default
profile (RELIABLE, VOLATILE):

| endpoint | reliability | durability |
| --- | --- | --- |
| request writer (client) | RELIABLE | **TRANSIENT_LOCAL** |
| reply reader (client) | RELIABLE | VOLATILE |
| request reader (server) | RELIABLE | VOLATILE |
| reply writer (server) | RELIABLE | **TRANSIENT_LOCAL** |

So a caller that asks for BEST_EFFORT, or for TRANSIENT_LOCAL on the readers, or
for a depth, is told OK and gets the table above. That is issue 1329's
"admitted, then ignored all the way to the wire", for services only — pub/sub
does carry its four fields.

Since issue 2107 the four service/client `*_get_actual_qos` slots report the
table, so the difference is visible to a caller who asks. Nothing refuses it.

## What would close it

Either refuse at service/client create any honoured policy that differs from
the fixed profile (the slot can still admit the services default), or make the
QoS reach the Agent (the client library would have to serialize it and the
Agent to apply it — both upstream, and the fork rule applies to the client
submodule). The refusal is local and cheap; the mask is per session, so it needs
a per-entity-kind check beside `xrce_supported_qos_policies`.

## Resolution

The QoS now reaches the Agent. Both upstream halves already support it,
through XML instead of BIN, so no fork was needed.

- **Agent.** Agent 2.4.3 `FastDDSMiddleware::create_{requester,replier}_by_xml`
  parses the string with Fast-DDS 2.14's own `XMLParser`
  (`fillDataNode(RequesterAttributes | ReplierAttributes)`).
  `FastDDS{Requester,Replier}::create_by_attributes` then copies the
  `<publisher>` / `<subscriber>` reliability, durability and history into the
  DataWriterQos / DataReaderQos it creates. It creates the endpoints in the
  same order as the BIN path (writer, then reader), so issue 1292's GID
  counting is unchanged.
- **Client.** `uxr_buffer_create_{requester,replier}_xml` sends the string
  as-is.

`service.c` builds the XML (`xrce_service_qos_xml`, `session.c`) with every
honoured field written explicitly, because a default `PublisherAttributes` is
TRANSIENT_LOCAL. It keeps the BIN path's `PREALLOCATED_WITH_REALLOC` history
memory policy, and omits depth when 0, as `uxrQoS_t` does. The XML is about
1 KB. If it does not fit the reliable output stream (small MTU x history), the
BIN create is sent instead. The four `*_get_actual_qos` slots report what the
XML carried, or the fixed profile above after a BIN fallback (`qos_applied`
in the server/client state).

### Measured

Live, against the pinned Agent with a stock `rmw_fastrtps_cpp` rclpy peer.
The peer read each endpoint's discovered QoS
(`get_{publishers,subscriptions}_info_by_topic(..., no_mangle=True)` on
`rr/add_two_intsReply` / `rq/add_two_intsRequest`). The new test
`xrce_ros2_interop::test_xrce_service_endpoints_hold_the_requested_qos`
asserts exactly this:

| server QoS | reply writer | request reader |
| --- | --- | --- |
| services default, XML (this fix) | RELIABLE / VOLATILE | RELIABLE / VOLATILE |
| BEST_EFFORT, XML (this fix) | BEST_EFFORT / VOLATILE | BEST_EFFORT / VOLATILE |
| either, BIN (negative control) | RELIABLE / **TRANSIENT_LOCAL** | RELIABLE / VOLATILE |

The negative control forced `xrce_service_qos_xml` to report "did not fit",
which takes the BIN fallback. Both cases fail, reproducing the table above.
The rest of `xrce_ros2_interop` passed on the XML path too, including
`test_ros2_service_xrce_client` (the requester), the request-id test, and
the graph tests, which count on the Agent's endpoint order.

Unit: `tests/actual_qos.c` covers the XML-applied report on both endpoints,
the BIN-fallback report, the XML content (both endpoints, KEEP_ALL with no
depth), and a too-small buffer being refused rather than truncated
(`just check rmw-xrce`, 12/12).
