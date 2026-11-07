---
id: 2323
title: "The Working-offline `.deb` recipe downloads in `ros:jazzy-ros-base`, so it carries neither ROS 2 itself nor the base-image packages a pristine Ubuntu lacks"
status: resolved
type: bug
area: book, offline
purpose: [industrial]
related: [phase-523, phase-537, issue-2264]
found: 2026-11-07
resolved: 2026-11-07
resolved_in: "book-r2-followup branch: Working offline downloads every package in a plain ubuntu image with the ROS 2 apt source"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I6: the book's `docker run … ros:jazzy-ros-base bash -c 'apt-get install -y --download-only …'` downloads 10 debs, 2 of them ROS. It misses `ros-jazzy-ros-base` and everything preinstalled in that image (ca-certificates, among others). The step then says "after installing ROS 2 the same way", which means nothing offline.
- Downloading the full set (534 debs, 206 MB) in `ubuntu:24.04` with the ROS apt source worked. The offline install then needed `libssl3t64` and `ca-certificates` at the target's exact versions.

## Next

Download in `ubuntu:24.04` (the target's base) with the ROS apt source added, include `ros-jazzy-ros-base`, and say the download image must match the target's point release.

## Resolution

Install options › Working offline › "1. Carry the host packages"
(`book/src/guide/build/install-options.md`, probe step 310) is rewritten as
Next asked:

- It downloads in plain `ubuntu:24.04` (`ubuntu:22.04` and `ros-humble-…` for
  Humble), adds the ROS 2 apt source the way Install now does (issue 2330:
  `ros2-apt-source` 1.3.0, no API lookup), and names `ros-jazzy-ros-base`
  beside `ros-jazzy-rmw-zenoh-cpp`.
- `apt-get install --download-only` runs with
  `-o Dir::State::status=` pointing at an empty file, so apt treats nothing as
  installed. The download then also carries what the image already had
  (`ca-certificates`, `libssl3t64`, `curl`, the essential set), the packages
  round two's offline install was missing.
- The page says the container's release must match the offline machine's,
  and to download after that machine's last update, since a carried package
  older than its installed copy is a downgrade. "After installing ROS 2 the
  same way" is gone: one `sudo apt install ./debs/*.deb` installs everything,
  ROS 2 included.

Measured 2026-11-07 on this host with docker. The page's step, run as
written, downloads **606 packages, 227 MB** for Jazzy, `ros-jazzy-ros-base`,
`ca-certificates` and `libssl3t64` among them. `apt-get install ./*.deb` of
the same set in a fresh `ubuntu:24.04` container with `--network none`
installs it: `/opt/ros/jazzy/setup.bash` sources (`ROS_DISTRO=jazzy`), and
`ros2`, `cmake` and the message-generation Python modules are present. The
Humble variant was not measured.

The `install-offline` probe track keeps its `awaits` (the probe container
runs no docker daemon); its text now names `docker run ubuntu:24.04`.
