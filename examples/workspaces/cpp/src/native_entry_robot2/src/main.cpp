// phase-263 Track C — robot2 per-host entry. The HOST robot2 codegen partition keeps
// only the listener, so this boots the listener alone on the native board.

#include <nros/main.hpp>

NROS_MAIN(::nros::board::NativeBoard, "demo_bringup:multihost.launch.xml");
