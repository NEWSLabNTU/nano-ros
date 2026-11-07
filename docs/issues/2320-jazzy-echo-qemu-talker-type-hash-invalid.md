---
id: 2320
title: "The QEMU talker shows up in Jazzy's `ros2 topic list`, but `ros2 topic echo` receives nothing: type hash INVALID, RELIABLE where the book says best-effort"
status: open
type: bug
area: rmw-zenoh, interop, book
purpose: [industrial, education]
related: [phase-523, issue-2260]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I3, E5: with `nros run sensor_node` streaming `Publishing: 'Hello World: N'`, `RMW_IMPLEMENTATION=rmw_zenoh_cpp ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort` prints nothing for 15–30 s. A plain reliable echo also prints nothing.
- `ros2 topic info -v /chatter --no-daemon` reports `Publisher count: 1`, `Topic type hash: INVALID` and `Reliability: RELIABLE`. A native `ros2 topic pub` on another topic shows `RIHS01_df66…` and echoes fine on the same host.
- It happens with a workspace scaffolded with Jazzy sourced (`ros_edition = "jazzy"`), and with a Humble scaffold edited to Jazzy. Both probes, on separate hosts.
- The chapter says "The talker publishes best-effort". The graph reports RELIABLE.

## Next

Unknown. Candidates: the Jazzy type hash is not reaching the liveliness token on the release's prebuilt message crates, or the edition switch never reaches the bare-metal image's keyexprs. Reproduce from the release in `ros:jazzy`, then compare the token keyexpr with a native publisher's. The chapter's QoS sentence is corrected by the book rewrite (#3183: the publisher is reliable, and a plain echo hears it); this issue keeps the interop half. This is the education route's ROS 2 graph lab, and right now it has no working emulator path.
