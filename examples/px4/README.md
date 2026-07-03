# examples/px4 — PX4 Autopilot integration

PX4 is integrated on its two native messaging surfaces (so the sub-dir axis is
the **transport case**, not an RMW): in-firmware **uORB** modules (C++) and an
**XRCE-DDS** companion (Rust). Just module: **`px4`** (`just/px4.just`).

## Prerequisites

```sh
source ./activate.sh
just px4 setup                # nros setup --source px4-rs --source px4-autopilot
                              # + PX4's ~50 own sub-submodules + python build deps
```

## RMW selection

None — PX4 is uORB-only for in-firmware modules (the Rust uORB backend was
retired in phase-115.K.4); the companion path speaks XRCE-DDS to the
`uxrce_dds_client` in PX4 firmware.

## Build & run

```sh
just px4 build-examples       # SITL with EXTERNAL_MODULES_LOCATION=examples/px4/cpp/uorb
just px4 build-fixtures       # px4-stub / companion XRCE fixtures (px4_msgs bindings)
just px4 test                 # unit: nros-rmw-uorb + nros-px4
just px4 test-sitl            # E2E: px4_e2e (Track A uORB) + px4_xrce_e2e (Track B)
```

## Cases

| Dir | What it is |
| --- | --- |
| `cpp/uorb/nros-register-check/` | canonical in-firmware uORB module smoke (built into SITL via `EXTERNAL_MODULES_LOCATION`) |
| `rust/xrce/offboard-companion/` | XRCE companion receiving `/fmu/out` telemetry (RFC-0039 Track B) |
| `rust/xrce/px4-stub/` | fake-PX4 stub publishing `/fmu/out/vehicle_odometry` (CI without SITL) |
| `rust/xrce/px4-probe/` | XRCE probe utility |

C and Rust-uORB cells are intentionally empty (see
[`examples/README.md`](../README.md) "Intentionally empty cells").
