/**
 * @file app_config.h
 * @brief Typed application configuration struct (`NROS_APP_CONFIG`).
 *
 * Phase 112.D contract: every nros example exposes a `static const
 * nros_app_config_t NROS_APP_CONFIG` populated from the example's own
 * configuration source. User code reads fields like
 * `NROS_APP_CONFIG.zenoh.locator` or `NROS_APP_CONFIG.network.ip`
 * uniformly across platforms.
 *
 * Two binding paths:
 *
 *   1. **CMake codegen** (FreeRTOS, NuttX, ThreadX, native) — examples
 *      call `nano_ros_generate_config_header()` to emit a per-binary
 *      `<build_dir>/include/nros/app_config.h` from a `config.toml`.
 *      That generated header takes precedence over this shipped file
 *      because the build dir is added before NanoRos's install include
 *      path.
 *
 *   2. **Zephyr Kconfig** (this file's `__ZEPHYR__` branch) — Kconfig
 *      values become `CONFIG_NROS_*` preprocessor macros. We synthesize
 *      `NROS_APP_CONFIG` from those at compile time, no codegen step.
 *
 * Out-of-tree consumers wanting a different binding path provide their
 * own `<nros/app_config.h>` earlier on the include path; this header is
 * the fallback.
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_APP_CONFIG_H
#define NROS_APP_CONFIG_H

#include <stdint.h>

typedef struct {
    struct {
        const char* locator;
        uint32_t domain_id;
    } zenoh;
    struct {
        uint8_t ip[4];
        uint8_t mac[6];
        uint8_t gateway[4];
        uint8_t netmask[4];
        uint8_t prefix;
    } network;
    struct {
        uint32_t app_priority;
        uint32_t zenoh_read_priority;
        uint32_t zenoh_lease_priority;
        uint32_t poll_priority;
        uint32_t app_stack_bytes;
        uint32_t zenoh_read_stack_bytes;
        uint32_t zenoh_lease_stack_bytes;
        uint32_t poll_interval_ms;
    } scheduling;
} nros_app_config_t;

#ifdef __ZEPHYR__
/* Phase 112.D.2 — Kconfig binding. Zephyr examples set CONFIG_NROS_*
 * via prj.conf; we synthesize NROS_APP_CONFIG without an extra codegen
 * step. Defaults cover values the user can omit.
 *
 * Zenoh / XRCE locator: prefer the explicit zenoh locator string, else
 * synthesize one from the XRCE agent address (UDP). DDS examples leave
 * it empty (RTPS is locator-less). */
#include <autoconf.h>

#ifndef CONFIG_NROS_ZENOH_LOCATOR
#define CONFIG_NROS_ZENOH_LOCATOR ""
#endif
#ifndef CONFIG_NROS_DOMAIN_ID
#define CONFIG_NROS_DOMAIN_ID 0
#endif

#if defined(__GNUC__) || defined(__clang__)
__attribute__((unused))
#endif
static const nros_app_config_t NROS_APP_CONFIG = {
    .zenoh =
        {
            .locator = CONFIG_NROS_ZENOH_LOCATOR,
            .domain_id = CONFIG_NROS_DOMAIN_ID,
        },
    /* Zephyr provides network configuration via DTS / NET_CONFIG; the
     * struct stays present so portable code references it uniformly,
     * but the values default to zero — examples that need static IP
     * read from device-tree / Kconfig directly. */
    .network = {{0}, {0}, {0}, {0}, 0},
    .scheduling = {0, 0, 0, 0, 0, 0, 0, 0},
};

#else /* !__ZEPHYR__ */
/* Phase 212.M-F.10 Path C — non-Zephyr branch.
 *
 * The universal `NROS_APP_CONFIG` read promise is preserved across
 * platforms (every example reads `NROS_APP_CONFIG.zenoh.locator` /
 * `.network.ip` / etc. with the same field paths). What moved in
 * M-F.10 is who POPULATES the symbol:
 *
 *   - Before: per-binary `app_config.h` was emitted by the cmake
 *     codegen `nano_ros_generate_config_header()` from a per-example
 *     `nros.toml`, baking a TU-local `static const` at every include
 *     site.
 *
 *   - After: each board crate's `build.rs` emits a one-shot
 *     `const nros_app_config_t NROS_APP_CONFIG = { ... };` translation
 *     unit (from the board's default Rust `Config`) baked into the
 *     board's staticlib. This header declares the symbol as `extern`
 *     so any TU that includes `<nros/app_config.h>` and references
 *     `NROS_APP_CONFIG.*` resolves it at link time against that
 *     board-emitted definition.
 *
 * During the M-F.10.3 → M-F.10.5 transition the legacy cmake codegen
 * path may still emit a per-binary header earlier on the include
 * path; that header carries its own `static const` initialiser and
 * shadows this `extern` declaration. The two paths coexist until
 * M-F.10.5 retires the codegen.
 *
 * Out-of-tree consumers that want their own population path can drop
 * a `<nros/app_config.h>` earlier on the include path (same escape
 * hatch as before). */
extern const nros_app_config_t NROS_APP_CONFIG;
#endif /* __ZEPHYR__ */

#endif /* NROS_APP_CONFIG_H */
