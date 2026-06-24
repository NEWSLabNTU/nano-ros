// phase-263 Track C — robot1 per-host entry. The HOST robot1 codegen partition keeps
// only the talker, so this boots the talker alone on the native board.

#include <nros/main.hpp>

NROS_MAIN(::nros::board::NativeBoard, "demo_bringup:multihost.launch.xml");
