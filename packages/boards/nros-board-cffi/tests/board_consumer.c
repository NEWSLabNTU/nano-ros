/*
 * Phase 173.4 — C-consumer compile proof for <nros/board.h>.
 *
 * Compiled (to an object, no link) by tests/c_abi.rs. Validates that:
 *   - the header is valid C,
 *   - the five `nros_board_*` declarations parse,
 *   - the `nros_board_app_fn` typedef + `nros_board_run` callsite shape
 *     match how a standalone C application would drive a board.
 *
 * This is the C side of the ABI mirror: `tests/export_compiles.rs`
 * proves the Rust `nros_board_export!` macro *emits* the symbols; this
 * proves a C TU can *consume* them with matching signatures.
 */

#include <nros/board.h>

/* The user application body a C app would write. */
static int32_t my_app(void *user) {
    (void) user;
    /* Real apps call into the nros C API here (publishers, etc.). */
    return 0;
}

/* The standalone-C entry a board's startup would call. `cfg` is the
 * opaque board config pointer (built by the board's own C
 * constructor). Mirrors what generated `main`/startup does in Rust. */
void c_app_entry(const void *cfg, void *user) {
    /* Full driver: hardware init + app + exit, one call. */
    nros_board_run(cfg, my_app, user);
}

/* Exercise the individual primitives too, so their declarations are
 * type-checked against real callsites. */
void c_primitives(const void *cfg) {
    nros_board_init_hardware(cfg);
    const uint8_t banner[] = "hello from C";
    nros_board_println(banner, sizeof(banner) - 1);
    nros_board_exit_success();
}
