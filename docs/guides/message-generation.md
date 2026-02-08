# Message Binding Generation

nano-ros uses generated Rust bindings for ROS 2 message types. The `nano-ros generate-rust` (or `cargo nano-ros generate-rust`) command generates `no_std` compatible bindings from `package.xml` dependencies.

## Overview

The binding generator lives in `colcon-nano-ros/packages/cargo-nano-ros/` and provides:
- `nano-ros` standalone binary and `cargo nano-ros` subcommand
- Pure Rust, `no_std` compatible output using `heapless` types
- Automatic dependency resolution via ament index or bundled interfaces
- `.cargo/config.toml` generation for crate patches

## Prerequisites

1. **package.xml in project root** - Declares ROS interface dependencies
   ```xml
   <?xml version="1.0"?>
   <package format="3">
     <name>my_package</name>
     <version>0.1.0</version>
     <description>My nano-ros package</description>
     <maintainer email="dev@example.com">Developer</maintainer>
     <license>Apache-2.0</license>
     <depend>std_msgs</depend>
     <depend>geometry_msgs</depend>
     <export>
       <build_type>ament_cargo</build_type>
     </export>
   </package>
   ```

2. **nano-ros tool installed**
   ```bash
   # From the nano-ros repository root
   just install-cargo-nano-ros

   # Or manually:
   cargo install --path colcon-nano-ros/packages/cargo-nano-ros --locked

   # Or from git (external users):
   cargo install --git https://github.com/jerry73204/nano-ros --path colcon-nano-ros/packages/cargo-nano-ros
   ```

3. **ROS 2 environment** (optional for standard types)

   Standard interfaces (`std_msgs`, `builtin_interfaces`) are bundled with nano-ros
   and work without ROS 2. For additional packages (e.g., `geometry_msgs`, `sensor_msgs`),
   source a ROS 2 environment:
   ```bash
   source /opt/ros/humble/setup.bash
   ```

## Workflow

**Step 1: Create package.xml**

Declare your ROS interface dependencies in `<depend>` tags:
```xml
<depend>std_msgs</depend>      <!-- For std_msgs::msg::Int32, String, etc. -->
<depend>example_interfaces</depend>  <!-- For service types -->
```

**Step 2: Generate bindings**

```bash
cd my_project
nano-ros generate-rust              # standalone binary
# or: cargo nano-ros generate-rust  # cargo subcommand (equivalent)
```

This will:
1. Parse `package.xml` to find dependencies
2. Resolve transitive dependencies (ament index + bundled interfaces)
3. Filter to interface packages (those with msg/srv/action)
4. Generate bindings to `generated/` directory

**Step 3: Add dependencies to Cargo.toml**

Reference the generated crates using crates.io version specifiers:
```toml
[dependencies]
std_msgs = { version = "*", default-features = false }
example_interfaces = { version = "*", default-features = false }
```

The `.cargo/config.toml` patches redirect these to local paths.

## Git Dependency Workflow

For projects that consume nano-ros as a **git dependency** (not from within the nano-ros repo), use `--nano-ros-git` instead of `--nano-ros-path`:

**Step 1:** Add git dependency to `Cargo.toml`:
```toml
[dependencies]
nano-ros = { git = "https://github.com/jerry73204/nano-ros", default-features = false, features = ["std"] }
std_msgs = { version = "*", default-features = false }
```

**Step 2:** Create `package.xml` (same as above).

**Step 3:** Generate bindings with git patches:
```bash
source /opt/ros/humble/setup.bash
cargo nano-ros generate-rust --config --nano-ros-git
```

This generates `.cargo/config.toml` with git-based patches:
```toml
[patch.crates-io]
nano-ros-core = { git = "https://github.com/jerry73204/nano-ros" }
nano-ros-serdes = { git = "https://github.com/jerry73204/nano-ros" }
std_msgs = { path = "generated/std_msgs" }
builtin_interfaces = { path = "generated/builtin_interfaces" }
```

