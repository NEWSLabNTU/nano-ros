# No-op find module for rcutils — phase-488; part of nano-ros's CMake package.
# ros2/demos `composition` finds it and uses none of its API. nano-ros does not
# ship rcutils, so a source that includes `<rcutils/...>` fails on that include,
# naming it. Without this module `find_package(rcutils)` fell through to the
# installed ROS package in CONFIG mode, whose extras need ament internals
# ("Unknown CMake command normalize_path"), before any source was compiled.
set(rcutils_FOUND TRUE)
