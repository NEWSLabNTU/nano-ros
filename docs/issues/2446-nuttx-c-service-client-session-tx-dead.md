---
id: 2446
title: "NuttX C service client's session dies at its first declare under tier 2 (zenoh-pico -100/-103), so the cell gets no response"
status: open
type: bug
severity: medium
area: [nuttx, rmw-zenoh, testing, ci]
related: [0870, 1034, 2221, phase-466]
found: 2026-11-11
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**: the
one real failure of the first scheduled tier-2 run to reach its cells since at least
2026-11-06.

## Measured

Scheduled tier-2 run 42971493635 (2026-11-11, `main` `7d60c42daf`, runner
`nano-ros-runner-newslab-118-server`), job `tier 2 (1-wise matrix)`, step
`just ci matrix`: `Real failures: 1 / 1 total failures`.

`nros-tests::rtos_e2e test_rtos_service_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
failed at 181.03 s: `nuttx c service E2E failed — got 0 responses (expected >= 1)`.
The server's ready banner had been seen (`start_server_then_client` starts the client
only after it), and the router was `rmw_zenohd` on port 8310.

The client's stderr, from the run's `post-submit-junit` artifact:

```
zpico: z_liveliness_declare_token failed: -100 for '@ros2_lv/0/a424b7…/0/0/NN/%/%/add_client'
zpico: z_liveliness_declare_token failed: -103 for '@ros2_lv/0/a424b7…/0/0/NN/%/%/add_client'
[ERROR] nros: [    0.056000] liveliness: node `add_client` declared no token …
zpico: z_liveliness_declare_token failed: -103 for '@ros2_lv/…/SC/%/%/add_client/%add_two_ints/…'
[ERROR] nros: [    1.349000] zpico Generic -> ConnectionFailed (the two are indistinguishable downstream)
… repeated every ~0.79 s to 13.2 s …
[nuttx c] responses: 0
```

`-100` is `_Z_ERR_TRANSPORT_TX_FAILED` and `-103` is `_Z_ERR_TRANSPORT_NOT_AVAILABLE`
(`zenoh-pico/include/zenoh-pico/utils/result.h`). So the client opened a session, its
first transmission failed within 55 ms of boot, and the transport was gone from then
on; every service call after that is `ConnectionFailed`.

In the same run the NuttX C **action** pair passed in 4.19 s and the NuttX C **pubsub**
pair in 71.0 s, so two NuttX QEMU guests against one router is not broken in general.

## Same signature as issue 0870

[Issue 0870](archived/0870-nuttx-cpp-action-client-transport-tx-failed.md) (NuttX C++
action client, `-100` at declare, then `Generic -> ConnectionFailed`) was closed on
2026-09-04 green, **with the cause unattributed**. Its "what would settle it" item 1
was "run the cell inside a full `just ci matrix` rather than solo", because the
original red came from a parallel sweep. This is that run producing the signature,
on a different NuttX client cell (C service rather than C++ action). Filed separately
rather than reopened because it is a different cell; the class is the same.

## What the verdict could not say

The service cell printed only the client's output. Whether the **server** still held a
session when the client's died (one router-side event taking both, or the client
alone) was not captured. The action cell already prints `Server boot` and
`Server post-boot`; the service cell now does too (this issue's first commit), so the
next red answers that.

## What it is NOT

- Not a lane skip: the junit rewrite counts it as the run's one real failure.
- Not issue 2416 (XRCE) or 2420 (freertos-posix `no_session`): different RMW and
  platform, though 2420 is the same "session refused or dropped at start" family.

## What would close it

The next red prints both sides, which attributes the drop to the client, the server
or the router. A fix then follows from that. Per the no-retries ruling (issue 2221),
the cell is not re-rolled. Acceptance: the cell passes in tier-2 runs on main, and the
cause is named.
