/* Issue 1752 — the BOARD half of the VOID entry shim's status probe.
 *
 * A host-process board (freertos-posix, threadx-linux) calls `app_main()` and
 * then ends the process; the exit status it can report is only what the shim
 * hands it through `nros_app_main_returned()`. Before the fix the shim
 * discarded `nros_app_main`'s status, so this recorder was never called and
 * every case below read the SENTINEL — the probe's negative control is that
 * old header, which fails here on the first case.
 *
 * Deliberately does NOT include `<nros/app_main.h>`: the board files that take
 * the hook (`threadx_hooks.c` is also built on the Rust lane, with no nros-c
 * include path) declare the prototype themselves, so this one does too. */

#include <stdio.h>

void app_main(void);
void nros_app_main_returned(int exit_code);
extern int app_main_status_probe_rc;

#define SENTINEL (-12345)
static int recorded = SENTINEL;

void nros_app_main_returned(int exit_code) { recorded = exit_code; }

int main(void) {
    /* status nros_app_main returns -> exit code the board must be handed */
    static const struct {
        int status;
        int exit_code;
    } cases[] = {
        {0, 0},
        {1, 1},
        {3, 3},
        /* NROS_RET_* are negative; exit(2) keeps the low byte. */
        {-4, 252},
        /* low byte 0: a raw `exit(256)` would read as SUCCESS. */
        {256, 1},
    };
    int failures = 0;
    for (unsigned i = 0; i < sizeof(cases) / sizeof(cases[0]); i++) {
        recorded = SENTINEL;
        app_main_status_probe_rc = cases[i].status;
        app_main();
        if (recorded != cases[i].exit_code) {
            fprintf(stderr,
                    "FAIL: nros_app_main returned %d; the board was handed %d, expected %d%s\n",
                    cases[i].status, recorded, cases[i].exit_code,
                    recorded == SENTINEL ? " (the shim never called nros_app_main_returned)"
                                         : "");
            failures++;
        }
    }
    if (failures != 0) {
        return 1;
    }
    printf("APP-STATUS-CARRIED\n");
    return 0;
}
