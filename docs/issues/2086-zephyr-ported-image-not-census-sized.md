---
id: 2086
title: "A ported ament workspace on Zephyr is not sized from its census: the Zephyr module never records the leaf's entities"
status: open
type: limitation
area: [zephyr, cmake, sizing]
severity: medium
found: 2026-10-26
related: [2043, 1556]
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
