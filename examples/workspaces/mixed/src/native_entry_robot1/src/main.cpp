// phase-263 Track C — robot1 per-host entry. The HOST robot1 codegen partition keeps
// the C talker + Rust heartbeat, booting them on the native board.

#include <nros/main.hpp>

NROS_MAIN(::nros::board::NativeBoard, "demo_bringup:multihost.launch.xml");
