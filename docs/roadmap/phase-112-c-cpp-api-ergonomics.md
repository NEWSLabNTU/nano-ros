# Phase 112: C/C++ API Ergonomics Pass

**Goal:** Close the day-to-day API ergonomics gap between `nros-c`/`nros-cpp` and `rclc`/`rclcpp` so a hello-world is the same line count, the same shape, and free of platform leaks.

**Status:** Not Started
**Priority:** High
**Depends on:** Phase 21 (C API), Phase 79 (unified platform abstraction), Phase 83 (thin-wrapper compliance)
**Related:** `docs/research/sdk-ux/SYNTHESIS.md` UX-2, UX-3, UX-4, UX-8, UX-21, UX-26
**Sibling:** Phase 111 ships the `nros` CLI; this phase fixes what users see *inside* their `main.c`.

---

## Overview

Cross-RTOS UX research shows nros-c hello-worlds run ~2× the rclc line count, mostly from:

1. **Manual serialize-then-publish_raw two-step.** Every publish site is `serialize → check → publish_raw` instead of typed `nros_publish(&pub, &msg)`.
2. **Open-coded error checks.** 4-line `if (ret != …) { printf; cleanup; return; }` blocks at every API call. rclc has 1-line `RCCHECK`/`RCSOFTCHECK` macros.
3. **Platform leaks in user `main`.** FreeRTOS uses `app_main(void)` + CMake-injected `APP_ZENOH_LOCATOR` macros; Zephyr uses `int main(void)` + manual `zpico_zephyr_wait_network()` + Kconfig `CONFIG_NROS_*`. User code is not portable across RTOSes.
4. **Self-contained-example rule cheats.** `examples/qemu-arm-freertos/c/zenoh/talker/CMakeLists.txt:5` does `include(.../../../cmake/freertos-support.cmake)` — hard escape from the example dir.
5. **`rustapp` package-name leak** — Zephyr Rust examples must be named `rustapp` to satisfy `zephyr-lang-rust`'s `rust_cargo_application()`. New users hit this with cryptic CMake failures.

This phase is a focused user-code-ergonomics sweep. No new transports, no new RMWs, no architecture changes.

---

## Architecture

### A. Typed publish

Codegen change in `packages/codegen/packages/nros-codegen-c/`. For each message type `<pkg>::msg::<Type>`, emit alongside the existing `<type>_serialize`:

```c
nros_ret_t nros_publisher_publish_<pkg>_msg_<type>(
    nros_publisher_t *pub,
    const <pkg>_msg_<type> *msg);
```

Internally: serialize onto a per-publisher inline buffer (sized at compile time from the message bound) and call `nros_publish_raw`. The buffer lives in the publisher struct so no heap. Provide a `_Generic` macro:

```c
#define NROS_PUBLISH(pub, msg) _Generic((msg), \
    std_msgs_msg_int32: nros_publisher_publish_std_msgs_msg_int32, \
    /* ... emitted by codegen ... */ \
    )(&(pub), &(msg))
```

C++ side: `Publisher<T>::publish(const T&)` already typed in `nros-cpp`; verify and document.

The raw path stays for zero-copy users (Phase 99 / 99.L).

### B. Error-check macros

Ship `<nros/check.h>` with:

```c
#define NROS_CHECK(call) do { \
    nros_ret_t _r = (call); \
    if (_r != NROS_RET_OK) { \
        printf("[nros] %s:%d %s -> %d\n", __FILE__, __LINE__, #call, _r); \
        return; \
    } \
} while (0)

#define NROS_SOFTCHECK(call) do { \
    nros_ret_t _r = (call); \
    if (_r != NROS_RET_OK) { \
        printf("[nros] %s:%d %s -> %d\n", __FILE__, __LINE__, #call, _r); \
    } \
} while (0)
```

Sweep every `examples/**/c/**/*.c` and `examples/**/cpp/**/*.cpp` to use them. Document in `book/src/reference/c-api.md`.

### C. Unified user entry: `nros_app_main`

Define `int nros_app_main(int argc, char **argv)` as the **only** user-visible entry point. Per-platform glue calls it after:

- Network-readiness wait (smoltcp link-up, Zephyr DHCP, FreeRTOS lwIP up).
- Executor arena init.
- Board hardware init.
- RTOS task create (where applicable).

Implementation: each `nros-platform-<rtos>` crate exposes a startup shim. Examples drop the per-RTOS `app_main(void)` / `int main(void)` / `extern "C" rust_main()` differences and just write `int nros_app_main(int argc, char **argv) { ... }`.

Backward-compat: keep the existing entry shapes as deprecated shims (1 release).

### D. `APP_*` macro injection → typed config struct

Auto-generate `nros_app_config.h` from `config.toml` (FreeRTOS / NuttX / ThreadX / bare-metal) or Kconfig (Zephyr) at build time:

```c
typedef struct {
    struct { const char *locator; uint32_t domain_id; } zenoh;
    struct { uint8_t ip[4], mac[6], gateway[4], netmask[4]; } network;
    struct { uint32_t app, zenoh_read, zenoh_lease, poll; } priority;
    struct { uint32_t app, zenoh_read, zenoh_lease; } stack_bytes;
} nros_app_config_t;

extern const nros_app_config_t NROS_APP_CONFIG;
```

