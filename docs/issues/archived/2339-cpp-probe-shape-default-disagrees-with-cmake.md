---
id: 2339
title: "The metadata probe drives every C++ component without a SHAPE as `rclcpp`, and cmake drives `nano_ros_add_node` ones as `configure`, so every scaffolded C++ node fails its probe"
status: resolved
type: bug
area: [cli, cpp, testing]
severity: high
found: 2026-11-07
resolved: 2026-11-07
related: [phase-531]
---

## What

`nros new host_ws --board linux --template talker --lang cpp`, then `nros
build` (book `guide/platforms/linux-host.md`, found by the book session). The
metadata probe failed:

    probe_host_node__host_node.cpp:27: error: no matching function for call to 'host_node::Talker::Talker(rclcpp::NodeHandle&)'
    probe_host_node__host_node.cpp:28: error: 'class host_node::Talker' has no member named 'ok'
    … 'error_what', 'error_code'

`nros build` still exited 0. A failed probe is a sync note (`no producer for
host_node::host_node`), and the image's own entry compiles. So the node got
no sidecar, and every pool the image derives from one fell back to the model
alone.

## Why

There are two component shapes, and the entry template has one arm for each.
`rclcpp` placement-news the class with a `NodeHandle` and checks
`ok()`/`error_what()`/`error_code()`, which `rclcpp::Node` provides to a class
that IS-A node. `configure` default-constructs the class and calls
`configure(node)`. The verbs default differently:

| verb | default SHAPE |
| --- | --- |
| `nano_ros_node_register`, and `nano_ros_add_node` (forwards SHAPE only when given) | `configure` |
| `nros_components_register_node` | `rclcpp` |

The CLI's cmake scanner recorded an omitted SHAPE as `None`.
`ComponentDeclaration::probe_shape()` filled that with `rclcpp` for every
verb, and its doc claimed this matched "the cmake verb's own default". It
matches one of the two verbs. Every scaffold role is a `nano_ros_add_node`
configure class, so the probe drove all of them down the arm they do not have.
The image's entry, which cmake renders, took the right one.

## Fix

- Each scanner states the shape: the call's own value, lowercased as cmake
  does, else ITS verb's default (`workspace::stated_shape`, with
  `NODE_REGISTER_DEFAULT_SHAPE` and `REGISTER_NODE_DEFAULT_SHAPE`).
  `workspace::probe_target_tests::the_scanner_defaults_are_the_cmake_verbs_defaults`
  reads both defaults out of the cmake source, so a moved default fails there.
- Not a detection idiom in the entry template. The arm is a function of the
  DECLARED shape, and a wrong shape also breaks the constructor
  (`Talker(NodeHandle&)`), which no member detection can paper over.
  Detecting `ok()` would only hide a misdeclared component. For the same
  reason the scaffold templates do not grow `ok()`/`error_*()`: a configure
  class reports through `configure`'s `Result`.
- `check-scaffold-builds` had built every role and language and stayed green
  through this, for two reasons. A failed probe exits 0, and the gate's
  "own artifact" predicate accepted the PROBE's own compile of `Talker.cpp`.
  It now refuses a `no producer for` note, requires a sidecar per node package
  written by the build, and no longer counts objects under `nros-metadata/`.
  Negative control: against the unfixed CLI it fails `cpp_project_talker`
  and passes `rust_project_talker`.
