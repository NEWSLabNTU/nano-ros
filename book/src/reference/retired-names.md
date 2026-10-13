# Retired names

nano-ros's cmake variables use one prefix, `NROS_`. Older releases used
`NANO_ROS_` for some of them, and the release before this one still accepted
those spellings as deprecated aliases. They are now **retired**: a configure
that sets one — in a `CMakeLists.txt`, with `-D` on the command line, or left
behind in a build directory's `CMakeCache.txt` — stops with an error that names
the replacement. Nothing is adopted under the new name any more, so rename the
old spelling where you set it.

| Retired | Use instead |
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

## A build directory from an older release

An older nano-ros wrote some of these names into the cache itself, so a build
directory it first configured carries them even though nothing sets them now.
The error lists those entries and prints the command that drops exactly them:

```sh
cmake -U NANO_ROS_PLATFORM -U NANO_ROS_RMW <build-dir>
```

Deleting the build directory's `CMakeCache.txt` works too.

## What is not on this list

`nano_ros_DIR` and `nano_ros_ROOT` are CMake's own `find_package(nano_ros)`
hints and keep their spelling.

`NANO_ROS_ROOT` as an **environment** variable is retired by its own rule: a
configure that finds it set stops, naming `NROS_REPO_DIR`.

The error table is kept for one more release, so a project that skips a
release still hears what to type. After that the old names are simply unknown
variables.
