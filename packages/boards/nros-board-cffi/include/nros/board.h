#ifndef NROS_BOARD_H
#define NROS_BOARD_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* RFC-0054: `noreturn` is part of the contract (the Rust bindings are
 * generated from this header, and divergence must survive as `-> !`).
 * C11 `_Noreturn` / C++11 `[[noreturn]]` / GNU-attribute fallback. */
#if defined(__cplusplus)
#define NROS_BOARD_NORETURN [[noreturn]]
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
#define NROS_BOARD_NORETURN _Noreturn
#else
#define NROS_BOARD_NORETURN __attribute__((noreturn))
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

/* ---- Image identity ---- */

/** The `version` a caller of this header writes into `nros_board_image_config_t`. */
#define NROS_BOARD_IMAGE_CONFIG_VERSION 1

/** `nros_board_image_config_t::domain_id` value meaning "the board's default". */
#define NROS_BOARD_IMAGE_DOMAIN_DEFAULT (-1)

/**
 * The image's identity on the network, handed to a C-rooted board's start at
 * RUN time (issue 2555). Each image's generated entry fills one from its own
 * `system.toml`, so two images built in one configure (one runtime archive)
 * still boot as two hosts. A board reads what it can use and keeps its own
 * default for every field left unset.
 *
 * - `version`: `NROS_BOARD_IMAGE_CONFIG_VERSION`. A board refuses a version it
 *   does not know rather than reading fields past the struct's end.
 * - `net`: the image's resolved network, in the `NROS_IMAGE_NET` text form
 *   (RFC-0103 D10f; `nros_board_common::image_net`). `NULL`: the board's.
 * - `locator`: the RMW locator the image dials. `NULL`: the board's.
 * - `domain_id`: the ROS domain id, or `NROS_BOARD_IMAGE_DOMAIN_DEFAULT`.
 *
 * Every string is NUL-terminated and must outlive the image: the generated
 * entry passes string literals.
 */
typedef struct nros_board_image_config_t {
    uint32_t version;
    const char *net;
    const char *locator;
    int32_t domain_id;
} nros_board_image_config_t;

/* ---- Entry workflow ---- */

/**
 * Board entry driver: run the board's full boot flow with the user
 * application, then exit. Family-agnostic — for direct-exec boards
 * (bare-metal, esp-hal) the application runs on the boot stack; for
 * kernel-spawn families (FreeRTOS, ThreadX) the board allocates an app
 * task and starts the scheduler, running the application in task
 * context. Both shapes export this same symbol (the Rust side routes
 * through `BoardEntry::run`). Never returns.
 */
NROS_BOARD_NORETURN void nros_board_run(const void *cfg, nros_board_app_fn app, void *user);

/* ---- Board primitives ---- */

/** Run board-specific hardware init (clock tree, pin mux, peripheral
 *  wakes) for the given opaque config. */
void nros_board_init_hardware(const void *cfg);

/** Emit one status / banner / error line. `msg` is a UTF-8 byte slice
 *  of `len` bytes (no trailing newline required; the board appends
 *  one). */
void nros_board_println(const uint8_t *msg, size_t len);

/** Terminate the firmware after a successful run. Never returns. */
NROS_BOARD_NORETURN void nros_board_exit_success(void);

/** Terminate the firmware after a failed run. Never returns. */
NROS_BOARD_NORETURN void nros_board_exit_failure(void);

#ifdef __cplusplus
}
#endif

#endif /* NROS_BOARD_H */
