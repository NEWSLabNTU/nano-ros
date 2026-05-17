# PlatformIO (integration shell)

Phase 139 ships a PlatformIO library spec under
`integrations/platformio/`. Consumers add nano-ros to their
`platformio.ini` like any other PIO library.

## Prereqs

- PlatformIO Core (`pip install platformio` or use the VSCode
  extension)

## One-liner add to `platformio.ini`

```ini
[env:my_target]
platform = espressif32
board = esp32dev
framework = espidf
lib_deps = nano-ros@*
```

Then:

```bash
pio run
```

Once published to the PlatformIO Library Registry, `nano-ros@*`
resolves automatically. During local development:

```ini
lib_deps = file:///path/to/nano-ros/integrations/platformio
```

## Minimal user `src/main.cpp`

```cpp
#include <nros/init.h>

int main() {
    nros_support_t s = nros_support_get_zero_initialized();
    (void)s;
    return 0;
}
```

## Per-framework note

PlatformIO honours the `frameworks` field in `library.json`:
nano-ros declares `arduino`, `espidf`, and `zephyr`. The actual
build behaviour delegates to the underlying framework — `espidf`
goes through `integrations/esp-idf/`, `zephyr` through
`integrations/zephyr/`. `arduino` uses the bundled bare-metal
sources directly.

## ESP-IDF gotcha — `lib_deps` ≠ IDF component

Phase 149.7 documents a sharp edge: PlatformIO's `lib_deps` resolves
libraries into `.pio/libdeps/<board>/<name>/`, but those resolved
libraries are **not** automatically registered as ESP-IDF
components. `idf_component_register(...)` in the library's
`CMakeLists.txt` is invisible to PIO's espidf-framework build unless
you point `EXTRA_COMPONENT_DIRS` at the libdeps tree.

Workaround in your project's root `CMakeLists.txt`:

```cmake
# Before idf_component_register / project() — add the PIO-resolved
# libdeps tree to the ESP-IDF component search path so nano-ros's
# integrations/esp-idf/CMakeLists.txt fires its
# idf_component_register(...) like a normal component.
list(APPEND EXTRA_COMPONENT_DIRS
    $ENV{PROJECT_DIR}/.pio/libdeps/$ENV{PIOENV}/nano-ros/integrations/esp-idf)
```

Without this, the user's `main/main.c` `#include <nros/init.h>` will
hit a missing-header error even though `lib_deps` "resolved" the
library — PIO copied the files in but ESP-IDF never saw them.

Tracked upstream as a long-standing PIO + ESP-IDF interaction. See
[PlatformIO community thread](https://community.platformio.org/t/esp-idf-libraries-vs-components/47608)
for the underlying mechanics.
