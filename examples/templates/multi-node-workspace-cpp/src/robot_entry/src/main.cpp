// Entry pkg — boots the `demo_bringup` topology against the native board,
// the TYPED way (RFC-0043).
//
// `nano_ros_entry(... TYPED LAUNCH "demo_bringup:system.launch.xml")` drives
// `nros codegen entry --lang cpp --typed --metadata …` at configure time. The
// generated TU constructs each launch node's C++ component object, calls
// `configure(node)` (binds the real member callbacks by identity), and hands
// the setup fn to `NativeBoard::run_components` (init → setup → spin_once loop
// → shutdown) — the REAL executor, no synthesizing interpreter.
//
// `NROS_MAIN(...)` here expands to nothing functional (a doc / IDE hint that
// mirrors the Rust `nros::main!(launch = "…")` shape); the cmake fn generates
// the actual `int main()`.

#include <nros/main.hpp>

NROS_MAIN(::nros::board::NativeBoard, "demo_bringup:system.launch.xml");
