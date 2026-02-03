# Troubleshooting

Common issues and solutions when working with nano-ros.

## zenoh-pico Multiple Client Issues

### Symptoms

When running multiple zenoh-pico clients (e.g., a talker and listener) simultaneously connecting to the same router, you may see errors like:

```
<err> nano_ros_bsp: Failed to open zenoh session: -3
<err> rust: rustapp: BSP init failed: InitFailed(-5)
```

Or publisher/subscriber declarations fail with:
- Error code `-128` (`_Z_ERR_GENERIC`)
- Error code `-78` (`_Z_ERR_SYSTEM_OUT_OF_MEMORY`)

The first client typically succeeds while subsequent clients fail during `z_declare_publisher()` or `z_declare_subscriber()`.

### Root Cause

This is caused by the **Z_FEATURE_INTEREST** feature in zenoh-pico. The interest protocol implements write filtering to optimize network traffic by only sending data to interested subscribers. However, this feature has issues when multiple clients connect to the same router:

1. The router tracks "interest" for each client
2. When creating a publisher, zenoh-pico creates a write filter context
3. The filter creation calls `_z_session_rc_clone_as_weak()` which can fail
4. This manifests as `_Z_ERR_SYSTEM_OUT_OF_MEMORY` (-78) even when memory is available

The failure occurs in `zenoh-pico/src/net/filtering.c`:
```c
ctx->zn = _z_session_rc_clone_as_weak(zn);
if (_Z_RC_IS_NULL(&ctx->zn)) {
    _z_write_filter_ctx_clear(ctx);
    z_free(ctx);
    _Z_ERROR_RETURN(_Z_ERR_SYSTEM_OUT_OF_MEMORY);
}
```

### Solution

nano-ros disables `Z_FEATURE_INTEREST` by default for all builds:

**Native builds** (build.rs):
```rust
let dst = cmake::Config::new(&zenoh_pico_build)
    // ...
    .define("Z_FEATURE_INTEREST", "0")
    .build();
```

**Zephyr builds** (CMakeLists.txt):
```cmake
zephyr_compile_definitions(Z_FEATURE_INTEREST=0 Z_FEATURE_MATCHING=0)
```

Note: `Z_FEATURE_MATCHING` depends on `Z_FEATURE_INTEREST`, so both must be disabled.

### References

- [rmw_zenoh_pico](https://github.com/micro-ROS/rmw_zenoh_pico) disables this feature by default
- zenoh-pico filtering code: `src/net/filtering.c`
- zenoh-pico interest protocol: `src/session/interest.c`

---

## Network Configuration Issues

### Zephyr and QEMU Subnet Conflicts

**Symptom**: Network communication fails when running both Zephyr native_sim and QEMU tests.

**Cause**: Both Zephyr and QEMU were using the same subnet (192.0.2.0/24).

**Solution**: nano-ros uses separate subnets:
- **Zephyr (native_sim)**: 192.0.2.0/24
  - Bridge: `zeth-br` at 192.0.2.2
  - Talker: 192.0.2.1
  - Listener: 192.0.2.3
- **QEMU (MPS2-AN385)**: 192.0.3.0/24
  - TAP interface: `tap0` at 192.0.3.1
  - Guests: 192.0.3.10+

### TAP Interface Setup

**Symptom**: QEMU cannot connect to the network.

**Solution**: Run the network setup script:
```bash
sudo ./scripts/qemu/setup-network.sh
```

This creates and configures the TAP interface with proper permissions.

### Zephyr Bridge Setup

**Symptom**: Zephyr native_sim instances cannot communicate.

**Solution**: Run the Zephyr network setup:
```bash
sudo ./scripts/zephyr/setup-network.sh
```

This creates the `zeth-br` bridge for Zephyr native simulator instances.

---

## Build Issues

### zenoh-pico Submodule Not Found

**Symptom**:
```
zenoh-pico submodule not found at /path/to/zenoh-pico. Run: git submodule update --init
```

**Solution**:
```bash
git submodule update --init --recursive
```

### CMake Cache Stale

**Symptom**: Changes to CMake defines (like `Z_FEATURE_INTEREST`) don't take effect.

**Solution**: Clean the build cache:
```bash
cargo clean -p zenoh-pico-shim-sys
touch crates/zenoh-pico-shim-sys/build.rs
cargo build
```

For Zephyr builds, clean the west build directory:
```bash
rm -rf build/
west build -b native_sim/native/64 <app>
```

---

## Test Failures

### Tests Hang Forever

**Symptom**: Integration tests hang indefinitely.

**Possible causes**:
1. **No zenohd router running**: Many tests require a zenohd router
2. **Port already in use**: Another process is using port 7447
3. **Network misconfigured**: TAP or bridge interfaces not set up

**Solutions**:
```bash
# Check if zenohd is running
pgrep zenohd

# Check if port 7447 is in use
ss -tlnp | grep 7447

# Verify network interfaces
ip addr show tap0
ip addr show zeth-br
```

### Zephyr Tests Skip

**Symptom**: All Zephyr tests are skipped.

**Cause**: The test framework couldn't find the Zephyr workspace or required components.

**Solution**: Ensure the Zephyr workspace is properly configured:
```bash
# The workspace should be at ../nano-ros-workspace relative to nano-ros
ls -la zephyr-workspace  # Should be a symlink

# Or set explicitly
export ZEPHYR_WORKSPACE=/path/to/nano-ros-workspace

# Verify west is available
west --version

# Verify TAP network
ip addr show zeth-br
```

---

## zenoh-pico Error Codes

| Code | Name                           | Description                                      |
|------|--------------------------------|--------------------------------------------------|
| -3   | `_Z_ERR_TRANSPORT_OPEN_FAILED` | Could not connect to router                      |
| -78  | `_Z_ERR_SYSTEM_OUT_OF_MEMORY`  | Memory allocation failed (or write filter issue) |
| -128 | `_Z_ERR_GENERIC`               | Generic error                                    |

For the complete list, see `zenoh-pico/include/zenoh-pico/utils/result.h`.
