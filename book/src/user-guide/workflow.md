# Application Workflow

nano-ros users usually want one path: prepare package, write node,
build it, then deploy to target. Use Concepts only when workflow raises
a technical question.

## 1. Prepare Workspace and Package

Start from a ROS 2 workspace and clone nano-ros once under `src/`:

```bash
mkdir -p ~/ros2_ws/src
cd ~/ros2_ws/src
git clone --depth=1 --branch=v1.0.0 https://github.com/NEWSLabNTU/nano-ros.git
cd ~/ros2_ws
./src/nano-ros/tools/setup.sh --target=posix-zenoh
```

Then add your package beside `nano-ros`, not inside it. A normal layout:

```text
~/ros2_ws/
├── src/
│   ├── nano-ros/
│   └── my_robot_node/
│       ├── package.xml
│       ├── Cargo.toml        # Rust, if using Rust API
│       ├── CMakeLists.txt    # C/C++, if using CMake
│       ├── config.toml       # embedded targets
│       └── src/
└── build/ install/ log/
```

See [Package Preparation](package-preparation.md).

## 2. Write Node Code

Choose API language first:

- Rust: use `nros`, generated message crates, and `Executor`.
- C: include `nros/nros.h` and generate interfaces with CMake.
- C++: include `nros/nros.hpp` and use typed wrappers.

Start with [First Native Rust Node](../getting-started/native.md), then
adapt to C or C++ through the API references.

## 3. Generate Messages

If you use custom `.msg`, `.srv`, or `.action` files, generate bindings
inside the workspace/build tree. See
[Message Binding Generation](message-generation.md).

## 4. Configure Target

Pick one platform and one RMW backend:

```text
<platform>-<rmw>
posix-zenoh
freertos-xrce
zephyr-dds
```

Runtime configuration is available on POSIX. Embedded targets usually
use `config.toml`, CMake, Kconfig, or Cargo features. See
[Configuration](configuration.md).

## 5. Build, Test, Deploy

For workspace consumers:

```bash
colcon build
source install/setup.bash
```

For target-specific deployment, go to the matching platform guide.
Each guide should show toolchain setup, package layout, code example,
build command, run/flash command, and deployment notes.

See [Deployment Workflow](deployment.md).
