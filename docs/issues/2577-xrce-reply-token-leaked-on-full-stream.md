---
id: 2577
title: "XRCE: a reply that fails on a full reliable stream leaks its reply token, and after 4 the service is dead for the process's life"
status: open
type: bug
area: [rmw, xrce]
severity: high
found: 2026-09-18
related: [rfc-0005, rfc-0008, 2576, 2578, 2579]
---

GitHub: NEWSLabNTU/nano-ros#1076 (reported by wicanr2)

**Standalone, one of three XRCE device-side issues from the same bring-up**
(with issue 2578, the unbounded session flush, and issue 2576, the Agent restart). No active phase owns the XRCE backend on the device.

Setup: bare-metal STM32F427 on Renode, XRCE over serial, Agent v2.4.3, ROS 2
Jazzy. Reported at `fb89182c03`; the code is unchanged on main 2026-11-15.

## What happens

When `uxr_buffer_reply` fails because the reliable output stream is full,
`xrce_service_send_response_locked`
(`packages/rmw/xrce/nros-rmw-xrce/src/service.c`, ~:594) returns without
releasing the reply token it took:

```c
uint16_t req = uxr_buffer_reply(&st->session, st->output_reliable, ss->replier_oid,
                                reply_sample_id, (uint8_t*)(uintptr_t)body, body_len);
if (req == UXR_INVALID_REQUEST_ID) {
    return NROS_RMW_RET_ERROR;     /* ss->slot->reply_tokens[seq].in_use stays true */
}
```

The executor calls `send_response` once per request and never retries, so the
token leaks. After `XRCE_MAX_PENDING_REPLIES` (4) leaks, `take_request`
returns `WOULD_BLOCK` forever. The success path releases the token even when
the flush does not drain, so the token belongs to this call; only the failure
path keeps it.

Hard to see from the node: `ctx.reply()` returning `Ok` means only that the
reply reached the executor's buffer; the send happens after the callback and
its failure never reaches node code. Once the tokens are gone the symptom is
"requests silently dropped", which reads as a transport problem.

## Evidence

One arm + duplicate + disarm every 3.5 s, 30 sets, counters on each branch of
`service.c`:

| build | `send_called` | `send_ok` | `send_buffer_fail` | `take_wouldblock` | `req_drop_full` | client saw |
|---|---|---|---|---|---|---|
| pinned | 33 | 29 | 4 | 9743 | 17 | 29 of 54, then every request times out |
| token released on failure | 83 | 75 | 8 | 0 | 0 | 75 of 83 (the 8 = `send_buffer_fail`) |

After the fix a full stream still drops an occasional reply, but the service
keeps working and an idempotent client retry succeeds. Raising
`NROS_XRCE_STREAM_HISTORY` 4 -> 16 makes the stream fill less often (CCM heap
12.6 -> 24.9 KB) but alone does not help: the two are independent.

## Suggested fix

Release the token in the failure branch:

```c
if (req == UXR_INVALID_REQUEST_ID) {
    ss->slot->reply_tokens[seq].in_use = false;
    return NROS_RMW_RET_ERROR;
}
```

The reporter offered this as a PR. Larger alternative: keep the token and have
the executor retry the send, which would not lose the reply.
