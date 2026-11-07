---
id: 2320
title: "The QEMU talker shows up in Jazzy's `ros2 topic list`, but `ros2 topic echo` receives nothing: type hash INVALID, RELIABLE where the book says best-effort"
status: resolved
type: bug
area: rmw-zenoh, interop, book
purpose: [industrial, education]
related: [phase-523, phase-537, issue-2260]
found: 2026-11-07
resolved_in: "adoption-r2-provenance branch: a workspace syncs its messages for its bringups' edition"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I3, E5: with `nros run sensor_node` streaming `Publishing: 'Hello World: N'`, `RMW_IMPLEMENTATION=rmw_zenoh_cpp ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort` prints nothing for 15–30 s. A plain reliable echo also prints nothing.
- `ros2 topic info -v /chatter --no-daemon` reports `Publisher count: 1`, `Topic type hash: INVALID` and `Reliability: RELIABLE`. A native `ros2 topic pub` on another topic shows `RIHS01_df66…` and echoes fine on the same host.
- It happens with a workspace scaffolded with Jazzy sourced (`ros_edition = "jazzy"`), and with a Humble scaffold edited to Jazzy. Both probes, on separate hosts.
- The chapter says "The talker publishes best-effort". The graph reports RELIABLE.

## Next

Unknown. Candidates: the Jazzy type hash is not reaching the liveliness token on the release's prebuilt message crates, or the edition switch never reaches the bare-metal image's keyexprs. Reproduce from the release in `ros:jazzy`, then compare the token keyexpr with a native publisher's. The chapter's QoS sentence is corrected by the book rewrite (#3183: the publisher is reliable, and a plain echo hears it); this issue keeps the interop half. This is the education route's ROS 2 graph lab, and right now it has no working emulator path.

## Resolution (2026-11-07)

**Cause.** A sync bug, not the RMW. `nros new <ws>` keeps `system.toml` in
`src/<pkg>_bringup/`, and `resolve_sync_edition` read only
`<ws>/system.toml`, so it fell back to Humble for every workspace. The image
then carried `std_msgs::msg::dds_::String_TypeHashNotSupported` (measured in
the `nros-0.6.0-nros8` image's strings), and rmw_zenoh_cpp lists such a topic
with `Topic type hash: INVALID` and never delivers it. That is why editing
`ros_edition` changed nothing.

**Fix.** With no root `system.toml`, a workspace takes its edition from its
bringups (the root and `src/`, found the way the planner finds them).
Bringups that disagree are refused with both files named, because the message
crates are shared. Tests: `a_workspace_takes_its_edition_from_its_bringups`
and `bringups_that_disagree_on_the_edition_are_refused`.

**Verified.** In `ros:jazzy`, a release asset built from this branch
(`PROBE_TRACK=installed PROBE_ASSET_OUT`) was put through the book's First
Project with Jazzy sourced:
- the generated hash is `RIHS01_df668c74…`, the hash a native Jazzy publisher
  shows;
- `ros2 topic info -v` shows that hash;
- `ros2 topic echo /chatter --once` printed `data: 'Hello World: 6'`.

The publisher is RELIABLE. The book's "best-effort" sentence is corrected by
the book rewrite (#3183).
