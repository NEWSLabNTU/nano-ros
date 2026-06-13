/* §212.L.9 metadata fixture (issue 0041) — a trivial, include-free C TU.
 * The test (`cmake_node_register_metadata`) inspects the `nros-metadata.json`
 * that `nano_ros_node_register(... LANGUAGE C ...)` emits at CONFIGURE time;
 * the compiled object's content is irrelevant — it only has to compile. It
 * must NOT `#include <nros/node_pkg.h>`: this fixture deliberately does not
 * `find_package`/`add_subdirectory` nano-ros, so no `NanoRos::NanoRos` target
 * carries that header's include dir (mirrors the include-free `dummy.cpp`). */
int phase212_l9_c_stub(void) { return 0; }
