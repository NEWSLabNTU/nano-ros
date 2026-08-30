---
id: 449
title: "native c/listener carried an NROS_SUB_TYPE message-type switch no ROS demo has"
status: resolved
type: tech-debt
area: examples
related: [phase-338, issue-0183]
---

## What it was

`examples/native/c/listener` carried a second callback
(`subscription_callback_int32`) and a runtime switch:

```c
const char* sub_type = getenv("NROS_SUB_TYPE");
int use_int32 = sub_type && (strcmp(sub_type, "int32") == 0 || ...);
```

It was the LAST entry in `example_portability`'s `KNOWN_DIVERGENCE` — 1 of an
original 41 — and its reason claimed **PERMANENT**.

## Why it existed

`tests/declarative_bridge_zenoh_to_cyclonedds.rs` drove that fixture with
`NROS_SUB_TYPE=int32`. The ws-bridge demo forwards `std_msgs/Int32` on
`/chatter` (issue #183) and the message type is baked into the wire keyexpr, so
a `String` subscription cannot match an `Int32` topic — the test had to pick the
type at RUN time against a prebuilt binary.

`bins/int32-sink` already existed for exactly this and had removed the same
switch from the RUST listener in phase-338 W3 — but it carried only
`rmw-{zenoh,xrce}`, and this test needed cyclonedds. So the C example kept a
switch to cover a gap in a test bin.

## Why "PERMANENT" was wrong

Nothing about the example made the switch inherent. **The standard examples
follow the ROS 2 demos, which publish `std_msgs/String`** — a runtime
message-type switch misrepresents what a user is reading, and it existed purely
to serve one test.

## Resolution (2026-08-06)

Maintainer call: fix the message type in the standard examples; put test-only
payload shapes in a test bin.

* `bins/int32-sink` gained an `rmw-cyclonedds` feature, a `fixtures.toml` row
  and a resolver arm — completing the axis the examples already have.
* `declarative_bridge_zenoh_to_cyclonedds.rs` now resolves
  `build_int32_sink_rmw(Rmw::Cyclonedds)` instead of the C example, and no
  longer sets `NROS_SUB_TYPE`. Markers are unchanged (`Received: N`, the shared
  `INT32_LISTENER_LOG_PREFIX`).
* The switch and the int32 callback are deleted from the example, which is now
  `std_msgs/String`-only like every other copy.

**`example_portability` is at ZERO outstanding divergences** — 18 of 18
`(lang, program, group)` triples byte-identical after normalization.

## Note on what was NOT verified

The bridge e2e itself could not run here: it needs host ROS 2 with
`rmw_cyclonedds_cpp` and a prebuilt `bridge-cyclonedds` workspace fixture,
neither available on this host, so it reports `[SKIPPED]`. Verified instead:
the cyclone sink builds through `fixtures-build.sh`, the test compiles against
the new resolver, the asserted marker is identical on both binaries, and
`just check c` passes. The end-to-end run is owed on a host that has ROS.
