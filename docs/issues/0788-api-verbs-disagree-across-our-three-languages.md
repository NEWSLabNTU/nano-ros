---
id: 788
title: "The same API verb is spelled differently in our C, C++ and Rust — and in
  two cases one language ships both spellings"
status: open
type: bug
area: api
related: [rfc-0036, phase-379, issue-0783, issue-0784]
---

## Problem

Phase 379 correlates the nano-ros user API against rclc/rclcpp/rclrs. Its most
valuable findings are not mismatches with ROS 2 — they are places where **our own
three languages disagree with each other**, which no ROS 2 comparison would ever
surface and which a user hits the moment they read two of our examples.

Six, found across four stages:

| concept | C | C++ | Rust | ROS 2 |
| --- | --- | --- | --- | --- |
| reply to a service request | `nros_service_send_response_raw` (phase-379 W5, 2026-08-27) | `Service::send_response` | `ServiceTrait::send_response` | `send_response` |
| is the server up? | `nros_client_service_is_ready` **and** `nros_client_server_available` | `Client::server_available` | `ClientTrait::is_server_ready` **and** `ClientTrait::server_available` | `service_is_ready` |
| non-blocking receive | `nros_service_take_request` but `nros_client_try_recv_response` | `try_recv*` throughout | `try_recv*` throughout | `take` |
| cancelled timer | (no predicate at all) | `Timer::is_cancelled` → `is_canceled` (phase-379 W5, 2026-08-26) | `Timer::is_canceled` | `is_canceled` |
| create a subscription | `nros_subscription_init` | `Node::create_subscription` | `Node::create_subscriber` | `create_subscription` |
| serialized bytes | `publish_raw` | `publish_raw` | `publish_raw` | `serialized` |

Two rows are worse than a mismatch: **one library ships both spellings.**
`nros_service_send_response` sits beside `nros_service_send_reply_raw` in the same
header, and `ClientTrait` declares `is_server_ready` and `server_available` in the
same trait.

**The service-reply row LANDED 2026-08-27 (phase-379 W5).** All three languages
say `send_response`: C exports one symbol, `nros_service_send_response_raw`
(`_raw` being the family convention for the byte-buffer entry points, not a
distinction from a typed twin), and both old C spellings plus
`Service::send_reply` survive only as deprecated forwarders. The C duplicate is
gone in the strong sense — `nros_service_send_response` was a permanent
`NROS_RET_NOT_INIT` stub, so its export was deleted rather than kept. The Rust
trait method was renamed with no forwarder: it is required, so the rename breaks
implementors, and a compile error is the right answer for them. Rows in
`docs/reference/api-parity-ledger/service.json`
(`cpp:Service::send_response` carries the history).

The `non-blocking receive` row is deliberately still open: `nros_service_take_request`
is the unimplemented twin of the working `nros_service_try_recv_request_raw`, and
settling it is `c:take_request`'s job, not the reply verb's.

## Why it matters

The drop-in claim is made per language — RFC-0036's premise is that a ROS 2
developer can read and write nano-ros. A developer who learns our C and then
reads our C++ has to learn it twice, and the two spellings are not distinguishable
by any rule they could infer. Worse, `send_response` versus `send_reply_raw`
inside one header reads as a deliberate distinction and is not one.

The last three rows are also ROS 2 mismatches, so they are covered by the
campaign's `rename` verdicts. The first three are not: fixing only the ROS 2
mismatch would leave `nros_service_send_response` and `Service::send_reply`
disagreeing with each other while both "matched" nothing.

## Evidence

`scripts/api-parity.py --topic service`, `--topic timer`, `--topic pubsub`, and
the `rename` rows in `docs/reference/api-parity-ledger/{service,timer,pubsub,node}.json`.
Each carries the file and line.

## Direction

Not decided here — phase 379 W5 owns the rename sweep and this issue exists so
the sweep is scoped correctly:

* **Pick each verb once, for every entity, in every language.** A sweep that
  aligns C++ to ROS 2 and leaves C disagreeing is the same defect with better
  paperwork.
* **Delete the duplicate spellings** rather than aliasing them. Both surviving
  pairs are recent enough that neither has an external user.
* The `take` versus `try_recv` decision is the widest — it touches every entity
  in all three languages — and should be settled before the narrower ones so the
  rest follow from it.

Anything the sweep changes should be re-checked with `scripts/api-parity.py`,
not by reading, which is the whole reason the correlator exists.
