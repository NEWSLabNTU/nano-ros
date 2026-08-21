# Message Binding Generation

nros uses generated Rust bindings for ROS 2 message types. The `nros generate-rust` command generates `no_std` compatible bindings from `package.xml` dependencies.

## Overview

The binding generator lives in the in-tree CLI sub-workspace at `packages/cli/` and provides:
- `nros` standalone binary
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
     <description>My nros package</description>
     <maintainer email="dev@example.com">Developer</maintainer>
     <license>Apache-2.0</license>
     <depend>std_msgs</depend>
     <depend>geometry_msgs</depend>
     <export>
       <build_type>ament_cargo</build_type>
     </export>
   </package>
   ```

2. **nros tool built + on PATH**

   The `nros` CLI is built from the in-tree sub-workspace at
   `packages/cli/` and put on PATH by the activate file:
   ```bash
   # From the nano-ros repository root
   ./scripts/bootstrap.sh      # builds packages/cli/target/release/nros
   source ./activate.sh        # OR: direnv allow / source ./activate.fish
   ```
   See [Installation](../getting-started/installation.md) for the full
   walkthrough.

3. **ROS 2 environment** (optional for standard types)

   Standard interfaces (`std_msgs`, `builtin_interfaces`) are bundled with nros
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
nros sync
```

This will:
1. Parse `package.xml` to find dependencies
2. Resolve transitive dependencies (ament index + bundled interfaces)
3. Filter to interface packages (those with msg/srv/action)
4. Generate bindings to `generated/` directory
5. Write the `[patch.crates-io]` entries into `.cargo/config.toml`

(`nros generate-rust` is the codegen-only primitive — same steps 1–4
with no patch side-effects; add `--generate-config` to also write the
patches.)

**Step 3: Add dependencies to Cargo.toml**

Reference the generated crates using crates.io version specifiers:
```toml
[dependencies]
std_msgs = { version = "*", default-features = false }
example_interfaces = { version = "*", default-features = false }
```

The `.cargo/config.toml` patches redirect these to local paths.

### Why msg crates are RMW-agnostic

Generated msg crates carry **only the wire-format data type** — no
backend-specific code, no per-RMW Cargo feature. A user manifest is
the plain pair:

```toml
[dependencies]
std_msgs = { version = "*", default-features = false }
nros     = { version = "*", features = ["rmw-cyclonedds"] }   # RMW choice lives here
```

Transport choice and message schema are orthogonal concerns and the
manifest reflects that. This matches upstream rclcpp + rclrs, which
both ship msg packages RMW-agnostic and let the RMW pick which
descriptor representation it wants at runtime.

For DDS-based backends that need a per-type descriptor on the wire
(Cyclone DDS today), the `nros-rmw-cyclonedds` shim builds those
descriptors **lazily on first pub/sub for a given message type**,
walks the static field schema exposed by `nros-serdes` (the
`Message` trait with `const TYPE_NAME` + `const FIELDS`), and caches
the result in a bounded `no_std` registry. No per-msg-pkg backend
code is required.

