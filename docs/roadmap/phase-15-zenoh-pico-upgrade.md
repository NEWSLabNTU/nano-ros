# zenoh-pico Upgrade to 1.7.x

**Goal**: Upgrade zenoh-pico from 1.5.1 to 1.7.x for compatibility with zenohd 1.7.x and to resolve transport issues.

**Status**: Complete ✅

## Background

### Current State (After Upgrade)
- zenoh-pico version: **1.7.2** ✅
- zenohd version: **1.7.2** ✅
- Protocol compatibility: Resolved for native builds

### Original Symptoms (Pre-Upgrade)
When running Zephyr talker/listener with zenohd 1.7.2 and zenoh-pico 1.5.1:
1. Session opens successfully
2. Publisher/subscriber declarations succeed (after Z_FEATURE_INTEREST fix)
3. `z_publisher_put` immediately fails with error -100
4. Session closes (error -73 `_Z_ERR_SESSION_CLOSED`)

---

## Completed Tasks

### 1. ✅ Update west.yml
```yaml
- name: zenoh-pico
  remote: eclipse-zenoh
  revision: 1.7.2
  path: modules/lib/zenoh-pico
```

### 2. ✅ Update zenoh-pico submodule
```bash
cd crates/zenoh-pico-shim-sys/zenoh-pico
git fetch --tags
git checkout 1.7.2
```

### 3. ✅ Apply Z_FEATURE_INTEREST Patch
The `scripts/zephyr/setup.sh` patches `config.h` to disable Z_FEATURE_INTEREST:
```c
#define Z_FEATURE_INTEREST 0  // nano-ros patch: disabled for multi-client support
#define Z_FEATURE_MATCHING 0  // nano-ros patch: disabled (depends on INTEREST)
```

### 4. ✅ Native Builds Working
Native talker/listener communication works correctly with zenoh-pico 1.7.2:
- Session opens successfully
- Publisher/subscriber declarations succeed
- Messages are delivered correctly

---

## Issues Discovered During Upgrade

### Issue 1: Ephemeral Port Conflict (FIXED)

**Symptom**: When running multiple Zephyr native_sim instances, both pick the same ephemeral TCP port, causing connection conflicts.

**Root Cause**: Zephyr native_sim uses a test entropy source that produces identical random number sequences. Both instances generate the same "random" ephemeral port.

**Solution**: Pass different `--seed` values to each native_sim instance:
```bash
# Listener
./build-listener/zephyr/zephyr.exe --seed=12345

# Talker
./build-talker/zephyr/zephyr.exe --seed=67890
```

**Fix Applied**: Updated `ZephyrProcess::start()` in `crates/nano-ros-tests/src/zephyr.rs` to automatically use unique seeds.

### Issue 2: Zephyr Listener Crash on Message Receipt (FIXED)

**Symptom**: Zephyr listener creates subscriber successfully but crashes when receiving the first message.

**Debug Output**:
```
bsp_zephyr: sub->callback=0x60, sub->user_data=0x4  # Garbage values!
```

**Root Cause**: Rust/C FFI struct stability issue. The C BSP stored a pointer to the `NanoRosSubscriber` struct during `nano_ros_bsp_create_subscriber()`. However, Rust then moved the struct when returning it from the function, leaving the C pointer dangling.

```rust
// WRONG - struct moves when returned
let mut sub = NanoRosSubscriber { ... };
nano_ros_bsp_create_subscriber(&mut sub, ...);  // C stores pointer
Ok(BspSubscriber { sub, ... })  // sub MOVES here!
```

**Solution**: Use static storage for the subscriber struct to ensure a stable memory address:
```rust
static mut SUBSCRIBER_STORAGE: StaticSubscriber = ...;

let sub_ptr = unsafe {
    let storage_ptr = addr_of_mut!(SUBSCRIBER_STORAGE);
    (*storage_ptr).0.as_mut_ptr()
};
nano_ros_bsp_create_subscriber(sub_ptr, ...);  // Pointer stays valid
```

**Fix Applied**: Updated `examples/zephyr/rs-listener/src/lib.rs` to use static storage.

**Documentation**: Added "Rust/C FFI Issues" section to `docs/guides/troubleshooting.md`.

---

## Testing Status

### Working ✅
- [x] Native talker/listener communication
- [x] Zephyr talker/listener communication
- [x] zenoh-pico 1.7.2 compiles for native targets
- [x] zenoh-pico 1.7.2 compiles for Zephyr targets
- [x] Z_FEATURE_INTEREST patch still needed and applied
- [x] Multiple Zephyr clients communicating via router

### Not Tested Yet
- [ ] ROS 2 interop (rmw_zenoh) with zenoh-pico 1.7.2
- [ ] QEMU bare-metal examples

---

## Files Updated

| File | Change |
|------|--------|
| `west.yml` | Updated zenoh-pico revision to 1.7.2 ✅ |
| `crates/zenoh-pico-shim-sys/zenoh-pico` | Updated submodule to 1.7.2 ✅ |
| `scripts/zephyr/setup.sh` | Patches config.h (unchanged, still works) ✅ |
| `crates/nano-ros-tests/src/zephyr.rs` | Added unique seed support for native_sim ✅ |
| `crates/nano-ros-tests/tests/zephyr.rs` | Fixed pattern matching for BSP log format ✅ |
| `examples/zephyr/rs-listener/src/lib.rs` | Fixed callback crash using static storage ✅ |
| `docs/guides/troubleshooting.md` | Added Rust/C FFI section ✅ |

---

## Remaining Work

### Medium Priority
1. Test ROS 2 interop with upgraded zenoh-pico
2. Update QEMU examples for zenoh-pico 1.7.2
3. Run full test suite (`just test-rust`)

### Low Priority
4. Consider contributing port conflict fix upstream (to Zephyr or zenoh-pico)
5. Apply same static storage fix to other Zephyr examples if needed
6. Consider a more general solution for Rust/C FFI struct stability

---

## References

- [zenoh-pico releases](https://github.com/eclipse-zenoh/zenoh-pico/releases)
- [zenoh-pico CHANGELOG](https://github.com/eclipse-zenoh/zenoh-pico/blob/main/CHANGELOG.md)
- [Zephyr native_sim documentation](https://docs.zephyrproject.org/latest/boards/native/native_sim/doc/index.html) - `--seed` parameter
- [zenoh compatibility matrix](https://zenoh.io/docs/getting-started/compatibility/)
