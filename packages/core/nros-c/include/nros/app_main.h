/**
 * @file app_main.h
 * @brief Unified user-application entry point.
 *
 * Phase 112.C contract — every nros example, on every RTOS, defines:
 *
 *     int nros_app_main(int argc, char **argv);
 *
 * The `NROS_APP_MAIN_REGISTER()` macro at file scope emits the correct
 * platform entry shim (`void app_main(void)`, `int main(void)`, or
 * `int main(int argc, char **argv)`) that forwards into the user's
 * `nros_app_main`. User code is portable across RTOSes; only the
 * one-line registration knows which entry the linker expects.
 *
 * Platform selection (in order):
 *   1. `__ZEPHYR__` defined  → emits `int main(void)` (Zephyr kernel
 *      calls main directly).
 *   2. `NROS_HOST_POSIX` defined (set via `-DNROS_HOST_POSIX` from a
 *      native example's build system) → emits `int main(int argc,
 *      char **argv)` with full argv pass-through.
 *   3. Otherwise (FreeRTOS, NuttX, ThreadX, bare-metal) → emits
 *      `void app_main(void)`. Per-platform startup chains call this
 *      after platform init (network, executor arena, board hw).
 *
 * To opt out and pick the shim explicitly, define one of:
 *   `NROS_APP_MAIN_REGISTER_VOID`   — `void app_main(void)`
 *   `NROS_APP_MAIN_REGISTER_ZEPHYR` — `int main(void)`
 *   `NROS_APP_MAIN_REGISTER_POSIX`  — `int main(int argc, char **argv)`
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_APP_MAIN_H
#define NROS_APP_MAIN_H

#ifdef __cplusplus
extern "C" {
#endif

/// User application entry point. Define exactly once per binary.
///
/// Returns 0 on success, non-zero on failure (forwarded by the
/// platform shim where a return code matters; ignored on RTOS targets
/// where the entry shim returns void).
int nros_app_main(int argc, char **argv);

#ifdef __cplusplus
}
#endif

/* ---- Platform-specific entry shims ---- */

/* C++ files want `extern "C"` linkage on the platform entry symbol so
 * the kernel/RTOS can find it. Plain C files don't need the qualifier
 * (and C compilers reject it). */
#ifdef __cplusplus
#define NROS_APP_MAIN_LINKAGE extern "C"
#else
#define NROS_APP_MAIN_LINKAGE
#endif

#define NROS_APP_MAIN_REGISTER_VOID()                                                              \
    NROS_APP_MAIN_LINKAGE void app_main(void) {                                                    \
        (void)nros_app_main(0, (char **)0);                                                        \
    }

#define NROS_APP_MAIN_REGISTER_ZEPHYR()                                                            \
    NROS_APP_MAIN_LINKAGE int main(void) {                                                         \
        return nros_app_main(0, (char **)0);                                                       \
    }

#define NROS_APP_MAIN_REGISTER_POSIX()                                                             \
    NROS_APP_MAIN_LINKAGE int main(int argc, char **argv) {                                        \
        return nros_app_main(argc, argv);                                                          \
    }

/* Auto-detect the right shim. Users who want a different choice
 * invoke one of the explicit `NROS_APP_MAIN_REGISTER_*` macros above
 * directly instead of `NROS_APP_MAIN_REGISTER()`. */
#if defined(__ZEPHYR__)
#define NROS_APP_MAIN_REGISTER() NROS_APP_MAIN_REGISTER_ZEPHYR()
#elif defined(NROS_HOST_POSIX)
#define NROS_APP_MAIN_REGISTER() NROS_APP_MAIN_REGISTER_POSIX()
#else
#define NROS_APP_MAIN_REGISTER() NROS_APP_MAIN_REGISTER_VOID()
#endif

#endif /* NROS_APP_MAIN_H */
