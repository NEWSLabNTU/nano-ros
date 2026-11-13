---
id: 2552
title: "The example metadata-coverage test does not see the 82 C/C++ packages built with `nano_ros_auto_add_library`"
status: open
type: test-gap
severity: low
area: [cli, testing]
related: [phase-553]
found: 2026-11-14
---

**Found by** phase-553 W5. `every_node_declaring_example_is_a_metadata_candidate`
(`packages/cli/nros-cli-core/tests/example_metadata_coverage.rs`) asks, for every
example package that declares a node, whether `Workspace::component_declarations()`
yields one. Its classifier recognised a CMake node package only by
`nano_ros_node_register(`. When W5 moved the bare-metal talkers to
`nano_ros_add_node(`, the verb `nros new` emits, they fell out of the count and the
test's floor tripped. W5 taught the classifier that verb.

## What is still missed

82 tracked `examples/**/CMakeLists.txt` call `nano_ros_auto_add_library(`, the
ament_cmake_auto-shaped component verb the C/C++ workspaces use. The classifier
treats every one as not a node, so none is checked: a component whose declarations
the CLI cannot read would pass. This is the "reach narrower than the rule" shape
(issue 0196): the rule is "every node-declaring example", the reach is two verbs.

## Fix

Classify by what the CLI's own static parser reads as a component, not by a list
of verb spellings, so the two cannot disagree. Then re-measure the floor, which
will rise.
