---
id: 2978
title: "An XRCE sample of 64 KiB or more publishes without error and is lost, or delivered cut to its size modulo 65536"
status: open
type: bug
severity: high
area: [rmw-xrce]
related: [phase-574]
found: 2026-11-26
---

## What happened

Phase 574 W2 ran the `stress-xrce` bench
(`packages/testing/nros-bench/stress-xrce`) on the native POSIX host, through the SDK store's Micro XRCE-DDS Agent 2.4.3.
The image was sized so the reliable stream and the subscriber's ring entry
both hold the sample: `transport_mtu` 4096, `stream_history` 512,
`subscriber_buffer_bytes` 1 MiB + 1024. Five samples per size, every byte
checked. The sizes from 64,000 bytes up were run two or three times each with
the same result, except that one of three 64,000-byte runs received nothing:

| data bytes | publish | receive |
| --- | --- | --- |
| 32,768 to 65,400 | 5 sent | 5 valid |
| 65,535 | 5 sent | none, and the listener hangs at `close` |
| 65,536 | 5 sent | none |
| 70,000 | 5 sent | 5 received, **all 4,464 bytes long**, reported as success |

4,464 is 70,000 mod 65,536. The subscriber took a sample of the wrong size
with no error, so the application's own decoder is the only thing that can
notice. Every publish returned `Ok`.

## Why

A sample travels as one XRCE DATA submessage, and a submessage's length is a
16-bit field on the wire (the client's `uxrOnTopicFunc` takes
`uint16_t length`; `read_submessage_format` in the client's `session.c` reads
it as `uint16_t`). The reliable stream fragments and reassembles the
submessage, but the length inside it still wraps. So no MTU, stream history
or buffer size carries 64 KiB over XRCE; the limit is the protocol's.

`nros-rmw-xrce` does not know this limit. The publish path accepts any size
the stream window holds, and the receive path (`xrce_topic_callback`,
`xrce_stage_inbound`) trusts the 16-bit length it is given.

## What a fix needs

- Publish: refuse a sample whose DATA submessage would exceed the 16-bit
  length, with `MessageTooLarge`, before anything is written.
- Build: refuse a `subscriber_buffer_bytes` (or service buffer) above the
  same limit, since no sample can fill it.
- Receive: where the reassembled buffer holds more bytes than the 16-bit
  length says, report `MessageTooLarge` instead of staging a cut sample. A
  sample relayed by the Agent from a DDS publisher reaches this path with no
  nano-ros publisher to refuse it.
- A regression cell in the large-message test: a 70,000-byte sample is
  refused at publish, and no cut sample is ever taken.

The 65,535-byte case (nothing received and a hang at close) is in the same
range and is checked by the same cell.

## Progress (2026-11-26)

**Fixed: a nano-ros sender refuses.** Every WRITE_DATA the backend sends now
goes through one of four checked writers in `nros-rmw-xrce/src/session.c`
(`xrce_write_topic`, `xrce_write_request`, `xrce_write_reply`,
`xrce_prepare_topic_stream`). Each returns `MESSAGE_TOO_LARGE` for a body the
16-bit length cannot state, before anything is written. That covers publish
(raw and streamed), service request and reply, the action messages (which
travel as those), and the graph sample. A test in `nros-rmw-xrce-cffi`
(`write_data_writers.rs`) refuses a direct call to the client's writer
anywhere else, so a new send path cannot skip the check. `nros-node` used to
flatten every publish error to `PublishFailed`; it now passes the backend's
own error up, so the Rust caller sees `MessageTooLarge`. The C surface maps
it to `RCLC_RET_FULL` and C++ to `NROS_CPP_RET_FULL`, as for any
`MessageTooLarge`.

Measured with the large-message probe on an image whose stream window
(8 KiB MTU x 16) is wider than 64 KiB, so only the length check can refuse:
64 KiB and 70,000-byte publishes return `MessageTooLarge`, three of three,
and 16 KiB still delivers intact. The cells are in `tests/large_msg.rs`
(`xrce_sample_over_the_write_data_length_is_refused_at_publish`,
`large_message_is_delivered_intact_on_the_host::xrce_16k`).

**Still open: a sample the Agent relays from a DDS publisher.** A ROS 2
node on DDS can publish a sample over 64 KiB; the Agent writes it with a
wrapped length and the client hands nano-ros the cut sample. The receive
callback cannot tell a wrapped length from a short sample: its buffer runs
past the submessage (several submessages share one message) and a
fragmented sample is read through a callback. This needs the Agent's side
of the fix below, or a check against the type's own serialized size.

**Real messages that cross the limit.** A `vision_msgs/Detection2DArray`
costs 440 B per detection (25 B base), so more than about 148 detections
exceeds 65,400 bytes; `mgonzs13/yolo_ros`'s `max_det` default is 300
(132,025 B). A `yolo_msgs/DetectionArray` (about 192 B each) crosses only
above about 340 detections. Measured with rclpy `serialize_message` on
Humble; `yolo_msgs` computed from its 4.7.1 `.msg` files. See phase 574 W1.

**Carrying samples past 64 KiB (not done).** The upstream client has
`UCLIENT_TWEAK_XRCE_WRITE_LIMIT`: on its fragmented path
(`uxr_prepare_output_stream_fragmented`) a WRITE_DATA longer than 65,535
bytes is sent with length 0, which the Agent reads as "to the end of the
reassembled message". The backend uses neither that path nor the flag, and
whether the 2.4.3 Agent honours length 0 in both directions is unmeasured.
That is the route to XRCE samples above 64 KiB; the refusal comes first.

The listener in the measurements above also hung at `close` after a run in
which nothing arrived (65,400 and 65,535 bytes against a 17 KiB receive
buffer); not investigated here.

## Reproduce

```
cd packages/testing/nros-bench/stress-xrce
nros dev codegen rust
NROS_XRCE_TRANSPORT_MTU=4096 NROS_XRCE_CUSTOM_TRANSPORT_MTU=4096 \
NROS_XRCE_STREAM_HISTORY=512 NROS_XRCE_SUBSCRIBER_BUFFER_BYTES=1049600 \
NROS_XRCE_SUBSCRIBER_RING_DEPTH=2 NROS_PARAMS_SERVICE_INBOX_BYTES=8192 \
  cargo build --release
MicroXRCEAgent udp4 -p 2019 &
MODE=listener PAYLOAD_SIZE=70000 EXPECTED_COUNT=5 ./target/release/xrce-stress-test &
MODE=talker PAYLOAD_SIZE=70000 PUBLISH_COUNT=5 ./target/release/xrce-stress-test
# listener: Received: seq=0 size=4464 valid=false (x5)
```