User writes `nros_support_init(&support, NROS_APP_CONFIG.zenoh.locator, NROS_APP_CONFIG.zenoh.domain_id)` instead of `APP_ZENOH_LOCATOR`/`APP_DOMAIN_ID` macros from CMake. Same struct shape on every RTOS regardless of underlying source.

This is a transition stop on the way to Phase 116 (`nano-ros.toml`), not the final form.

### E. Self-contained examples

`cmake/freertos-support.cmake`, `cmake/nuttx-support.cmake`, `cmake/threadx-support.cmake`, `cmake/baremetal-support.cmake` move into the `find_package(NanoRos)` install (Phase 75 layout) as `find_package(NanoRosFreeRTOS)` etc. Examples drop the `include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/<plat>-support.cmake")` escape and use `find_package(NanoRosFreeRTOS REQUIRED)` instead.

Acceptance: copy any `examples/<plat>/<lang>/<rmw>/<usecase>/` directory to `/tmp/foo`, run `cmake -B build -S /tmp/foo`, build succeeds.

### F. Drop the `rustapp` rename

Zephyr Rust examples must be named `rustapp` because `zephyr-lang-rust`'s `rust_cargo_application()` looks for that name. Two-track fix:

1. Wrap the call in a `nros_rust_application(<crate-name>)` cmake macro that aliases the produced `librustapp.a` to `lib<crate-name>.a` for downstream linking. User crates can be named anything.
2. Upstream a relaxation patch to `zephyr-lang-rust` so the dependency-name knob is parameterized. Track upstream PR; remove our wrapper once merged.

---

## Work Items

- [ ] **112.A.1** Codegen — emit `nros_publisher_publish_<pkg>_<type>` per message type
- [ ] **112.A.2** Codegen — emit `_Generic`-based `NROS_PUBLISH(pub, msg)` umbrella macro
- [ ] **112.A.3** Sweep examples to use typed publish
- [ ] **112.B.1** Add `<nros/check.h>` with `NROS_CHECK`/`NROS_SOFTCHECK`
- [ ] **112.B.2** Sweep all C/C++ examples to use the macros
- [ ] **112.C.1** Define `nros_app_main` contract; document in `book/src/reference/c-api.md`
- [ ] **112.C.2** Per-platform startup shim in each `nros-platform-<rtos>` crate
- [ ] **112.C.3** Migrate examples to `nros_app_main`; keep deprecated shims one release
- [ ] **112.D.1** Auto-generate `nros_app_config.h` from `config.toml`; emit from `nros_generate_interfaces()` cmake fn
- [ ] **112.D.2** Zephyr variant — read Kconfig values into the same struct
- [ ] **112.D.3** Drop `target_compile_definitions(... APP_*)` blocks in example CMakeLists
- [ ] **112.E.1** Move `cmake/<plat>-support.cmake` into the `find_package` install layout
- [ ] **112.E.2** Examples switch to `find_package(NanoRos<Plat> REQUIRED)`
- [ ] **112.E.3** Acceptance test: `tests/integration/copy-out-example.sh` copies an example to `/tmp` and builds
- [ ] **112.F.1** Add `nros_rust_application(<crate-name>)` cmake macro
- [ ] **112.F.2** Migrate Zephyr Rust examples off the `rustapp` name
- [ ] **112.F.3** Track upstream `zephyr-lang-rust` PR

**Files:**
- `packages/codegen/packages/nros-codegen-c/src/templates/` (typed publish + umbrella macro)
- `packages/core/nros-c/include/nros/check.h` (new)
- `packages/core/nros-c/include/nros/app_main.h` (new)
- `packages/core/nros-platform-{freertos,nuttx,threadx,baremetal,zephyr,posix}/src/startup.rs`
- `cmake/NanoRosFreeRTOS-config.cmake.in` etc. (new — Phase 75 layout)
- `examples/**/{c,cpp}/**/CMakeLists.txt` (sweep)
- `examples/**/{c,cpp}/**/src/main.{c,cpp}` (sweep — `NROS_CHECK`, `nros_app_main`, typed publish)
- `tests/integration/copy-out-example.sh` (new)
- `book/src/reference/c-api.md`, `book/src/reference/cpp-api.md` (update)

---

## Acceptance criteria

- A `nros-c` talker hello-world is ≤ 50 lines (current: 98).
- Diff between `examples/qemu-arm-freertos/c/zenoh/talker/src/main.c` and `examples/zephyr/c/zenoh/talker/src/main.c` is exactly: zero lines (or only `#include`s for platform-specific drivers if any).
- `tests/integration/copy-out-example.sh` passes for every example in CI.
- A Zephyr Rust example renamed to `my_talker` builds and runs end-to-end.
- `target_compile_definitions(... APP_*)` removed from every example `CMakeLists.txt`.
- `book/src/reference/c-api.md` documents `NROS_PUBLISH`, `NROS_CHECK`, `nros_app_main`, `NROS_APP_CONFIG`.

## Notes

- `nros_app_main` is a stop on the way to Phase 116 (`nano-ros.toml`). Don't over-design the config struct shape — keep it adjacent to `config.toml`'s current keys so the migration is mechanical.
- Risk: typed publish increases publisher struct size (inline serialize buffer). Cap with a Cargo feature `typed-publish-buffer-size = "256"` (default) for users who care.
- Out of scope: changing the underlying `nros_publish_raw` contract (zero-copy users depend on it).
