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
/// Returns 0 on success, non-zero on failure. The `int main` shims return
/// it; the `void app_main(void)` shim hands it to the board through
/// `nros_app_main_returned()` (issue 1752), which a host-process board turns
/// into the process exit status and an MCU board ignores.
int nros_app_main(int argc, char** argv);

/// The VOID shim's status hand-off (issue 1752) — see the note above
/// `NROS_APP_MAIN_REGISTER_VOID`. `exit_code` is
/// `nros_app_exit_code(nros_app_main(...))`.
void nros_app_main_returned(int exit_code);

#ifdef __cplusplus
}
#endif

/* ---- Portable connect defaults (phase-287 W6) ----
 *
 * One example source builds native AND embedded: on the host these fall back
 * to "let the backend decide" / domain 0 (with `$NROS_LOCATOR` /
 * `$ROS_DOMAIN_ID` env overrides applied by the example before consulting
 * them); on an embedded board the build bakes both as target compile
 * definitions (NanoRosEntry.cmake board gate — e.g. `tcp/10.0.2.2:7447` for
 * QEMU slirp), so the `#ifndef` defaults below never fire there.
 *
 * phase-432 (W3.1 prerequisite) — this used to define both macros HERE, as
 * `""` and `0`, with no derivation, while `<nros/main.hpp>` derived them from
 * Kconfig. Same two names, two answers: a TU that saw only this header dialled
 * nothing on a board whose locator comes from `CONFIG_NROS_ZENOH_LOCATOR`.
 * That is free today only because an embedded C entry is routed to the C++
 * emitter; it stops being free the moment a pure C entry exists, and it would
 * arrive as issue #174's silent no-connect rather than as an error.
 *
 * One ladder now, in a C header both entry languages include. Issue 0330's
 * rule still holds and lives there: the bottom rung is the EMPTY STRING, not a
 * router endpoint, and the macro must keep expanding to a string literal
 * because callers pass it by VALUE rather than as a nullable pointer. */
#include "nros/entry_config.h"

/* ---- Platform-specific entry shims ---- */

/* C++ files want `extern "C"` linkage on the platform entry symbol so
 * the kernel/RTOS can find it. Plain C files don't need the qualifier
 * (and C compilers reject it). */
#ifdef __cplusplus
#define NROS_APP_MAIN_LINKAGE extern "C"
#else
#define NROS_APP_MAIN_LINKAGE
#endif

/* Issue 1752 — the VOID shim's status hand-off.
 *
 * `void app_main(void)` has nowhere to return a status, and this shim used to
 * DISCARD `nros_app_main`'s, so a host-process board (freertos-posix,
 * threadx-linux) that ends the process when the app returns could only say
 * `exit(0)` — a failed app reported success. The shim now hands the status to
 * `nros_app_main_returned()` before `app_main` returns. The board that CALLS
 * `app_main` defines it (strongly) and acts on the value once the call
 * returns; every other board gets the shim's own WEAK no-op, which is the right
 * answer on an MCU where nothing reads a status.
 *
 * Why a weak DEFINITION here rather than a weak REFERENCE in the board: a
 * weak-undefined function resolves to address 0, which RISC-V's
 * `R_RISCV_PCREL_HI20` cannot reach from `.text` at 0x80000000 (the note in
 * `nros-board-common/c/threadx_hooks.c`). A weak definition never leaves a
 * reference undefined, so every target links. The contract for a strong
 * definition: it lives in the SAME object as the board's call to `app_main` —
 * a linker never pulls an archive member just to replace a weak definition, so
 * a recorder in a member nothing else needs would silently lose to the no-op.
 *
 * The value handed over is an exit code, normalised once by
 * `nros_app_exit_code()`: 0 if and only if `nros_app_main` returned 0. A raw
 * `exit(status)` keeps only the low byte, so a status of 256 would have read
 * as success; it is folded to 1. */
static inline int nros_app_exit_code(int status) {
    if (status == 0) {
        return 0;
    }
    return (status & 0xff) != 0 ? (status & 0xff) : 1;
}

#define NROS_APP_MAIN_REGISTER_VOID()                                                              \
    NROS_APP_MAIN_LINKAGE __attribute__((weak)) void nros_app_main_returned(int exit_code) {       \
        (void)exit_code;                                                                           \
    }                                                                                              \
    NROS_APP_MAIN_LINKAGE void app_main(void) {                                                    \
        nros_app_main_returned(nros_app_exit_code(nros_app_main(0, (char**)0)));                   \
    }

#define NROS_APP_MAIN_REGISTER_ZEPHYR()                                                            \
    NROS_APP_MAIN_LINKAGE int main(void) {                                                         \
        return nros_app_main(0, (char**)0);                                                        \
    }

#define NROS_APP_MAIN_REGISTER_POSIX()                                                             \
    NROS_APP_MAIN_LINKAGE int main(int argc, char** argv) {                                        \
        return nros_app_main(argc, argv);                                                          \
    }

/* Phase 157 — NuttX external-app entry point. NuttX's
 * `apps/Application.mk` defines `-Dmain=<PROGNAME>_main` so the
 * `int main(int argc, char** argv)` symbol below gets renamed to
 * `<PROGNAME>_main` at compile time, which is what nshlib expects
 * for built-in command registration. Identical to the POSIX
 * variant — the rename trick is what makes it work. */
#define NROS_APP_MAIN_REGISTER_NUTTX()                                                             \
    NROS_APP_MAIN_LINKAGE int main(int argc, char** argv) {                                        \
        return nros_app_main(argc, argv);                                                          \
    }

/* Auto-detect the right shim. Users who want a different choice
 * invoke one of the explicit `NROS_APP_MAIN_REGISTER_*` macros above
 * directly instead of `NROS_APP_MAIN_REGISTER()`. */
#if defined(__ZEPHYR__)
#define NROS_APP_MAIN_REGISTER() NROS_APP_MAIN_REGISTER_ZEPHYR()
#elif defined(NROS_HOST_POSIX)
#define NROS_APP_MAIN_REGISTER() NROS_APP_MAIN_REGISTER_POSIX()
#elif defined(NROS_NUTTX_EXTERNAL_APP)
#define NROS_APP_MAIN_REGISTER() NROS_APP_MAIN_REGISTER_NUTTX()
#else
#define NROS_APP_MAIN_REGISTER() NROS_APP_MAIN_REGISTER_VOID()
#endif

#endif /* NROS_APP_MAIN_H */
