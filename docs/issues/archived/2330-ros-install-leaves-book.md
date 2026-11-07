---
id: 2330
title: "Install sends the reader to docs.ros.org for the ROS 2 apt source, whose release lookup fails behind a shared NAT (GitHub API rate limit)"
status: resolved
type: enhancement
area: book, install
purpose: [education]
related: [phase-523, phase-537]
found: 2026-11-07
resolved: 2026-11-07
resolved_in: "book-r2-followup branch: Install adds the ROS 2 apt source itself, at a pinned ros2-apt-source release"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- E1: following docs.ros.org's `curl -s https://api.github.com/repos/ros-infrastructure/ros-apt-source/releases/latest` returned `API rate limit exceeded`, so `dpkg -i` failed on an empty file (`unexpected end of file in archive magic version number`).
- In a lab where 30 seats share one NAT address, that limit is a realistic failure. The probe read version 1.3.0 off the `releases/latest` redirect instead.

## Next

Inline the apt-source step in Install. Use the redirect (no API) or a pinned version, and give the lab-image route (one prepared image, cloned per seat).

## Resolution

All three parts of Next, on the book side. No installer change was needed:
`scripts/install.sh` already fetches the release through the
`releases/latest/download/` redirect, not the API.

- **The step is inline.** Install › "Install ROS 2 first"
  (`book/src/start/install.md`) adds the apt source itself (probe step 4,
  `variant=fresh-host`): the prerequisites and `universe`, then
  `ros2-apt-source` 1.3.0 for the host's codename, then `dpkg -i` and
  `apt-get update`. The ROS 2 install line follows as step 5. The page no
  longer sends the reader to docs.ros.org.
- **No API lookup.** The version is pinned in the step, so it asks GitHub for
  the file and nothing else. The page says a newer release works by replacing
  both version numbers.
- **The lab-image route.** A paragraph after the catkin note says to prepare
  one machine or image with Install and the first board's `nros setup` done,
  then copy it to each seat.

Measured 2026-11-07 on this host: the step, run as root in `ubuntu:24.04` and
`ubuntu:22.04` containers, installs `ros2-apt-source` (`1.3.0~noble`,
`1.3.0~jammy`), and after `apt-get update` both
`ros-<distro>-ros-base` and `ros-<distro>-rmw-zenoh-cpp` resolve, for Jazzy
and Humble. The `install-ros` probe track still awaits a host without ROS 2
(its `awaits` now names both steps), so CI does not yet replay it.
