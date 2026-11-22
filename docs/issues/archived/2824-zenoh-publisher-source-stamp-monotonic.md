---
id: 2824
title: "A zenoh publisher stamped its samples' source timestamp from the monotonic clock, so a subscriber saw it decades before the receive stamp"
status: resolved
type: bug
severity: medium
area: [rmw, zenoh, clock]
related: [issue-2135, issue-2760, issue-1942]
found: 2026-11-22
resolved: 2026-11-22
---

## What

Tier-2 run 44916540238 (main cbf8c22e6f), `zero_copy`
`test_zero_copy_message_info`: issue 2135's check failed with

    received timestamp rx=1795277691049029542 is unset, before the source
    stamp ts=3235132280000000, or implausibly late

`rx` is 2026 in epoch nanoseconds. `ts` is 3,235,132.28 s, about 37 days, in
whole milliseconds: the host's MONOTONIC uptime. Delivery was fine; the two
stamps were on different clocks.

## Why

The publisher's `current_timestamp()` (`shim/publisher.rs`) derived the
attachment's source stamp from the shim's `now_ms()`. Issue 2760 moved
`now_ms()` from the wall clock to `nros_platform_clock_ns`, the monotonic
clock, which is right for the durations it measures (deadline QoS, publish
and match timing). The source stamp read the same helper, so it moved with it.
Nothing measured the stamp against the receiver's clock, except this cell.

The service client already stamped from the wall clock (issue 1942), and the
receive side stamps `received_timestamp` from the wall clock (issue 2135).
`rmw_zenoh_cpp` stamps both from `system_clock::now()`.

## Fix

`source_timestamp_ns(fallback)` in `shim/mod.rs` is the one spelling for an
attachment's source stamp: `wall_clock_ns()`, or the sequence-derived
fallback on a platform with no wall clock. The publisher and the service
client both call it. The service's private copy now forwards to it.

Sweep of every other writer of a source stamp:
- The other backends stamp through their own middleware. Cyclone uses the DDS
  sample's source timestamp; XRCE states it reports none for requests.
- Within the zenoh shim, the publisher's three publish paths and the service
  client are the only attachment writers, and all now go through the one
  function.
- `check-wall-clock-readers` is unchanged and green: the function reads the
  wall clock through `wall_clock_ns`, already listed.

Negative control:
`shim::tests::source_timestamp_is_on_the_wall_clock_the_receiver_stamps_from`
asserts the stamp is within a day of the host's epoch time, and that the
monotonic clock is not (otherwise the test could not tell the two apart).
Pointing `source_timestamp_ns` at the monotonic clock fails it (measured:
`source stamp 8989557460000000 is not on the wall clock`).

nros-rmw-zenoh: 133/133 lib tests pass, and clippy `-D warnings` is clean. The
`zero_copy` cell itself needs a native fixture build. It is in the batch's
runtime run.
