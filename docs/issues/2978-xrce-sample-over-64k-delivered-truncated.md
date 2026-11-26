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
