The `NANO_ROS_*` cmake names are retired. They were deprecated aliases of the
`NROS_*` names for one release; a configure that sets one now stops with an
error naming the replacement, instead of adopting it with a warning. This
covers a `-D` on the command line, a `set()` in a `CMakeLists.txt`, and an
entry left in a build directory's `CMakeCache.txt` by an older release (the
error prints the `cmake -U …` that drops exactly those).

| retired | write instead |
| --- | --- |
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
| `NANO_ROS_ROOT <path>` keyword of `nano_ros_workspace()` and friends | `NROS_REPO_DIR <path>` |

`nano_ros_DIR` and `nano_ros_ROOT` (CMake's `find_package` hints) are
unchanged. The error table is kept for one more release; after that an old
name is an unknown variable. See the book's "Retired names" page.
