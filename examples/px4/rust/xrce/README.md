# PX4 XRCE-DDS companion examples

Companion-side PX4 examples (RFC-0039 Track B / Phase 233): nano-ros nodes that
talk `px4_msgs` over a `MicroXRCEAgent` — the same agent PX4's
`uxrce_dds_client` connects to. Unlike the `px4/.../uorb/` examples (in-firmware
uORB), these run on the host or a peer MCU **beside** PX4.

Full bring-up (agent + SITL + QoS): [`docs/reference/px4-xrce-companion.md`](../../../../docs/reference/px4-xrce-companion.md).

| Example              | Role            | Topics                                                 |
|----------------------|-----------------|--------------------------------------------------------|
| `offboard-companion` | the companion   | sub `/fmu/out/vehicle_odometry`, pub `/fmu/in/offboard_control_mode` |
| `px4-stub`           | fake-PX4 driver | pub `/fmu/out/vehicle_odometry` (drives the companion without SITL)  |

## Generate `px4_msgs` first

The `generated/px4_msgs/` crate each example patches in is **not** committed
(`examples/**/generated/` is gitignored, like every other example's bindings).
Regenerate it from the PX4 `.msg` tree before building:

```bash
# from the repo root, for each example:
nros generate-px4-msgs \
  --px4 third-party/px4/PX4-Autopilot \
  --output examples/px4/rust/xrce/offboard-companion/generated
nros generate-px4-msgs \
  --px4 third-party/px4/PX4-Autopilot \
  --output examples/px4/rust/xrce/px4-stub/generated
```

`generate-px4-msgs` emits the whole `px4_msgs` package (235 messages); the
examples only need `VehicleOdometry` + `OffboardControlMode`, but the extra
modules are harmless (dead-code-allowed, compiled on demand).

## Build + run

```bash
MicroXRCEAgent udp4 -p 8888 &
NROS_LOCATOR=127.0.0.1:8888 cargo run -p px4-offboard-companion
```

## Self-test (no SITL, no companion)

`px4-stub` in loopback mode publishes **and** subscribes its own
`/fmu/out/vehicle_odometry` in one XRCE session — a single-session pub+sub
round-trips `px4_msgs` through a bare agent:

```bash
MicroXRCEAgent udp4 -p 8888 &
NROS_LOCATOR=127.0.0.1:8888 PX4_STUB_LOOPBACK=1 PX4_STUB_TICKS=200 cargo run -p px4-stub
# → "loopback rx[N]: ..." lines
```

This is the CI round-trip (`nros-tests::px4_xrce`, built by `just px4 build-fixtures`).

> The companion (subscribe `/fmu/out/*` **and** publish `/fmu/in/*` in one
> session) does not reliably receive — an `nros-rmw-xrce` bug where a publisher
> in the same session starves the subscriber's receive, independent of the
> agent or message type. It streams setpoints but its odometry callback rarely
> fires. Tracked in
> [issue 0026](../../../../docs/issues/0026-px4-xrce-bare-agent-type-matching.md).
