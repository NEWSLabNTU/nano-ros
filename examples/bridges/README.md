# examples/bridges/

Cross-RMW gateway binaries. Each example bridges two RMW backends
inside a single process — for instance, subscribing on a Zenoh
session and republishing on a DDS session. Because a bridge spans
transport slots, it does not belong to a single
`<plat>/<lang>/<rmw>/<example>` cell and lives outside that tree.

Current bridge examples live under the normal example tree when they
also exercise a platform/language-specific feature.

## Contents

- `../native/rust/bridge/tt-zenoh-to-xrce/` — POSIX Rust binary;
  Zenoh subscriber, XRCE publisher. Demonstrates the multi-RMW
  registry plus the Phase 110.G time-triggered scheduling path.
