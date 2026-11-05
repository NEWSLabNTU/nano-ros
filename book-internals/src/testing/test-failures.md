# Test failures

Failures in the repository's own test suites and checkout builds. The
user-facing troubleshooting page is the user book's
[Troubleshooting](../../guide/deploy/troubleshoot.html).

## Checkout: zenoh-pico submodule not found

**Symptom**:
```
zenoh-pico submodule not found at /path/to/zenoh-pico. Run: git submodule update --init
```

**Solution**: init that one submodule, scoped — **not** `--recursive`. The
transitive closure is large and mostly irrelevant to any one task, and the
launch toolchain's third-level runtime submodules are never built by nano-ros.

```bash
git submodule update --init packages/rmw/zenoh/zpico-sys/zenoh-pico
```

Better still, `nros setup <board> --rmw zenoh` (or
`nros setup --source zenoh-pico`) fetches it shallow.

## Checkout: zenoh-pico CMake defines do not take effect

After `cargo clean -p zpico-sys`, also `touch
packages/rmw/zenoh/zpico-sys/build.rs` so the build script reruns.

## Zephyr workspace patch

`just setup zephyr` applies the `Z_FEATURE_INTEREST` / `Z_FEATURE_MATCHING`
patch the user book's troubleshooting page describes to the in-tree Zephyr
workspace automatically.


## Tests Hang Forever

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

# Verify QEMU bare-metal TAP (only needed for QEMU bare-metal platforms)
ip addr show tap0
```

## Zephyr Tests Skip

**Symptom**: All Zephyr tests are skipped.

**Cause**: The test framework couldn't find the Zephyr workspace or required components.

**Solution**: Ensure the Zephyr workspace is properly configured:
```bash
# Default location: $repo/zephyr-workspace/ (gitignored, in-tree).
# Legacy setups: $repo/../nano-ros-workspace/ (auto-detected).
ls -la zephyr-workspace
nros doctor

# Or set explicitly
export NROS_ZEPHYR_WORKSPACE=/path/to/zephyr-workspace

# Verify west is available
west --version
```
