---
id: 2135
title: "No RMW backend reported received_timestamp on a taken sample or request; zero meant 'not reported' everywhere"
status: resolved
type: limitation
area: [rmw, zenoh, xrce, cyclonedds]
severity: low
found: 2026-10-29
related: [2081, 2088, 1942, 1495, 2136]
resolved: 2026-10-29
---

## What

Upstream `rmw_message_info_t::received_timestamp` (on a taken sample) and
`rmw_service_info_t::received_timestamp` (on a taken request) say when the
local RMW received the message. Every backend here left both at 0, which the
runtime reports as "not reported" (`None` in `ServiceInfo`, `Time(0)` in
`MessageInfo`).

Upstream's semantics, read from source:

| RMW | sample | request |
| --- | --- | --- |
| `rmw_zenoh_cpp` (Humble) | `system_clock::now()` in the sample callback (`get_system_time_in_ns`) | `system_clock::now()` when the query arrives (`ZenohQuery`) |
| `rmw_cyclonedds_cpp` Humble | `0` ("TODO … when implemented by Cyclone") | `0` |
| `rmw_cyclonedds_cpp` Jazzy | `system_clock::now()` at TAKE time | the same |

So the field is the receiver's WALL clock (ns since the Unix epoch), stamped
on receipt. The source timestamp a nano-ros publisher writes is read from the
same clock, `nros_platform_time_now_ns`.

## Resolution

Filled wherever the backend sees the arrival, on the platform wall clock
(`nros_platform_time_now_ns`; 0 on a platform with none, which still means
"not stamped"):

| backend | sample (`take_with_info` / `MessageInfo`) | request (`ServiceInfo`) |
| --- | --- | --- |
| zenoh | the ring producer stamps each slot (`zpico_ring_desc_t::recv_ns`, `_zpico_ring_stamp`) | `queryable_callback` stamps the inbox entry (`InboxEntry::set_recv_ns`); reported even when the attachment does not parse |
| XRCE | `xrce_topic_callback` stamps the ring entry; the NEW `take_with_info` slot reports it (and nothing else: no GUID or source time reaches the client) | `xrce_request_callback` stamps the entry; `take_request_with_info` reports it |
| Cyclone | unchanged, 0 | unchanged, 0 |

**Cyclone stays 0, deliberately.** Cyclone 0.10.5's `dds_sample_info_t` has no
reception time, and we do not keep a receive callback of our own. A stamp
taken in `take` would be the take time, a different fact. Humble upstream
writes 0 for the same reason. Jazzy upstream stamps the take time; we do not
follow it here.

**Client responses: out of scope, filed.** A reply's `rmw_service_info_t` has
no slot: `take_response` returns the sequence number only. Filed as issue
2136.

### Tests

Unit:
- `nros-rmw-xrce/tests/received_timestamp.c` (`just check rmw-xrce`, 13/13).
  It injects an arrival through the topic and request callbacks, waits 50 ms,
  then takes. The stamp must fall inside the injection window, not at the take.
  Negative control: with the callback stamps removed, both window checks fail.

Live, through a real router (`cargo nextest -p nros-rmw-zenoh --features
platform-posix --test zenoh_integration`):
- `a_taken_sample_reports_when_it_was_received` (new) and
  `a_service_request_reports_its_clients_identity` (its old `None` assertion
  flipped). The stamp must fall after the publish/send and before the take,
  and not before the source stamp. Negative control: with the stamps forced
  to 0, both fail.

Live end to end:
- `zero_copy::test_zero_copy_message_info`. A nano talker publishes through
  zenohd to `message-info-observer`, which now prints `rx=`. Every message
  has `rx > 0`, `rx >= ts`, and `rx - ts < 10 s`. Measured: `rx - ts` was
  about 0.7 ms.
- `xrce_ros2_interop::test_xrce_service_reports_the_stock_clients_request_id`
  (stock `ros2 service call` client through the Agent).
  `service-info-server` prints `received_ts=`, and every request's stamp is
  inside the test's wall-clock window and increasing. Negative control: with
  the request stamp removed, it fails `[None, None, None]`.

Docs: `book/src/design/rmw-vs-upstream.md` (the message-info section, the
service-request table and the XRCE slot ruling), `rmw-api-map.toml` plus the
regenerated `rmw-api-comparison.md`, `nros-core` `service_info.rs`, and two
ledger entries.
