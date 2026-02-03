# zenoh-pico Upgrade to 1.7.x

**Goal**: Upgrade zenoh-pico from 1.5.1 to 1.7.x for compatibility with zenohd 1.7.x and to resolve transport issues.

**Status**: Planning

## Background

### Current State
- zenoh-pico version: **1.5.1**
- zenohd version: **1.7.2**
- Protocol compatibility issue causing `z_publisher_put` to fail with `-100` (`_Z_ERR_TRANSPORT_TX_FAILED`)

### Symptoms
When running Zephyr talker/listener with zenohd 1.7.2:
1. Session opens successfully
2. Publisher/subscriber declarations succeed (after Z_FEATURE_INTEREST fix)
3. `z_publisher_put` immediately fails with error -100
4. Session closes (error -73 `_Z_ERR_SESSION_CLOSED`)

### Root Cause Analysis
The zenoh protocol may have changed between 1.5.x and 1.7.x, causing transport-level incompatibilities. The session handshake succeeds but data transmission fails.

---

## Upgrade Tasks

### 1. Update west.yml
```yaml
# Change from:
- name: zenoh-pico
  remote: eclipse-zenoh
  revision: 1.5.1
  path: modules/lib/zenoh-pico

# To:
- name: zenoh-pico
  remote: eclipse-zenoh
  revision: 1.7.2  # or latest 1.7.x tag
  path: modules/lib/zenoh-pico
```

### 2. Update zenoh-pico submodule (for native builds)
```bash
cd crates/zenoh-pico-shim-sys/zenoh-pico
git fetch --tags
git checkout 1.7.2
cd ../../..
git add crates/zenoh-pico-shim-sys/zenoh-pico
```

### 3. Review API Changes
Check for breaking changes between 1.5.1 and 1.7.x:
- [ ] Review zenoh-pico CHANGELOG
- [ ] Check for renamed functions/types
- [ ] Update `zenoh_shim.c` if needed
- [ ] Update Rust FFI bindings if needed

### 4. Update Z_FEATURE_INTEREST Patch
The `scripts/zephyr/setup.sh` patches `config.h` to disable Z_FEATURE_INTEREST. After upgrade:
- [ ] Verify if the patch is still needed
- [ ] Update sed patterns if config.h format changed
- [ ] Test with and without the patch

### 5. Rebuild and Test
```bash
# Update Zephyr workspace
./scripts/zephyr/setup.sh

# Rebuild Zephyr examples
cd ../nano-ros-workspace
source env.sh
west build -b native_sim/native/64 nano-ros/examples/zephyr/rs-talker -d build-talker --pristine
west build -b native_sim/native/64 nano-ros/examples/zephyr/rs-listener -d build-listener --pristine

# Run tests
just test-zephyr-rs
```

### 6. Update Native Builds
```bash
# Clean and rebuild
cargo clean -p zenoh-pico-shim-sys
cargo build -p zenoh-pico-shim-sys --features posix

# Test native examples
just test-rust-nano2nano
```

---

## Known Issues to Address

### Z_FEATURE_INTEREST
- **Issue**: Multiple clients cannot coexist when Z_FEATURE_INTEREST is enabled
- **Current fix**: Patch config.h to set `Z_FEATURE_INTEREST 0`
- **After upgrade**: Re-test if this is still needed in 1.7.x

### Transport TX Failed (-100)
- **Issue**: `z_publisher_put` fails immediately after session opens
- **Cause**: Likely protocol version mismatch between zenoh-pico 1.5.1 and zenohd 1.7.2
- **Expected resolution**: Upgrade should fix this

---

## Files to Update

| File | Change |
|------|--------|
| `west.yml` | Update zenoh-pico revision to 1.7.x |
| `crates/zenoh-pico-shim-sys/zenoh-pico` | Update submodule to 1.7.x |
| `scripts/zephyr/setup.sh` | Update patch if needed |
| `crates/zenoh-pico-shim-sys/build.rs` | Update CMake defines if needed |
| `crates/zenoh-pico-shim-sys/c/shim/zenoh_shim.c` | Update for API changes |
| `docs/troubleshooting.md` | Update version info |

---

## Testing Checklist

- [ ] Native talker/listener communication works
- [ ] Zephyr talker/listener communication works
- [ ] Multiple Zephyr clients can connect simultaneously
- [ ] ROS 2 interop (rmw_zenoh) still works
- [ ] QEMU bare-metal examples work
- [ ] All existing tests pass (`just test-rust`)

---

## References

- [zenoh-pico releases](https://github.com/eclipse-zenoh/zenoh-pico/releases)
- [zenoh-pico CHANGELOG](https://github.com/eclipse-zenoh/zenoh-pico/blob/main/CHANGELOG.md)
- [zenoh compatibility matrix](https://zenoh.io/docs/getting-started/compatibility/)
