# Design: Colcon Build Type for nano-ros

## Motivation

ROS 2 users expect to build projects with `colcon build`. nano-ros targets native, RTOS, and bare-metal platforms — none of which fit the standard `ament_cmake` or `ament_cargo` build types. A custom colcon build type would let users write:

```bash
colcon build --packages-select my_freertos_node
```

...and have it build a FreeRTOS QEMU firmware, a Zephyr application, or a native POSIX binary, with message generation handled automatically.

## Background: How Colcon Build Types Work

Colcon uses Python **entry points** for plugin discovery. A build type consists of:

| Component | Entry Point Group | Purpose |
|---|---|---|
| **Package Identification** | `colcon_core.package_identification` | Detect package type from filesystem |
| **Package Augmentation** | `colcon_core.package_augmentation` | Extract dependencies from manifest |
| **Build Task** | `colcon_core.task.build` | Execute the build |
| **Test Task** | `colcon_core.task.test` | Execute tests |

The flow:
1. Identification scans directories, finds a marker file (e.g., `Cargo.toml`, `CMakeLists.txt`), sets `pkg.type`
2. For ROS packages with `package.xml`, `colcon-ros` reads `<build_type>` from `<export>` and sets `pkg.type = f"ros.{build_type}"`
3. The build verb dispatches to the task registered under that type name

### Reference Implementations

| Plugin | Package Type | Marker | Build Tool |
|---|---|---|---|
| `colcon-cmake` | `cmake` | `CMakeLists.txt` | `cmake --build` |
| `colcon-cargo` | `cargo` | `Cargo.toml` | `cargo build` |
| `colcon-ros-cargo` | `ros.ament_cargo` | `package.xml` + `<build_type>ament_cargo` | `cargo ament-build` |
| `colcon-cargo-ros2` (ours) | `ros.cargo_ros2` | `package.xml` + `<build_type>cargo_ros2` | `cargo ros2` |

Source repos examined (cloned to `external/`):
- `external/colcon-core/` — plugin infrastructure
- `external/colcon-cargo/` — Cargo build type
- `external/colcon-ros-cargo/` — ROS 2 + Cargo integration
- `external/colcon-ros/` — ROS package identification

## Design: `colcon-nano-ros`

### Build Type Name

**`nano_ros`** — used in `package.xml`:

```xml
<package format="3">
  <name>my_freertos_node</name>
  <version>0.1.0</version>
  <export>
    <build_type>nano_ros</build_type>
  </export>
  <depend>std_msgs</depend>
  <depend>example_interfaces</depend>
</package>
```

Colcon-ros sets `pkg.type = "ros.nano_ros"`. Our plugin registers tasks under that name.

### Package Layout

A nano-ros colcon package looks like a standard Cargo package with a `package.xml`:

```
my_freertos_node/
  Cargo.toml          # Rust crate
  package.xml         # ROS 2 package manifest
  config.toml         # nano-ros platform config (zenoh locator, domain ID, etc.)
  src/
    main.rs
```

Or for C/C++:

```
my_freertos_node/
  CMakeLists.txt      # CMake project
  package.xml         # ROS 2 package manifest
  src/
    main.c
```

### What the Build Task Does

The `NanoRosBuildTask` needs to:

1. **Generate message bindings** — run `cargo nano-ros generate` (Rust) or `nano_ros_generate_interfaces()` (CMake) for dependencies declared in `package.xml`
2. **Build the firmware** — invoke the platform-specific build:
   - **Native POSIX**: `cargo build --release` or `cmake --build`
   - **FreeRTOS QEMU ARM**: `cargo build --release --target thumbv7m-none-eabi` or CMake with ARM toolchain
   - **Zephyr**: `west build`
   - **NuttX**: `cmake --build` with NuttX toolchain
3. **Install artifacts** — copy binary to install prefix

### Platform Selection

The target platform is specified via:

**Option A — `config.toml`** (nano-ros native):
```toml
[platform]
type = "freertos-qemu-arm"  # or "native", "zephyr", "nuttx"

[zenoh]
locator = "tcp/10.0.2.2:7447"
```

**Option B — colcon argument**:
```bash
colcon build --nano-ros-platform freertos-qemu-arm
```

**Option C — `package.xml` metadata**:
```xml
<export>
  <build_type>nano_ros</build_type>
  <nano_ros>
    <platform>freertos-qemu-arm</platform>
  </nano_ros>
</export>
```

Option A is preferred — it's already used by nano-ros examples and avoids polluting `package.xml` with build-system-specific metadata.

### Plugin Structure

