# Configuration {#configuration}

## Runtime Environment Variables

`nros::init()` reads these at startup when the locator argument is
empty:

| Variable | Description | Default |
|----------|-------------|---------|
| `ROS_DOMAIN_ID` | ROS 2 domain ID | `0` |
| `NROS_LOCATOR` | Router address (`tcp/…`, `udp/…`, `tls/…`). Legacy alias: `ZENOH_LOCATOR`. | `tcp/127.0.0.1:7447` |
| `NROS_SESSION_MODE` | Session mode: `client` or `peer`. Legacy alias: `ZENOH_MODE`. | `client` |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE` | Path to CA certificate (PEM) for TLS | — |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` | Base64-encoded CA cert (bare-metal) | — |

## C++ API Buffer Tuning (NROS_*)

Set these **before** `cmake --build`. After changing a value, delete
`CMakeCache.txt` and rebuild so the new value takes effect. The C and
C++ APIs share the same compile-time pool sizes.

| Variable | Description | Default |
|----------|-------------|---------|
| `NROS_EXECUTOR_MAX_CBS` | Max executor callback slots | `4` |
| `NROS_SUBSCRIPTION_BUFFER_SIZE` | Per-subscription receive buffer | `1024` |
| `NROS_LET_BUFFER_SIZE` | Buffer size for LET semantics per handle | `512` |
| `NROS_MAX_CONCURRENT_GOALS` | Max concurrent goals per action server | `4` |
| `NROS_MAX_PARAMETERS` | Max parameters in parameter server | `32` |
| `NROS_MAX_PARAM_NAME_LEN` | Max parameter name length | `64` |
| `NROS_MAX_STRING_VALUE_LEN` | Max string parameter value length | `256` |

## C++ Storage Sizes

The compile-time storage occupied by `nros::Publisher<M>`,
`nros::Subscription<M>`, etc. is derived from the matching Rust
`size_of::<RmwPublisher>()` via the `nros-sizes-build` probe (Phase 87).
You should not need to override these manually — they are populated in
the generated `nros_cpp_config_generated.h` header at build time.

## Transport Buffer Tuning

### Zenoh Backend (ZPICO_*)

| Variable | Description | Posix | Embedded |
|----------|-------------|-------|----------|
| `ZPICO_FRAG_MAX_SIZE` | Max reassembled message size | 65536 | 2048 |
| `ZPICO_BATCH_UNICAST_SIZE` | Max unicast batch | 65536 | 1024 |
| `ZPICO_BATCH_MULTICAST_SIZE` | Max multicast batch | 8192 | 1024 |
| `ZPICO_SUBSCRIBER_BUFFER_SIZE` | Per-subscriber buffer in zenoh shim | 1024 | 1024 |
| `ZPICO_SERVICE_BUFFER_SIZE` | Per-service-server buffer in zenoh shim | 1024 | 1024 |

### XRCE-DDS Backend (XRCE_*)

| Variable | Description | Posix | Embedded |
|----------|-------------|-------|----------|
| `XRCE_TRANSPORT_MTU` | Transport MTU (also sizes stream buffers) | 4096 | 512 |
| `XRCE_BUFFER_SIZE` | Per-entity static buffer size | 1024 | 1024 |
| `XRCE_STREAM_HISTORY` | Reliable stream history depth (≥ 2) | 4 | 4 |

## Example

Increase zenoh defrag to 128 KB for large point clouds:

```bash
ZPICO_FRAG_MAX_SIZE=131072 cmake --build build
```

After changing any `ZPICO_*` or `XRCE_*` variable, clean the transport
build cache:

```bash
cargo clean -p zpico-sys    # or: cargo clean -p xrce-sys
rm CMakeCache.txt && cmake --build build
```
