# nano-ros Examples

Copy-out templates for users porting nano-ros to a new platform / language / RMW.

**Non-example binaries live elsewhere** — see [Where else to look](#where-else-to-look).

## Tree shape

```
examples/
├── <platform>/<language>/<rmw>/<example>/     # canonical
├── bridges/<name>/                            # cross-RMW gateways
└── templates/<name>/                          # multi-platform recipes (Pattern A workspace, etc.)
```

- **Platform** (12): `native`, `esp32`, `stm32f4`, `px4`, `qemu-arm-baremetal`, `qemu-arm-freertos`, `qemu-arm-nuttx`, `qemu-esp32-baremetal`, `qemu-riscv64-threadx`, `threadx-linux`, `zephyr`
- **Language**: `c`, `cpp`, `rust`
- **RMW**: `zenoh`, `dds`, `xrce`, `cyclonedds`, `uorb`
- **Example** (cases): `talker`, `listener`, `service-{server,client}`, `action-{server,client}`, `custom-msg`, plus variant suffixes: `-rtic`, `-rtic-mixed`, `-async`, `-serial`, `-embassy`, `-aemv8r`, etc.

Each example is a standalone Cargo + CMake package — no walk-up to the parent tree, no workspace coupling. Copy any directory out, set `*_DIR` env vars (or `-D…`) for SDK paths, and it builds.

## Coverage matrix

Cell content: `<count>` of `talker|listener|service-{server,client}|action-{server,client}` cases present (max 6). `+` suffix indicates extras (custom-msg, parameters, lifecycle, RTIC variants, custom-transport, serial, embassy, async, etc.).

| Platform                  | Language | zenoh | dds | xrce | cyclonedds | uorb |
|---------------------------|----------|-------|-----|------|------------|------|
| `native`                  | c        | 6+    | 6   | 6    | –          | –    |
| `native`                  | cpp      | 6+    | 6   | –    | –          | –    |
| `native`                  | rust     | 6+    | 6   | 6+   | –          | –    |
| `esp32`                   | rust     | 2     | –   | –    | –          | –    |
| `stm32f4`                 | rust     | 1+rtic×6 | – | –   | –          | –    |
| `px4`                     | cpp      | –     | –   | –    | –          | nros_register_check |
| `px4`                     | rust     | –     | –   | –    | –          | (pending) |
| `qemu-arm-baremetal`      | rust     | 6+rtic+serial | 2 | –  | –          | –    |
| `qemu-arm-freertos`       | c        | 6     | –   | –    | –          | –    |
| `qemu-arm-freertos`       | cpp      | 6     | –   | –    | –          | –    |
| `qemu-arm-freertos`       | rust     | 6     | 2   | –    | –          | –    |
| `qemu-arm-nuttx`          | c        | 6     | –   | –    | –          | –    |
| `qemu-arm-nuttx`          | cpp      | 6     | –   | –    | –          | –    |
| `qemu-arm-nuttx`          | rust     | 6     | 2   | –    | –          | –    |
| `qemu-esp32-baremetal`    | rust     | 2     | 2   | –    | –          | –    |
| `qemu-riscv64-threadx`    | c        | 6     | –   | –    | –          | –    |
| `qemu-riscv64-threadx`    | cpp      | 6     | –   | –    | –          | –    |
| `qemu-riscv64-threadx`    | rust     | 6     | 2   | –    | –          | –    |
| `threadx-linux`           | c        | 6     | –   | –    | –          | –    |
| `threadx-linux`           | cpp      | 6     | –   | –    | –          | –    |
| `threadx-linux`           | rust     | 6     | 2   | –    | –          | –    |
| `zephyr`                  | c        | 6     | 6   | 6    | –          | –    |
| `zephyr`                  | cpp      | 6     | 6   | 6    | talker-aemv8r | – |
| `zephyr`                  | rust     | 6+async | 6+async | 6 | –         | –    |

Gap themes — see `docs/roadmap/phase-118-example-matrix-coverage.md` for the
plan that fills these:

- **DDS C/C++ on RTOS QEMU platforms** — Rust DDS present, C/C++ never followed.
- **XRCE absent on every embedded platform except Zephyr** — Phase 115.K.2 header-only backend needs a Rust adapter for bare-metal targets.
- **CycloneDDS** present only on `zephyr/cpp/cyclonedds/talker-aemv8r/` — Phase 117 RMW lands POSIX first.

## Sibling categories

### `bridges/` — cross-RMW gateways

Examples that bridge two RMW backends; span the transport slot so they don't fit one platform cell.

- `bridges/native-rust-zenoh-to-dds/` — zenoh ↔ dust-DDS gateway

### `templates/` — multi-platform copy-out recipes

Patterns that span platforms (multi-package workspace layouts, mixed C / C++ / Rust packages sharing one nano-ros install, etc.).

- `templates/multi-package-workspace/` — Pattern A workspace (C talker, C++ listener, Rust publisher under one nano-ros install)

## Where else to look

Test / bench / smoke binaries are NOT under `examples/`. They live with the integration-test crate so the example tree stays a clean copy-out surface.

- **`packages/testing/nros-bench/`** — perf, fairness, stress, large-msg
  - `executor-fairness`, `stress-{zenoh,xrce}`, `large-msg-{xrce,baremetal}`, `wcet-cycles-qemu`
- **`packages/testing/nros-smoke/`** — driver / board bringup (no nros API)
  - `stm32f4-smoltcp-echo`, `esp32-hello-world`
- **`packages/testing/nros-tests/bins/`** — fixture binaries that integration tests build & invoke
  - `cdr-roundtrip-qemu`, `lan9118-qemu`

Each is a standalone Cargo package with an empty `[workspace]` table (they nest under the `nros-tests` workspace member).

## Quick start

Each block assumes a built local install (`just install-local`) and a zenoh router running on `tcp/127.0.0.1:7447` (`build/zenohd/zenohd --listen tcp/127.0.0.1:7447`).

### Native Rust + zenoh

```bash
cd examples/native/rust/zenoh/talker   && cargo run     # terminal 2
cd examples/native/rust/zenoh/listener && cargo run     # terminal 3
```

### QEMU bare-metal Cortex-M3 (MPS2-AN385)

```bash
just qemu-baremetal setup
just qemu-baremetal build
just qemu-baremetal talker      # spawns QEMU + nros-rs-talker
```

### STM32F4 + RTIC (NUCLEO-F429ZI)

```bash
just stm32f4 setup
cd examples/stm32f4/rust/zenoh/talker-rtic
cargo build --release --target thumbv7em-none-eabihf
# flash with probe-rs / openocd
```

### Zephyr (native_sim) C + DDS

```bash
just zephyr setup
source ~/nano-ros-workspace/env.sh
west build -b native_sim/native/64 nano-ros/examples/zephyr/c/dds/talker
./build/zephyr/zephyr.exe
```

## ROS 2 interoperability

nano-ros pubs/subs are rmw_zenoh-compatible. Quickest round-trip:

```bash
# terminal 1
build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# terminal 2
cd examples/native/rust/zenoh/talker && cargo run

# terminal 3
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

For DDS-side interop (cyclonedds, dust-dds), see `docs/reference/rmw_zenoh_interop.md`.

## See also

- [`CLAUDE.md`](../CLAUDE.md) — development guidelines, "Examples = Standalone Projects" section
- [`docs/guides/zephyr-setup.md`](../docs/guides/zephyr-setup.md) — Zephyr workspace bootstrap
- [`docs/reference/rmw_zenoh_interop.md`](../docs/reference/rmw_zenoh_interop.md) — ROS 2 wire protocol
- [`docs/roadmap/phase-118-example-matrix-coverage.md`](../docs/roadmap/phase-118-example-matrix-coverage.md) — coverage-gap fill plan
- [`docs/roadmap/phase-131-examples-tree-revision.md`](../docs/roadmap/phase-131-examples-tree-revision.md) — this tree's restructuring history
