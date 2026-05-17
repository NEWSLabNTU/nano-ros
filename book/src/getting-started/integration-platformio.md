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
