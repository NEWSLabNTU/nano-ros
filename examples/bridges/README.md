# examples/bridges/

Cross-RMW gateway binaries. Each example bridges two RMW backends
inside a single process — for instance, subscribing on a Zenoh
session and republishing on a DDS session. Because a bridge spans
transport slots, it does not belong to a single
`<plat>/<lang>/<rmw>/<example>` cell and lives outside that tree.

Bridge examples that *also* exercise a platform/language-specific
feature may still live under the normal example tree; the canonical
home for plain cross-RMW gateways is this sibling category.

## Contents

- `tt-zenoh-to-xrce/` — POSIX Rust binary; Zenoh subscriber, XRCE
  publisher. Demonstrates the multi-RMW registry plus the Phase 110.G
  time-triggered scheduling path. Relocated 2026-06-02 from
  `examples/native/rust/bridge/` per §212.L sibling-category rule.
- `tt-zenoh-to-cyclonedds/` — POSIX Rust binary; Zenoh subscriber,
  Cyclone DDS publisher (issue #53). Same time-triggered frame as the
  XRCE sibling, plus the Cyclone `dds_topic_descriptor_t` staging step
  (`register_type_descriptor`) a raw Cyclone publisher requires.