```
colcon-nano-ros/
  setup.cfg                           # Entry points
  colcon_nano_ros/
    __init__.py
    task/
      __init__.py
      nano_ros/
        __init__.py
        build.py                      # NanoRosBuildTask
        test.py                       # NanoRosTestTask
```

Entry points in `setup.cfg`:
```ini
[options.entry_points]
colcon_core.task.build =
    ros.nano_ros = colcon_nano_ros.task.nano_ros.build:NanoRosBuildTask
colcon_core.task.test =
    ros.nano_ros = colcon_nano_ros.task.nano_ros.test:NanoRosTestTask
```

No custom package identification needed — `colcon-ros` handles it via `package.xml`.

### Build Task Implementation

```python
class NanoRosBuildTask(TaskExtensionPoint):
    async def build(self):
        pkg = self.context.pkg
        args = self.context.args

        # 1. Read platform config
        config = read_nano_ros_config(pkg.path)

        # 2. Generate message bindings for declared dependencies
        await generate_bindings(pkg, config)

        # 3. Build the package
        if (pkg.path / 'Cargo.toml').exists():
            await build_cargo(pkg, config, args)
        elif (pkg.path / 'CMakeLists.txt').exists():
            await build_cmake(pkg, config, args)

        # 4. Install artifacts
        await install_artifacts(pkg, config, args)
```

### Dependency Resolution

ROS interface dependencies are declared in `package.xml`:
```xml
<depend>std_msgs</depend>
<depend>example_interfaces</depend>
```

The build task:
1. Reads dependencies from `pkg.dependencies`
2. Finds interface `.msg`/`.srv`/`.action` files in the ROS install prefix (`AMENT_PREFIX_PATH`) or workspace
3. Runs `cargo nano-ros generate` (for Rust) which generates type bindings
4. For CMake packages, calls `nano_ros_generate_interfaces()` via the installed CMake config

### Comparison with Existing Build Types

| Aspect | `ament_cargo` (colcon-ros-cargo) | `cargo_ros2` (colcon-cargo-ros2) | `nano_ros` (proposed) |
|---|---|---|---|
| Build tool | `cargo ament-build` | `cargo ros2` | `cargo build` / `cmake` / `west` |
| Message gen | `ros2_rust` rosidl pipeline | `cargo-ros2` workspace bindgen | `cargo nano-ros generate` / CMake codegen |
| Target | Native Linux only | Native Linux only | Native + RTOS + bare-metal |
| Platform config | None (always host) | None | `config.toml` (platform, toolchain, zenoh) |
| Install layout | ament | ament | ament (native) or firmware blob (embedded) |
| Dependency resolution | `.cargo/config.toml` patches | Workspace-level `build/ros2_bindings/` | Per-package `generated/` dir |

### User Workflow

```bash
# Create a workspace
mkdir -p ~/nros_ws/src
cd ~/nros_ws/src

# Create a nano-ros package
cargo nano-ros new my_robot --platform freertos-qemu-arm
# Creates: Cargo.toml, package.xml, config.toml, src/main.rs

# Build
cd ~/nros_ws
colcon build

# Flash / Run
colcon test  # Runs on QEMU for FreeRTOS, native for POSIX
```

### Open Questions

1. **Should C/C++ and Rust packages use the same build type?** Both use `nano_ros`, with the build task auto-detecting Cargo.toml vs CMakeLists.txt. Alternative: `nano_ros_cargo` and `nano_ros_cmake`.

2. **How to handle cross-compilation toolchains?** The toolchain file path could be in `config.toml`, or auto-resolved from the platform name (e.g., `freertos-qemu-arm` → `cmake/toolchain/arm-freertos-armcm3.cmake`).

3. **Should the plugin depend on `colcon-cargo`?** For Rust packages, we could subclass `CargoBuildTask` (like `colcon-ros-cargo` does) to reuse cargo workspace resolution. For C/C++ packages, we'd need CMake handling.

4. **Integration with existing nano-ros `just` recipes?** The colcon build task could invoke `just build-<platform>` as a subprocess, leveraging existing tested build logic. Or it could replicate the build commands directly.

5. **Ament install layout for embedded targets?** Native builds produce ELF binaries that fit the ament `lib/<pkg>/` layout. Embedded builds produce firmware blobs (`.bin`, `.elf`). Where should these be installed?

## Related Work (Downloaded)

- `external/colcon-core/` — Colcon core (plugin infrastructure, extension points)
- `external/colcon-cargo/` — Cargo build type for colcon
- `external/colcon-ros-cargo/` — ROS 2 + Cargo (ament_cargo build type)
- `external/colcon-ros/` — ROS package identification from `package.xml`
