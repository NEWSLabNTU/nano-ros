# Design: Colcon Build Type for nano-ros

## Motivation

ROS 2 users expect to build projects with `colcon build`. nano-ros targets native, RTOS, and bare-metal platforms — none of which fit the standard `ament_cmake` or `ament_cargo` build types. A custom colcon build type would let users write:

```bash
colcon build --packages-select my_freertos_node
```

...and have it build a FreeRTOS QEMU firmware, a Zephyr application, or a native POSIX binary, with message generation handled automatically.

## Background: How Colcon Build Types Work

Colcon uses Python **entry points** for plugin discovery. A build type consists of:

| Component                  | Entry Point Group                    | Purpose                             |
|----------------------------|--------------------------------------|-------------------------------------|
| **Package Identification** | `colcon_core.package_identification` | Detect package type from filesystem |
| **Package Augmentation**   | `colcon_core.package_augmentation`   | Extract dependencies from manifest  |
| **Build Task**             | `colcon_core.task.build`             | Execute the build                   |
| **Test Task**              | `colcon_core.task.test`              | Execute tests                       |

The flow:
1. Identification scans directories, finds a marker file (e.g., `Cargo.toml`, `CMakeLists.txt`), sets `pkg.type`
2. For ROS packages with `package.xml`, `colcon-ros` reads `<build_type>` from `<export>` and sets `pkg.type = f"ros.{build_type}"`
3. The build verb dispatches to the task registered under that type name

### Reference Implementations

| Plugin                     | Package Type      | Marker                                    | Build Tool          |
|----------------------------|-------------------|-------------------------------------------|---------------------|
| `colcon-cmake`             | `cmake`           | `CMakeLists.txt`                          | `cmake --build`     |
| `colcon-cargo`             | `cargo`           | `Cargo.toml`                              | `cargo build`       |
| `colcon-ros-cargo`         | `ros.ament_cargo` | `package.xml` + `<build_type>ament_cargo` | `cargo ament-build` |
| `colcon-cargo-ros2` (ours) | `ros.cargo_ros2`  | `package.xml` + `<build_type>cargo_ros2`  | `cargo ros2`        |

Source repos examined (cloned to `external/`):
- `external/colcon-core/` — plugin infrastructure
- `external/colcon-cargo/` — Cargo build type
- `external/colcon-ros-cargo/` — ROS 2 + Cargo integration
- `external/colcon-ros/` — ROS package identification

## Design: `colcon-nano-ros`

### Build Type Naming: `nros.<lang>.<platform>`

The build type encodes both language and target platform as a dotted name in `package.xml`:

```xml
<package format="3">
  <name>my_freertos_node</name>
  <version>0.1.0</version>
  <export>
    <build_type>nros.rust.freertos</build_type>
  </export>
  <depend>std_msgs</depend>
  <depend>example_interfaces</depend>
</package>
```

`colcon-ros` reads `<build_type>` and sets `pkg.type = "ros.nros.rust.freertos"`. Our plugin registers task entry points for each `lang × platform` combination.

**Verified**: `catkin_pkg` accepts dots in `<build_type>`, and colcon's `get_task_extension()` matches entry point names by exact string — dots are valid Python entry point names.

**Language axis** (`<lang>`):

| Value  | Build tool    | Source marker    |
|--------|---------------|------------------|
| `rust` | `cargo build` | `Cargo.toml`     |
| `c`    | CMake         | `CMakeLists.txt` |
| `cpp`  | CMake         | `CMakeLists.txt` |

**Platform axis** (`<platform>`):

| Value       | Target              | Toolchain           |
|-------------|---------------------|---------------------|
| `native`    | Host (Linux/macOS)  | Native GCC/Clang    |
| `freertos`  | FreeRTOS QEMU ARM   | `arm-none-eabi-gcc` |
| `zephyr`    | Zephyr              | `west build`        |
| `nuttx`     | NuttX QEMU ARM      | `arm-none-eabi-gcc` |
| `threadx`   | ThreadX             | Platform-specific   |
| `baremetal` | Bare-metal QEMU ARM | `arm-none-eabi-gcc` |

### Entry Points

A single Python class handles all combinations by parsing the type string:

