# Phase 112: C/C++ API Ergonomics Pass

**Goal:** Close the day-to-day API ergonomics gap between `nros-c`/`nros-cpp` and `rclc`/`rclcpp` so a hello-world is the same line count, the same shape, and free of platform leaks.

**Status:** In Progress (A + B + C + D shipped, E/F pending)
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

- [x] **112.A.1** Codegen — emit `static inline {struct_name}_publish(pub, msg)` per message type. Uses `NROS_PUB_BUFFER_SIZE` (default 256, override-able) for the per-call serialize buffer. Submodule commit `d7876c2` in `packages/codegen`. Test extended in `rosidl-codegen` (`test_c_simple_message_generation`).
- [ ] **112.A.2** `_Generic`-based `NROS_PUBLISH(pub, msg)` umbrella macro — deferred. Per-type `_publish` is sufficient for current examples; umbrella macro requires a single header that knows every type and is best generated under Phase 116.
- [~] **112.A.3** Migrated FreeRTOS C zenoh talker. Combined Phase 112.B + D + A reduction: 98 -> 60 lines (-39%). Other examples still use the explicit serialize+publish_raw two-step; sweep tracked alongside 112.B.2.
- [x] **112.B.1** Added `<nros/check.h>` with `NROS_CHECK`/`NROS_SOFTCHECK`/`NROS_CHECK_RET`. Override-able log via `NROS_CHECK_LOG`. Re-exported from umbrella `<nros/nros.h>`.
- [~] **112.B.2** Swept FreeRTOS / NuttX / ThreadX-RISCV C zenoh talker + listener (6 files). Native (`int main`) and service/action/cpp examples deferred — patterns diverge.
- [x] **112.C.1** Defined `nros_app_main` contract in `<nros/app_main.h>` (re-exported from umbrella `<nros/nros.h>`). User defines `int nros_app_main(int argc, char **argv)`; macro `NROS_APP_MAIN_REGISTER_{VOID,ZEPHYR,POSIX}()` at file scope emits the linker entry shim. C/C++ both supported via `extern "C"` linkage. Auto-detect via `__ZEPHYR__` / `NROS_HOST_POSIX` / fallback void.
- [x] **112.C.2** Per-platform startup shim — implemented as header-only macros that emit the platform-correct `app_main` / `main` symbol forwarding to `nros_app_main`. No separate Rust shim needed: existing per-platform startup chains (FreeRTOS startup.c, NuttX entry, ThreadX board init, Zephyr `main`, native CRT) link to the macro-emitted symbol unchanged.
- [x] **112.C.3** Migrated 115 example sources to the new contract. Bulk transform via `tmp/migrate-app-main.py`. Coverage: FreeRTOS C+C++ × 6, NuttX C+C++ × 6, ThreadX-RISCV C+C++ × 6, ThreadX-Linux C+C++ × 6, Zephyr C+C++ × 3 RMWs × 6, native C+C++ × 3 RMWs × 6. Skipped: custom-msg (pure CDR test) + RTOS startup.c (platform chain).
- [x] **112.D.1** `nano_ros_generate_config_header(<config_file> <out_path>)` cmake function in `NanoRosReadConfig.cmake`. Template at `cmake/templates/nros_app_config.h.in`. Installed to `share/nano_ros/templates/`. Found via `CMAKE_CURRENT_FUNCTION_LIST_DIR` across in-tree, source-tree, and install layouts. FreeRTOS C zenoh talker migrated to use `NROS_APP_CONFIG.zenoh.locator` / `.domain_id`.
- [ ] **112.D.2** Zephyr variant — read Kconfig values into the same struct
- [ ] **112.D.3** Drop `target_compile_definitions(... APP_*)` blocks. **Deferred** — `startup.c` per-example-compiled, lwIP/netif still wants `APP_IP`/`APP_MAC` macros. Phase 116 (`nano-ros.toml`) cleans this up.
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
