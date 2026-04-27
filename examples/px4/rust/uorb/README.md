# PX4 nano-ros uORB examples

Two PX4 modules demonstrating nano-ros's uORB RMW backend:

- **`talker/`** — publishes a synthetic `SensorPing` at the WorkQueue's
  natural rate, using the direct typed `nros::uorb::publication` API.
- **`listener/`** — subscribes to the same topic, logs each message
  via `px4-log`.

Both modules share `SensorPing.msg`, generated into `#[repr(C)] struct
SensorPing` by `px4-msg-codegen`.

## Building

These examples are **PX4-target builds** — they require linking into a
PX4 binary. Add to your PX4 EXTERNAL_MODULES_LOCATION configuration,
or build inline against a PX4-Autopilot checkout:

```bash
# In your PX4-Autopilot tree:
make px4_sitl_default \
    EXTERNAL_MODULES_LOCATION=$NANO_ROS_DIR/examples/px4/rust/uorb
```

The `CMakeLists.txt` in each module includes
`px4-rs/cmake/px4-rust.cmake` (via `third-party/px4-rs` symlink) which
provides the `px4_rust_module()` helper.

## Running on PX4 SITL

After loading both modules into the SITL build:

```bash
# In the px4 shell:
nros_listener start
nros_talker start
```

Listener should log incoming `SensorPing` messages (timestamp, seq,
value).

## Host-mock testing (no SITL)

The crates are `#![no_std]` and require real PX4 FFI symbols, so they
can't be unit-tested on the host directly. To exercise the round-trip
path on the host without SITL, see the integration tests in
`packages/px4/nros-rmw-uorb/tests/` (they use `px4-uorb`'s std mock
broker instead of real uORB).

## See also

- [`book/src/getting-started/px4.md`](../../../../../book/src/getting-started/px4.md)
  — full PX4 integration guide
- [`docs/design/px4-rmw-uorb.md`](../../../../../docs/design/px4-rmw-uorb.md)
  — design rationale
- `third-party/px4/px4-rs/examples/heartbeat/` — comparable px4-rs-only
  example
