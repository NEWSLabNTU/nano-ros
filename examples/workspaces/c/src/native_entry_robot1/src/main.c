#include <nros/main.h>

/* phase-326 (issue 0364) — robot1 per-host entry. The per-host model (resolved with
 * `host:=robot1`) keeps
 * only the talker, so this boots the talker alone on the native board. */
NROS_MAIN_C(nros_board_native, "demo_bringup:multihost.launch.xml");
