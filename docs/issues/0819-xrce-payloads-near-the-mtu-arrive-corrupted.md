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

## The corruption is TAIL ZEROING, and it tracks the MTU (measured 2026-08-27)

Instrumenting `validate_payload` to report the first mismatching byte gives the
same answer on every sample, deterministically:

```
[probe0819] seq=0 first-bad payload-index=4068 (abs offset=4080) expected=0xe4 got=0x00
[probe0819] seq=1 first-bad payload-index=4068 (abs offset=4080) expected=0xe4 got=0x00
... 10/10 identical
```

So it is **not truncation** — `len` is the full 4096, `seq` and the `size_marker`
field survive, the CDR header survives, and the payload matches the expected
pattern up to offset 4080. The **last 16 bytes are zero-filled**, and
4080 = 4096 − 16 = MTU − 16.

**The boundary follows the MTU.** Rebuilt with
`NROS_XRCE_CUSTOM_TRANSPORT_MTU=8192` (`NROS_XRCE_BUFFER_SIZE=16384`), the same
4096-byte payload that was corrupt at the default MTU is now **10/10 valid**.
That rules out the receive ring, the agent's type size, and the payload size as
such: the only thing that moved was the datagram budget.

## Two failure modes near the MTU, and their windows do not agree

| MTU | payload | result |
| --- | --- | --- |
| 4096 (default) | 3584 | valid |
| 4096 | **4096** | delivered, **last 16 bytes zeroed** |
| 8192 | 4096 | valid |
| 8192 | 8176 | valid |
| 8192 | 8180 | valid |
| 8192 | 8184 | **no delivery at all** |
| 8192 | 8188 / 8192 | no delivery |

At MTU 8192 the transition is silence, not corruption — nothing arrives from
~MTU−12 upward. At MTU 4096 a payload of exactly the MTU DID arrive, zero-tailed.
Both cannot be the same rule, and the discrepancy is NOT explained. Candidates
not yet separated: the two builds also differed in `NROS_XRCE_BUFFER_SIZE`
(8192 vs 16384), and `UXR_CONFIG_UDP_TRANSPORT_MTU` may not move with
`NROS_XRCE_CUSTOM_TRANSPORT_MTU` on the native UDP transport even though raising
the latter demonstrably changed behaviour.

## Where to look next

`xrce_publisher_publish_raw` tries the non-fragmented fast path and returns
`NROS_RMW_RET_MESSAGE_TOO_LARGE` only when `uxr_buffer_topic` REFUSES:

```c
uint16_t req = uxr_buffer_topic(&st->session, st->output_reliable,
                                ps->datawriter_oid, body, body_len);
if (req != UXR_INVALID_REQUEST_ID) { ...; return NROS_RMW_RET_OK; }
/* TODO 115.K.2.x: fragmented fallback via
 * `uxr_prepare_output_stream_fragmented` ... skipped here until
 * a smoke test demonstrates the need. */
return NROS_RMW_RET_MESSAGE_TOO_LARGE;
```

The zero-tailed case is `uxr_buffer_topic` ACCEPTING a payload that does not fit
one datagram and emitting a short/padded submessage, so the guard never fires.
This measurement is the smoke test that TODO was waiting for: a payload larger
than one stream slot needs either the fragmented path or a hard refusal, and it
currently gets neither.

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

## Read of the client, 2026-08-27 — "add the fragmented path" is the WRONG fix

The "Where to look next" section points at `xrce_publisher_publish_raw`'s TODO:

```c
/* TODO 115.K.2.x: fragmented fallback via
 * `uxr_prepare_output_stream_fragmented` ... skipped here until
 * a smoke test demonstrates the need. */
```

and reads the measurement as that smoke test. Reading the vendored client says
otherwise, and this matters because it would be a week spent adding a path that
is already there.

### 1. Fragmentation is ALREADY compiled in, and unconditional

`uxr_prepare_reliable_buffer_to_write`
(`xrce-sys/micro-xrce-dds-client/src/c/core/session/stream/output_reliable_stream.c:100`)
has three branches: fits the current buffer, fits a new empty buffer, or
**`else if (length <= available_block_size * remaining_blocks)`** — a real
fragmenting path that emits `SUBMESSAGE_ID_FRAGMENT` across slots. It is not
behind a profile macro; the generated `uxr/client/config.h` for this build
defines no fragmentation gate to turn off.

So a payload larger than one slot does NOT fall off a cliff for want of a
fragmenting path. Something inside the existing one is wrong, or the path is not
being reached. That is a different investigation from the one the issue proposes.

### 2. Our slot size is EXACTLY the MTU, so any payload >= MTU must fragment

`XRCE_STREAM_BUFFER_SIZE = UXR_CONFIG_UDP_TRANSPORT_MTU * XRCE_STREAM_HISTORY`
(`internal.h:97`) and the stream is created with that buffer and
`XRCE_STREAM_HISTORY`, so one reliable slot is one MTU. That is consistent with
the measured boundary tracking the MTU rather than `NROS_XRCE_BUFFER_SIZE`, and
it means the 4096-byte payload at MTU 4096 is the FIRST size that needs
fragmentation at all. The cliff is the fragmentation threshold, not a buffer
limit.

### 3. `uxr_buffer_topic` cannot report a serialization overflow, by construction

```c
rv = uxr_init_base_object_request(&session->info, datawriter_id, &payload.base);
uxr_serialize_WRITE_DATA_Payload_Data(&ub, &payload);
UXR_PREPARE_SHARED_MEMORY(session, datawriter_id, &ub, (uint16_t) len, rv);
ucdr_serialize_array_uint8_t(&ub, buffer, len);
...
return rv;
```

`rv` is assigned BEFORE the payload is serialized and returned unconditionally.
If `ucdr_serialize_array_uint8_t` overflows, `ub.error` is set and nobody looks:
the function still returns a valid request id.

So `xrce_publisher_publish_raw`'s guard —

```c
if (req != UXR_INVALID_REQUEST_ID) { ...; return NROS_RMW_RET_OK; }
```

— is structurally incapable of catching a short serialization. That is true
whatever the root cause of the corruption is, and it is why the talker reported
success. **This is worth fixing on its own**: a publish path whose only error
signal cannot represent the failure it is most likely to hit.

### What this changes about the fix

* Adding `uxr_prepare_output_stream_fragmented` is probably NOT the fix; the
  reliable stream already fragments. Confirm the fragmenting branch is entered
  for the failing size before writing anything.
* The caller-side guard should be fixed regardless, but a size PRE-check must be
  computed against the fragmentation capacity
  (`available_block_size * remaining_blocks`), not against the MTU — checking
  the MTU would refuse payloads the client can legitimately fragment, which is a
  regression dressed as a fix.
* The send/receive question stays open and is now the FIRST thing to settle:
  the input reliable stream reassembles fragments, and a defect there produces
  exactly this signature (correct length, correct leading bytes, zeroed tail)
  without any send-side fault.

### Why this stops here

Reproducing needs a live Agent. `third-party/xrce/agent` is an UNINITIALISED
submodule on this host and building it pulls Fast-DDS/Fast-CDR, so no
measurement was taken and none of the above is confirmed against a running
system — it is a source read, and this issue has already recorded one
mis-attribution (phase-384 chased the `XRCE_BUFFER_SIZE` cliff, which refuses
loudly, and never reached this one). Whoever picks it up should get the agent
running first and check point 1 before touching the publisher.
