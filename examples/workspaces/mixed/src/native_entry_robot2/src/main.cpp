// phase-326 (issue 0364) — robot2 per-host entry. The per-host model (resolved with
// `host:=robot2`) keeps
// only the C++ listener, booting it on the native board.

#include <nros/main.hpp>

NROS_MAIN(::nros::board::LinuxBoard, "demo_bringup:multihost.launch.xml");
