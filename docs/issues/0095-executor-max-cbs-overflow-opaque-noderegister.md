---
id: 95
title: "Executor `MAX_CBS` overflow surfaces as an opaque `NodeRegister(<pkg>)` with no cause, and has no per-entry sizing knob"
status: open
type: bug
area: core
related: [phase-263, phase-264]
---

## Summary

A declarative workspace whose topology declares more callback entries than the
build-time `NROS_EXECUTOR_MAX_CBS` (default **4**) fails at runtime when the
overflowing entity is declared — but the failure is reported as an **opaque**
`nros: application error: NodeRegister("<pkg>")` with no indication that the cause
is an exhausted executor callback table. Discovered running the never-before-run
phase-263 A1 feature-showcase entry (`examples/workspaces/rust`,
`native_showcase_entry`), whose 4-node launch (talker + listener + add_server +
add_client) declares **5** callback entries:

1. talker timer `on_tick`
2. listener `/chatter` subscription
3. add_server `/add_two_ints` service server
4. add_client `/add_two_ints` service client
5. add_client timer `on_tick`  ← overflows `entries[MAX_CBS]` (default 4)

The 5th declaration (`create_timer_for_callback_name` in `service_client_pkg`)
returns `NodeError::BufferTooSmall` from `Executor::next_entry_slot`
(`packages/core/nros-node/src/executor/spin.rs`), which `create_entity`
(`node_runtime.rs`, `EntityKind::Timer` arm) maps to `NodeDeclError::Runtime`,
which `install_node_typed_with_params` collapses to a `-1` return, which the
`nros::node!` macro register wrapper turns into
`RuntimeError::NodeRegister(<pkg_name>)` (`nros-macros/src/lib.rs`). Every layer of
context is dropped on the way up.

## Two distinct gaps

### A — opaque diagnostic (HIGH)

The user sees only `NodeRegister("service_client_pkg")`. There is no hint that the
real cause is `MAX_CBS` exhaustion (vs. a transport failure, a bad QoS, a duplicate
name, …). A first-time runner cannot tell what to fix. The register seam should
preserve and surface the underlying `NodeError` (at minimum distinguishing
"capacity exhausted — raise `NROS_EXECUTOR_MAX_CBS`" from other failures).

### B — no per-entry sizing ergonomics (MEDIUM)

`MAX_CBS` (and the derived `ARENA_SIZE ≈ MAX_CBS × ~15 KB`) is a **build-time env
const** baked into `nros-node` at its compile, read by `nros-node/build.rs`. cargo
`[env]` in `.cargo/config.toml` is **workspace-global**, so raising it there would
also bloat the RAM-bound embedded entries in the same workspace (an esp32 entry with
`MAX_CBS = 16` derives a ~240 KB arena — well over budget). The `nros::main!` macro
*knows* the topology's entity count at expansion, but cannot resize a dependency's
fixed-size array after the fact. So today there is **no ergonomic, per-entry way** to
size the executor to the declared topology; a workspace mixing a fat native entry and
lean embedded entries must set the env per-build out-of-band.

## Workaround (in place)

The phase-263 A1 Track-D fixture row `workspace-rust-native-showcase`
(`examples/fixtures.toml`) sets `env = { NROS_EXECUTOR_MAX_CBS = "8" }` and builds into
its own `target-fixtures-showcase` dir (so the larger `MAX_CBS` doesn't churn the
`nros-node` fingerprint shared with the default rows). The embedded entries in the same
workspace boot only the minimal talker+listener launch, so they keep the default.

## Fix ideas

- **A:** thread the `NodeError` through `install_node_typed*` → the macro register
  wrapper; emit a `BufferTooSmall`-specific message naming `NROS_EXECUTOR_MAX_CBS`.
- **B:** consider deriving `MAX_CBS`/`ARENA_SIZE` per-entry from the baked topology
  (e.g. a generated `const` the entry passes to a const-generic `Executor`), or at
  least a per-entry build knob the entry's `Cargo.toml` metadata can carry, so cargo
  `[env]` global-ness stops being the only lever.
