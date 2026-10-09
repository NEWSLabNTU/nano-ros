/* Issue 1752 — the APPLICATION half of the VOID entry shim's status probe.
 *
 * An app exactly as every RTOS example spells it: `nros_app_main` plus
 * `NROS_APP_MAIN_REGISTER_VOID()`. The status it returns is whatever the
 * board half put in `app_main_status_probe_rc`, so one image can drive the
 * shim through every case. Linked with `app_main_status_board.c` (a board that
 * defines the strong `nros_app_main_returned`) and with
 * `app_main_status_weak.c` (one that does not, so the shim's weak no-op must
 * satisfy the link). Built as C AND as C++ by the `check-c` lane, because the
 * C++ examples expand the same macro with `extern "C"` linkage. */

#include <nros/app_main.h>

#ifdef __cplusplus
extern "C" {
#endif
int app_main_status_probe_rc = 0;
#ifdef __cplusplus
}
#endif

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    return app_main_status_probe_rc;
}

NROS_APP_MAIN_REGISTER_VOID()
