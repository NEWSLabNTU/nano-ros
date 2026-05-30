# local-msg-package — Phase 210.A.4 fixture

Demonstrates the **ROS-convention codegen** Phase 210 ships:

* `src/local_msgs/` — a **verbatim** ROS 2 msg package (stock `package.xml` +
  stock `CMakeLists.txt` calling `rosidl_generate_interfaces(...)`). Zero
  nano-ros-specific lines in the package's files. The same directory builds
  unchanged under `colcon build`.

* `src/consumer/` — a **verbatim** ROS 2 C++ consumer node. Its
  `CMakeLists.txt` calls `find_package(local_msgs REQUIRED)` +
  `target_link_libraries(consumer local_msgs::local_msgs)` (the stock-ROS
  shape). Its `consumer.cpp` `#include "local_msgs/msg/greeting.hpp"`
  (the stock-rosidl C++ header path).

* `CMakeLists.txt` (this dir) — the **only** nano-ros-specific file. Pulls
  nano-ros, points `NROS_INTERFACE_SEARCH_PATH` at `./src/`, includes
  `NrosRclcppCompat.cmake`, drives the two packages via `add_subdirectory`.

## Build

```sh
cmake -B build -S .
cmake --build build -j
./build/src/consumer/consumer        # publishes on /greetings via zenoh
```

## What's exercised

| Phase 210 piece | Where |
|---|---|
| `rosidl_generate_interfaces(...)` wrapper (210.A.1) | `src/local_msgs/CMakeLists.txt` |
| Smart Find-stub (`_NrosFindRosMsgPackage`, 210.A.2) | `find_package(local_msgs)` in `src/consumer/CMakeLists.txt` |
| Per-pkg Find delegators (210.A.3) | `find_package(std_msgs)` |
| Workspace Find-stub auto-emit (210.A.4) | `NROS_INTERFACE_SEARCH_PATH=./src` → auto-emits `Findlocal_msgs.cmake` so the consumer resolves it |
| `${pkg}::${pkg}` upstream-shape link target | `target_link_libraries(consumer local_msgs::local_msgs)` |

## Cross-build parity

The `src/` tree is what you'd drop into a colcon workspace. To prove parity:

```sh
cd src
colcon build               # upstream ROS 2 build of the SAME source.
```

Both build systems compile the same `consumer.cpp` against the same
`local_msgs/msg/Greeting.msg`; the difference is just which RCL implementation
links in.
