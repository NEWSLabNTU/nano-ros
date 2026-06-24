#include <nros/main.h>

/* phase-263 Track C — robot1 per-host entry. The HOST robot1 codegen partition keeps
 * only the talker, so this boots the talker alone on the native board. */
NROS_MAIN_C(nros_board_native, "demo_bringup:multihost.launch.xml");
