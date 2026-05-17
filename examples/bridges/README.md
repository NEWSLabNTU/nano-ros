# examples/bridges/

Cross-RMW gateway binaries. Each example bridges two RMW backends
inside a single process — for instance, subscribing on a Zenoh
session and republishing on a DDS session. Because a bridge spans
transport slots, it does not belong to a single
`<plat>/<lang>/<rmw>/<example>` cell and lives outside that tree.

Directory naming follows `<plat>-<lang>-<from>-to-<to>/` so the
RMW pair stays visible at a glance.

## Contents

- `native-rust-zenoh-to-dds/` — POSIX Rust binary; Zenoh
  subscriber, DDS publisher. Demonstrates the Phase 104 multi-RMW
  registry: both backend ctors fire at lib-load, then
  `Executor::open_with_rmw("zenoh", ...)` and
  `node_builder.rmw("dds")` pin each session to its intended
  backend.
