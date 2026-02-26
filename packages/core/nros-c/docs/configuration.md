# Configuration {#configuration}

## Runtime Environment Variables

`nros_support_init()` reads these at startup when `locator` is `NULL`:

| Variable | Description | Default |
|----------|-------------|---------|
| `ROS_DOMAIN_ID` | ROS 2 domain ID | `0` |
| `ZENOH_LOCATOR` | Router address (`tcp/…`, `udp/…`, or `tls/…`) | `tcp/127.0.0.1:7447` |
| `ZENOH_MODE` | Session mode: `client` or `peer` | `client` |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE` | Path to CA certificate (PEM) for TLS | — |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` | Base64-encoded CA cert (bare-metal) | — |

## C API Buffer Tuning (NROS_*)

Set these environment variables **before** `cmake --build`. After changing
a value, delete `CMakeCache.txt` and rebuild to force the new values to
take effect.

| Variable | Description | Default |
|----------|-------------|---------|
| `NROS_EXECUTOR_MAX_HANDLES` | Max handles in a C API executor | `16` |
| `NROS_MAX_SUBSCRIPTIONS` | Max subscriptions | `8` |
| `NROS_MAX_TIMERS` | Max timers | `8` |
| `NROS_MAX_SERVICES` | Max services | `4` |
| `NROS_LET_BUFFER_SIZE` | Buffer size for LET semantics per handle | `512` |
| `NROS_MESSAGE_BUFFER_SIZE` | Max buffer for subscription/service data | `4096` |
| `NROS_MAX_CONCURRENT_GOALS` | Max concurrent goals per action server | `4` |
| `NROS_MAX_PARAMETERS` | Max parameters in parameter server | `32` |
| `NROS_MAX_PARAM_NAME_LEN` | Max parameter name length | `64` |
| `NROS_MAX_STRING_VALUE_LEN` | Max string parameter value length | `256` |
| `NROS_MAX_ARRAY_LEN` | Max parameter array length | `32` |
| `NROS_MAX_BYTE_ARRAY_LEN` | Max byte array parameter length | `256` |

## Transport Buffer Tuning

### Zenoh Backend (ZPICO_*)

| Variable | Description | Posix | Embedded |
|----------|-------------|-------|----------|
| `ZPICO_FRAG_MAX_SIZE` | Max reassembled message size | 65536 | 2048 |
| `ZPICO_BATCH_UNICAST_SIZE` | Max unicast batch before fragmentation | 65536 | 1024 |
| `ZPICO_BATCH_MULTICAST_SIZE` | Max multicast batch size | 8192 | 1024 |
| `ZPICO_SUBSCRIBER_BUFFER_SIZE` | Per-subscriber buffer in zenoh shim | 1024 | 1024 |
| `ZPICO_SERVICE_BUFFER_SIZE` | Per-service-server buffer in zenoh shim | 1024 | 1024 |

### XRCE-DDS Backend (XRCE_*)

| Variable | Description | Posix | Embedded |
|----------|-------------|-------|----------|
| `XRCE_TRANSPORT_MTU` | Transport MTU (also sizes stream buffers) | 4096 | 512 |
| `XRCE_BUFFER_SIZE` | Per-entity static buffer size | 1024 | 1024 |
| `XRCE_STREAM_HISTORY` | Reliable stream history depth (>= 2) | 4 | 4 |

## Example

Increase zenoh defrag to 128 KB for large point clouds:

```bash
ZPICO_FRAG_MAX_SIZE=131072 cmake --build build
```

After changing any `ZPICO_*` or `XRCE_*` variable, clean the transport
build cache:

```bash
# Cargo-based build
cargo clean -p zpico-sys    # or: cargo clean -p xrce-sys

# CMake-based build
rm CMakeCache.txt && cmake --build build
```
