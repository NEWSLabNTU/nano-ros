/* Issue 1752 — a board that does NOT take the status hook (every MCU board).
 *
 * Linked beside `app_main_status_app.c` with no `nros_app_main_returned`
 * definition of its own: the shim's WEAK no-op must satisfy the link, so the
 * hook costs a board that ignores the status nothing. A link failure here is
 * the regression (the shim calling a hook only some boards define). */

#include <stdio.h>

void app_main(void);
extern int app_main_status_probe_rc;

int main(void) {
    app_main_status_probe_rc = 7;
    app_main();
    printf("APP-STATUS-WEAK-DEFAULT-LINKED\n");
    return 0;
}
