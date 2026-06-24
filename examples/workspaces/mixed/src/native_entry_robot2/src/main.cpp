// phase-263 Track C — robot2 per-host entry. The HOST robot2 codegen partition keeps
// only the C++ listener, booting it on the native board.

#include <nros/main.hpp>

NROS_MAIN(::nros::board::NativeBoard, "demo_bringup:multihost.launch.xml");
