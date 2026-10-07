---
id: 1742
title: "Most `nros_{platform,board}_link_app` seams apply no panic policy, and only one is held to it"
status: open
type: bug
area: [build, cmake, tooling]
severity: low
found: 2026-10-07
related: [issue-0719, issue-1735, issue-1614]
---

## What was measured

Folding `check-image-paths-apply-policy` into `check-cmake-image-policy`
(issue 1735) made the gate read each `function()`/`macro()` scope. Its first
run flagged `nano_ros_node_register` in `cmake/NanoRosNodeRegister.cmake`: the
NuttX and ThreadX typed-entry CARRIERS each `add_executable` an image, link
`NanoRos::NanoRos(Cpp)`, and apply no policy themselves. Instead they hand the
image to `nros_platform_link_app(...)`.

Whether that delegation is sound depends on the seam. Of the 17 tracked
`nros_{platform,board}_link_app` definitions under `cmake/`, three call
`nros_apply_panic_policy` (`cmake/NanoRosEntry.cmake`,
`cmake/platform/nano-ros-nuttx.cmake`,
`cmake/board/nano-ros-board-rv-virt-threadx.cmake`), and `REQUIRED_SEAMS` in
the gate holds exactly ONE of them (NuttX) to it. So a ThreadX carrier on
`threadx-linux` reaches a seam that applies nothing, and no gate asks.

## What the gate does today

`check-cmake-image-policy` treats a scope that calls a link seam as having
DELEGATED the policy. That reading is correct exactly when the seam applies it.
That is a statement about every seam, and today it is true for three of 17.

## Needs a ruling

Either every seam an image can reach WITHOUT going through `nano_ros_entry()`
applies the policy (add them to `REQUIRED_SEAMS` and fix the ones that do not),
or the carriers in `nano_ros_node_register` apply it themselves before calling
the seam. Hosted `threadx-linux` links `std` and may not need a
`#[panic_handler]`, so whether the gap is LIVE depends on that board. Nothing
measured it here.
