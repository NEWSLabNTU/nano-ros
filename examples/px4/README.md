# examples/px4 — PX4 Autopilot integration

PX4 is integrated on its two native messaging surfaces (so the sub-dir axis is
the **transport case**, not an RMW): in-firmware **uORB** modules (C++) and an
**XRCE-DDS** companion (Rust). Just module: **`px4`** (`just/px4.just`).

## This tree is a foreign-build integration, not an example layout

`examples/px4/` is **class X** in
[`examples/README.md` § Layout classes](../README.md#layout-classes): it matches
neither canonical shape — a standalone leaf `<platform>/<lang>/<example>`
([RFC-0026](../../docs/design/0026-example-directory-layout.md)) nor a colcon
workspace with a generated entry
([RFC-0098](../../docs/design/0098-generated-leaf-build-config.md) D9). It reads
like an outlier to migrate. **It is not one, and nothing here moves.** Unifying
it produces a firmware tree PX4's build cannot find.

1. **`src/modules/<name>/{CMakeLists.txt,Kconfig}` is PX4's layout, not ours.**
   PX4 consumes `cpp/firmware/` and `cpp/bridge/` directly through
   `EXTERNAL_MODULES_LOCATION` (see `build-sitl-example` and
   `build-bridge-example` in `just/px4.just`), which mandates that directory
   shape and the `Kconfig` beside the module. Each is its own root because PX4
   takes exactly **one** such location per build. They are copy-**into**-PX4
   sources, not nano-ros applications: no `system.toml`, and nothing for
   `nros build` to generate.
2. **The sub-directory level is the TRANSPORT CASE, not the language.** `cpp/`
   and `rust/` name *which messaging surface* — in-firmware uORB versus the
   XRCE-DDS companion — and the language follows from that, not the other way
   round. So a reader expecting the usual `<lang>/` level will go looking for a
   `cpp/` companion and a `rust/` firmware module. **Neither can exist**: C is
   not on the PX4 module API, and the Rust uORB backend was retired in
   phase-115.K.4.
3. **There is no RMW axis.** In-firmware is uORB-only; the companion speaks
   XRCE-DDS to `uxrce_dds_client`. The `<rmw>` coordinate every other example
   carries is not a free choice here.

**Decision, recorded so it stops being a recurring question:
`rust/companion/*` stays where it is.** Those three *are* movable — ordinary
host cargo bins with a `package.xml` and no `system.toml`, so
`px4/rust/<example>` would be well-formed. Moving them would delete the one
directory level that records the transport case, to buy uniformity with leaves
that do not share PX4's constraints. Not worth it.

Measured in
[issue 1516](../../docs/issues/archived/1516-px4-is-a-foreign-build-integration.md);
written down by
[phase-470](../../docs/roadmap/archived/phase-470-example-layout-unification.md) W4.

## Prerequisites

```sh
source ./activate.sh
just setup px4                # nros setup --source px4-rs --source px4-autopilot
                              # + PX4's ~50 own sub-submodules + python build deps
```

## RMW selection

None — PX4 is uORB-only for in-firmware modules (the Rust uORB backend was
retired in phase-115.K.4); the companion path speaks XRCE-DDS to the
`uxrce_dds_client` in PX4 firmware.

## Build & run

```sh
just px4 build-examples       # SITL with EXTERNAL_MODULES_LOCATION=packages/testing/nros-px4-register-check
just px4 build-fixtures       # px4-stub / companion XRCE fixtures (px4_msgs bindings)
just px4 test-sitl            # E2E: px4_xrce_e2e (Track B). Track A is build-only — see issue 0356
```

## Cases

| Dir | What it is |
| --- | --- |
| `cpp/firmware/` | in-firmware uORB interop module (`nros_uorb_demo`) — `just px4 build-sitl-example` |
| `cpp/bridge/` | in-firmware uORB→RMW bridge (`nros_uorb_bridge`) + its generated-message FFI staticlib — `just px4 build-bridge-example` |
| `rust/companion/offboard-companion/` | XRCE companion receiving `/fmu/out` telemetry (RFC-0039 Track B) |
| `rust/companion/px4-stub/` | fake-PX4 stub publishing `/fmu/out/vehicle_odometry` (CI without SITL) |
| `rust/companion/px4-probe/` | XRCE probe utility |

C and Rust-uORB cells are intentionally empty (see
[`examples/README.md`](../README.md) "Intentionally empty cells").
