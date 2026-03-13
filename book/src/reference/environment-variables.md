# Environment Variables Reference

## Configuration File

All environment variables can be set in a `.env` file at the project root:

    cp .env.example .env
    # Edit .env — uncomment and adjust values as needed

- **justfile** — `.env` is auto-loaded. Missing file silently ignored.
- **direnv** — `.envrc` sources `.env` if present.
- **Manual** — `set -a; source .env; set +a` before `cargo build`.

Variables in `.env` take precedence over justfile defaults but are overridden by explicit shell exports.

## Runtime Configuration

Examples use `ExecutorConfig::from_env()` for configuration:

| Variable                               | Description                                    | Default              |
|----------------------------------------|------------------------------------------------|----------------------|
| `ROS_DOMAIN_ID`                        | ROS 2 domain ID                                | `0`                  |
| `ZENOH_LOCATOR`                        | Router address (`tcp/…`, `udp/…`, or `tls/…`)  | `tcp/127.0.0.1:7447` |
| `ZENOH_MODE`                           | Session mode: `client` or `peer`               | `client`             |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE`        | Path to CA certificate (PEM) for TLS           | (none)               |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` | Base64-encoded CA certificate for TLS          | (none)               |
| `ZENOH_TLS_VERIFY_NAME_ON_CONNECT`     | Verify server hostname in TLS (`true`/`false`) | (none)               |

### TLS Notes

- **POSIX**: requires `libmbedtls-dev` (`just setup` installs it). File-path and base64 cert loading are both supported.
- **Bare-metal**: only `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` is supported (no filesystem). The certificate is embedded at build time.
- The `link-tls` Cargo feature must be enabled on both the example and the `nros` crate.

## Build-Time Configuration

| Variable         | Description                                                                                        | Required                            |
|------------------|----------------------------------------------------------------------------------------------------|-------------------------------------|
| `ZENOH_PICO_DIR` | CMake install prefix for pre-built zenoh-pico (use with `system-zenohpico` feature on `zpico-sys`) | Only with `system-zenohpico`        |
| `SSID`           | WiFi network name for ESP32 examples                                                               | Required for `build-examples-esp32` |
| `PASSWORD`       | WiFi password for ESP32 examples                                                                   | Required for `build-examples-esp32` |

### FreeRTOS / NuttX / ThreadX SDK Paths

These are auto-resolved by justfile recipes (defaulting to `external/` paths from `just setup-freertos` / `just setup-nuttx` / `just setup-threadx`). Override via env vars if sources are elsewhere.

| Variable              | Default                      | Description                        |
|-----------------------|------------------------------|------------------------------------|
| `FREERTOS_DIR`        | `external/freertos-kernel`   | FreeRTOS kernel source             |
| `FREERTOS_PORT`       | `GCC/ARM_CM3`                | FreeRTOS portable layer            |
| `LWIP_DIR`            | `external/lwip`              | lwIP source                        |
| `FREERTOS_CONFIG_DIR` | Board crate's `config/`      | `FreeRTOSConfig.h` + `lwipopts.h` |
| `NUTTX_DIR`           | `external/nuttx`             | NuttX RTOS source                  |
| `NUTTX_APPS_DIR`      | `external/nuttx-apps`        | NuttX apps source                  |
| `THREADX_DIR`         | `external/threadx`           | ThreadX kernel source              |
| `THREADX_CONFIG_DIR`  | Board crate's `config/`      | ThreadX config (`tx_user.h`)       |
| `NETX_DIR`            | `external/netxduo`           | NetX Duo source                    |
| `NETX_CONFIG_DIR`     | Board crate's `config/`      | NetX Duo config (`nx_user.h`)      |

## Buffer Tuning

All optional -- platform-appropriate defaults apply if unset.

### Zenoh-pico (`ZPICO_*`)

| Variable                           | Description                                            | Default          | Crate          |
|------------------------------------|--------------------------------------------------------|------------------|----------------|
| `ZPICO_FRAG_MAX_SIZE`              | Max reassembled message size after defragmentation     | `65536` / `2048` | zpico-sys      |
| `ZPICO_BATCH_UNICAST_SIZE`         | Max unicast batch size before fragmentation            | `65536` / `1024` | zpico-sys      |
| `ZPICO_BATCH_MULTICAST_SIZE`       | Max multicast batch size                               | `8192` / `1024`  | zpico-sys      |
| `ZPICO_MAX_PUBLISHERS`             | Max concurrent publishers in zenoh shim                | `8`              | zpico-sys      |
| `ZPICO_MAX_SUBSCRIBERS`            | Max concurrent subscribers in zenoh shim               | `8`              | zpico-sys      |
| `ZPICO_MAX_QUERYABLES`             | Max concurrent queryables in zenoh shim                | `8`              | zpico-sys      |
| `ZPICO_MAX_LIVELINESS`             | Max concurrent liveliness tokens in zenoh shim         | `16`             | zpico-sys      |
| `ZPICO_MAX_PENDING_GETS`          | Max concurrent in-flight service calls                 | `4`              | zpico-sys      |
| `ZPICO_SUBSCRIBER_BUFFER_SIZE`     | Per-subscriber static buffer in zenoh shim             | `1024`           | nros-rmw-zenoh |
| `ZPICO_SERVICE_BUFFER_SIZE`        | Per-service-server static buffer in zenoh shim         | `1024`           | nros-rmw-zenoh |
| `ZPICO_GET_REPLY_BUF_SIZE`         | Stack buffer for service client replies                | `4096`           | zpico-sys      |
| `ZPICO_GET_POLL_INTERVAL_MS`       | Single-threaded polling interval in `zenoh_shim_get()` | `10`             | zpico-sys      |
| `ZPICO_SMOLTCP_MAX_SOCKETS`        | Max concurrent TCP sockets (smoltcp)                   | `4`              | zpico-smoltcp  |
| `ZPICO_SMOLTCP_MAX_UDP_SOCKETS`    | Max concurrent UDP sockets (smoltcp)                   | `2`              | zpico-smoltcp  |
| `ZPICO_SMOLTCP_BUFFER_SIZE`        | Per-socket staging buffer (smoltcp)                    | `2048`           | zpico-smoltcp  |
| `ZPICO_SMOLTCP_CONNECT_TIMEOUT_MS` | TCP connection timeout (smoltcp)                       | `30000`          | zpico-smoltcp  |
| `ZPICO_SMOLTCP_SOCKET_TIMEOUT_MS`  | TCP read/write timeout (smoltcp)                       | `10000`          | zpico-smoltcp  |

