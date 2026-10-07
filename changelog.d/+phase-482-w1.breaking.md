`cmake/compat/` is gone. What it held is now part of nano-ros's own CMake
package, under new paths:

- `cmake/compat/NrosRclcppCompat.cmake` → `cmake/NanoRosAmentSurface.cmake`
  (the `ament_*` and `rclcpp_components_register_node` verbs);
- `cmake/compat/stubs/` → `cmake/find/` (the `Find<pkg>.cmake` modules);
- `cmake/compat/diagnostic-updater/` → `packages/api/nros-diagnostic-updater/`;
- `<rclcpp/rclcpp.hpp>` and `<rclcpp_components/register_node_macro.hpp>` are
  nros-cpp headers, on the include path of every C++ target that links
  nano-ros.

**If your `CMakeLists.txt` includes `cmake/compat/NrosRclcppCompat.cmake`,
change the path to `cmake/NanoRosAmentSurface.cmake`.** A project that reaches
nano-ros through `find_package(nano_ros)` or a nano-ros workspace changes
nothing. `nros/rclcpp_components_compat.hpp` is deleted; include
`<rclcpp_components/register_node_macro.hpp>`, as upstream code already does.
