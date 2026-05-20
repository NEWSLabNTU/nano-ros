#ifndef NROS_BOARD_H
#define NROS_BOARD_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @file board.h
 * @brief Canonical C ABI for the nros board-entry layer.
 *
 * A board implementor supplies the symbols declared in this header.
 * Every nros firmware links exactly one board implementation;
 * resolution is performed at link time. There is no runtime
 * registration step. The board layer sits one tier above the
 * platform layer (`<nros/platform.h>`): the platform provides system
 * primitives (clock, alloc, threading); the board provides the
 * *entry workflow* — hardware bring-up, status output, and process
 * exit — that drives the user application.
 *
 * Implementations may be written in any language with a C ABI. For
 * Rust board crates, the `nros_board_export!` macro in
 * `nros-board-cffi` re-emits a `Board` trait impl as
 * `#[unsafe(no_mangle)] extern "C"` symbols matching the names below.
 *
 * # The config pointer
 *
 * `cfg` is an opaque pointer to a board-specific configuration object
 * the board implementation understands (peripheral selection, IP /
 * baud-rate settings, RMW binding — Phase 173.5). The generic ABI
 * never inspects it; the board casts it back to its concrete type.
 * Board crates expose their own C constructor for the config (e.g.
 * `nros_board_<name>_config_from_toml`); building that object is out
 * of scope for this generic header.
 *
 * # noreturn
 *
 * `nros_board_run`, `nros_board_exit_success`, and
 * `nros_board_exit_failure` never return: control terminates the
 * firmware (semihosting exit, chip reset, `wfi` halt, or
 * `process::exit`, board's choice).
 */

/**
 * User application entry. Receives the `user` cookie passed to
 * `nros_board_run`. Returns `0` on success, non-zero on error; the
 * board maps the result onto `exit_success` / `exit_failure`.
 */
typedef int32_t (*nros_board_app_fn)(void *user);

/* ---- Entry workflow ---- */

/**
 * Direct-exec board entry driver: run hardware init, invoke the user
 * application, then exit. Used by direct-exec board families
 * (bare-metal, esp-hal) where the application runs on the boot stack.
 * Kernel-spawn families (FreeRTOS, ThreadX) ship their own task-based
 * driver and do not export this symbol. Never returns.
 */
void nros_board_run(const void *cfg, nros_board_app_fn app, void *user);

/* ---- Board primitives ---- */

/** Run board-specific hardware init (clock tree, pin mux, peripheral
 *  wakes) for the given opaque config. */
void nros_board_init_hardware(const void *cfg);

/** Emit one status / banner / error line. `msg` is a UTF-8 byte slice
 *  of `len` bytes (no trailing newline required; the board appends
 *  one). */
void nros_board_println(const uint8_t *msg, size_t len);

/** Terminate the firmware after a successful run. Never returns. */
void nros_board_exit_success(void);

/** Terminate the firmware after a failed run. Never returns. */
void nros_board_exit_failure(void);

#ifdef __cplusplus
}
#endif

#endif /* NROS_BOARD_H */
