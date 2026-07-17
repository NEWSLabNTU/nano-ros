---
id: 233
title: "RMW runtime-coverage backlog — the cyclone/xrce-on-RTOS cells the phase-295 matrix makes visible as BuildOnly"
status: open
type: enhancement
area: testing
related: [rfc-0051, phase-295, issue-0067, issue-0214, issue-0215]
---

## Context (phase-295 W6, 2026-07-18)

RFC-0051's matrix (`packages/testing/nros-tests/src/matrix.rs`) makes the
long-standing RMW-runtime-coverage debt VISIBLE for the first time: runtime
e2e is ~entirely zenoh, while cyclonedds/xrce on the RTOS platforms sit as
`Tier::BuildOnly` (fixture links, no runtime lane) or `Tier::CarveOut`.
W6's decision pass triaged every gap cell. This issue tracks the cells
judged **worth implementing**. As of 2026-07-18 the fixture-wireable cells
are all done (native rust cyclone service; threadx-linux C cyclone
service+action; threadx-linux C++ cyclone pubsub). What remains needs real
code or a new fixture: the rust cyclone ACTION creation gap (#2) and the
threadx-riscv64 C++ cyclone build variant (#5). The CarveOuts below are
firm design decisions, NOT tracked here.

## Worth-implementing cells (BuildOnly → Runtime)

Each needs a proven example pair + a fixtures.toml cyclone row + flipping
the matrix cell to `Runtime` (the `example_e2e` consumer then runs it):

1. ~~native rust cyclonedds **service**~~ **DONE (2026-07-18)** —
   `test_native_cyclonedds_rust_service` delivers (server sees the request,
   client prints `Result of add_two_ints: 5`); fixtures.toml cyclone rows +
   the Runtime cell landed. The BuildOnly-"unproven" flag was correct to
   check and is now disproven.
2. **native rust cyclonedds action** — the rust action pair over cyclone
   FAILS AT CREATION (`ActionCreationFailed`, deterministic), while C++
   cyclone action works. Root cause: the typed-action-descriptor path
   C/C++'s `descriptors.cpp` provides has no pure-rust equivalent (the #67
   marker covers pub/sub + service create, not action-type descriptors).
   This is a rust cyclone BACKEND fix, not a fixture-wiring task — the
   remaining half of this cell, and the reason it stays BuildOnly.
3. ~~threadx-linux C cyclonedds **service + action**~~ **DONE (2026-07-18)**
   — `test_threadx_linux_cyclonedds_{service,action}`: the embedded ThreadX
   C server drives a native POSIX client over Cyclone (service → result 5;
   action → full order-10 Fibonacci), mirroring the #215 pubsub interop
   lane. Both matrix cells flipped to Runtime.
4. threadx-linux C++ cyclonedds pubsub — **DONE (2026-07-18)**:
   `test_threadx_linux_cyclonedds_cpp_talker_to_native_listener` (the C++
   sibling of the #215 C lane); cell flipped to Runtime.
5. **threadx-riscv64 C++ cyclonedds pubsub** — remaining. Unlike the
   others this has NO fixture yet (the riscv64 example set builds C + rust
   cyclone two-QEMU pubsub, #214, but not C++). Needs a cpp cyclone build
   variant added to the `just threadx_riscv64` recipe + a two-QEMU lane
   mirroring `test_threadx_riscv64_cyclonedds_two_qemu_{pubsub,rust_pubsub}`.
   Fixture-creation work, not just wiring.

## Firm CarveOuts (recorded in the matrix, NOT this issue's scope)

- **freertos / nuttx rust × xrce** — CarveOut: no XRCE agent-locator bake
  path exists on those platforms (the agent-port bake is Zephyr-only, via
  `CONFIG_NROS_XRCE_AGENT_PORT`); rust-XRCE-on-bare-RTOS is not a shipped
  configuration.
- **nuttx (arm/riscv) rust × cyclonedds** — CarveOut: the Cyclone-on-RTOS
  path is C/C++ only (the cyclone descriptors + ddsrt come through the
  C/C++ module link); the pure-rust NuttX image has no cyclone backend
  symbol, same class as the zephyr rust #163 finding.
- **threadx-riscv64 rust/C++ action** — CarveOut: action examples are not
  implemented on threadx-riscv64 (the platform's example set is
  pubsub+service; C++ service is likewise absent, port slots reserved in
  `platform.rs`).

## Direction

Take cell (1) first (cheapest, highest parity value — closes the last
native rust RMW gap). Each cell landed flips one `BuildOnly` → `Runtime` in
the matrix + adds its fixtures.toml row; the coverage gate keeps both sides
honest. When all four ship, this issue resolves.