### XRCE-DDS (`XRCE_*`)

| Variable                               | Description                                                              | Default        | Crate         |
|----------------------------------------|--------------------------------------------------------------------------|----------------|---------------|
| `XRCE_TRANSPORT_MTU`                   | Custom transport MTU; also sizes stream buffers (4x MTU) and UDP staging | `4096` / `512` | xrce-sys      |
| `XRCE_MAX_SUBSCRIBERS`                 | Max concurrent subscribers                                               | `8`            | nros-rmw-xrce |
| `XRCE_MAX_SERVICE_SERVERS`             | Max concurrent service servers                                           | `4`            | nros-rmw-xrce |
| `XRCE_MAX_SERVICE_CLIENTS`             | Max concurrent service clients                                           | `4`            | nros-rmw-xrce |
| `XRCE_BUFFER_SIZE`                     | Per-slot static buffer size                                              | `1024`         | nros-rmw-xrce |
| `XRCE_STREAM_HISTORY`                  | Reliable stream history depth (must be >= 2)                             | `4`            | nros-rmw-xrce |
| `XRCE_ENTITY_CREATION_TIMEOUT_MS`      | Timeout for entity creation                                              | `1000`         | nros-rmw-xrce |
| `XRCE_SERVICE_REPLY_TIMEOUT_MS`        | Timeout for service replies                                              | `1000`         | nros-rmw-xrce |
| `XRCE_SERVICE_REPLY_RETRIES`           | Number of service reply retries                                          | `5`            | nros-rmw-xrce |
| `XRCE_MAX_SESSION_CONNECTION_ATTEMPTS` | Max session connection attempts                                          | `10`           | xrce-sys      |
| `XRCE_MIN_SESSION_CONNECTION_INTERVAL` | Min interval between connection attempts (ms)                            | `25`           | xrce-sys      |
| `XRCE_MIN_HEARTBEAT_TIME_INTERVAL`     | Min heartbeat interval (ms)                                              | `100`          | xrce-sys      |
| `XRCE_UDP_META_COUNT`                  | In-flight UDP packets per direction (smoltcp)                            | `4`            | xrce-smoltcp  |

### Core (`NROS_*`)

| Variable                        | Description                                                                              | Default | Crate       |
|---------------------------------|------------------------------------------------------------------------------------------|---------|-------------|
| `NROS_EXECUTOR_MAX_CBS`         | Max executor callback slots (compile-time fixed array size)                              | `4`     | nros-node   |
| `NROS_EXECUTOR_ARENA_SIZE`      | Executor arena size in bytes (compile-time fixed array size)                             | `4096`  | nros-node   |
| `NROS_SUBSCRIPTION_BUFFER_SIZE` | Default subscription/service buffer size (bytes)                                         | `1024`  | nros-node   |
| `NROS_EXECUTOR_MAX_HANDLES`     | Max handles in a C API executor                                                          | `16`    | nros-c      |
| `NROS_MAX_SUBSCRIPTIONS`        | Max subscriptions in a C API executor                                                    | `8`     | nros-c      |
| `NROS_MAX_TIMERS`               | Max timers in a C API executor                                                           | `8`     | nros-c      |
| `NROS_MAX_SERVICES`             | Max services in a C API executor                                                         | `4`     | nros-c      |
| `NROS_LET_BUFFER_SIZE`          | Buffer size for LET semantics per handle                                                 | `512`   | nros-c      |
| `NROS_MESSAGE_BUFFER_SIZE`      | Max buffer size for subscription/service data                                            | `4096`  | nros-c      |
| `NROS_MAX_CONCURRENT_GOALS`     | Max concurrent goals per action server (compile-time constant, not env-var configurable) | `4`     | nros-c      |
| `NROS_MAX_PARAMETERS`           | Max parameters in parameter server                                                       | `32`    | nros-params |
| `NROS_MAX_PARAM_NAME_LEN`       | Max parameter name length                                                                | `64`    | nros-params |
| `NROS_MAX_STRING_VALUE_LEN`     | Max string parameter value length                                                        | `256`   | nros-params |
| `NROS_MAX_ARRAY_LEN`            | Max parameter array length                                                               | `32`    | nros-params |
| `NROS_MAX_BYTE_ARRAY_LEN`       | Max byte array parameter length                                                          | `256`   | nros-params |
