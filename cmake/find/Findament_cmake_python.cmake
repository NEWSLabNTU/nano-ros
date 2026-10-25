# No-op find module for ament_cmake_python — phase-488; part of nano-ros's CMake package.
# A cmake-only helper package (Nav2's `nav2_common`) finds it to install a
# Python module. nano-ros has no install layout, so `ament_python_install_package`
# (NanoRosAmentSurface.cmake) installs nothing. Without this module the call fell
# through to the installed ROS package in CONFIG mode, whose extras need ament
# internals ("Unknown CMake command get_executable_path").
set(ament_cmake_python_FOUND TRUE)
