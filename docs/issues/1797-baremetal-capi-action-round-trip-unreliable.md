---
id: 1797
title: "Bare-metal C/C++ action pairs do not reliably complete a round trip: the C++ client never sees its goal accepted, the C one passes 1 run in 2"
status: open
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
