---
id: 2261
title: "The book's Ubuntu prerequisite `python3-catkin-pkg` makes the ROS 2 apt install fail"
status: open
type: bug
area: book, setup
purpose: [industrial, education]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I8/E7, installation.md "Host prerequisites", then the ROS 2 apt install: `apt-get install ros-jazzy-ros-base ros-jazzy-rmw-zenoh-cpp` → `trying to overwrite '/usr/lib/python3/dist-packages/catkin_pkg/__init__.py', which is also in package python3-catkin-pkg 1.0.0-2`. Ubuntu's `python3-catkin-pkg` conflicts with ROS's `python3-catkin-pkg-modules`.
- Workaround: `dpkg -r --force-depends python3-catkin-pkg; apt --fix-broken install`.

## Next

Drop `python3-catkin-pkg` from the Ubuntu prerequisites when ROS 2 is (or will be) installed, or say to install ROS 2 first; check the index's `apt` rows for the same package (`scripts/lib/index_packages.py`).
