---
id: 2330
title: "Install sends the reader to docs.ros.org for the ROS 2 apt source, whose release lookup fails behind a shared NAT (GitHub API rate limit)"
status: open
type: enhancement
area: book, install
purpose: [education]
related: [phase-523]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- E1: following docs.ros.org's `curl -s https://api.github.com/repos/ros-infrastructure/ros-apt-source/releases/latest` returned `API rate limit exceeded`, so `dpkg -i` failed on an empty file (`unexpected end of file in archive magic version number`).
- In a lab where 30 seats share one NAT address, that limit is a realistic failure. The probe read version 1.3.0 off the `releases/latest` redirect instead.

## Next

Inline the apt-source step in Install. Use the redirect (no API) or a pinned version, and give the lab-image route (one prepared image, cloned per seat).
