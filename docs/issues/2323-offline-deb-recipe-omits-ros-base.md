---
id: 2323
title: "The Working-offline `.deb` recipe downloads in `ros:jazzy-ros-base`, so it carries neither ROS 2 itself nor the base-image packages a pristine Ubuntu lacks"
status: open
type: bug
area: book, offline
purpose: [industrial]
related: [phase-523, issue-2264]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I6: the book's `docker run … ros:jazzy-ros-base bash -c 'apt-get install -y --download-only …'` downloads 10 debs, 2 of them ROS. It misses `ros-jazzy-ros-base` and everything preinstalled in that image (ca-certificates, among others). The step then says "after installing ROS 2 the same way", which means nothing offline.
- Downloading the full set (534 debs, 206 MB) in `ubuntu:24.04` with the ROS apt source worked. The offline install then needed `libssl3t64` and `ca-certificates` at the target's exact versions.

## Next

Download in `ubuntu:24.04` (the target's base) with the ROS apt source added, include `ros-jazzy-ros-base`, and say the download image must match the target's point release.
