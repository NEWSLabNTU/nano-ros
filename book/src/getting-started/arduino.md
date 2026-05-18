# Arduino IDE library (`nros`)

The `arduino/nros/` directory in this repo ships as a precompiled
Arduino library for ESP32 chips. An Arduino sketch publishes /
subscribes to ROS 2 topics directly through a `zenohd` router — no
micro-ROS Agent, no Rust toolchain on the developer's machine.

> **Status:** Phase 23. ESP32-C3 (RISC-V) `libnanoros.a` builds
> end-to-end via the in-repo pipeline; ESP32 + ESP32-S3 (Xtensa)
> need the esp-rs Rust toolchain (`espup install`) — tracked as
> Phase 23.2.x. See
> [`docs/roadmap/phase-23-arduino-precompiled.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/phase-23-arduino-precompiled.md)
> for live status.

## Quick start (5 minutes)

1. **Run a zenoh router** somewhere on your network:

   ```bash
   ./build/zenohd/zenohd
   ```

   (Built by `just zenohd setup`; ships in the GitHub Releases
   tarball too.)

2. **Install the `nros` Arduino library**. Until v1, build it
   locally:

   ```bash
   just esp_idf setup            # one-time: installs ESP-IDF v5.3
   just build-arduino-libs       # ARDUINO_LIB_TARGETS=esp32c3 by default
   just package-arduino          # → build/arduino/nano-ros-arduino-v<ver>.zip
   ```

   In Arduino IDE: `Sketch → Include Library → Add .ZIP Library…`
   and pick the zip from `build/arduino/`. (Once we publish on the
   Arduino Library Manager, `Library Manager → search nros →
   Install` will replace this step.)

3. **Open `File → Examples → nros → Talker`**, set
   `WIFI_SSID` / `WIFI_PASS` / `ZENOH_LOCATOR` to your router's
   `tcp/<host>:7447` (or `udp/<host>:7447`), and upload.

4. **Verify on the host**:

   ```bash
   export RMW_IMPLEMENTATION=rmw_zenoh_cpp
   ros2 topic echo /chatter std_msgs/Int32
   ```

   Messages appear with the same wire format upstream `rmw_zenoh`
   uses.

## Why nros instead of micro-ROS

|                       | micro-ROS                        | nros                                |
|-----------------------|----------------------------------|-------------------------------------|
| Host-side process     | micro-ROS Agent                  | `zenohd` (single static binary)     |
| Middleware            | Micro XRCE-DDS                   | zenoh-pico                          |
| Wire interop with ros2| Via Agent's DDS bridge           | Native via `rmw_zenoh`              |
| Library size          | ~22 MB per board                 | Target ~2 MB per chip               |
| Setup                 | install Agent + ROS 2            | run zenohd                          |
| Sketch shape          | `rclc_*` / `RCCHECK`             | `nros_*` / `NRCHECK` (mirror)       |

Coming from micro-ROS: the function signatures and macros are
intentionally parallel (`set_microros_wifi_transports` →
`set_nanoros_wifi_transports`; `RCCHECK` → `NRCHECK`;
`rclc_executor_spin_some` → `nros_spin_once`). Migration is one
find-and-replace.

## Hardware support

| Board                          | Chip      | Status                                       |
|--------------------------------|-----------|----------------------------------------------|
| ESP32-C3 DevKitC               | ESP32-C3  | `libnanoros.a` builds + smokes OK            |
| Arduino Nano ESP32             | ESP32-S3  | Needs Phase 23.2.x esp-rs Xtensa toolchain   |
| ESP32 DevKitC                  | ESP32     | Needs Phase 23.2.x esp-rs Xtensa toolchain   |
| RP2040 + Nina W102             | RP2040    | Future (no WiFi on RP2040 directly)          |

## Troubleshooting

- **`libnanoros.a` is empty** — you cloned the repo without
  building the per-arch archives. Run `just build-arduino-libs`;
  see Phase 23.2 for the build pipeline.
- **WiFi connects but no `/chatter` on the host** — confirm
  zenohd is reachable from the ESP32's network (use the same
  `ZENOH_LOCATOR` you'd pass to `nros::ExecutorConfig::new`).
  Open the IDE's serial monitor at 115200 baud; the
  `NRCHECK` macros print `Error N at file:line` on failure.
- **`Library Manager` cannot find `nros`** — until v1, install
  via `Add .ZIP Library…`. Library Manager submission tracked as
  Phase 23.6.4.

## Contributor flow: rebuilding the library

The pipeline used by CI:

1. `just esp_idf setup` — installs ESP-IDF v5.3 into
   `esp-idf-workspace/esp-idf/` (gitignored). Extended SDK tier
   only; not pulled by the default `just setup`.
2. `just build-arduino-libs` — drives the two-pass IDF cross
   build:
   - First pass: `idf.py reconfigure` lets the Phase 139
     integration shell walk every `__idf_<comp>` target's
     `INTERFACE_INCLUDE_DIRECTORIES` and write the FreeRTOS /
     lwIP / esp_* dirs to `<build>/nros_esp_idf_rust_cflags.env`.
   - Source that file under `set -a` so
     `CFLAGS_<rust-target>` is in the shell env (Corrosion's
     `cmake -E env` wrapper inherits ninja's launch env).
   - Second pass: `idf.py build` produces the per-component
     static archives.
   - `ar crsT` bundles `libnros_c.a` /
     `libnros_rmw_zenoh_staticlib.a` /
     `libnros-platform-esp-idf.a` / `libzenohpico.a` /
     `libzpico_platform_aliases.a` /
     `libnros_c_weak_stubs.a` into
     `arduino/nros/src/<arch>/libnanoros.a`.
3. `just package-arduino` — zips
   `arduino/nros/` into
   `build/arduino/nano-ros-arduino-v<ver>.zip`.
4. `just test-arduino-transport` — host smoke for the
   transport-setup glue. Compiles
   `arduino/nros/src/nros_arduino.cpp` against the
   `tests/arduino/mock_wifi/` stubs; verifies
   `set_nanoros_wifi_transports` / `nanoros_ping`. CI-able
   without ESP-IDF or hardware.

## Adding custom messages

The library ships pre-generated C headers for common packages
(`std_msgs`, `geometry_msgs`, `sensor_msgs`). For custom message
types:

```bash
# In your message package (with package.xml + msg/*.msg):
cargo nano-ros generate-c

# Copy the generated headers into arduino/nros/src/<pkg>/ and
# rebuild the per-arch libraries to embed the message type IDs:
just build-arduino-libs
just package-arduino
```

Until the library is on the Arduino Library Manager, the
contributor flow is the user flow.

## Related

- [Phase 23 roadmap](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/phase-23-arduino-precompiled.md) — full status, subphase coverage matrix, deferred work.
- [ESP-IDF integration shell](./integration-esp-idf.md) — same
  `add_subdirectory(<nano-ros>)` shape the Arduino library
  builder consumes.
