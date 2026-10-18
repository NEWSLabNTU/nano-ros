/**
 * @file app_config.h
 * @brief Typed application configuration struct (`NROS_APP_CONFIG`).
 *
 * Phase 112.D contract: every nros example exposes a `static const
 * nros_app_config_t NROS_APP_CONFIG` populated from the example's own
 * configuration source. User code reads fields like
 * `NROS_APP_CONFIG.transport.locator` or `NROS_APP_CONFIG.network.ip`
 * uniformly across platforms.
 *
 * Two binding paths:
 *
 *   1. **Board crate `build.rs` emission** (FreeRTOS, NuttX, ThreadX,
 *      native — Path C post-Phase 212.M-F.10): each board crate's
 *      `build.rs` writes a generated `nros_app_config_def.c` into
 *      `$OUT_DIR/` defining `const nros_app_config_t NROS_APP_CONFIG`
 *      with values from the board's Rust `Config` defaults. The C
 *      definition lands in the board's staticlib; linker resolves
 *      the `extern` declaration in this header against that symbol.
 *      Pre-212.M-F.10 the same role was filled by the retired
 *      `cmake/NanoRosConfig.cmake::nano_ros_generate_config_header()`
 *      codegen path; Path C moved it to source-side emission.
 *
 *   2. **Zephyr Kconfig** (this file's `__ZEPHYR__` branch) — Kconfig
 *      values become `CONFIG_NROS_*` preprocessor macros. We synthesize
 *      `NROS_APP_CONFIG` from those at compile time, no codegen step.
 *
 * Out-of-tree consumers wanting a different binding path provide their
 * own definition site (a `.c` TU that defines `const nros_app_config_t
 * NROS_APP_CONFIG = { ... };`) before this header's `extern` decl is
 * resolved at link time.
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
    } transport;
    struct {
        uint8_t ip[4];
        uint8_t mac[6];
        uint8_t gateway[4];
        uint8_t netmask[4];
        uint8_t prefix;
    } network;
    struct {
        uint32_t app_priority;
        uint32_t rx_priority;
        uint32_t keepalive_priority;
        uint32_t poll_priority;
        uint32_t app_stack_bytes;
        uint32_t rx_stack_bytes;
        uint32_t keepalive_stack_bytes;
        uint32_t poll_interval_ms;
    } scheduling;
} nros_app_config_t;

#ifdef __ZEPHYR__
/* Phase 112.D.2 — Kconfig binding. Zephyr examples set CONFIG_NROS_*
 * via prj.conf; we synthesize NROS_APP_CONFIG without an extra codegen
 * step. Defaults cover values the user can omit.
 *
 * RFC-0071 D10 (phase-506 W3) — the locator and domain are the ROLES
 * `entry_config.h` resolves (`NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID`),
 * which each backend's Zephyr module maps its own Kconfig onto. This header
 * names no backend; it read `CONFIG_NROS_ZENOH_LOCATOR` here, so an XRCE or
 * Cyclone image's `.transport.locator` was always "". */
#include <autoconf.h>
#include <rclc/detail/entry_config.h>

#if defined(__GNUC__) || defined(__clang__)
__attribute__((unused))
#endif
static const nros_app_config_t NROS_APP_CONFIG = {
    .transport =
        {
            .locator = NROS_ENTRY_LOCATOR,
            .domain_id = NROS_ENTRY_DOMAIN_ID,
        },
    /* Zephyr provides network configuration via DTS / NET_CONFIG; the
     * struct stays present so portable code references it uniformly,
     * but the values default to zero — examples that need static IP
     * read from device-tree / Kconfig directly. */
    .network = {{0}, {0}, {0}, {0}, 0},
    .scheduling = {0, 0, 0, 0, 0, 0, 0, 0},
};

#else  /* !__ZEPHYR__ */
/* Phase 212.M-F.10 Path C — non-Zephyr branch.
 *
 * The universal `NROS_APP_CONFIG` read promise is preserved across
 * platforms (every example reads `NROS_APP_CONFIG.transport.locator` /
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
 *     so any TU that includes `<rclc/detail/app_config.h>` and references
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
 * a `<rclc/detail/app_config.h>` earlier on the include path (same escape
 * hatch as before). */
extern const nros_app_config_t NROS_APP_CONFIG;
#endif /* __ZEPHYR__ */

#endif /* NROS_APP_CONFIG_H */
