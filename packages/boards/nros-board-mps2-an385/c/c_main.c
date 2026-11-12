/*
 * The `main` of a C-rooted mps2-an385 image (RFC-0130 D2, phase-553 W4).
 *
 * cortex-m-rt (in the image's Rust staticlib) owns `Reset`: it initialises
 * `.data`/`.bss` and calls `main`. This `main` is the board's, as an RTOS
 * board's `startup.c` is: it hands the generated entry's `app_main` to the
 * board's C-ABI start, which runs the C/C++ static constructors, brings the
 * clock, semihosting log and network up, runs the application, reports its
 * result and halts. It never returns.
 *
 * The generated entry defines `app_main` (`NROS_APP_MAIN_REGISTER_VOID`) and
 * reports the runner's exit code through the weak `nros_app_main_returned`
 * hook; the strong definition here carries it back to the board, so a failed
 * image is reported as one.
 */
#include <stdint.h>

extern void app_main(void);
extern void nros_board_mps2_an385_start(int (*app)(void)) __attribute__((noreturn));
/* newlib's semihosting (rdimon) stdio: C `printf` reaches the host console. */
extern void initialise_monitor_handles(void);

static int nros_board_app_exit_code;

void nros_app_main_returned(int exit_code) {
    nros_board_app_exit_code = exit_code;
}

static int nros_board_run_app(void) {
    app_main();
    return nros_board_app_exit_code;
}

int main(void) {
    initialise_monitor_handles();
    nros_board_mps2_an385_start(nros_board_run_app);
}
