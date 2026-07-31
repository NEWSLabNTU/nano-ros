// phase-326 (issue 0364) — robot1 per-host entry. The per-host model (resolved with
// `host:=robot1`) keeps
// only the talker, so this boots the talker alone on the native board.

#include <nros/main.hpp>

NROS_MAIN(::nros::board::NativeBoard, "demo_bringup:multihost.launch.xml");
