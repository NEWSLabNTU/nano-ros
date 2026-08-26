---
id: 819
title: "XRCE payloads at/above the transport MTU are DELIVERED CORRUPTED rather
  than refused"
status: open
type: bug
area: rmw
related: [phase-384, issue-0741]
---

## Symptom

With `NROS_XRCE_BUFFER_SIZE` raised high enough that the receive ring is not the
constraint, a payload at or above the UDP transport MTU (4096) is still
received — but the bytes are wrong. The subscriber reports success and the
payload validator rejects every sample:

```
payload=3584  RECV_DONE: received=10 valid=10 invalid=0
payload=4096  RECV_DONE: received=10 valid=0  invalid=10
```

There is no error, no counter, and no refusal. `try_recv_raw` returns
`Ok(Some(len))`; only an application that validates its own payload can tell.

## Measured boundary

`packages/testing/nros-bench/stress-xrce`, built with
`NROS_XRCE_BUFFER_SIZE=8192`, nano -> Agent -> nano, varying `PAYLOAD_SIZE`:

| payload bytes | received | valid |
| --- | --- | --- |
| 2048 | 10 | 10 |
| 2560 | 10 | 10 |
| 3072 | 10 | 10 |
| 3584 | 10 | 10 |
| **4096** | **10** | **0** |

So the cliff sits between 3584 and 4096, which is `UXR_CONFIG_UDP_TRANSPORT_MTU`
(4096) less the XRCE submessage overhead. That is consistent with the payload
being truncated to what fits one datagram while `len` still reports the full
size, but the truncation point has NOT been confirmed in the client — see below.

## Why this is worth its own issue

This is the failure mode that phase-384 was originally written about and did not
find. The 1024-byte cliff that phase investigated is `XRCE_BUFFER_SIZE`
(`packages/rmw/xrce/nros-rmw-xrce/src/internal.h:69`), it is raisable via
`NROS_XRCE_BUFFER_SIZE`, and it REFUSES loudly
(`NROS_RMW_RET_MESSAGE_TOO_LARGE`). Raising past it exposes this one, which does
not refuse at all.

A refusal an integrator can see is a configuration problem. Silent corruption
that only a payload validator catches is a data-integrity problem, and every
probe an integrator would reach for — sample counts, liveliness, drop counters —
reports healthy.

## Not yet established

* **Where the truncation happens.** MTU arithmetic fits the numbers, but the
  client's fragmentation path has not been read. `xrce_publisher_publish_raw`
  returns `NROS_RMW_RET_MESSAGE_TOO_LARGE` when `uxr_buffer_topic` refuses, and
  the talker did NOT report that here, so something accepted the publish.
* **Whether the send or the receive side corrupts.** Both halves were the same
  binary in this measurement.
* **Whether a larger MTU moves the cliff.** If it does, that both confirms the
  cause and gives integrators a knob; if it does not, the truncation is
  elsewhere.

## Reproduce

```
cd packages/testing/nros-bench/stress-xrce
NROS_XRCE_BUFFER_SIZE=8192 cargo build --release
# start an agent on :8700, then run the binary twice (MODE=listener / MODE=talker)
# with PAYLOAD_SIZE=4096 against it; the listener prints valid=false for each.
```

Note the talker/listener pair must share `STRESS_TOPIC` and the agent address.
