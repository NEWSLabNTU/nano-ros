# Configuration {#configuration}

## Runtime Environment Variables

`rclc_support_init()` reads these at startup when `locator` is `NULL`:

| Variable | Description | Default |
|----------|-------------|---------|
| `ROS_DOMAIN_ID` | ROS 2 domain ID | `0` |
| `NROS_LOCATOR` | Router address (`tcp/…`, `udp/…`, or `tls/…`). The retired `ZENOH_LOCATOR` is refused at init. | `tcp/127.0.0.1:7447` |
| `NROS_SESSION_MODE` | Session mode: `client` or `peer`. The retired `ZENOH_MODE` is refused at init. | `client` |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE` | Path to CA certificate (PEM) for TLS | — |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` | Base64-encoded CA cert (bare-metal) | — |

## C API Buffer Tuning (NROS_*)

Set these environment variables **before** `cmake --build`. After changing
a value, delete `CMakeCache.txt` and rebuild to force the new values to
take effect.

| Variable | Description | Default |
|----------|-------------|---------|
| `NROS_EXECUTOR_MAX_CBS` | Max executor callback slots (shared with Rust API) | `4` |
| `NROS_EXECUTOR_SUBSCRIPTION_BUFFER_BYTES` | Per-entity receive buffer size (shared with Rust API) | `1024` |
| `NROS_MAX_CONCURRENT_GOALS` | Max concurrent goals per action server | `4` |
| `NROS_MAX_PARAMETERS` | Max parameters in parameter server | `32` |
| `NROS_PARAMS_MAX_NAME_BYTES` | Max parameter name length | `64` |
| `NROS_PARAMS_MAX_STRING_VALUE_BYTES` | Max string parameter value length | `256` |
| `NROS_MAX_ARRAY_LEN` | Max bool/integer/double array parameter length | `32` |
| `NROS_MAX_STRING_ARRAY_LEN` | Max string-array parameter length, in elements | `32` |
| `NROS_PARAMS_MAX_BYTE_ARRAY_BYTES` | Max byte array parameter length | `256` |
| `NROS_PARAMS_MAX_DESCRIPTION_BYTES` | Max parameter description length (0 = none) | `256` |

## Transport Buffer Tuning

### Zenoh Backend (ZPICO_*)

| Variable | Description | Posix | Embedded |
|----------|-------------|-------|----------|
| `NROS_ZENOH_FRAG_MAX_BYTES` | Max reassembled message size | 65536 | 2048 |
| `NROS_ZENOH_BATCH_UNICAST_BYTES` | Max unicast batch before fragmentation | 65536 | 1024 |
| `NROS_ZENOH_BATCH_MULTICAST_BYTES` | Max multicast batch size | 8192 | 1024 |
| `NROS_SUBSCRIBER_BUFFER_BYTES` | Per-subscriber buffer in zenoh shim | 1024 | 1024 |
| `NROS_ZENOH_SERVICE_BUFFER_SIZE` | Per-service-server buffer in zenoh shim | 1024 | 1024 |

### XRCE-DDS Backend (XRCE_*)

| Variable | Description | Posix | Embedded |
|----------|-------------|-------|----------|
| `XRCE_TRANSPORT_MTU` | Transport MTU (also sizes stream buffers) | 4096 | 512 |
| `XRCE_BUFFER_SIZE` | Per-entity static buffer size | 1024 | 1024 |
| `XRCE_STREAM_HISTORY` | Reliable stream history depth (>= 2) | 4 | 4 |

## Example

Increase zenoh defrag to 128 KB for large point clouds:

```bash
NROS_ZENOH_FRAG_MAX_BYTES=131072 cmake --build build
```

After changing any `ZPICO_*` or `XRCE_*` variable, clean the transport
build cache:

```bash
# Cargo-based build
cargo clean -p zpico-sys    # or: cargo clean -p xrce-sys

# CMake-based build
rm CMakeCache.txt && cmake --build build
```
