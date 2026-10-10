---
id: 1795
title: "No lane builds an RTOS TYPED carrier image (`nano_ros_node_register(... TYPED DEPLOY threadx|nuttx)`): every in-tree ThreadX/NuttX C/C++ image takes `nano_ros_entry()`, so the carrier route's link has no coverage"
status: open
type: tech-debt
area: [build, cmake, testing, threadx, nuttx]
severity: low
found: 2026-10-11
related: [issue-1753, issue-1742, rfc-0043]
---

## What was measured

`cmake/NanoRosNodeRegister.cmake` has an RTOS carrier arm for ThreadX
(`_NRC_DEPLOY` with `NANO_ROS_PLATFORM threadx`) and one for NuttX. Each
synthesises a single-node C++ entry TU and links the image. No row in
`examples/fixtures.toml` reaches either arm: every ThreadX and NuttX C/C++
example and workspace builds through `nano_ros_entry()`.

That route failed silently twice:

- **Issue 1742.** The carrier applied no panic policy, so the `nros-c`
  staticlib stopped with `#[panic_handler] function required`.
- **Issue 1753.** Once 1742 was fixed, the rv-virt-threadx carrier failed at
  the final link. Its generated entry holds `static ::rclcpp::Node`, which
  needs the C++ static-object ABI. That ABI was missing on the board.

Each was found only by building a carrier image by hand, while working on
something else.

## Why it matters

The two routes differ in exactly the way that broke. The carrier's TU is C++
with a static object that has a destructor, while an entry-route C image has
none. So the entry route's coverage says nothing about the carrier's link.
Issue 1753's gate (`check-crt-free-runtime-symbols`) now covers that class
statically, but no gate covers the route itself. The next divergence between
the two routes will not be caught either.

## Direction

Add one build fixture per RTOS carrier arm: the rv-virt-threadx carrier
(the `pkg_c_talker` shape issue 1753 used, with `DEPLOY threadx`), and a
NuttX one. Each row also needs the matching `matrix::CELLS` entry, or a
compile-check row if a link is the whole assertion. That is the minimum. A
boot cell is better, since issue 1753's image boots and publishes.

**Acceptance:** each carrier arm is built by a lane. Then removing
`cxx_static_abi.c` from `nano-ros-board-rv-virt-threadx.cmake` fails that
lane's build, independently of the static gate.
