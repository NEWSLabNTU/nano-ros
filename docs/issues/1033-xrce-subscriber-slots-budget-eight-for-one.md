---
id: 1033
title: "The XRCE session budgets eight 32-deep subscriber rings, so a one-subscriber image pays 266,368 bytes for seven it will never use"
status: open
area: rmw, memory
severity: high
found: 2026-09-04
related: [0968, 0827, 0900, 0965, 0460, phase-392, phase-412]
---

# 62% of the XRCE session struct is slots the image does not have

## Measured

`sizeof(xrce_session_state_t)` on the zephyr cpp-listener-xrce image, read from
its own DWARF, after the MTU fix in issue 0968 took it from 427,968:

| member | bytes | share |
| --- | ---: | ---: |
| `subscriber_slots[8]` @ 33,296 | **266,368** | 86% |
| `service_server_slots[4]` @ 4,384 | 17,536 | 6% |
| `output` + `input_reliable_buf` @ 8,192 | 16,384 | 5% |
| `service_client_slots[4]` @ 1,040 | 4,160 | 1% |
| rest | 5,248 | 2% |
| **total** | **309,696** | |

Against a `CONFIG_NROS_ZEPHYR_HEAP_SIZE` of 65,536. The image is a LISTENER: it
has **one** subscriber, no service server and no service client.

A slot is `XRCE_SUBSCRIBER_RING_DEPTH` (32) x `XRCE_BUFFER_SIZE` (1024) plus 528
bytes of bookkeeping. So the image reserves 32 buffered samples for each of
eight subscriptions, having declared one.

## This is issue 0827's class, one backend over

0827 is "unused RMW pools dominate static RAM" for zenoh — `SERVICE_BUFFERS` and
`LARGE_PAYLOADS` sized for entities a talker does not have. This is the same
shape on XRCE, and it is worse in one respect: zenoh's pools are `.bss` that
`just mem-report` can see, while this is a single heap allocation that only
shows up as a boot-time failure.

## The knobs exist and are honoured — nothing sets them

`NROS_XRCE_MAX_SUBSCRIBERS`, `NROS_XRCE_MAX_SERVICE_SERVERS`,
`NROS_XRCE_MAX_SERVICE_CLIENTS` and `NROS_XRCE_SUBSCRIBER_RING_DEPTH` all reach
`build.rs` and all have a minimum of 1. `internal.h`'s own header comment says
what they buy:

> The default `xrce_session_state_t` is ~390 KB; a pub-only bare-metal node can
> drop it well below 32 KB by setting subscribers/services to 0 and smaller
> per-entity buffers.

So this is not a defect in the mechanism. It is that every Zephyr XRCE example
ships the 8/4/4 defaults, and those defaults are sized for a workload none of
them run.

`CONFIG_NROS_XRCE_SUBSCRIBER_RING_DEPTH` is the exception worth checking: the
other three have Kconfig entries and this one appears not to, so on a Zephyr
image the ring depth may not be settable at all.

## Two ways to fix it, and the choice is the campaign's

1. **State the caps per example.** Correct immediately, and it is the twenty
   hand-picked numbers issue 0827 declined — planted in the demos people copy.
2. **Derive them**, from the entity inventory issue 0965 built. A C++ component
   image CAN declare `ENTITIES`, and `examples/workspaces/cpp` now does; these
   zephyr example leaves do not. The count of `sub:` entries IS
   `XRCE_MAX_SUBSCRIBERS`, exactly as `NROS_DERIVED_MAX_SUBSCRIBERS` already
   works for the zenoh pools (phase-412 W1).

(2) is the campaign's answer and reuses machinery that exists. What it needs is
the zephyr example leaves to declare, plus the XRCE caps joining the derivable
ladder beside the zenoh ones.

## Do not just raise the heap

Raising `NROS_ZEPHYR_HEAP_SIZE` past 310 KB would make the three cells in
[issue 0968](0968-tier2-runtime-failures-unreproduced.md) pass while leaving an
image that reserves 86% of its session struct for entities it does not create.
That is the shape this campaign exists to remove, and the boot failure is
currently the only thing making it visible.
