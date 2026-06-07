// Entry pkg — boots the `demo_bringup` topology against the native
// board.
//
// Phase 219.D body collapses to one declarative line. The cmake fn
// `nano_ros_entry(LAUNCH "demo_bringup:system.launch.xml")` drives
// `nros codegen entry --lang cpp` at configure time, emits the real
// `int main()` body into `${CMAKE_CURRENT_BINARY_DIR}/native_entry_nros_main_generated.cpp`,
// and auto-links the matching Node-pkg static libs. Their target names
// still end in `_component` for ABI compatibility.
//
// The `NROS_MAIN(...)` macro here expands to nothing functional — it's
// a doc / IDE hint that mirrors the Rust `nros::main!(launch = "…")`
// shape; the cmake fn is what actually generates code.

#include <nros/main.hpp>

NROS_MAIN(::nros::board::NativeBoard, "demo_bringup:system.launch.xml");
