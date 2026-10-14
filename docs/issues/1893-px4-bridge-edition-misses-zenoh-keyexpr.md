---
id: 1893
title: "PX4 bridge: `build-bridge-example <topics> <edition>` reaches the generated TYPE_HASH but not the zenoh backend, so a `jazzy` image publishes Humble data keys"
status: open
type: bug
severity: medium
area: [px4, rmw-zenoh, build]
related: [issue-0362, issue-0436, issue-1896, phase-325]
found: 2026-10-14
---

## Measured

Found while landing phase-325 W3.4 (the bridge's real-ROS-2-subscriber test,
`packages/testing/nros-px4-sitl-test/tests/px4_bridge_ros2_e2e.rs`).

`just px4 build-bridge-example debug_key_value jazzy` and `... humble` were both
built and both run against the same ROS 2 **Humble** `ros2 topic echo`
(`rmw_zenoh_cpp` 0.1.9). Both delivered — the Humble subscriber decoded
`key: velx`, advancing `value`, from either image:

```
# image generated with --ros-edition jazzy
TYPE_HASH = "RIHS01_9f784632ce2d700b9ea22685b6849e45d3f6028e81e6655b74c7d92b303b6198"   (in bin/px4)
ROS 2 humble subscriber decoded 3 bridged samples, first Sample { timestamp: 1668000, key: "velx", value: 3.0 }, ...
```

That is not "rmw_zenoh ignores the hash". It is the archive ignoring the
edition. `nros-rmw-zenoh`'s DATA key expression is chosen by its OWN cargo
feature, not by the message's `TYPE_HASH`
(`packages/rmw/zenoh/nros-rmw-zenoh/src/keyexpr.rs`, `to_key`):

* `not(any(ros-iron, ros-jazzy))` → `<domain>/<topic>/<type>/TypeHashNotSupported`
* `ros-iron | ros-jazzy` → `<domain>/<topic>/<type>/<type_hash>`

and the recipe's step [3/4] builds `libnros_cpp.a` with
`--features std,rmw-<rmw>-cffi,bridge,platform-posix` — no edition. Nor would
adding `ros-jazzy` there help: `nros-cpp/ros-jazzy` forwards to `nros` and
`nros-c` only, and the only crates forwarding an edition to `nros-rmw-zenoh`
are `nros-board-linux` and `nros-rmw-zenoh-staticlib` — the second archive the
bridge deliberately stopped linking (issue 0436: two zenoh-pico copies in one
image broke `z_open`).

So the `edition` argument (default `jazzy`) changes the hash the generator
writes into the header — and therefore what the liveliness token advertises —
while the data key stays Humble's. An image built with the README's default
command is a Humble publisher wearing a Jazzy hash.

## Not measured

Whether a real Jazzy (`rmw_zenoh_cpp` 0.2+) subscriber receives from the
default `jazzy` image. This host has no Jazzy install. The expectation from the
key shape is that it does NOT (it subscribes on its own RIHS01 segment), which
would make the README's default command produce an image no Jazzy peer sees.

## What would close it

The edition is one value with three destinations — the generator's
`--ros-edition`, the zenoh backend's `ros-*` feature, and whatever nros-cpp
itself keys on — and today it reaches one. Forward an edition feature from
`nros-cpp` to `nros-rmw-zenoh` (as `nros-board-linux` does), pass
`ros-<edition>` in step [3/4], and measure against a Jazzy subscriber. Until
then the W3.4 test pins `humble`, which is the one pairing whose two halves
agree.
