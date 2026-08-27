---
id: 838
title: "Two ways concurrent tests end up on one Cyclone domain: a BAKED domain
  shared by four tests, and a slot partition that wraps above 25 nextest threads"
status: resolved
type: bug
area: testing
related: [issue-0703, issue-0707, issue-0672, issue-0835, phase-383]
resolved_in: phase-383 W9.c
---

## Problem

Cyclone is brokerless RTPS and derives its ports from the domain id, so two live
participants on one domain either cross-talk or fail to bind. Two independent
defects put concurrent tests there.

### 1. A baked domain shared by four tests (the observed failure)

`nros_tests::alloc::domain_of` statically allocates **domain 107** to the
(threadx-linux, c, pubsub) coordinate, and `just threadx_linux build-fixtures`
bakes it into the image with `-DNROS_DOMAIN_ID`. The embedded side cannot be
told a different domain at run time, so every test that talks to one of those
images must use 107 as well. Four do:

```
test_threadx_linux_cyclonedds_talker_to_native_listener
test_threadx_linux_cyclonedds_cpp_talker_to_native_listener
test_threadx_linux_cyclonedds_service
test_threadx_linux_cyclonedds_action
```

nextest runs each test in its own process, in parallel, and nothing serialized
them — so those four ran as four concurrent participants on one domain. SPDP for
domain 107 is port `7400 + 250*107` = 34150:

```
ddsi_udp_create_conn: failed to bind to ANY:34150: address in use
nros_support_init(&app.support, locator, domain_id) -> -1
native-c-cyclonedds-listener did not print `Subscriber created for topic:` within 30s
```

Those exact four were the only `native_api` failures on this host: 32/36.

### 2. The slot partition wraps above 25 threads

`unique_ros_domain_id` gives each nextest global slot a disjoint block:
slot `s` owns `[s*4, s*4+3]` of `1..=101`, i.e. `TEST_DOMAIN_MAX /
DOMAINS_PER_SLOT` = **25 slots**. `domain_in_slot`'s own doc says what happens
past that, and what to do:

> Beyond 25 slots the blocks wrap and collisions resume. … the fix there would
> be to cap `test-threads`, not to widen a range whose upper bound is set by
> Linux's ephemeral port floor.

**The cap was never applied.** nextest defaults to the CPU count, so on a
32-core host slots 25..31 alias onto slots 0..6 — deterministically, not as a
race: slot 25 takes domains 1..4 alongside slot 0, which is the domain-1 hazard
issue 0672 recorded as "reachable, not yet observed".

```
slots 0..39: 59 colliding domains; first aliasing slot = 25
(1, [0, 25]) (2, [0, 25]) (3, [0, 25]) (4, [0, 26]) …
```

`a_slots_domains_never_land_on_a_live_neighbours` proves the grid is
collision-free *inside the bound*. Nothing tied that bound to the number of
slots nextest actually creates, so the property held and the system violated it.

## Fix

**(1)** `[test-groups.cyclone-baked-domain-107]` with `max-threads = 1`, bound by
`binary(native_api) and test(threadx_linux_cyclonedds)`. The filter selects on
the shared RESOURCE rather than listing the four names, so a fifth test on the
same baked domain joins by existing — the retired `zephyr-qos-port` group above
it is the cautionary case, where a group stopped covering every sharer and so
protected nothing. Verified with `cargo nextest show-config test-groups`, which
lists the members rather than merely accepting the filter. `native_api` went
32/36 → **36/36**.

**(2)** `test-threads = 25` in `[profile.default]`, which is
`TEST_DOMAIN_MAX / DOMAINS_PER_SLOT` and not a performance knob. Since that is
one fact in two files — the constants in `nros-tests`, the number in
`.config/nextest.toml` — `domain_partition_matches_the_nextest_cap` reads the
config file and fails if they drift, including when the key is absent entirely.
Verified to fail on 24.

## Note

The cost of (2) is real: 25 threads on a 32-core host. Widening the range is not
available — `TEST_DOMAIN_MAX` is pinned at 101 by the RTPS port formula against
Linux's ephemeral floor (issue 0703), and `a_every_reachable_domain_keeps_its_rtps_ports_out_of_the_ephemeral_range`
gates it. Raising throughput means shrinking `DOMAINS_PER_SLOT` from 4, which
costs headroom for tests that allocate several domains (`interop_e2e::interop`
uses 3), or giving statically-baked domains their own range outside the
partition so they stop competing with it.
