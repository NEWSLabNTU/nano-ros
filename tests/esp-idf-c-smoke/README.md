# nros esp-idf-c smoke test

Minimal ESP-IDF project that pulls `nros-platform-esp-idf-c` via
`EXTRA_COMPONENT_DIRS` and exercises one symbol from each capability
category. Runs on qemu-system-{riscv32,xtensa} so no real hardware
is required.

## Prerequisites

ESP-IDF must be installed via:

```bash
just esp_idf setup
```

(opt-in — not pulled by top-level `just setup`, which uses esp-hal
bare-metal). The setup script clones esp-idf into
`$repo/esp-idf-workspace/esp-idf/` and runs `./install.sh` for the
target chip.

## Build + run

```bash
just esp_idf build-c-port             # default target: esp32c3
just esp_idf test-c-port               # build + boot on qemu

# Or for esp32 (xtensa):
just esp_idf build-c-port esp32
just esp_idf test-c-port esp32
```

Expected serial output ends with:

```
I (...) nros_smoke: nros esp-idf-c smoke PASS
```

## What it covers

- `nros_platform_clock_ms` monotonicity (sleeps 50 ms, asserts delta ≥ 20 ms).
- `nros_platform_alloc` / `nros_platform_dealloc` round-trip.
- `nros_platform_yield_now` + `nros_platform_sleep_ms`.
- `nros_platform_random_u32` (logged for inspection).
- `nros_platform_timer_create_periodic` fires ≥ 4 times over 150 ms.
- Implicitly exercises the FreeRTOS-backed task/mutex/condvar paths
  because IDF's FreeRTOS scheduler is the runtime substrate.

Network paths (`nros_platform_tcp_*` / `_udp_*`) require an active
`esp_netif` + Wi-Fi/Ethernet driver; covered by separate integration
tests on real hardware.
