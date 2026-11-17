# Bring your own C++ library

<!-- type: how-to · level: 3 -->

**Needs first:** [Mix languages](mix-languages.md), [FreeRTOS on mps2-an385](../platforms/freertos.md)

Use this page when a C++ node package needs a general-purpose library that
nano-ros does not provide: Eigen, a JSON parser, a filter library. The
example links Eigen into a C++ talker for the `mps2-an385-freertos` board.

nano-ros ships no copies of general-purpose C++ libraries, patched or not.
You bring your own copy and patch it for your board yourself. The package's
`CMakeLists.txt` finds and links it as it would without nano-ros; nano-ros
neither provides it nor blocks it.

There are two ways to bring the copy:

- **As a package in your workspace**, beside your node packages. The copy
  lives in your repository with your patches applied, and every board builds
  from it. This page's steps take this route.
- **From a prefix you own**, where the library's own CMake install put it.
  [Use a copy installed elsewhere](#use-a-copy-installed-elsewhere) covers
  it.

## Before you start

- Your copy of the library's source, with your patches applied. This page
  uses Eigen 3.4 at `$HOME/src/eigen`. Eigen is header-only, so the copy is
  its `Eigen/` directory.
- A C++ workspace for the board, as the
  [FreeRTOS starter](../../starters/mps2-an385-freertos.md) makes one.

This page uses a workspace named `eigen_ws`, whose node package is
`eigen_node`:

```bash probe=110 track=installed variant=own-library
nros setup mps2-an385-freertos --rmw zenoh
source /opt/ros/humble/setup.bash
nros new eigen_ws --board mps2-an385-freertos --template talker --lang cpp
cd eigen_ws
```

## The board's toolchain decides what builds

`nros build` compiles every package with the board's cross toolchain, not
the host's compiler, so a library must build for the board's CPU and C
library. Two things follow.

**CMake can find the host's copy.** The board toolchain files let
`find_package` search host paths, so a host with `libeigen3-dev` installed
satisfies a plain `find_package(Eigen3)` with `/usr/share/eigen3`: a copy you
did not patch, found without a word. The steps below name your copy and
turn the host's locations off.

**FreeRTOS compiles C++ freestanding.** The FreeRTOS boards build C++ with
`-ffreestanding`. The nano-ros C++ API needs no standard library, but Eigen
does, and libstdc++ refuses `<cmath>` or `<vector>` there with *This header is
not available in freestanding mode*. `nano_ros_hosted_cxx()` makes the C++ of
the package that calls it hosted, and leaves every other package as it was.
The call does nothing on POSIX, Zephyr and NuttX, whose C++ is already hosted.
ThreadX on RISC-V refuses it: that board's C++ library is a freestanding shim,
with no hosted library to switch to.

## Steps

The steps add a package for the library and change three files of the node
package:

```tree
eigen_ws/
└── src/
    ├── eigen/                       (new)
    │   ├── package.xml
    │   ├── CMakeLists.txt
    │   ├── cmake/
    │   │   └── Eigen3Config.cmake
    │   └── include/
    │       └── eigen3/
    │           └── Eigen/
    └── eigen_node/
        ├── package.xml              (changed)
        ├── CMakeLists.txt           (changed)
        └── src/
            └── Talker.cpp           (changed)
```

### Add the library as a package

Copy your Eigen into the new package:

```bash probe=120 track=installed variant=own-library
mkdir -p src/eigen/include/eigen3 src/eigen/cmake
cp -r $HOME/src/eigen/Eigen src/eigen/include/eigen3/
```

Give it a `package.xml`. Its name is what a node package depends on:

```xml probe=121 track=installed variant=own-library file=src/eigen/package.xml
<?xml version="1.0"?>
<package format="3">
  <name>eigen</name>
  <version>3.4.0</version>
  <description>Eigen 3.4, with this project's patches.</description>
  <maintainer email="you@example.com">you</maintainer>
  <license>MPL-2.0</license>
  <buildtool_depend>ament_cmake</buildtool_depend>
  <export>
    <build_type>ament_cmake</build_type>
  </export>
</package>
```

A config file defines Eigen's own target, `Eigen3::Eigen`, over the copied
headers:

```cmake probe=122 track=installed variant=own-library file=src/eigen/cmake/Eigen3Config.cmake
get_filename_component(_root "${CMAKE_CURRENT_LIST_DIR}/.." ABSOLUTE)
if(NOT TARGET Eigen3::Eigen)
    add_library(Eigen3::Eigen INTERFACE IMPORTED GLOBAL)
    set_target_properties(Eigen3::Eigen PROPERTIES
        INTERFACE_INCLUDE_DIRECTORIES "${_root}/include/eigen3")
endif()
set(Eigen3_FOUND TRUE)
set(EIGEN3_INCLUDE_DIR "${_root}/include/eigen3")
```

The package's `CMakeLists.txt` loads it and records where it is, so a
node package's `find_package(Eigen3)` finds this copy:

```cmake probe=123 track=installed variant=own-library file=src/eigen/CMakeLists.txt
cmake_minimum_required(VERSION 3.22)
project(eigen LANGUAGES CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/Eigen3Config.cmake")
set(Eigen3_DIR "${CMAKE_CURRENT_SOURCE_DIR}/cmake" CACHE PATH "this workspace's Eigen" FORCE)

if(COMMAND ament_package)
    ament_package()
endif()
```

### Depend on it from the node package

Add `<depend>eigen</depend>` to `src/eigen_node/package.xml`. It builds the
library's package before the node's:

```xml probe=130 track=installed variant=own-library file=src/eigen_node/package.xml
<?xml version="1.0"?>
<package format="3">
  <name>eigen_node</name>
  <version>0.1.0</version>
  <description>eigen_node — nano-ros C++ Node pkg.</description>
  <maintainer email="you@example.com">you</maintainer>
  <license>Apache-2.0</license>
  <depend>std_msgs</depend>
  <depend>eigen</depend>
  <export>
    <build_type>nros_cmake</build_type>
  </export>
</package>
```

In `src/eigen_node/CMakeLists.txt`, call `nano_ros_hosted_cxx()`, then find
and link Eigen. `NO_DEFAULT_PATH` keeps the host's copy out: CMake takes the
`Eigen3_DIR` the library's package recorded, and looks nowhere else.

```cmake probe=131 track=installed variant=own-library file=src/eigen_node/CMakeLists.txt
cmake_minimum_required(VERSION 3.22)
project(eigen_node VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Eigen needs the hosted C++ standard library. This makes the C++ of this
# package hosted on a board whose toolchain is freestanding.
nano_ros_hosted_cxx()

# The build's source probe configures this package without its workspace
# dependencies, so it loads the library's package itself.
if(NOT TARGET Eigen3::Eigen)
    add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../eigen" eigen)
endif()

find_package(Eigen3 REQUIRED NO_MODULE NO_DEFAULT_PATH)
message(STATUS "Eigen from ${Eigen3_DIR}")

nano_ros_add_node(eigen_node CLASS eigen_node::Talker
    SOURCES src/Talker.cpp)

target_link_libraries(eigen_node_eigen_node_component
    PUBLIC std_msgs__nano_ros_cpp
    PRIVATE Eigen3::Eigen)
```

The `add_subdirectory` fallback is there for the source probe, which
`nros build` runs before the build to read each package's metadata. Today it
configures a node package alone, so a header or target from a sibling
package is missing there unless the package loads it.

### Use the library

This talker turns `(count, 0)` a quarter turn with a fixed-size matrix
before it publishes. Replace `src/eigen_node/src/Talker.cpp` with:

```cpp probe=132 track=installed variant=own-library file=src/eigen_node/src/Talker.cpp
#include "eigen_node/Talker.hpp"

#include <cstdio>

#include <Eigen/Dense>

namespace eigen_node {

void Talker::on_tick() {
    ++count_;
    // A quarter turn: (count, 0) becomes (0, count).
    const Eigen::Matrix2f turn{{0.0f, -1.0f}, {1.0f, 0.0f}};
    const Eigen::Vector2f v = turn * Eigen::Vector2f(float(count_), 0.0f);
    char payload[64];
    std::snprintf(payload, sizeof(payload), "Hello World: %d (%.0f, %.0f)", count_,
                  double(v.x()), double(v.y()));
    std_msgs::msg::String msg;
    msg.data = payload;
    if (pub_.publish(msg).ok()) {
        std::printf("Publishing: '%s'\n", msg.data.c_str());
    }
}

::rclcpp::Result Talker::configure(::rclcpp::Node& node) {
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
}

} // namespace eigen_node
```

### Build and run

```bash probe=140 track=installed variant=own-library
nros build
```

The configure output names the copy it found, inside your workspace:

```text
-- Eigen from /home/you/eigen_ws/src/eigen/cmake
```

Boot the image in QEMU. Each line carries the turned vector:

```bash probe=150 probe-run=120 track=installed variant=own-library
source /opt/ros/humble/setup.bash
nros run eigen_node
```

```text probe-expect=150
Publishing: 'Hello World: 1 (0, 1)'
Publishing: 'Hello World: 2 (0, 2)'
```

## Use a copy installed elsewhere

A library you build and install with its own CMake, into a prefix such as
`$HOME/opt/eigen`, needs no package of its own. Drop the `add_subdirectory`
fallback and the `<depend>eigen</depend>`, and name the prefix in the
bringup's `system.toml`:

```toml
[image_defaults.build]
cmake_args = ["-DEigen3_DIR=/home/you/opt/eigen/share/eigen3/cmake"]
```

`[image_defaults.build] cmake_args` reaches every image's configure and the
source probe, and changing it runs the probe again. A path that differs per
image, such as a cross image and a host image built against different
copies, goes in that image's `[image.<id>.build] cmake_args`; the probe still
reads only the `[image_defaults.build]` one.

## What hosted C++ costs

> **Warning:** hosted C++ can bring libstdc++'s heap and exception machinery
> into the image. A `std::vector`, a dynamic-size Eigen matrix or a
> `std::string` allocates through `operator new`, from the package's own
> code rather than through the platform's allocator. An image with a package
> that calls `nano_ros_hosted_cxx()` cannot claim to be free of allocation:
> [the project's allocation check](https://github.com/NEWSLabNTU/nano-ros/blob/main/scripts/check-no-alloc-image.py)
> reports `operator new` in its `cxx-operator-new` family. Fixed-size Eigen
> types, as in the talker above, keep their storage inline.

The FreeRTOS toolchain still compiles with `-fno-exceptions` and `-fno-rtti`,
so a library that throws must be built with its no-exceptions option.

## If it doesn't work

- *This header is not available in freestanding mode*: the package does not
  call `nano_ros_hosted_cxx()`, or calls it in a different directory from the
  target that includes the header.
- *probe FAILED for eigen_node::eigen_node: … Could not find a package
  configuration file provided by "Eigen3"*: the node package lacks the
  `add_subdirectory` fallback, so the source probe never loaded the library's
  package. For a copy installed elsewhere, `[image_defaults.build]
  cmake_args` lacks its `-DEigen3_DIR`, or names a directory without
  `Eigen3Config.cmake`.
- The configure line names `/usr/share/eigen3`: the `find_package` call lacks
  `NO_DEFAULT_PATH`, and CMake found the host's copy.
- A compile error inside the library: the library does not support this
  board's CPU or C library as it is, and needs a patch to your copy. Eigen 3.4,
  for one, does not compile its vectorized `psincos` on an `arm-none-eabi` CPU
  with NEON, where newlib's `int32_t` is `long`: its scratch buffer is `int`
  where Eigen's integer packets expect `int32_t`.

## Reference

- [Third-party code](../../reference/third-party-code.md): what an image
  compiles in besides your libraries.
- [Two surfaces: freestanding, and `NROS_CPP_STD`](../../reference/rclcpp-compatibility.md#two-surfaces-freestanding-and-nros_cpp_std):
  the nano-ros C++ API with and without the standard library.
