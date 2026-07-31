#include <nros/main.h>

/* phase-326 (issue 0364) — robot2 per-host entry. The per-host model (resolved with
 * `host:=robot2`) keeps
 * only the listener, so this boots the listener alone on the native board. */
NROS_MAIN_C(nros_board_native, "demo_bringup:multihost.launch.xml");
