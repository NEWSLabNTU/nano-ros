# examples/bridges/

Cross-RMW gateway binaries. Each example bridges two RMW backends
inside a single process — for instance, subscribing on a Zenoh
session and republishing on a DDS session.

A bridge is the one thing the canonical tree genuinely cannot hold. There the
path is `<plat>/<lang>/<example>` and the RMW is picked at BUILD time — one
example, one backend per build. A bridge holds two backends open at once, so no
build-time choice describes it; it needs a category of its own rather than a
cell. (The retired `<plat>/<lang>/<rmw>/<example>` form this file used to cite
was deleted for Zephyr by phase 168.6.C and finished off by phase-316 — see
RFC-0026.)

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
