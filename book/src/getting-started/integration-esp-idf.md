# ESP-IDF (integration shell)

> **Building nano-ros ESP32 examples in this repo?** The bare-metal
> esp-hal path is at [ESP32](./esp32.md); the ESP-IDF C-port runs
> via `just esp_idf setup` (separate from this user-facing
> component).

Phase 139 ships an ESP-IDF component under `integrations/esp-idf/`.
ESP-IDF projects pull nano-ros via `idf.py add-dependency` (once
published to the ESP Component Registry) or via a local path during
development.

## Prereqs

- ESP-IDF ≥ 5.1
- `idf.py` on `PATH` (source `$IDF_PATH/export.sh`)

## One-liner add to your `main/idf_component.yml`

Once published:

```yaml
dependencies:
  nano-ros:
    version: "*"
```

During local development:

```yaml
dependencies:
  nano-ros:
    path: "../components/nano-ros/integrations/esp-idf"
```

Then:

```bash
idf.py set-target esp32
idf.py build
```

## Minimal user `main/CMakeLists.txt`

```cmake
idf_component_register(
    SRCS "main.c"
    INCLUDE_DIRS "."
    REQUIRES nano-ros)
```

## Minimal user `main/main.c`

```c
#include <nros/init.h>
#include <stdio.h>

void app_main(void) {
    nros_support_t s = nros_support_get_zero_initialized();
    (void)s;
    printf("nano-ros linked into ESP-IDF app\n");
}
```

## Configuration

`menuconfig → Component config → nano-ros` exposes:

- `NROS_RMW` (`zenoh` | `dds` | `xrce` | `cyclonedds`)
- `NROS_ROS_EDITION` (`humble` | `iron`)

Both are surfaced via the component's `Kconfig.projbuild` and forward
to `NANO_ROS_*` CMake cache vars.

## Rust glue via `esp-idf-sys`

ESP-IDF has no first-party Rust integration; the bridge is the
[`esp-rs`](https://github.com/esp-rs) stack. `esp-idf-sys`'s
`build.rs` can build ESP-IDF natively (via `embuild`) and inject
extra components into that build tree using the
`[package.metadata.esp-idf-sys]` keys in `Cargo.toml`. Phase 152.7
documents this as the canonical path for nano-ros users who want to
drive their build from Cargo rather than `idf.py`.

```toml
# user_project/Cargo.toml
[dependencies]
esp-idf-svc = { version = "0.49", default-features = false }

[package.metadata.esp-idf-sys]
esp_idf_version = "branch:release/v5.1"
esp_idf_tools_install_dir = "workspace"
# Inject nano-ros as an extra ESP-IDF component into the embedded
# IDF mini-project that esp-idf-sys builds.
extra_components = [
    { component_dirs = ["../nano-ros/integrations/esp-idf"],
      bindings_header = "../nano-ros/packages/core/nros-c/include/nros/init.h" },
]
```

This lands `integrations/esp-idf/CMakeLists.txt` inside the embedded
IDF project at build time; ESP-IDF discovers it via the standard
`COMPONENT_DIRS` walk. `bindings_header` tells `bindgen` to emit
Rust FFI for the nano-ros C surface.

See [`esp-rs/esp-idf-template`](https://github.com/esp-rs/esp-idf-template)
for the canonical Cargo + ESP-IDF project scaffold and
[`esp-idf-sys` BUILD-OPTIONS.md](https://github.com/esp-rs/esp-idf-sys/blob/master/BUILD-OPTIONS.md)
for the full env / `[metadata]` schema.