Tracking + sizing knob (`NROS_CYCLONEDDS_MAX_TYPES`): see
[`docs/roadmap/archived/phase-212-ux-cargo-native-and-file-consolidation.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/archived/phase-212-ux-cargo-native-and-file-consolidation.md)
section 212.K.7.

## Git Dependency Workflow

For projects that consume nros as a **git dependency** (not from within the nros repo), use `--nano-ros-git` instead of `--nano-ros-path`:

**Step 1:** Add git dependency to `Cargo.toml`:
```toml
[dependencies]
nros = { git = "https://github.com/NEWSLabNTU/nano-ros", default-features = false, features = ["std"] }
std_msgs = { version = "*", default-features = false }
```

**Step 2:** Create `package.xml` (same as above).

**Step 3:** Generate bindings with git patches:
```bash
source /opt/ros/humble/setup.bash
nros generate-rust --generate-config --nano-ros-git
```

This generates `.cargo/config.toml` with git-based patches:
```toml
[patch.crates-io]
nros-core = { git = "https://github.com/NEWSLabNTU/nano-ros" }
nros-serdes = { git = "https://github.com/NEWSLabNTU/nano-ros" }
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
nros generate-rust [OPTIONS]

Options:
      --manifest <PATH>       Path to package.xml [default: package.xml]
  -o, --output <DIR>          Output directory [default: generated]
      --generate-config       Generate .cargo/config.toml with [patch.crates-io] entries
                              (alias: --config)
      --nano-ros-path <PATH>  Path to nros crates (for config patches, local dev)
      --nano-ros-git          Use nros git repo for config patches (external users)
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
std = ["nros-core/std", "nros-serdes/std"]
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
cd examples/native/rust/talker && cargo build --features zenoh
cd examples/native/rust/service-client && cargo build --features zenoh
```

## Regenerating Bindings

To regenerate after ROS package updates or dependency changes:
```bash
nros generate-rust --force
```

## Bundled Interfaces

nros ships standard `.msg` files for common packages so codegen works without a
ROS 2 environment:

- `std_msgs` (Bool, Int32, String, Header, etc.)
- `builtin_interfaces` (Time, Duration)

These are located at `packages/cli/interfaces/`. When a ROS 2 environment is sourced,
the ament index takes precedence over bundled files.

## Troubleshooting

**"Package 'X' not found in ament index or bundled interfaces"**
- For standard types (`std_msgs`, `builtin_interfaces`): should work without ROS 2
- For other packages: source ROS 2 environment: `source /opt/ros/humble/setup.bash`
- Check package is installed: `ros2 pkg list | grep X`
- Install if missing: `sudo apt install ros-humble-X`

**Build errors with generated code**
- Regenerate with `--force` flag
- Check nros crate compatibility

## Which CMake spelling (one table)

Five codegen entry points exist in the CMake surface. They are NOT
interchangeable spellings of one thing — each serves one audience, and
two are back-compat only. Pick by what you are doing:

| You are… | Use | Notes |
|---|---|---|
| **Consuming** an existing msg package from an app/node pkg | `find_package(<pkg> REQUIRED)` | **The recommended default.** The smart Find-stub resolves the package through the 3-layer search (workspace → ament → bundled) and generates bindings on the fly. Zero codegen calls in your file. |
| **Defining** msgs in a message package | `rosidl_generate_interfaces(${PROJECT_NAME} msg/… DEPENDENCIES …)` | **Recommended** — the stock ROS shape, intercepted by nano-ros. Your msg pkg carries zero nano-ros-specific lines and cross-builds under colcon unchanged. |
| Defining msgs and you want the nano-ros verb explicitly (or C bindings) | `nano_ros_generate_interfaces(<name> <files…> [LANGUAGE C\|CPP] [DEPENDENCIES …])` | The ament-shape alias (RFC-0048 §5). **Defaults to `LANGUAGE CPP`** like rosidl — pass `LANGUAGE C` for C. |
| Building a **workspace root** with several msg pkgs | `nros_workspace_interfaces()` after `set(NROS_INTERFACE_SEARCH_PATH …/src)` | Bulk-builds every workspace msg pkg in topological order; one line, no per-pkg `add_subdirectory`. |
| Generating **this pkg's `package.xml` dependencies** (no find_package flow) | `nros_find_interfaces([LANGUAGE C\|CPP] [SKIP_INSTALL])` | Consume-by-manifest: resolves the transitive interface closure of your own `package.xml`. Used by the workspace node pkgs. Defaults `LANGUAGE CPP`. |
| — | `nros_generate_interfaces(<target> <files…>)` | **Deprecated for new code** (Phase 210.E.4). The low-level generator everything above routes through; direct calls are back-compat only. |

## C Code Generation (CMake)

`nano_ros_generate_interfaces(… LANGUAGE C)` generates C bindings for `.msg`, `.srv`,
and `.action` files (the function **defaults to C++** — the `LANGUAGE C`
argument is what makes it the C generator). It uses a bundled codegen library — no external `nros` binary needed.

### Prerequisites

`nano_ros_generate_interfaces()` becomes available automatically once
the consumer's `CMakeLists.txt` invokes `add_subdirectory(nano-ros)`.
The codegen tool ships inside the in-tree `nros` CLI binary
(`packages/cli/target/release/nros`; the old `nros-codegen`
submodule is retired); cmake auto-resolves it from
`PATH` / `packages/cli/target/release/` / the transitional
`${NROS_HOME:-~/.nros}/bin/`. No separate build step.

### Usage

See `examples/native/c/custom-msg/CMakeLists.txt` for a complete example.
