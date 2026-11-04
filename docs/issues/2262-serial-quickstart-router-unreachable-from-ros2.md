---
id: 2262
title: "The serial quick-start's router config listens only on the serial PTY, so a ROS 2 node cannot reach it"
status: open
type: bug
area: book, serial
purpose: [industrial]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I9, serial-transport.md "2. Create the PTY pair": with the page's `ZENOH_CONFIG_OVERRIDE` (serial endpoint only), `ros2 topic echo` → ``Unable to connect to a Zenoh router. Have you started a router with `ros2 run rmw_zenoh_cpp rmw_zenohd`?``.
- Workaround: add `"tcp/127.0.0.1:7447"` to `listen/endpoints`. The page's `nros dev sdk path qemu` also fails on the release (issue 2255).

## Next

The page's router line listens on both the serial endpoint and the default TCP one, so "See it from ROS 2" works as written.