**Step 4: Use in code**

```rust
use std_msgs::msg::Int32;
use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};

let msg = Int32 { data: 42 };
```

## Command Options

```bash
nano-ros generate-rust [OPTIONS]
# or: cargo nano-ros generate-rust [OPTIONS]

Options:
      --manifest-path <PATH>  Path to package.xml [default: package.xml]
  -o, --output <DIR>          Output directory [default: generated]
      --config                Generate .cargo/config.toml with [patch.crates-io] entries
      --nano-ros-path <PATH>  Path to nano-ros crates (for config patches, local dev)
      --nano-ros-git          Use nano-ros git repo for config patches (external users)
      --force                 Overwrite existing bindings
  -v, --verbose               Enable verbose output
```

## Generated Output Structure

```
my_project/
├── package.xml              # Your dependency declarations
├── Cargo.toml               # Your package manifest
├── src/
│   └── main.rs              # Your code using generated types
├── generated/               # Generated bindings (do not edit)
│   ├── std_msgs/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs       # #![no_std]
│   │       └── msg/
│   │           ├── mod.rs
│   │           └── int32.rs
│   └── builtin_interfaces/  # Transitive dependency
│       └── ...
└── .cargo/
    └── config.toml          # [patch.crates-io] entries
```

## Generated Code Features

**no_std by default:**
```rust
#![no_std]

pub mod msg;
```

**std feature for optional std support:**
```toml
[features]
default = []
std = ["nano-ros-core/std", "nano-ros-serdes/std"]
```

**heapless types for embedded:**
```rust
pub struct String {
    pub data: heapless::String<256>,
}

pub struct Arrays {
    pub data: heapless::Vec<i32, 64>,
}
```

**Service types with Request/Response:**
```rust
pub struct AddTwoInts;
pub struct AddTwoIntsRequest { pub a: i64, pub b: i64 }
pub struct AddTwoIntsResponse { pub sum: i64 }

impl RosService for AddTwoInts {
    type Request = AddTwoIntsRequest;
    type Reply = AddTwoIntsResponse;
}
```

## Standalone Package Mode

Examples are configured as standalone packages (excluded from workspace) because each has its own `.cargo/config.toml` patches. Build each example from its own directory:
```bash
cd examples/native-rs-talker && cargo build
cd examples/native-rs-service-client && cargo build
```

## Regenerating Bindings

To regenerate after ROS package updates or dependency changes:
```bash
cargo nano-ros generate-rust --force
```

## Bundled Interfaces

nano-ros ships standard `.msg` files for common packages so codegen works without a
ROS 2 environment:

- `std_msgs` (Bool, Int32, String, Header, etc.)
- `builtin_interfaces` (Time, Duration)

These are located at `colcon-nano-ros/interfaces/`. When a ROS 2 environment is sourced,
the ament index takes precedence over bundled files.

## Troubleshooting

**"Package 'X' not found in ament index or bundled interfaces"**
- For standard types (`std_msgs`, `builtin_interfaces`): should work without ROS 2
- For other packages: source ROS 2 environment: `source /opt/ros/humble/setup.bash`
- Check package is installed: `ros2 pkg list | grep X`
- Install if missing: `sudo apt install ros-humble-X`

**Build errors with generated code**
- Regenerate with `--force` flag
- Check nano-ros crate compatibility

## C Code Generation (CMake)

The `nano_ros_generate_interfaces()` CMake function generates C bindings for `.msg`, `.srv`,
and `.action` files. It uses a bundled codegen library — no external `nano-ros` binary needed.

### Prerequisites

Build the codegen library once:
```bash
just build-codegen-lib
# or: cargo build -p nano-ros-codegen-c --release --manifest-path colcon-nano-ros/packages/Cargo.toml
```

### Usage

See `examples/native/c-custom-msg/CMakeLists.txt` for a complete example. CMake
automatically compiles a thin wrapper at configure time that links against the
`libnano_ros_codegen_c.a` static library.
