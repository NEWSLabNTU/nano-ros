---
id: 2259
title: "First Project requires ROS 2 Humble with `ros-humble-rmw-zenoh-cpp`, which cannot be installed on Ubuntu 24.04, and links a container recipe that does not exist"
status: resolved
type: bug
area: book
purpose: [industrial, education]
related: [phase-523, rfc-0116]
found: 2026-11-05
resolved_in: "docs/523-book-stops branch: Book: the ROS 2 edition follows the Ubuntu release"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I7/E6, first-project.md intro: "ROS 2 Humble with `ros-humble-rmw-zenoh-cpp`". Humble has no Ubuntu 24.04 packages, and the book's own ros2-interop.md and support-status.md say to use Jazzy (Humble ships no `rmw_zenoh_cpp` apt package). The "container the install page describes" link leads to no container. No page gives ROS 2 install steps.
- Both probes installed Jazzy from docs.ros.org (off-book) to continue.

## Next

First Project names the edition the host can install (Jazzy on 24.04), gives or links the install steps, and drops or fixes the container reference.

## Resolution (2026-11-07)

Install, the Tutorial and every guide prerequisite now name the edition the host can install: Humble on Ubuntu 22.04, Jazzy on 24.04, with `ros-<distro>-rmw-zenoh-cpp`. Install gives the apt line, the Tutorial says to write `jazzy` where its commands say `humble`, and the dead link to a container was removed. The probes chose Jazzy because `talk-to-ros2.md` and `support-status.md` said Humble ships no `rmw_zenoh_cpp` apt package. That claim was false: `ros-humble-rmw-zenoh-cpp` 0.1.11 is in the jammy index, checked 2026-11-07. Both pages now say so (the support table is fixed in `scripts/gen-support-status.py`).