```ini
[options.entry_points]
colcon_core.task.build =
    ros.nros.rust.native = colcon_nano_ros.task.build:NrosBuildTask
    ros.nros.rust.freertos = colcon_nano_ros.task.build:NrosBuildTask
    ros.nros.rust.zephyr = colcon_nano_ros.task.build:NrosBuildTask
    ros.nros.rust.nuttx = colcon_nano_ros.task.build:NrosBuildTask
    ros.nros.rust.baremetal = colcon_nano_ros.task.build:NrosBuildTask
    ros.nros.c.native = colcon_nano_ros.task.build:NrosBuildTask
    ros.nros.c.freertos = colcon_nano_ros.task.build:NrosBuildTask
    ros.nros.cpp.native = colcon_nano_ros.task.build:NrosBuildTask
    ros.nros.cpp.freertos = colcon_nano_ros.task.build:NrosBuildTask
```

The task extracts `lang` and `platform` from the type string at runtime:
```python
class NrosBuildTask(TaskExtensionPoint):
    async def build(self):
        # pkg.type = "ros.nros.rust.freertos"
        _, _, lang, platform = self.context.pkg.type.split(".")
        # lang = "rust", platform = "freertos"
```

**Advantages over a single `nano_ros` build type:**
- Platform is explicit — no side-channel `config.toml` for platform selection
- Colcon can filter by platform: `colcon build --packages-select-build-type ros.nros.rust.freertos`
- No `if/elif` chain in the build task — the type IS the dispatch key
- Mixed-platform workspaces work naturally (one package targets freertos, another targets native)
- `config.toml` remains for runtime config (zenoh locator, domain ID) — not build-system concerns

### Package Layout

```
my_freertos_node/
  Cargo.toml          # Rust crate
  package.xml         # ROS 2 package manifest (build_type = nros.rust.freertos)
  config.toml         # Runtime config: zenoh locator, domain ID (optional)
  src/
    main.rs
```

Or for C/C++:

```
my_freertos_node/
  CMakeLists.txt      # CMake project
  package.xml         # ROS 2 package manifest (build_type = nros.c.freertos)
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

### Plugin Structure

```
colcon-nano-ros/
  setup.cfg                           # Entry points (one per lang×platform)
  colcon_nano_ros/
    __init__.py
    task/
      __init__.py
      build.py                        # NrosBuildTask (single class, all combos)
      test.py                         # NrosTestTask
```

No custom package identification needed — `colcon-ros` handles it via `package.xml`.

### Build Task Implementation

```python
class NrosBuildTask(TaskExtensionPoint):
    async def build(self):
        pkg = self.context.pkg
        args = self.context.args

        # Parse "ros.nros.rust.freertos" → lang="rust", platform="freertos"
        _, _, lang, platform = pkg.type.split(".")

        # 1. Generate message bindings for <depend> entries in package.xml
        await self.generate_bindings(pkg, lang)

        # 2. Build the package using the appropriate tool
        if lang == "rust":
            await self.build_cargo(pkg, platform, args)
        else:  # "c" or "cpp"
            await self.build_cmake(pkg, lang, platform, args)

        # 3. Install artifacts to colcon install prefix
        await self.install(pkg, platform, args)

    async def build_cargo(self, pkg, platform, args):
        target = PLATFORM_TARGETS[platform]  # e.g., "thumbv7m-none-eabi"
        cmd = ["cargo", "build", "--release"]
        if target:
            cmd += ["--target", target]
        await run(self.context, cmd, cwd=str(pkg.path), env=self.build_env(platform))

    async def build_cmake(self, pkg, lang, platform, args):
        toolchain = PLATFORM_TOOLCHAINS[platform]  # e.g., "arm-freertos-armcm3.cmake"
        cmd = ["cmake", "-S", str(pkg.path), "-B", str(self.build_dir)]
        if toolchain:
            cmd += [f"-DCMAKE_TOOLCHAIN_FILE={toolchain}"]
        cmd += [f"-DCMAKE_PREFIX_PATH={self.install_base}"]
        await run(self.context, cmd)
        await run(self.context, ["cmake", "--build", str(self.build_dir)])
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

| Aspect                | `ament_cargo` (colcon-ros-cargo) | `cargo_ros2` (colcon-cargo-ros2)       | `nros.*.*` (proposed)                          |
|-----------------------|----------------------------------|----------------------------------------|------------------------------------------------|
| Build type            | Single: `ament_cargo`            | Single: `cargo_ros2`                   | Per-target: `nros.rust.freertos`, `nros.c.native`, ... |
| Build tool            | `cargo ament-build`              | `cargo ros2`                           | `cargo build` / `cmake` / `west`               |
| Message gen           | `ros2_rust` rosidl pipeline      | `cargo-ros2` workspace bindgen         | `cargo nano-ros generate` / CMake codegen      |
| Target                | Native Linux only                | Native Linux only                      | Native + RTOS + bare-metal                     |
| Platform selection    | N/A                              | N/A                                    | Encoded in build type name                     |
| Install layout        | ament                            | ament                                  | ament (native) or firmware blob (embedded)     |
| Dependency resolution | `.cargo/config.toml` patches     | Workspace-level `build/ros2_bindings/` | Workspace-level `build/nros_bindings/`         |

