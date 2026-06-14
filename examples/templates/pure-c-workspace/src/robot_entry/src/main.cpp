// Entry pkg — boots the `demo_bringup` topology against the native board, the
// TYPED way (RFC-0043, phase-247).
//
// All node pkgs in this workspace are pure C (NROS_C_COMPONENT), but the typed
// carrier entry TU is C++ by design — `NativeBoard::run_components` (the real
// executor: init -> setup -> spin_once loop -> shutdown) lives in C++, and the
// generated TU constructs each C node via its C-ABI factory + configure seam
// (`__nros_c_component_<pkg>_{create,configure}`). This mirrors every typed C
// carrier in the tree (e.g. the single-package `nano_ros_node_register(TYPED
// LANGUAGE C)` path also emits a C++ entry TU). No interpreter, no callback
// names.
//
// `nano_ros_entry(... TYPED LAUNCH "demo_bringup:system.launch.xml")` drives
// `nros codegen entry --lang cpp --typed --metadata …` at configure time, which
// generates the real `int main()`. `NROS_MAIN(...)` here is a doc / IDE hint.

#include <nros/main.hpp>

NROS_MAIN(::nros::board::NativeBoard, "demo_bringup:system.launch.xml");
