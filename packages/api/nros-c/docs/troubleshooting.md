# Troubleshooting {#troubleshooting}

See @ref error_codes for the full `nros_ret_t` table with cause and
recovery for each return code.

## Message Too Large / Truncated

Messages pass through multiple buffer layers. A message must fit every
layer to be delivered intact:

| Layer | Env var | Posix default |
|-------|---------|---------------|
| Defragmentation | `ZPICO_FRAG_MAX_SIZE` | 65536 |
| Batch size | `ZPICO_BATCH_UNICAST_SIZE` | 65536 |
| Shim buffer | `NROS_SUBSCRIBER_BUFFER_SIZE` | 1024 |

For large messages, increase the transport limits (set before building)
and the receive class, `NROS_SUBSCRIBER_BUFFER_SIZE`:

```bash
ZPICO_FRAG_MAX_SIZE=131072 NROS_SUBSCRIBER_BUFFER_SIZE=8192 cmake --build build
```

After changing any buffer variable, clean the build cache:

```bash
cargo clean -p zpico-sys   # Cargo-based build
rm CMakeCache.txt           # CMake-based build
```

## zenoh Version Mismatch

zenoh-pico and zenohd must be the same version. Symptoms:
`z_publisher_put failed: -100` (`_Z_ERR_TRANSPORT_TX_FAILED`) followed
by `-73` (`_Z_ERR_SESSION_CLOSED`).

The router is the one ROS ships: `ros2 run rmw_zenoh_cpp rmw_zenohd`.
nano-ros vendors none (RFC-0075), and the old `just build-zenohd` recipe
was retired with the vendored copy.

## Build Issues

- **Submodule not found** — init that ONE submodule, scoped:
  `git submodule update --init packages/rmw/zenoh/zpico-sys/zenoh-pico`.
  Never `--recursive` on nano-ros — the transitive closure is large and
  mostly irrelevant to any one task. `nros setup <board> --rmw zenoh`
  provisions it for you, shallow, and is the preferred route.
- **CMake cache stale** (changed env vars not taking effect) — delete
  `CMakeCache.txt` and rebuild. For Cargo-based builds, run
  `cargo clean -p zpico-sys` then rebuild.

## FFI Callback Crashes

### Stable Pointer Requirement

The C API stores pointers to structs passed during initialisation
(e.g., `nros_publisher_t`, `nros_subscription_t`). These structs
**must not be moved** after initialisation. Use static or heap-allocated
storage:

```c
/* CORRECT: static storage — address is stable */
static nros_publisher_t pub;
nros_publisher_init(&pub, &node, &type_info, "/topic");

/* CORRECT: heap storage — address is stable */
nros_publisher_t *pub = calloc(1, sizeof(*pub));
nros_publisher_init(pub, &node, &type_info, "/topic");

/* WRONG: stack variable returned from a function — address invalidated */
nros_publisher_t make_pub(void) {
    nros_publisher_t pub = nros_publisher_get_zero_initialized();
    nros_publisher_init(&pub, ...);
    return pub;  /* copy invalidates internal pointers */
}
```

### Callback ABI

All callbacks passed to the nros C API must use the C calling convention.
In mixed C/C++ projects, declare callbacks as `extern "C"`:

```cpp
extern "C" void my_subscription_cb(const uint8_t *data, size_t len,
                                   const nros_message_info_t *info,
                                   void *context) {
    /* ... */
}
```

## zenoh-pico Error Codes

| Code | Name | Meaning |
|------|------|---------|
| -3 | `_Z_ERR_TRANSPORT_OPEN_FAILED` | Cannot connect to router |
| -73 | `_Z_ERR_SESSION_CLOSED` | Session closed after failure |
| -78 | `_Z_ERR_SYSTEM_OUT_OF_MEMORY` | Allocation failed |
| -100 | `_Z_ERR_TRANSPORT_TX_FAILED` | Transport transmission failed |
| -128 | `_Z_ERR_GENERIC` | Generic error |

The `nros_ret_t` return values map to:

| Value | Constant | Meaning |
|-------|----------|---------|
| 0 | `NROS_RET_OK` | Success |
| -1 | `NROS_RET_ERROR` | Generic error |
| -2 | `NROS_RET_TIMEOUT` | Operation timed out |
| -7 | `NROS_RET_NOT_INIT` | Object not initialised |
| -10 | `NROS_RET_PUBLISH_FAILED` | Publish failed |
