---
id: 1797
title: "Bare-metal C/C++ action pairs do not reliably complete a round trip: the C++ client never sees its goal accepted, the C one passes 1 run in 2"
status: resolved
resolved: 2026-10-11
type: bug
area: [rmw, testing]
severity: medium
found: 2026-10-11
related: [1512, 0387, phase-477]
---

## Measured

The first time the bare-metal C/C++ leaves ran against a router (issue 1512's
runtime lane, `tests/emulator.rs::test_qemu_baremetal_capi_e2e`, each pair on
its own `alloc::port`):

| case | result |
| --- | --- |
| C pubsub, C service, C++ pubsub, C++ service | pass, first try |
| C action | FAIL then PASS (`retries = 2`) |
| C++ action | FAIL 3/3 |

Two different C++ symptoms:

- In the test run: the client logs `action server did not appear within 10s`
  — `wait_for_action_server` timed out although the server had been up for 5 s.
- By hand (same images, same port, router first, server, then client 4 s
  later): the server logs `Received goal request with order 10`, four
  `Publish feedback` and `Goal succeeded` at 4.11 s; the client logs `Sending
  goal` at 0.31 s and nothing after — `send_goal` never returns, so neither
  acceptance nor result is ever printed.

So the goal REACHES the server and the server answers; what the client does
not get is the reply (or, in the first symptom, the discovery that precedes
it). Pubsub and services on the same images and the same router are reliable,
so the transport is up; the action client's multi-endpoint exchange is what
fails.

## Context

- The server completes the goal inside its goal callback (feedback, then
  `complete_goal`, then returns ACCEPT), so the result and status are sent
  BEFORE the accept response — an ordering a client that waits for acceptance
  first must tolerate.
- The client's receive rings are 4 deep where it asked for 10 (`qos: client
  '/fibonacci/_action/send_goal' asked for KEEP_LAST(10); this image's receive
  ring holds 4`), on all three action services.
- The Rust bare-metal set has no action cell to compare against (its action
  cells are `CarveOut`, "pubsub-only by design").

## What is NOT the cause

The quiet-spin defect fixed with issue 1512 (a smoltcp quiet tick reported as
ConnectionFailed, which stopped C/C++ images after 16 quiet spins): with it
fixed the action images run their full 20 s, and the server answers.

## Acceptance

Both `(QemuBaremetal, C|Cpp, Zenoh, Action)` cells move from `BuildOnly` to
`Runtime`, and the `c_action` / `cpp_action` cases pass on the first try in
five consecutive runs.

## Resolution (2026-10-11)

**Cause: the bare-metal read desynchronised the TCP stream on a burst.**
The smoltcp arm of `zpico_spin_once` read with `single_read = true`, i.e.
zenoh-pico's `_z_unicast_recv_t_msg`. That path reads a batch's 2-byte length
prefix and then decodes ONE transport message from the WHOLE rx zbuf — not
from a view bounded to the batch. A FRAME's payload is "everything left in the
buffer it is given" (`_z_frame_decode`: `msg->_payload = zbf`), so when one TCP
read delivers two batches, the frame decode consumes the next batch's length
prefix as network messages and the stream never recovers: the reply inside is
never decoded, and every later read waits on a garbage size, silently.

Two batches in one read is what a router sends when a server bursts — here the
action server publishing feedback, status and result inside its goal callback
and then answering the goal. Which reply was swallowed depended on how the
router batched the burst: the C++ client lost the goal reply, the C client the
result reply. Pubsub and services never burst, which is why they were reliable.

**Fix (zpico.c, our code; the zenoh-pico fork is untouched):** the smoltcp arm
reads with `single_read = false`. That path decodes each batch over
`_z_zbuf_view(…, to_read)`, drains every complete batch already buffered, and
since nano-ros #567 compacts rather than resets, so a partial trailing batch
survives to the next call — the property `single_read = true` had been chosen
for. Its quiet-tick code `_Z_NO_DATA_PROCESSED` maps to 0 like the single-read
`_Z_ERR_TRANSPORT_NOT_ENOUGH_BYTES` (issue 1512 / 0387).

Sweep: `git grep -n 'single_read *= *true' -- packages ':!packages/rmw/zenoh/zpico-sys/zenoh-pico'`
— that was the only stream-link caller.

## Measured

| | before | after |
| --- | --- | --- |
| `c_action` + `cpp_action`, 8 consecutive runs, `--retries 0` | 8 of 16 passed | **16 of 16** |
| whole `tests/emulator.rs` | — | 19/20; the one red is `test_qemu_wcet_benchmark`'s known UNMET PRECONDITION (no DWT under QEMU, issue 0403). Rust BSP/RTIC/XRCE cells on the same smoltcp path pass |

Both `(QemuBaremetal, C|Cpp, Zenoh, Action)` cells are `Runtime`, and
`test_qemu_baremetal_capi_e2e` carries `c_action` / `cpp_action`. A failing
case now prints BOTH images' transcripts, since "the server answered" vs "the
goal never arrived" is the first question.

Not verified: the ESP32-C3 QEMU bare-metal board, the other `ZPICO_SMOLTCP`
consumer (not provisioned here).
