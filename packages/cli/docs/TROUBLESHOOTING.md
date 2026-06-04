# Troubleshooting Guide

Common issues and solutions when using cargo-ros2.

## Table of Contents

- [Environment Issues](#environment-issues)
- [Build Errors](#build-errors)
- [Cache Problems](#cache-problems)
- [Installation Issues](#installation-issues)
- [Performance Issues](#performance-issues)
- [Advanced Debugging](#advanced-debugging)

---

## Environment Issues

### "Failed to load ament index" or "AMENT_PREFIX_PATH not set"

**Symptoms**:
```
Error: Failed to load ament index (is ROS 2 sourced?)
```

**Cause**: ROS 2 environment not sourced.

**Solution**:
```bash
# Source your ROS 2 installation
source /opt/ros/humble/setup.bash

# Verify environment
echo $AMENT_PREFIX_PATH
# Should output: /opt/ros/humble

echo $ROS_DISTRO
# Should output: humble (or your distro)

# Try again
cargo ros2 build
```

**Permanent fix**: Add to your `.bashrc` or `.zshrc`:
```bash
# Add to ~/.bashrc
source /opt/ros/humble/setup.bash
```

---

### "Package 'foo' not found in ament index"

**Symptoms**:
```
Error: Package 'vision_msgs' not found in ament index
```

**Cause**: Package not installed or not in AMENT_PREFIX_PATH.

**Solution 1 - Install missing package**:
```bash
# Find package name (usually ros-<distro>-<package>)
apt search ros-humble-vision

# Install package
sudo apt install ros-humble-vision-msgs

# Verify installation
cargo ros2 info vision_msgs
```

**Solution 2 - Source workspace overlay**:
```bash
# If package is in a local workspace
source /path/to/my_workspace/install/setup.bash

# Verify AMENT_PREFIX_PATH includes both paths
echo $AMENT_PREFIX_PATH
# Should output: /path/to/my_workspace/install:/opt/ros/humble

# Try again
cargo ros2 build
```

---

### Wrong ROS distro detected

**Symptoms**:
```
Using packages from humble but I have jazzy installed
```

**Cause**: Multiple ROS installations or workspace overlays sourced incorrectly.

**Solution**:
```bash
# Start fresh shell (no ROS sourced)
exit  # or open new terminal

# Source only desired ROS distro
source /opt/ros/jazzy/setup.bash

# Verify
echo $ROS_DISTRO
# Should output: jazzy

# Clean cache and rebuild
cargo ros2 cache clean
cargo ros2 build
```

---

## Build Errors

### "cargo-ros2-bindgen not found"

**Symptoms**:
```
Error: cargo-ros2-bindgen not found. Please build it first with 'cargo build'
```

**Cause**: cargo-ros2-bindgen binary not in PATH or target/debug.

**Solution 1 - Build the tools**:
```bash
# Navigate to cargo-ros2 source directory
cd /path/to/cargo-ros2

# Build all binaries
just build

# Or build specific binary
cargo build --package cargo-ros2-bindgen
```

**Solution 2 - Install to PATH**:
```bash
# Install binaries to ~/.cargo/bin
cargo install --path cargo-ros2-bindgen
cargo install --path cargo-ros2

# Verify
which cargo-ros2-bindgen
cargo ros2 --version
```

---

### "linker error: undefined reference to rosidl_..."

**Symptoms**:
```
error: linking with `cc` failed
  = note: undefined reference to `rosidl_typesupport_c__get_message_type_support_handle__std_msgs__msg__String'
```

**Cause**: ROS C libraries not found by linker.

**Solution**:
```bash
# Ensure ROS is sourced (sets library paths)
source /opt/ros/humble/setup.bash

# Verify library paths
echo $LD_LIBRARY_PATH
# Should include /opt/ros/humble/lib

# Clean and rebuild
cargo clean
cargo ros2 build
```

**If problem persists**:
```bash
# Manually set library path
export LD_LIBRARY_PATH=/opt/ros/humble/lib:$LD_LIBRARY_PATH

# Or add to .bashrc
echo 'export LD_LIBRARY_PATH=/opt/ros/humble/lib:$LD_LIBRARY_PATH' >> ~/.bashrc
```

---

### "no such file or directory: .cargo/config.toml"

**Symptoms**:
```
Error: Failed to read .cargo/config.toml: No such file or directory
```

**Cause**: Normal on first run - `.cargo/` directory doesn't exist yet.

**Solution**:
```bash
# Create directory manually
mkdir -p .cargo

# Run again - cargo-ros2 will create config.toml
cargo ros2 build
```

This error is usually transient and resolves itself on the second run.

---

### Compilation fails with "trait bounds not satisfied"

**Symptoms**:
```
error[E0277]: the trait bound `Foo: Message` is not satisfied
```

**Cause**: Stale bindings or incomplete trait implementations.

**Solution**:
```bash
# Rebuild specific package
cargo ros2 cache rebuild foo

# Or clean everything
cargo ros2 cache clean

# Rebuild
cargo ros2 build
```

---

## Cache Problems

### Stale bindings after updating ROS packages

**Symptoms**:
- Old message fields still present after apt upgrade
- Missing new fields from updated package

**Cause**: Cache checksum doesn't detect system package updates (apt updates).

**Solution**:
```bash
# Rebuild specific package
cargo ros2 cache rebuild geometry_msgs

# Or clean all cache
cargo ros2 cache clean
cargo ros2 build
```

**Explanation**: Checksums are calculated from interface files. If apt updates a package but files have same content, checksum doesn't change. Force rebuild to regenerate.

---

### Cache fills up disk space

**Symptoms**:
```
df -h shows target/ros2_bindings/ is very large
```

**Cause**: Many packages cached over time.

**Solution**:
```bash
# List cached packages
cargo ros2 cache list

# Clean all cache
cargo ros2 cache clean

# Or selectively remove unused packages
cargo ros2 cache rebuild unused_package  # removes from cache
```

**Prevention**: Add to `.gitignore`:
```
/target/
/.ros2_bindgen_cache
/.cargo/config.toml
```

---

### "Cache checksum mismatch" warnings

**Symptoms**:
```
Warning: Cache checksum mismatch for 'foo', regenerating...
```

**Cause**: Normal behavior when interface files changed.

**Solution**: No action needed - cargo-ros2 automatically regenerates. This is working as designed.

---

## Installation Issues

### ament-build fails: "Binary not found"

**Symptoms**:
```
Warning: Binary not found: my_binary (did you run with --release?)
```

**Cause**: Binary not built yet, or built with wrong profile.

**Solution**:
```bash
# If you used --release flag
cargo ros2 ament-build --install-base install/my_pkg --release

# Verify binary exists
ls target/release/my_binary

# If binary doesn't exist, cargo build failed
# Check build output for errors
cargo build --release
```

---

### ament-build creates empty lib/ directory

**Symptoms**:
```
install/my_pkg/lib/ is empty but I have binaries
```

**Cause**: Package detected as library-only (no [[bin]] or src/main.rs).

**Solution**:
```bash
# Check package type detection
ls -la src/
# Should have src/main.rs for binary package

# Or add [[bin]] section to Cargo.toml
cat >> Cargo.toml << EOF
[[bin]]
name = "my_binary"
path = "src/main.rs"
EOF

# Rebuild
cargo ros2 ament-build --install-base install/my_pkg --release
```

---

### Install fails: "Permission denied"

**Symptoms**:
```
Error: Failed to create directory: Permission denied
```

**Cause**: Installing to system directory without sudo.

**Solution**:
```bash
# Don't use sudo with cargo-ros2!
# Instead, install to local directory

# Install to local workspace
cargo ros2 ament-build --install-base install/my_pkg --release

# Or install to writable location
cargo ros2 ament-build --install-base ~/ros_workspace/install/my_pkg --release
```

---

## Performance Issues

### Slow binding generation

**Symptoms**:
- `cargo ros2 build` takes 30+ seconds
- Many packages regenerating unnecessarily

**Cause**: Cache misses or disabled parallelization.

**Solutions**:

**1. Check cache status**:
```bash
# See what's cached
cargo ros2 cache list

# If empty, first build will be slow (normal)
```

**2. Verify parallel generation**:
```bash
# Watch output for parallel indicator
cargo ros2 build --verbose

# Should see:
# Generating bindings for 3 packages...
# ⠁ [00:00:05] [##########>-----] 2/3 Generating geometry_msgs
```

**3. Reduce dependencies**:
```bash
# Audit Cargo.toml
# Remove unused ROS dependencies
# Each dependency requires binding generation
```

---

### Slow cargo build after bindings

**Symptoms**:
- Binding generation fast
- `cargo build` takes long time

**Cause**: Normal Rust compilation, not related to cargo-ros2.

**Solution**:
```bash
# Use release profile (faster runtime, slower compile)
cargo ros2 build

# Or use check instead of build (type-check only)
cargo ros2 check  # much faster

# Or use cargo's own caching
# Subsequent builds are incremental
```

---

### Hot build still regenerates bindings

**Symptoms**:
- Second `cargo ros2 build` regenerates packages
- Cache exists but not used

**Cause**: Output directory deleted (`cargo clean`).

**Solution**:
```bash
# Don't use `cargo clean` - use cargo-ros2 clean instead
cargo ros2 clean  # preserves cache metadata

# Or rebuild cache if you already ran cargo clean
cargo ros2 build  # will regenerate (one-time cost)
```

---

## Advanced Debugging

### Enable verbose output

```bash
# All commands support --verbose
cargo ros2 build --verbose
cargo ros2 ament-build --install-base install/my_pkg --verbose --release
cargo ros2 info std_msgs --verbose

# Shows:
# - Ament package discovery
# - Cache hit/miss decisions
# - Binding generation progress
# - File operations
```

---

### Inspect cache contents

```bash
# View cache JSON
cat .ros2_bindgen_cache | jq .

# Sample output:
# {
#   "entries": {
#     "std_msgs": {
#       "package_name": "std_msgs",
#       "checksum": "a1b2c3d4...",
#       "ros_distro": "humble",
#       "timestamp": 1730764800,
#       "output_dir": "/home/user/project/target/ros2_bindings/std_msgs"
#     }
#   }
# }
```

---

### Inspect generated bindings

```bash
# List generated packages
ls target/ros2_bindings/

# View generated code
cat target/ros2_bindings/std_msgs/src/msg/rmw.rs
cat target/ros2_bindings/std_msgs/src/msg/idiomatic.rs

# Check Cargo.toml dependencies
cat target/ros2_bindings/geometry_msgs/Cargo.toml
```

---

### Manually test binding generation

```bash
# Use cargo-ros2-bindgen directly
cargo-ros2-bindgen \
  --package std_msgs \
  --output target/test/std_msgs \
  --verbose

# Compile generated package
cargo build --manifest-path target/test/std_msgs/Cargo.toml
```

---

### Check ROS environment

```bash
# Verify all environment variables
env | grep -E 'ROS|AMENT'

# Should show:
# AMENT_PREFIX_PATH=/opt/ros/humble
# ROS_DISTRO=humble
# ROS_VERSION=2
# ROS_PYTHON_VERSION=3
# (and more)

# Verify package discovery
ros2 pkg list | grep std_msgs
# Should output: std_msgs

# Verify package location
ros2 pkg prefix std_msgs
# Should output: /opt/ros/humble
```

---

### Debug with strace (Linux)

```bash
# Trace file operations
strace -e open,openat,stat cargo ros2 build 2>&1 | grep ament

# Trace library loading
strace -e open,openat cargo build 2>&1 | grep rosidl
```

---

### Debug with RUST_LOG

```bash
# Enable debug logging (if implemented)
RUST_LOG=debug cargo ros2 build

# Or specific modules
RUST_LOG=cargo_ros2::workflow=debug cargo ros2 build
```

---

## Getting Help

### Check Documentation

- [README.md](../README.md) - Project overview
- [CLI_REFERENCE.md](CLI_REFERENCE.md) - Complete command reference
- [DESIGN.md](DESIGN.md) - Architecture details
- [examples/](../examples/) - Working examples

### Report Issues

If you encounter a bug:

1. **Check if it's already reported**: [GitHub Issues](https://github.com/yourusername/cargo-ros2/issues)

2. **Gather information**:
   ```bash
   # Cargo-ros2 version
   cargo ros2 --version

   # ROS environment
   echo "ROS_DISTRO=$ROS_DISTRO"
   echo "AMENT_PREFIX_PATH=$AMENT_PREFIX_PATH"

   # Rust version
   rustc --version
   cargo --version

   # OS information
   lsb_release -a  # Ubuntu/Debian
   uname -a        # All systems

   # Verbose output
   cargo ros2 build --verbose 2>&1 | tee build.log
   ```

3. **Create minimal reproduction**:
   ```bash
   # Minimal Cargo.toml
   cargo new minimal_repro
   cd minimal_repro
   echo 'std_msgs = "*"' >> Cargo.toml
   cargo ros2 build --verbose
   ```

4. **Open issue** with:
   - Description of problem
   - Expected behavior
   - Actual behavior
   - Steps to reproduce
   - Environment information
   - Verbose output logs

---

## Common Pitfalls

### ❌ Using sudo with cargo-ros2

```bash
# WRONG - Don't do this
sudo cargo ros2 build
```

**Why**: Cargo tools should never run as root. Use local install directories.

---

### ❌ Mixing cargo clean and cargo ros2 clean

```bash
# WRONG - Don't do this
cargo clean
# Now cache is inconsistent!
```

**Why**: `cargo clean` removes output but not cache metadata. Use `cargo ros2 clean` instead.

---

### ❌ Editing generated code

```bash
# WRONG - Don't edit these files
vim target/ros2_bindings/std_msgs/src/msg/rmw.rs
```

**Why**: Generated code is overwritten on next generation. Modify templates in rosidl-codegen instead.

---

### ❌ Adding generated packages to version control

```bash
# WRONG - Don't commit generated code
git add target/ros2_bindings/
```

**Why**: Generated code should not be in version control. Add to `.gitignore` instead.

---

## Quick Reference

| Problem | Solution |
|---------|----------|
| "Failed to load ament index" | `source /opt/ros/humble/setup.bash` |
| "Package 'foo' not found" | `sudo apt install ros-humble-foo` |
| "cargo-ros2-bindgen not found" | `just build` in cargo-ros2 source |
| Stale bindings | `cargo ros2 cache rebuild <pkg>` |
| Slow first build | Normal - bindings cached after first run |
| Permission denied on install | Use local install path, not system |
| Linker errors | `source /opt/ros/humble/setup.bash` |

---

**Last Updated**: 2025-11-04
