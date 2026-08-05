---
id: 418
title: "Raw action feedback/result payloads carry an extra CDR header, so they are wire-incompatible with ROS 2 and with nano-ros's own typed path"
status: open
type: bug
area: rmw
related: [phase-338, rfc-0069, issue-0035]
---

## Symptom

A Node-class (raw-registered) action server and a typed action client complete
the goal handshake and then fail on both remaining channels:

```
[INFO ] Goal accepted by server, waiting for result
[ERROR] Error receiving feedback: Transport(DeserializationError)
[ERROR] Failed to get result: ServiceRequestFailed
```

The server itself is healthy — it logs `Received goal request with order 1`,
`Executing goal`, `Goal succeeded`. Only the payloads fail to decode.

Found during phase-338 W3, after fixing a *separate* defect on the same path
(the bare-action-type keyexpr bug, `7a7068af9`). That fix made the action
**discoverable**; this one is what stops it being **usable**.

## Cause: one CDR header too many, on purpose

`nros/src/node.rs` serializes raw action feedback and results **with a CDR
encapsulation header inside the envelope**. The wire therefore carries:

```
feedback:  [outer CDR header][goal_id (4+16 B)][INNER CDR header][body]
result:    [outer envelope                    ][INNER CDR header][body]
```

ROS 2's `<Action>_FeedbackMessage` and `<Action>_GetResult_Response` carry a
**single** encapsulation header.

Both producer sites document the inner header as deliberate, and the result one
documents the failure mode of removing it naively:

> "Without the header the reader eats the first data word (e.g. a sequence
> length) → empty/garbage payload (issue #35 M-F.23 follow-up: action result
> `sequence` deserialized to len 0)."

The raw **consumer** is symmetric with the raw producer, which is why nobody
noticed: `action_core.rs::try_recv_feedback_raw` reads the outer header via
`CdrReader::new_with_header`, then the `goal_id`, and the caller reads the body
via `CallbackCtx::message` — `new_with_header` again, consuming the inner one.

So **raw ↔ raw is self-consistent** and **raw ↔ {ROS 2, nano-ros typed} is not**.
Exactly the shape of the keyexpr bug fixed in `7a7068af9`, one layer down.

## Why it stayed hidden

Every runtime cell that exercises actions pairs a raw server with a raw client
(embedded entries are Node-class on both ends), so the double header cancels.
The mismatch only appears when one end uses the typed path — which is precisely
what a real `rcl_action` peer does, and what `examples/native/rust/action-client`
does today.

## Scope of a fix

Producer (2 sites): `nros/src/node.rs` — `publish_feedback` and `complete_goal`.

Consumer (must change in the same commit, or raw↔raw breaks):
`nros-node/src/executor/action_core.rs::try_recv_feedback_raw` and the
`CallbackCtx::message` read path for feedback/result.

Also audit, do not assume: any C / C++ / ffi action client that reads feedback
or result payloads.

**Correcting an earlier overclaim of mine:** I first recorded that the generated
C++ message exports (`rosidl-codegen/packs/cpp/message_exports.rs.jinja`) encode
this convention. They do not — their `new_with_header` is ordinary per-message
CDR, which is correct ROS 2 behaviour for a topic payload. The blast radius is
the action envelope specifically, not codegen.

## Why this needs a decision, not just a patch

The fix direction is obvious (match ROS 2) but it is a **wire format change**:
it makes existing nano-ros↔nano-ros action pairs incompatible across the
version boundary, and it invalidates the convention `try_recv_feedback_raw` and
the `CallbackCtx` read path encode. That is RFC-shaped — see
[RFC-0069](../design/0069-action-payload-envelope.md).

## Consequence, stated plainly

Raw-registered action servers and clients have **never** been wire-compatible
with ROS 2 on feedback or result payloads. Until this lands,
`examples/native/rust/{action-server,action-client,service-client}` cannot
migrate to Node-class (phase-338 W3), because their counterparts use the typed
path.

## Follow-through (2026-08-05)

`0403a8b53` fixed the producer and left three acceptance items open. Two are now
done, and closing the third turned up a live bug the first pass had recorded as
harmless.

**The retired `payload_has_cdr_encap` sniff was a second instance of #35.** The
consumer chose between "read directly" and "splice the enclosing encap" by
SNIFFING whether the payload began with an encapsulation header. That is a value
test: a leading `int32` of 256 is `00 01 00 00`, byte for byte the LE header.
While the producer wrote an inner header only Cyclone took the other branch, so
it never mattered — but 0418 made every payload header-less, so the sniff decided
every decode, and a body whose first word was 256 had that word eaten as framing.
Issue #35's corruption, reintroduced by the guard against it, in the commit that
fixed its cousin.

Fixed by deleting the branch: post-0418 the payload is field bytes on every path,
so the consumer splices unconditionally. All three sites — `read_action_field`,
the feedback path, and a THIRD in the result path that the first sweep missed —
now behave the same.

Two executor tests had to change with it (`test_action_client_callbacks_fire_at_spin`,
`test_action_client_feedback_burst_buffered`): they built payloads with
`CdrWriter::new_with_header`, and kept passing after the producer was fixed
*because* the consumer still sniffed and found the header they wrote. They were
asserting the retired wire format against a new producer.

**Acceptance now:**

* Regression test — DONE. `action_envelope_tests` in `arena.rs`, three cases,
  verified to fail 2/3 against the pre-fix consumer rather than assumed to cover.
* A real ROS 2 client drives a raw server — DONE. The "this host lacks ROS" note
  was wrong; humble + `rmw_zenoh_cpp` are installed. `ros2 action send_goal
  --feedback` against the Node-class `action-server` returns feedback, result and
  `SUCCEEDED`.
* Every action Runtime cell on real targets — PARTLY. All 8 zephyr `native_sim`
  action cells green (3 RMWs × 3 languages, all raw↔raw). The freertos / nuttx /
  threadx QEMU lanes still need the full embedded fixture build.
