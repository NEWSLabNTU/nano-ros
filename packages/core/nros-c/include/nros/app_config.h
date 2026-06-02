/**
 * @file app_config.h
 * @brief Universal `NROS_APP_CONFIG` surface — `<nros/app_config.h>`.
 *
 * Phase 212.M-F.10.1 — this is the canonical path consumers reach via
 * `#include <nros/app_config.h>`. The shipped fallback header lives at
 * `nros/zephyr/app_config.h` (named for historical reasons — Phase
 * 112.D moved it there to avoid a shadowing collision with the
 * cmake-codegen per-binary `<build_dir>/include/nros/app_config.h`,
 * see commit a06948c44). This wrapper re-exports it on the canonical
 * `<nros/app_config.h>` path so non-Zephyr board startup TUs (which
 * include `<nros/app_config.h>`) resolve to the same struct + extern
 * declaration as the Zephyr Kconfig synthesis path.
 *
 * Both paths now share one source of truth (`nros/zephyr/app_config.h`):
 *   - Under `__ZEPHYR__`, the Kconfig branch synthesises a TU-local
 *     `static const NROS_APP_CONFIG` from `CONFIG_NROS_*` macros.
 *   - Otherwise, the non-Zephyr branch declares
 *     `extern const nros_app_config_t NROS_APP_CONFIG;`, resolved at
 *     link time against each board crate's build.rs-emitted
 *     definition (Path C, M-F.10.3).
 *
 * During the M-F.10.3 → M-F.10.5 transition the legacy cmake codegen
 * still emits a per-binary `<build_dir>/include/nros/app_config.h`
 * earlier on the include path; that header shadows this wrapper +
 * carries its own `static const` initialiser. Both paths coexist
 * until M-F.10.5 retires the codegen.
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_APP_CONFIG_H
/* Delegate to the canonical fallback. The included header defines
 * `NROS_APP_CONFIG_H` itself, so this guard is a no-op safeguard
 * against double-include of the wrapper. */
#include <nros/zephyr/app_config.h>
#endif /* NROS_APP_CONFIG_H */
