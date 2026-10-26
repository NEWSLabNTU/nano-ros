---
id: 2086
title: "A ported ament workspace on Zephyr is not sized from its census: the Zephyr module never records the leaf's entities"
status: resolved
type: limitation
area: [zephyr, cmake, sizing]
severity: medium
found: 2026-10-26
related: [2043, 1556, 1662]
resolved: 2026-10-27
resolved_in: "the PR that archives this file"
---

## What

Issue 2043 sized a ported ament workspace from its own program. A `system.toml`
stating `entities = "census"` makes the configure take a census: a host
build of the same sources, run once. It then writes the leaf's sizing
descriptor, and the runtime crates read their pools from it. That works on
the `find_package(nano_ros)` road (FreeRTOS, measured) and on the
`add_subdirectory` road (Linux, measured).

The Zephyr module takes neither path. Its arm of `nano_rosConfig.cmake` does
not call `nros_record_leaf_entity_facts`. Its Kconfig derive sentinel is the
front end for a Zephyr image's sizes. So a ported workspace on Zephyr
still needs its pools, heap and derived switches stated by hand in
`[image.zephyr.knobs]`. Phase 518 measured all three:

- the pools (node 1 and node 2: executor callbacks, zenoh tables,
  liveliness);
- the nros heap (`heap_size`, which defaults to 65 536 when the image
  declares no parameter capability);
- `local_queryable`, which derived OFF for a ported `composition` and
  silently cut the in-image client off from the in-image server.

## Direction

The census and the descriptor are road-independent: `nros ws entity-census
take --leaf <dir>` builds the leaf for the host whatever its board, and
`sizing-descriptor --from-leaf` composes from that census. What is missing
is the Zephyr consumer. Two options:

1. The module's `module_ext_root` hook (`zephyr/cmake/nros_image_kconfig.cmake`)
   takes the census, as `nros_record_leaf_entity_facts` does, and renders the
   census-derived counts as Kconfig defaults beside the image's knobs. Those
   are the `Kconfig.from-system-toml` rows phase-484 W5b already writes.
2. The Zephyr cargo lane reads `NROS_SIZING_DESCRIPTOR` the way a west entry
   already does since issue 1407, and the C lane's knob resolver reads the
   same file.

Option 1 also reaches the C-side Kconfig symbols (`CONFIG_NROS_*`) and the
derived switches, which a descriptor alone does not.

## Resolution

Option 2's road with option 1's reach: the Zephyr module's existing
`west-leaf-sizing` producer (issue 1662's, for a standalone Rust leaf) now
answers for a C/C++ application too, and the census also reaches Kconfig.

- **The census is taken before Kconfig.** The module's `module_ext_root` hook
  (`zephyr/cmake/nros_image_kconfig.cmake`) calls `nros_leaf_census_take`, the
  step the `find_package(nano_ros)` road already ran, now in its own
  `cmake/NanoRosLeafCensus.cmake` so both roads share one spelling.
- **The host census build can build a Zephyr application.** Such an
  application opens with `find_package(Zephyr)`. The census build points
  `Zephyr_DIR` at a generated no-op `ZephyrConfig.cmake`, so the application's
  own sources build for the host and `find_package(nano_ros)` takes its host
  arm.
- **`west-leaf-sizing` sizes a C/C++ leaf.** With no cargo package, it composes
  the leaf's declaration (`[[component]] entities`, or the census) through
  `sizing_descriptor::leaf_declaration`, the composer `sizing-descriptor
  --from-leaf` now uses too. It writes the entity-inventory fragment the knob
  resolver reads (pools, executor callbacks, liveliness, `local_queryable`)
  and the sizing descriptor it names to cargo. A leaf that declares nothing
  gets nothing, exactly as before.
- **The parameter store's Kconfig axis comes from the census.** A census that
  records a parameter states `CONFIG_NROS_PARAM_STORE=y` in the leaf's rendered
  fragment, which carves the store and raises the nros heap's default to
  192 KiB (issue 1702's rule).
- **A program with several nodes is counted node by node.** The leaf
  composer gave a census-backed component ONE inventory row, and the
  inventory counts a node per row, so a ported `manual_composition` derived
  `NROS_EXECUTOR_MAX_NODES = 1` and its second node failed `NOT_INIT` at
  boot. Each recorded node is now its own row. This also changes the
  `find_package(nano_ros)` road, where the same composer runs.

Not derived: Zephyr's `CONFIG_MAX_PTHREAD_COND_COUNT`. The pool is Kconfig and
the leaf's own conf states it. With derived counts the configure's existing
floor check now fires on a too-small value and names the number. Both
phase-518 ports carried 16 and needed 36.

## Measured

QEMU mps2-an385, Zephyr 3.7, stock ROS 2 Humble peer over zenoh. Each image
states no `[image.zephyr.knobs]` at all, only `entities = "census"` and
`CONFIG_MAX_PTHREAD_COND_COUNT=36`:

- phase-518 node 1 (`autoware_external_cmd_selector`): derived 12
  subscribers, 9 publishers, 16 queryables, 30 liveliness, 21 callbacks,
  `PARAM_STORE=y` (heap 196,608). It shows in `ros2 node list`, publishes its
  mode and answers `select_external_command` (`success=True`, mode 1 -> 2).
  Before the parameter axis it halted with `HEAP EXHAUSTED` on the 64 KiB
  default.
- ros2/demos composition (`manual_composition`, four nodes): derived 4 nodes,
  27 queryables, 34 liveliness, `local_queryable = 1`. All four nodes are
  listed, the listener hears the talker, the in-image client logs
  `Got result: [5]` every period, and `ros2 service call /add_two_ints` gets
  `sum=5`. Before the per-node rows it aborted on `create_subscription`
  (`NOT_INIT`).
