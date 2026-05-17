# ESP-IDF (integration shell)

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
