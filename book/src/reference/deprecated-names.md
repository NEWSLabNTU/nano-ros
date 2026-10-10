# Deprecated names

nano-ros's cmake variables use one prefix, `NROS_`. Older releases used
`NANO_ROS_` for some of them; those spellings still work as **deprecated
aliases** — a configure that sets one adopts it under the new name and prints a
deprecation warning naming the replacement. Setting both the old and the new
name to different values is a configure error.

| Deprecated | Use instead |
|---|---|
| `NANO_ROS_PLATFORM` | `NROS_PLATFORM` |
| `NANO_ROS_FEATURES` | `NROS_FEATURES` |
| `NANO_ROS_SAFETY_E2E` | `NROS_SAFETY_E2E` |
| `NANO_ROS_ROS_EDITION` | `NROS_ROS_EDITION` |
| `NANO_ROS_GEN_CACHE_DIR` | `NROS_GEN_CACHE_DIR` |
| `NANO_ROS_SKIP_BOOTSTRAP` | `NROS_SKIP_BOOTSTRAP` |
| `NANO_ROS_BUILD_CODEGEN` | `NROS_BUILD_CODEGEN` |
| `NANO_ROS_CMAKE_DIR` | `NROS_CMAKE_DIR` |
| `NANO_ROS_RMW` | `NROS_RMW` |
| `NANO_ROS_BOARD` | `NROS_BOARD` |
| `NANO_ROS_ROOT` (cmake variable) | `NROS_REPO_DIR` |
| `NANO_ROS_ROOT <path>` (keyword of `nano_ros_workspace()` and friends) | `NROS_REPO_DIR <path>` |

The aliases will be removed in a later release: first each becomes a configure
error naming its replacement, then the old name stops being recognised at all.
Update a project's `CMakeLists.txt` and build scripts to the new names when you
see the warning.

`nano_ros_DIR` and `nano_ros_ROOT` are not on this list: they are CMake's own
`find_package(nano_ros)` hints and keep their spelling.

`NANO_ROS_ROOT` as an **environment** variable is not an alias: it is retired
and a configure that finds it set stops, naming `NROS_REPO_DIR`.
