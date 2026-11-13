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
#include <errno.h>
#include <stddef.h>
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

/*
 * newlib's heap (phase-553 W7, issue 2494). `malloc` grows its heap through
 * `_sbrk`, from `end` (the end of the statics) upward. rdimon's `_sbrk` is
 * weak and stops only at the CURRENT stack pointer, so with a shallow stack
 * it hands out memory inside the main stack's region, which the stack later
 * overwrites with no diagnostic. This one stops at the stack's guard, which
 * `memory.x` places below the stated stack.
 */
extern char end;
extern char _nros_stack_guard;

void* _sbrk(ptrdiff_t incr) {
    static char* heap_end;
    if (heap_end == NULL) {
        heap_end = &end;
    }
    char* prev = heap_end;
    if (incr > 0 ? (size_t)incr > (size_t)(&_nros_stack_guard - heap_end)
                 : (size_t)(-incr) > (size_t)(heap_end - &end)) {
        errno = ENOMEM;
        return (void*)-1;
    }
    heap_end += incr;
    return prev;
}

int main(void) {
    initialise_monitor_handles();
    nros_board_mps2_an385_start(nros_board_run_app);
}
