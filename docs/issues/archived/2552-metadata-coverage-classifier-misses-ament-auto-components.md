---
id: 2552
title: "The example metadata-coverage test does not see the 82 C/C++ packages built with `nano_ros_auto_add_library`"
status: resolved
resolved_in: "the change that archives this issue"
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

## Resolved (2026-11-19)

The node verbs are one list now. `workspace::CMAKE_NODE_VERBS` names the
three calls `discover_cmake_node_metadata` reads:
- `nano_ros_node_register`;
- `nano_ros_add_node`;
- `nros_components_register_node`, the split spelling's registration beside
  `nano_ros_auto_add_library`, which builds only the library.

The discovery's loops use the same constants, so the two cannot disagree.
`example_metadata_coverage`'s classifier reads that list instead of its own.

The 83 split-spelling packages are now classified, and every one yields a
component declaration. The floor rose from 39 to 125 (measured: 125
classified, 42 before). Negative control: with the third verb dropped from
the list, the test counts 42 and fails the floor.
