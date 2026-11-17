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

## Before you start

- Your copy of the library installed into a prefix you own, with the
  library's own CMake install. This page uses Eigen 3.4 installed into
  `$HOME/opt/eigen`, so `$HOME/opt/eigen/share/eigen3/cmake` holds
  `Eigen3Config.cmake`. Apply your patches to the source before you install
  it, and keep them in your own repository.
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

`nros build` compiles the package with the board's cross toolchain, not the
host's compiler, so a library must build for the board's CPU and C library.
Two things follow.

**CMake can find the host's copy.** The board toolchain files let
`find_package` search host paths, so a host with `libeigen3-dev` installed
satisfies `find_package(Eigen3)` with `/usr/share/eigen3`: a copy you did not
patch, found without a word. The first step below shuts that out.

**FreeRTOS compiles C++ freestanding.** The FreeRTOS boards build C++ with
`-ffreestanding`. The nano-ros C++ API needs no standard library, but Eigen
does, and libstdc++ refuses `<cmath>` or `<vector>` there with *This header is
not available in freestanding mode*. `nano_ros_hosted_cxx()` makes the C++ of
the package that calls it hosted, and leaves every other package as it was.
The call does nothing on POSIX, Zephyr and NuttX, whose C++ is already hosted.
ThreadX on RISC-V refuses it: that board's C++ library is a freestanding shim,
with no hosted library to switch to.

## Steps

The steps change two files of the package:

```tree
eigen_ws/
└── src/
    └── eigen_node/
        ├── CMakeLists.txt     (changed)
        └── src/
            └── Talker.cpp     (changed)
```

### Find your copy and link it

In `src/eigen_node/CMakeLists.txt`, call `nano_ros_hosted_cxx()` after
`find_package(nano_ros)`. Then find Eigen with the host's own locations
turned off, so only the copy you name when you build can answer:

```cmake probe=130 track=installed variant=own-library file=src/eigen_node/CMakeLists.txt
cmake_minimum_required(VERSION 3.22)
project(eigen_node VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Eigen needs the hosted C++ standard library. This makes the C++ of this
# package hosted on a board whose toolchain is freestanding.
nano_ros_hosted_cxx()

# Your copy, and only your copy: these options keep CMake from falling
# back to a copy installed on the host.
find_package(Eigen3 3.4 REQUIRED NO_MODULE
    NO_SYSTEM_ENVIRONMENT_PATH NO_CMAKE_SYSTEM_PATH
    NO_CMAKE_PACKAGE_REGISTRY NO_CMAKE_SYSTEM_PACKAGE_REGISTRY)
message(STATUS "Eigen ${Eigen3_VERSION} from ${Eigen3_DIR}")

nano_ros_add_node(eigen_node CLASS eigen_node::Talker
    SOURCES src/Talker.cpp)

target_link_libraries(eigen_node_eigen_node_component
    PUBLIC std_msgs__nano_ros_cpp
    PRIVATE Eigen3::Eigen)
```

### Use the library

This talker turns `(count, 0)` a quarter turn with a fixed-size matrix
before it publishes. Replace `src/eigen_node/src/Talker.cpp` with:

```cpp probe=131 track=installed variant=own-library file=src/eigen_node/src/Talker.cpp
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

### Build, naming your copy

`Eigen3_ROOT` is CMake's own variable for where to look for `Eigen3`. Set in
the environment, it reaches every CMake run the build makes:

```bash probe=140 track=installed variant=own-library
Eigen3_ROOT=$HOME/opt/eigen nros build
```

The configure output names the copy it found:

```text
-- Eigen 3.4.0 from /home/you/opt/eigen/share/eigen3/cmake
```

Without `Eigen3_ROOT`, the build stops with *Could not find a package
configuration file provided by "Eigen3"* rather than taking the host's.
CMake keeps the copy it found in the build's cache, so later builds and
`nros run` find it without the variable.

### Run it

Boot the image in QEMU. Each line carries the turned vector:

```bash probe=150 probe-run=120 track=installed variant=own-library
source /opt/ros/humble/setup.bash
nros run eigen_node
```

```text probe-expect=150
Publishing: 'Hello World: 1 (0, 1)'
Publishing: 'Hello World: 2 (0, 2)'
```

## Keep the library in the package instead

A library with no install step, or one you keep in the repository with your
patches applied, can live in the package. Copy its headers to
`src/eigen_node/third_party/eigen/` and replace the `find_package(Eigen3 …)`
lines of the first step with a target of your own:

```cmake
add_library(eigen_node_eigen INTERFACE)
target_include_directories(eigen_node_eigen SYSTEM INTERFACE
    ${CMAKE_CURRENT_SOURCE_DIR}/third_party/eigen)
add_library(Eigen3::Eigen ALIAS eigen_node_eigen)
```

Nothing is searched for, so nothing on the host can be picked up. For a
library that compiles sources, `add_subdirectory(third_party/<lib>)` builds
it with the board's toolchain, as part of the package.

## What hosted C++ costs

> **Warning:** hosted C++ can bring libstdc++'s heap and exception machinery
> into the image. A `std::vector`, a dynamic-size Eigen matrix or a
> `std::string` allocates through `operator new`, from the package's own
> code rather than through the platform's allocator. An image with a package
> that calls `nano_ros_hosted_cxx()` cannot claim to be free of allocation:
> [check-no-alloc-image](https://github.com/NEWSLabNTU/nano-ros/blob/main/scripts/check-no-alloc-image.py)
> reports `operator new` in its `cxx-operator-new` family. Fixed-size Eigen
> types, as in the talker above, keep their storage inline.

The FreeRTOS toolchain still compiles with `-fno-exceptions` and `-fno-rtti`,
so a library that throws must be built with its no-exceptions option.

## If it doesn't work

- *This header is not available in freestanding mode*: the package does not
  call `nano_ros_hosted_cxx()`, or calls it in a different directory from the
  target that includes the header.
- *Could not find a package configuration file provided by "Eigen3"*:
  `Eigen3_ROOT` is unset, or names a prefix without
  `share/eigen3/cmake/Eigen3Config.cmake`.
- The configure line names `/usr/share/eigen3`: the `find_package` call lacks
  the options of the first step, and CMake found the host's copy.
- A compile error inside the library: the library does not support this
  board's CPU or C library as it is, and needs a patch to your copy. Eigen 3.4,
  for one, does not compile its vectorized `psincos` on an `arm-none-eabi` CPU
  with NEON, where newlib's `int32_t` is `long`
  ([#4013](https://github.com/NEWSLabNTU/nano-ros/issues/4013)).

## Reference

- [Third-party code](../../reference/third-party-code.md): what an image
  compiles in besides your libraries.
- [Two surfaces: freestanding, and `NROS_CPP_STD`](../../reference/rclcpp-compatibility.md#two-surfaces-freestanding-and-nros_cpp_std):
  the nano-ros C++ API with and without the standard library.