### User Workflow

```bash
# Create a workspace
mkdir -p ~/nros_ws/src
cd ~/nros_ws/src

# Create a nano-ros package (scaffolds package.xml with build_type)
cargo nano-ros new my_robot --lang rust --platform freertos
# Creates: Cargo.toml, package.xml (build_type=nros.rust.freertos), config.toml, src/main.rs

# Build everything in the workspace
cd ~/nros_ws
colcon build

# Build only FreeRTOS packages
colcon build --packages-select-build-type ros.nros.rust.freertos

# Run tests (QEMU for embedded, native for POSIX)
colcon test
```

### Mixed-Platform Workspace Example

A workspace can contain packages targeting different platforms:

```
nros_ws/src/
  robot_brain/          # Runs on Linux host
    package.xml         # build_type = nros.rust.native
    Cargo.toml
    src/main.rs

  motor_controller/     # Runs on FreeRTOS MCU
    package.xml         # build_type = nros.c.freertos
    CMakeLists.txt
    src/main.c

  sensor_driver/        # Runs on Zephyr
    package.xml         # build_type = nros.rust.zephyr
    Cargo.toml
    src/main.rs
```

```bash
# Build all
colcon build

# Build only the MCU firmware
colcon build --packages-select motor_controller
```

Colcon resolves dependencies between packages (e.g., shared message types) and builds them in the correct order.

### Design Decisions

1. **Install layout**: Follow ament/Unix conventions for all targets:
   ```
   install/<pkg>/
     lib/<pkg>/node_binary          # Native ELF binary
     lib/<pkg>/firmware.elf         # Embedded firmware
     share/<pkg>/package.xml        # ROS package manifest
     share/<pkg>/config.toml        # Runtime config (if present)
   ```

2. **Self-contained plugin**: No dependency on `colcon-cargo`. The embedded cross-compilation concerns are different enough from host Cargo builds that reusing `CargoBuildTask` adds complexity without benefit. The plugin bundles its own build library (Rust + PyO3), following the `colcon-cargo-ros2` pattern.

3. **Bundled build library**: The build logic (platform resolution, toolchain selection, message generation orchestration) lives in a Rust library exposed to Python via PyO3, packaged in the same wheel. This mirrors the `colcon-cargo-ros2` architecture:
   ```
   colcon-nano-ros/
     pyproject.toml                 # Maturin-based build (Rust + Python)
     colcon_nano_ros/               # Python colcon plugin
       task/build.py                # NrosBuildTask
       task/test.py                 # NrosTestTask
     src/                           # Rust library (PyO3)
       lib.rs                       # Platform config, toolchain resolution, codegen orchestration
   ```
   The Rust library handles: reading `config.toml`, resolving toolchain paths, invoking `cargo nano-ros generate` for message codegen, and constructing the correct build commands for each `lang × platform` combination.

4. **Workspace-level message generation**: Interface bindings are generated once per workspace (under `build/nros_bindings/<interface_pkg>/`), shared by all packages. A `PackageAugmentationExtensionPoint` collects all interface packages declared in `<depend>` across the workspace before the build phase, then generates bindings in a single pass. This avoids redundant codegen when multiple packages depend on the same interfaces (e.g., `std_msgs`).

### Open Questions

1. **Board-specific config within a platform?** FreeRTOS has multiple boards (MPS2-AN385, STM32F4, ESP32). Options:
   - Encode in build type: `nros.rust.freertos.mps2_an385` (4-level — heavy)
   - Board in `config.toml` (keeps build type to 3 levels)
   - Board as a colcon build argument: `colcon build --cmake-args -DNROS_BOARD=mps2-an385`

2. **`cargo nano-ros build` subcommand?** The colcon plugin could invoke a `cargo nano-ros build` subcommand that encapsulates the platform-specific build logic. This keeps the build logic in Rust, avoids duplicating it in Python, and is usable outside of colcon (standalone CLI). The colcon plugin becomes a thin wrapper that calls the subcommand with the right arguments.

## Related Work (Downloaded)

- `external/colcon-core/` — Colcon core (plugin infrastructure, extension points)
- `external/colcon-cargo/` — Cargo build type for colcon
- `external/colcon-ros-cargo/` — ROS 2 + Cargo (ament_cargo build type)
- `external/colcon-ros/` — ROS package identification from `package.xml`
