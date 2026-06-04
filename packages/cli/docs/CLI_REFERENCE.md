# cargo-ros2 CLI Reference

Complete command-line reference for cargo-ros2 and cargo-ros2-bindgen tools.

## Table of Contents

- [cargo ros2](#cargo-ros2) - Main build tool
  - [build](#cargo-ros2-build) - Build with bindings
  - [check](#cargo-ros2-check) - Check project
  - [clean](#cargo-ros2-clean) - Clean artifacts
  - [ament-build](#cargo-ros2-ament-build) - Install to ament
  - [cache](#cargo-ros2-cache) - Cache management
  - [info](#cargo-ros2-info) - Package information
- [cargo-ros2-bindgen](#cargo-ros2-bindgen) - Binding generator

---

## cargo ros2

Main build tool for ROS 2 Rust projects. Orchestrates binding generation, caching, and compilation.

### Synopsis

```bash
cargo ros2 <COMMAND> [OPTIONS]
```

### Global Options

| Option | Description |
|--------|-------------|
| `-v, --verbose` | Enable verbose output for debugging |
| `-h, --help` | Print help information |
| `-V, --version` | Print version information |

---

## cargo ros2 build

Build the project with automatic ROS 2 binding generation.

### Synopsis

```bash
cargo ros2 build [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `--bindings-only` | Generate bindings without running cargo build |

### Description

The `build` command performs a complete workflow:

1. **Discover ROS dependencies** from Cargo.toml
2. **Check cache** for each package (SHA256-based)
3. **Generate missing bindings** to `target/ros2_bindings/`
4. **Update .cargo/config.toml** with patch entries
5. **Invoke cargo build** (unless `--bindings-only`)

### Examples

```bash
# Standard build
cargo ros2 build

# Generate bindings without building
cargo ros2 build --bindings-only

# Verbose output
cargo ros2 build --verbose
```

### How It Works

**Discovery**: Parses Cargo.toml dependencies and cross-references with packages in ament index (via `AMENT_PREFIX_PATH`).

**Caching**: Each package has a SHA256 checksum calculated from its interface files (.msg, .srv, .action). Bindings are regenerated only if:
- Package not in cache
- Checksum changed (source files modified)
- Output directory missing

**Parallel Generation**: When multiple packages need generation, they're processed in parallel using rayon for significant performance gains (3-5x speedup).

**Patching**: Writes `[patch.crates-io]` entries to `.cargo/config.toml` to redirect Cargo to local bindings.

### Performance

- **Cold build**: First-time generation for all dependencies (~10-15s per package)
- **Hot build**: Cache hit, no regeneration (<5s for most projects)
- **Parallel speedup**: 3-5x faster when 3+ packages need generation

---

## cargo ros2 check

Check the project without building binaries.

### Synopsis

```bash
cargo ros2 check [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `--bindings-only` | Generate bindings without running cargo check |

### Description

Similar to `cargo ros2 build` but runs `cargo check` instead of `cargo build`. Faster for type-checking without producing binaries.

### Examples

```bash
# Check project
cargo ros2 check

# Check with verbose output
cargo ros2 check --verbose
```

---

## cargo ros2 clean

Clean generated bindings and cache.

### Synopsis

```bash
cargo ros2 clean
```

### Description

Removes:
- `target/ros2_bindings/` - Generated binding packages
- `.ros2_bindgen_cache` - Cache metadata file

**Note**: Does not modify `.cargo/config.toml` patches. Use `cargo ros2 cache clean` for that.

### Examples

```bash
# Clean all bindings and cache
cargo ros2 clean
```

---

## cargo ros2 ament-build

Build and install package to ament index layout (colcon-compatible).

### Synopsis

```bash
cargo ros2 ament-build --install-base <PATH> [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `--install-base <PATH>` | **Required.** Installation base directory |
| `--release` | Build with release profile (optimized) |

### Description

Performs a complete three-phase build and installation:

**Phase 1: Generate Bindings**
- Discovers ROS dependencies
- Generates bindings (same as `cargo ros2 build --bindings-only`)

**Phase 2: Build Package**
- Runs `cargo build` (or `cargo build --release`)
- Compiles all binaries and libraries

**Phase 3: Install to Ament**
- Detects package type (library vs binary)
- Creates ament index markers
- Installs binaries to `lib/`
- Installs source files to `share/`
- Installs metadata (package.xml if present)

### Directory Structure Created

```
<install-base>/<package-name>/
├── lib/
│   └── <package-name>/          # Binaries (if any)
│       └── my_binary
├── share/
    └── <package-name>/
        ├── rust/                # Source files
        │   ├── Cargo.toml
        │   ├── Cargo.lock
        │   └── src/
        ├── package.xml          # Metadata (optional)
        └── ament_index/         # Discovery markers
            └── resource_index/
                ├── packages/
                │   └── <package-name>
                └── package_type/
                    └── <package-name>
```

### Examples

```bash
# Install debug build
cargo ros2 ament-build --install-base install/my_robot

# Install release build (recommended for deployment)
cargo ros2 ament-build --install-base install/my_robot --release

# Install to specific location
cargo ros2 ament-build --install-base /opt/ros/my_ws/install/my_robot --release
```

### Library vs Binary Detection

The tool automatically detects package type:

**Library Package** (no binaries):
- No `[[bin]]` sections in Cargo.toml
- No `src/main.rs` file
- Only source files installed

**Binary Package**:
- Has `[[bin]]` sections or `src/main.rs`
- Binaries installed with executable permissions (Unix)

---

## cargo ros2 cache

Cache management commands.

### Synopsis

```bash
cargo ros2 cache <SUBCOMMAND>
```

### Subcommands

- `list` - List all cached bindings
- `rebuild <PACKAGE>` - Force rebuild specific package
- `clean` - Clean all cached bindings

---

### cargo ros2 cache list

List all cached package bindings.

#### Synopsis

```bash
cargo ros2 cache list
```

#### Description

Displays a table of cached packages showing:
- Package name
- ROS distro (e.g., humble, jazzy)
- Checksum (first 9 chars)
- Output directory path

#### Example Output

```
Cached ROS 2 package bindings:

Package                        ROS Distro      Checksum     Output Directory
----------------------------------------------------------------------------------------------------
geometry_msgs                  humble          abc123...    /path/to/target/ros2_bindings/geometry_msgs
sensor_msgs                    humble          def456...    /path/to/target/ros2_bindings/sensor_msgs
std_msgs                       humble          ghi789...    /path/to/target/ros2_bindings/std_msgs

Total: 3 package(s)
```

---

### cargo ros2 cache rebuild

Force rebuild bindings for a specific package.

#### Synopsis

```bash
cargo ros2 cache rebuild <PACKAGE>
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `<PACKAGE>` | Package name to rebuild |

#### Description

Removes the package from cache and triggers regeneration on next build.

**Note**: This doesn't regenerate immediately - run `cargo ros2 build` afterward.

#### Examples

```bash
# Rebuild std_msgs
cargo ros2 cache rebuild std_msgs

# Then rebuild project
cargo ros2 build
```

---

### cargo ros2 cache clean

Clean all cached bindings.

#### Synopsis

```bash
cargo ros2 cache clean
```

#### Description

Same as `cargo ros2 clean` - removes bindings directory and cache file.

#### Examples

```bash
# Clean everything
cargo ros2 cache clean
```

---

## cargo ros2 info

Show detailed information about a ROS 2 package.

### Synopsis

```bash
cargo ros2 info <PACKAGE>
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<PACKAGE>` | Package name to inspect |

### Description

Displays:
- Package name
- Share directory path
- Interface files:
  - Messages (.msg)
  - Services (.srv)
  - Actions (.action)
- Cache status (cached or not)
- Checksum (if cached)
- Output directory (if cached)

### Examples

```bash
# Show info for geometry_msgs
cargo ros2 info geometry_msgs
```

### Example Output

```
Package: geometry_msgs
Share directory: /opt/ros/humble/share/geometry_msgs

Interfaces:
  Messages (32):
    - Accel
    - AccelStamped
    - AccelWithCovariance
    - AccelWithCovarianceStamped
    - Inertia
    - InertiaStamped
    - Point
    - Point32
    - PointStamped
    - Polygon
    - PolygonStamped
    - Pose
    - Pose2D
    - PoseArray
    - PoseStamped
    - PoseWithCovariance
    - PoseWithCovarianceStamped
    - Quaternion
    - QuaternionStamped
    - Transform
    - TransformStamped
    - Twist
    - TwistStamped
    - TwistWithCovariance
    - TwistWithCovarianceStamped
    - Vector3
    - Vector3Stamped
    - Wrench
    - WrenchStamped

Cache status: ✓ Cached
  Checksum: a1b2c3d4e5f6g7h8
  Output: /home/user/project/target/ros2_bindings/geometry_msgs
  ROS Distro: humble
```

---

## cargo-ros2-bindgen

Low-level binding generator CLI. Most users should use `cargo ros2 build` instead.

### Synopsis

```bash
cargo-ros2-bindgen --package <PACKAGE> --output <PATH> [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `--package <NAME>` | **Required.** ROS package name to generate bindings for |
| `--output <PATH>` | **Required.** Output directory for generated package |
| `--package-path <PATH>` | Optional. Local package path (overrides ament search) |
| `--verbose` | Enable verbose output |

### Description

Generates Rust bindings for a single ROS 2 package. This is the underlying tool used by `cargo ros2` but can be invoked directly for advanced use cases.

**Workflow**:
1. Discovers package via ament index (or uses `--package-path`)
2. Parses all interface files (.msg, .srv, .action)
3. Generates Rust code (RMW and idiomatic layers)
4. Creates Cargo.toml with dependencies
5. Creates build.rs for C library linking
6. Writes complete package to output directory

### Examples

```bash
# Generate std_msgs bindings
cargo-ros2-bindgen --package std_msgs --output target/test/std_msgs

# Generate from local package
cargo-ros2-bindgen \
  --package my_msgs \
  --package-path /path/to/my_msgs \
  --output target/test/my_msgs

# Verbose output
cargo-ros2-bindgen \
  --package geometry_msgs \
  --output target/test/geometry_msgs \
  --verbose
```

### Generated Package Structure

```
<output>/<package>/
├── Cargo.toml           # Package manifest with dependencies
├── build.rs             # Links C libraries (rosidl_generator_c, etc.)
└── src/
    ├── lib.rs           # Module exports
    ├── msg/
    │   ├── mod.rs
    │   ├── rmw.rs       # C-compatible FFI types
    │   └── idiomatic.rs # User-friendly Rust types
    ├── srv/
    │   ├── mod.rs
    │   ├── rmw.rs
    │   └── idiomatic.rs
    └── action/
        ├── mod.rs
        ├── rmw.rs
        └── idiomatic.rs
```

---

## Environment Variables

| Variable | Description |
|----------|-------------|
| `AMENT_PREFIX_PATH` | **Required.** Colon-separated list of ROS install paths. Set by sourcing ROS setup.bash. |
| `ROS_DISTRO` | ROS distribution name (e.g., humble, jazzy). Used for cache tagging. |

### Example

```bash
# Source ROS to set environment variables
source /opt/ros/humble/setup.bash

# Verify variables
echo $AMENT_PREFIX_PATH
# /opt/ros/humble

echo $ROS_DISTRO
# humble
```

---

## Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1 | General error (missing dependencies, build failure, etc.) |
| 101 | Missing Cargo.toml |
| 102 | ROS not sourced (AMENT_PREFIX_PATH not set) |
| 103 | Package not found in ament index |

---

## Files and Directories

### Project Files

| Path | Description |
|------|-------------|
| `.ros2_bindgen_cache` | Cache metadata (JSON) with checksums and timestamps |
| `.cargo/config.toml` | Cargo config with patch entries (auto-generated) |
| `target/ros2_bindings/` | Generated binding packages (project-local) |

### Cache Format

```json
{
  "entries": {
    "std_msgs": {
      "package_name": "std_msgs",
      "checksum": "a1b2c3d4e5f6...",
      "ros_distro": "humble",
      "package_version": null,
      "timestamp": 1730764800,
      "output_dir": "/home/user/project/target/ros2_bindings/std_msgs"
    }
  }
}
```

---

## Performance Tips

1. **Use `--release` for ament-build**: Production deployments should use release builds:
   ```bash
   cargo ros2 ament-build --install-base install/my_pkg --release
   ```

2. **Parallel generation**: For projects with many dependencies, generation is automatically parallelized. Expect 3-5x speedup on multi-core systems.

3. **Cache reuse**: Avoid `cargo ros2 clean` unless necessary. The cache prevents unnecessary regeneration.

4. **Incremental builds**: Use `cargo ros2 build` repeatedly - only modified packages regenerate.

---

## Troubleshooting

### "Failed to load ament index"

**Cause**: ROS not sourced (AMENT_PREFIX_PATH not set).

**Solution**:
```bash
source /opt/ros/humble/setup.bash  # or your ROS distro
cargo ros2 build
```

### "Package 'foo' not found in ament index"

**Cause**: Package not installed or not in AMENT_PREFIX_PATH.

**Solution**:
```bash
# Install package
sudo apt install ros-humble-foo

# Or add workspace overlay
source /path/to/my_ws/install/setup.bash
```

### Stale bindings after updating ROS packages

**Cause**: Cache checksum doesn't detect system package updates.

**Solution**:
```bash
# Rebuild specific package
cargo ros2 cache rebuild foo

# Or clean everything
cargo ros2 cache clean
cargo ros2 build
```

### ".cargo/config.toml: No such file or directory"

**Cause**: Normal on first run - directory will be created.

**Solution**: Run command again, it will succeed after directory creation.

---

## See Also

- [README.md](../README.md) - Project overview
- [DESIGN.md](DESIGN.md) - Architecture details
- [ROADMAP.md](ROADMAP.md) - Development progress
- [examples/](../examples/) - Example projects

---

**Last Updated**: 2025-11-04
