# Package Preparation

This page shows the package shape nano-ros expects before you write
application code.

## Workspace Layout

Use one nano-ros checkout per ROS 2 workspace:

```text
~/ros2_ws/
├── src/
│   ├── nano-ros/
│   └── my_pkg/
│       ├── package.xml
│       ├── Cargo.toml
│       ├── CMakeLists.txt
│       ├── config.toml
│       └── src/
└── build/ install/ log/
```

Do not copy nano-ros into every package. Keep it as a shared workspace
dependency so message generation and local builds are reused.

## Setup Toolchain

Run setup from the nano-ros clone:

```bash
cd src/nano-ros
just setup tier=default        # Phase 142 SDK tiers (minimal | default | extended)
```

For a narrower fetch (single platform + RMW only), invoke the
underlying script:

```bash
tools/setup.sh --target=posix-zenoh
tools/setup.sh --list-targets       # print known platforms / RMWs
```

Setup fetches required submodules, installs/checks Rust targets, and
reports missing system packages. It never runs `sudo` — it tells you
what to install.

Add `nano_ros_link_rmw(... RMW <rmw>)` to your CMakeLists.txt — that
helper emits the strong-stub `nros_app_register_backends()` C TU
that links the right vtable on bare-metal / FreeRTOS / NuttX / Zephyr /
ESP-IDF where `linkme` distributed-slice contribution isn't picked up
automatically. POSIX builds can rely on the linkme path, but the
helper is harmless there.

## Rust Package

Minimal Rust package:

```toml
[dependencies]
nros = { path = "../nano-ros/packages/core/nros",
         default-features = false,
         features = ["std", "rmw-cffi", "platform-posix", "ros-humble"] }
```

Generated message crates are added by the message-generation workflow.

## C or C++ Package

Minimal CMake package (Phase 140 — `add_subdirectory` is the only
consumption shape):

```cmake
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW      zenoh)
add_subdirectory(<path-to-nano-ros> nano_ros)
nano_ros_generate_interfaces(std_msgs "msg/Int32.msg")
target_link_libraries(my_node PRIVATE NanoRos::NanoRos)
nros_platform_link_app(my_node)
nano_ros_link_rmw(my_node RMW zenoh)
```

Use the platform guide for target-specific CMake options and link
helpers.

## Package Metadata

Keep `package.xml` explicit. Downstream packages should declare
dependencies on nano-ros and message packages they use:

```xml
<depend>nano-ros</depend>
<depend>std_msgs</depend>
```

## Next

- [First Native Rust Node](../getting-started/native.md)
- [Message Binding Generation](message-generation.md)
- [Deployment Workflow](deployment.md)
