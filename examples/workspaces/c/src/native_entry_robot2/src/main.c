#include <nros/main.h>

/* phase-263 Track C — robot2 per-host entry. The HOST robot2 codegen partition keeps
 * only the listener, so this boots the listener alone on the native board. */
NROS_MAIN_C(nros_board_native, "demo_bringup:multihost.launch.xml");
