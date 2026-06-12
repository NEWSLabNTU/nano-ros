---
id: 44
title: esp-idf build of nros-platform-freertos/platform.c fails — `_heap_start`/`_heap_end` undeclared
status: open
type: bug
area: esp32
related: [issue-0041]
---

Building any esp-idf example/fixture (`idf.py build` for esp32c3) fails compiling
`packages/core/nros-platform-freertos/src/platform.c` against esp-idf's FreeRTOS
config:

```
.../esp-idf/components/freertos/config/riscv/include/freertos/FreeRTOSConfig_arch.h:59:37:
  error: '_heap_end' undeclared (first use in this function)
.../FreeRTOSConfig_arch.h:59:50:
  error: '_heap_start' undeclared (first use in this function)
FAILED: esp-idf/nano-ros/nano_ros_root/nros_platform_freertos/.../platform.c.obj
```

`FreeRTOSConfig_arch.h` (esp-idf 5.3, riscv) uses `_heap_start` / `_heap_end` (the
linker-script heap-region symbols) in a config macro; `platform.c` includes it but
nothing in the nros-platform-freertos compile unit declares those `extern char
_heap_*[]` symbols, so the **compile** (not link) fails.

**Surfaced by** issue 0041's compile-in-test conversion: the esp-idf tests
(`esp32_idf_talker_builds`, `esp32_idf_listener_builds`, `cli_bringup_esp_idf` —
formerly `phase212_m7_esp32_*` / `phase212_h5_esp_idf`) build these examples via
`idf.py`. They are gated (deselected when `idf.py` is absent, which is the normal
host), so this build break has been **invisible** — the tests never ran. Moving the
build to a build-stage fixture (`scripts/build/idf-fixtures.sh`, run by `just esp32
build-fixtures`) makes `idf.py set-target` succeed (with
`-DNANO_ROS_SKIP_BOOTSTRAP=ON`) but the build then fails here. **Not a regression
of 0041** — pre-existing; the conversion just exposed it.

**Fix direction (esp32 / nros-platform-freertos owner):** declare the heap-region
symbols in the nros-platform-freertos esp-idf compile (e.g. `extern char
_heap_start[]; extern char _heap_end[];` ahead of the `FreeRTOSConfig_arch.h`
use, or provide them via the component's linker fragment), or guard the
`platform.c` path that pulls in that arch config on esp-idf. Once it builds, the
0041 esp-idf fixtures produce ELFs and the three tests resolve them (they are
already converted to consume `require_idf_fixture(...)`).
