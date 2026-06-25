# ws-bridge-rust — cross-RMW bridge showcase (phase-263 B3) · WORK IN PROGRESS

A declarative cross-RMW gateway: a `[[bridge]]` in `src/demo_bringup/system.toml`
forwards `/chatter` from the **zenoh** session to a **cyclonedds** session in one
process, so a stock `rmw_cyclonedds_cpp` peer (`ros2 topic echo /chatter`) sees a
nano-ros publisher that only speaks zenoh.

## Status

This workspace is **partially landed** — the engine foundations are in place, the
end-to-end bake→build flow is not yet wired (tracked in issue **#99**):

- ✅ `talker_pkg` (publishes `std_msgs/Int32` on `/chatter`) — builds.
- ✅ `system.toml` with `[[domain]]`×2 + `[[bridge]] gw` — validated: `nros plan`
  emits a correct `build.transports` (2) + `plan.bridges` (endpoints resolved).
- ✅ Planner populates `transports` + `bridges` from `[[bridge]]` (issue #99 step 0).
- ✅ Native Rust entries link + register `nros-rmw-cyclonedds-sys` (cyclone-Rust
  codegen, board-gated).
- ⏳ The baked orchestration entry (the bridge relay) is **not yet built** — see
  issue #99 for the remaining cascade (the bake's thin plan record, component
  metadata for topic resolution, the pure-cargo baked-entry build lane).

Unlike the macro-shaped Track-B siblings (B1/B2/B4/B5/B6, plain `nros::main!`),
the bridge entry must be the BAKED orchestration entry (`nros codegen-system` →
generate → `cargo build`) because `nros::main!` emits no bridge relay.

## Reference

The working imperative bridge: `examples/bridges/tt-zenoh-to-{xrce,cyclonedds}`
(manual multi-session + TT scheduling + type-descriptor staging). The declarative
path this workspace targets is RFC-0009.

## Build (once the flow lands)

```sh
git submodule update --init third-party/dds/cyclonedds   # vendored C++ CycloneDDS
nros ws sync
nros codegen-system --bringup demo_bringup --out build/demo_bringup
# → generate the baked orchestration entry + cargo build (see issue #99)
```
