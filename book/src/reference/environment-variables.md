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

| Variable                               | Description                                                     | Default              |
|----------------------------------------|-----------------------------------------------------------------|----------------------|
| `ROS_DOMAIN_ID`                        | ROS 2 domain ID                                                 | `0`                  |
| `NROS_LOCATOR`                         | RMW locator (`tcp/…`, `udp/…`, `serial/…`, or `tls/…`)          | `tcp/127.0.0.1:7447` |
| `NROS_SESSION_MODE`                    | Session mode: `client` or `peer`                                | `client`             |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE`        | Path to CA certificate (PEM) for TLS                            | (none)               |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` | Base64-encoded CA certificate for TLS                           | (none)               |
| `ZENOH_TLS_VERIFY_NAME_ON_CONNECT`     | Verify server hostname in TLS (`true`/`false`)                  | (none)               |

> **Retired legacy names** (phase-484 W8c): `ZENOH_LOCATOR` and `ZENOH_MODE`
> are no longer read. Setting either is a startup ERROR naming its
> replacement (`NROS_LOCATOR` / `NROS_SESSION_MODE`) — Rust panics in
> `resolve_hosted` / returns `InitError::RetiredEnvVar` from `nros::init`,
> C / C++ init returns `RCL_RET_INVALID_ARGUMENT` / `NROS_CPP_RET_INVALID_ARGUMENT`
> and logs the reason. Refused rather than ignored: an ignored locator would
> dial the default router and read as "no delivery". `ZENOH_TLS_*` names are kept because TLS is currently
> zenoh-specific.

### TLS Notes

- **POSIX**: requires `libmbedtls-dev` (`just setup base` checks it). File-path and base64 cert loading are both supported.
- **Bare-metal**: only `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` is supported (no filesystem). The certificate is embedded at build time.
- The `link-tls` Cargo feature must be enabled on both the example and the `nros` crate.

## Build-Time Configuration

| Variable         | Description                                                                                        | Required                            |
|------------------|----------------------------------------------------------------------------------------------------|-------------------------------------|
| `ZENOH_PICO_DIR` | CMake install prefix for pre-built zenoh-pico (use with `system-zenohpico` feature on `zpico-sys`) | Only with `system-zenohpico`        |
| `SSID`           | WiFi network name for ESP32 examples                                                               | Required for `build-examples-esp32` |
| `PASSWORD`       | WiFi password for ESP32 examples                                                                   | Required for `build-examples-esp32` |
| `NROS_EXTRA_BOARD_PATH` | Extra board-search roots (PATH-style `:` separated). Each entry is a directory shaped like `packages/boards/` — subdirs carrying `nros-board.toml` crates or `*/boards/<name>/board.cmake` bundles. Read by the `nros` CLI's board catalog AND `nano_ros_use_board()` (also settable as a CMake cache var there). Board keys stay global: a name found under two roots is an error, never shadowed. | (unset) |

### ARM FVP (`FVP_BaseR_AEMv8R`)

Not license-gated: `nros setup --tool arm-fvp` fetches the model from a pinned
Arm CDN permalink (`[tool.arm-fvp]` in `nros-sdk-index.toml`, digest-checked) into
the SDK store. Arm publishes it for **x86_64 Linux only**. You need none of the
variables below unless you installed a model yourself. See the
[ARM FVP getting-started chapter](../getting-started/arm-fvp.md) for the
end-to-end build+run walk-through.

| Variable          | Description                                                                          | Default |
|-------------------|--------------------------------------------------------------------------------------|---------|
| `ARMFVP_BIN_PATH` | Directory containing `FVP_BaseR_AEMv8R` (Zephyr-canonical, highest priority).        | (unset) |
| `ARM_FVP_DIR`     | Install root; resolver scans `models/Linux64_GCC-*/` underneath.                     | (unset) |

`scripts/zephyr/resolve-fvp-bin.sh` resolves, in order: `ARMFVP_BIN_PATH`,
`ARM_FVP_DIR`, `FVP_BaseR_AEMv8R` on `PATH`, then the store copy
(`nros sdk-path arm-fvp`). The `just zephyr run-fvp-*` / `verify-fvp-runtime`
recipes export the result as `ARMFVP_BIN_PATH` at BUILD time as well as run time,
because Zephyr's `armfvp.cmake` caches the location at configure. When nothing
resolves, a recipe you NAMED fails with the remedy (`nros setup --tool arm-fvp`);
reached through a fixture fan-out it skips and is reported.

For a model you downloaded by hand, `scripts/installers/arm-fvp-installer.sh`
(with `ARM_FVP_DIR` set to the extraction root) symlinks it to
`~/.nros/sdks/arm-fvp/current/` and prints the `export ARMFVP_BIN_PATH=…` line.
`nros doctor --board fvp-aemv8r-smp` reports whether the model resolves.

### Vendored source trees

You normally set none of these. `nros setup` puts each tree in the shared
store, and every build locates it through one ladder (RFC-0103 D3): this
variable, then the project's `[sources]` row in `system.toml`, then a checkout
submodule you are editing, then the store, then the checkout's own copy.
`nros locate <name> --why` prints which rung answered. Set a variable only to
build against a tree of your own.

| Variable                 | Tree                         |
|--------------------------|------------------------------|
| `FREERTOS_DIR`           | FreeRTOS kernel source       |
| `LWIP_DIR`               | lwIP source                  |
| `NUTTX_DIR`              | NuttX RTOS source            |
| `NUTTX_APPS_DIR`         | NuttX apps source            |
| `THREADX_DIR`            | ThreadX kernel source        |
| `NETX_DIR`               | NetX Duo source              |
| `CYCLONEDDS_SOURCE_DIR`  | Cyclone DDS source           |

`FREERTOS_PORT`, `FREERTOS_CONFIG_DIR`, `THREADX_CONFIG_DIR` and
`NETX_CONFIG_DIR` are board facts: each board's `nros-board.toml` states
them, and the build carries them from there. They are not user inputs.

## Buffer Tuning

All optional -- platform-appropriate defaults apply if unset. See
[Configuration](../user-guide/configuration.md) for deployment-scenario
guidance and platform guides for target-specific sizing.

### Zenoh-pico (`NROS_ZENOH_*`)

Each knob here was spelled `ZPICO_<X>` before phase-484 W5a (RFC-0103 D2b).
That old spelling is now **retired** (phase-484 W8): set in the environment or
in an `[image.<id>] env` row, it stops the build with an error naming the
`NROS_ZENOH_*` name to use. The full list is [the retired-names table](retired-names.md#retired-knob-spellings).

| Variable                           | Description                                            | Default          | Crate          |
|------------------------------------|--------------------------------------------------------|------------------|----------------|
| `NROS_ZENOH_FRAG_MAX_SIZE`              | Max reassembled message size after defragmentation     | `65536` / `2048` | zpico-sys      |
| `NROS_ZENOH_BATCH_UNICAST_SIZE`         | Max unicast batch size before fragmentation            | `65536` / `1024` | zpico-sys      |
| `NROS_ZENOH_BATCH_MULTICAST_SIZE`       | Max multicast batch size                               | `8192` / `1024`  | zpico-sys      |
| `NROS_ZENOH_MAX_PUBLISHERS`             | Max concurrent publishers in zenoh shim                | `8`              | zpico-sys      |
| `NROS_ZENOH_MAX_SUBSCRIBERS`            | Max concurrent subscribers in zenoh shim               | `8`              | zpico-sys      |
| `NROS_ZENOH_MAX_QUERYABLES`             | Max concurrent queryables in zenoh shim                | `8`              | zpico-sys      |
| `NROS_ZENOH_MAX_LIVELINESS`             | Max concurrent liveliness tokens in zenoh shim. DERIVED when the image declares anything: one per node name plus the session's own, one per publisher, subscriber, service server and client, and the parameter and lifecycle families' servers and publisher (issue 1713). A stated value wins. | `16` when nothing is declared | zpico-sys      |
| `NROS_ZENOH_MAX_PENDING_GETS`          | Max concurrent in-flight service calls                 | `4`              | zpico-sys      |
| `NROS_ZENOH_MAX_TL_PUBLISHERS`          | Retention slots for TRANSIENT_LOCAL publishers in the zenoh shim. Each also takes one `NROS_ZENOH_MAX_QUERYABLES` slot: a transient-local publisher declares a queryable so a late joiner can query its last sample. An image whose endpoints are declared DERIVES this (an `action_server` row counts one for its `/status`), and a stated value below the declared demand fails the build naming the knob. | `2` | nros-rmw-zenoh |
| `NROS_ZENOH_TL_RETAIN_BYTES`            | Bytes one retained TRANSIENT_LOCAL sample may hold. A larger publish is SENT but not retained, and says so once at WARN. | `1024` | nros-rmw-zenoh |
| `NROS_ZENOH_TL_RETAIN_DEPTH`            | Samples each TRANSIENT_LOCAL publisher retains and replays, oldest first, to a late joiner. DERIVED from the deepest `KEEP_LAST(N)` a transient-local publisher declares (issue 1709); the publisher is granted and advertises `min(asked, this)`. Must be >= 1. | `1` when nothing is declared | nros-rmw-zenoh |
| `NROS_SUBSCRIBER_BUFFER_SIZE`      | `small` receive-class slot per subscriber in the zenoh shim; the executor arena reads the same name. (`ZPICO_SUBSCRIBER_BUFFER_SIZE` is a dead spelling — nothing reads it, issue 1181.) | `1024` | nros-rmw-zenoh |
| `NROS_ZENOH_SUBSCRIBER_LARGE_SIZE`      | `large` size-class slot, for subscriptions whose type does not fit the small block | `16384` | nros-rmw-zenoh |
| `NROS_ZENOH_SUBSCRIBER_SIZE_THRESHOLD`  | The small/large class split: a subscription whose `rx_buffer_hint` exceeds it is served from `large`. The cmake message-bound derivation assumes the shipped value (`NROS_MESSAGE_BOUNDS_DEFAULT_SMALL_CEILING`), so moving it can make a derived class count disagree with runtime routing, which surfaces as a failed `create_subscription`. | `2048` | nros-rmw-zenoh |
| `NROS_ZENOH_PUBLISHER_TX_BUFFER_SIZE`   | Per-publisher TX arena for the zero-copy loan path; costs `NROS_ZENOH_MAX_PUBLISHERS` x this (issue 0813). | `1024` | nros-rmw-zenoh |
| `NROS_ZENOH_MAX_LARGE_SUBSCRIBERS`      | How many `large`-class blocks this image reserves. **`0` is legal** (phase-403 W4): an image whose subscribed types all fit `NROS_SUBSCRIBER_BUFFER_SIZE` declares 0 and stops paying `RING_DEPTH x LARGE_SIZE` — 65,536 B at the defaults — for a class it never routes into. A hint no class can hold then fails `create_subscription` rather than dropping every sample. **DERIVED, on both lanes** — a Zephyr image gets it from Kconfig's `-1` sentinel, and since issue 1125 a cargo leaf gets it from `nros sync`'s gitignored `.cargo/nros-managed-env.toml`, joined over the types the leaf's own components subscribe to. Setting it here still wins. | `2` | nros-rmw-zenoh |
| `NROS_ZENOH_SERVICE_BUFFER_SIZE`        | Per-service-server static buffer in zenoh shim         | `1024`           | nros-rmw-zenoh |
| `NROS_PARAM_SERVICE_INBOX_BYTES`   | Bytes one request slot holds in the inbox ring of a ROS parameter service queryable (six per node) or a REP-2002 lifecycle one (five per node) -- the BUILTIN family, which the zenoh shim selects by service name (phase-461 W2b). **DERIVED**: with nothing stated, the contract's declared parameters bound it, as the largest request a `set_parameters` over the worst-declared node can produce (672 B on the Autoware Safety Island, against the transport's 1,024). A contract declaring a string- or array-valued parameter needs the parameter store's capacities to price, which the RMW cannot see, so the shim abstains there and the user-service slot stands. Setting it here still wins. | derived, else `NROS_SERVICE_INBOX_BYTES` | nros-rmw-zenoh |
| `NROS_PARAM_SERVICE_INBOX_DEPTH`   | Requests one parameter or lifecycle service queryable holds before the newest is dropped. One, because this traffic is one request and one reply from a client that waits (`ros2 param`, rclcpp's `SyncParametersClient`) and a node's services are polled serially in one spin. Raise it to 2 for an image serving a parameter dashboard. Must be >= 1. | `1` | nros-rmw-zenoh |
| `NROS_ZENOH_GET_REPLY_BUF_SIZE`         | Stack buffer for service client replies                | `4096`           | zpico-sys      |
| `NROS_ZENOH_GET_POLL_INTERVAL_MS`       | Single-threaded polling interval in `zenoh_shim_get()` | `10`             | zpico-sys      |
| `NROS_SMOLTCP_MAX_SOCKETS`        | Max concurrent TCP sockets (smoltcp); brokered default since Phase 204.2. (`ZPICO_SMOLTCP_MAX_SOCKETS` is retired.) | `1` (brokered) | nros-smoltcp |
| `NROS_SMOLTCP_MAX_UDP_SOCKETS`    | Max concurrent UDP sockets (smoltcp); 1 by default, 4 with the `nros-smoltcp/rtps` feature (Phase 204.2). (`ZPICO_SMOLTCP_MAX_UDP_SOCKETS` is retired.) | `1` (brokered) | nros-smoltcp |
| `NROS_SMOLTCP_BUFFER_SIZE`        | Per-socket staging buffer (smoltcp). (`ZPICO_SMOLTCP_BUFFER_SIZE` is retired.)                          | `2048`           | nros-smoltcp  |
| `NROS_SMOLTCP_CONNECT_TIMEOUT_MS` | TCP connection timeout (smoltcp). (`ZPICO_SMOLTCP_CONNECT_TIMEOUT_MS` is retired.)                      | `30000`          | nros-smoltcp  |
| `NROS_SMOLTCP_SOCKET_TIMEOUT_MS`  | TCP read/write timeout (smoltcp). (`ZPICO_SMOLTCP_SOCKET_TIMEOUT_MS` is retired.)                       | `10000`          | nros-smoltcp  |

### XRCE-DDS (`NROS_XRCE_*`)

These are read by `nros-rmw-xrce-cffi`'s build script, either from the
environment or — on Zephyr — from `CONFIG_<name>` in `.config`. They set C
`#define`s of the same name **without** the `NROS_` prefix; the define is not
itself an environment variable, so exporting `XRCE_BUFFER_SIZE` has no effect
and reports no error.

| Variable                           | Description                                                                 | Default | Min | Crate         |
|------------------------------------|-----------------------------------------------------------------------------|---------|-----|---------------|
| `NROS_XRCE_BUFFER_SIZE`            | Per-slot receive buffer. See the note below — this is the receive ceiling.   | `1024`  | 64  | nros-rmw-xrce |
| `NROS_XRCE_SUBSCRIBER_RING_DEPTH`  | Queued samples per subscriber                                               | `32`    | 1   | nros-rmw-xrce |
| `NROS_XRCE_MAX_SUBSCRIBERS`        | Max concurrent subscribers. On Zephyr, DERIVED — see below.                 | `8`     | 0   | nros-rmw-xrce |
| `NROS_XRCE_MAX_SERVICE_SERVERS`    | Max concurrent service servers. On Zephyr, DERIVED — see below.             | `4`     | 0   | nros-rmw-xrce |
| `NROS_XRCE_MAX_SERVICE_CLIENTS`    | Max concurrent service clients                                              | `4`     | 0   | nros-rmw-xrce |
| `NROS_XRCE_STREAM_HISTORY`         | Reliable stream history depth; sizes the per-session output buffer          | `16`    | 4   | nros-rmw-xrce |
| `NROS_XRCE_CUSTOM_TRANSPORT_MTU`   | Custom transport MTU; stream buffers are `MTU x STREAM_HISTORY`             | `4096`  | 128 | nros-rmw-xrce |

Dropping `MAX_SUBSCRIBERS`, `MAX_SERVICE_*` and `SUBSCRIBER_RING_DEPTH` to 1,
`BUFFER_SIZE` to 256, `STREAM_HISTORY` to 4 and `CUSTOM_TRANSPORT_MTU` to 512
takes the session struct from ~390 KB to ~10-20 KB.

`BUFFER_SIZE`, the three `MAX_*` caps, `STREAM_HISTORY` and
`CUSTOM_TRANSPORT_MTU` are knob-registry rows, so a project states them per
image as `[image.<id>.knobs.xrce] buffer_size = 256` (`max_subscribers`,
`max_service_servers`, `max_service_clients`, `stream_history`,
`custom_transport_mtu`) rather than as an `env` row (RFC-0103 D6).

#### On Zephyr the three entity caps DERIVE, and the minimum is 0

`CONFIG_NROS_XRCE_MAX_SUBSCRIBERS` and `CONFIG_NROS_XRCE_MAX_SERVICE_SERVERS`
default to `-1`, which means *take the number from the entities this image
declares* (the contract sidecar beside the bringup's launch file, or a standalone
leaf's `system.toml` `[[component]] entities` rows). A listener that declares
one subscription and no service is built with one subscriber slot and zero
service-server slots, which is what took the session struct from 427,968 bytes
to 59,088 on the zephyr cpp listener ([issue
1033](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/1033-xrce-subscriber-slots-budget-eight-for-one.md)).
Stating a number in Kconfig or the environment still wins over the derivation,
in both directions.

Zero is a legal, honoured value for all three caps — it is not rounded up to 1.
So creating an entity the image never declared does not truncate silently: the
create fails with `InvalidConfig` and the backend logs which cap it hit, which
knob moves it, and that the count came from the declaration.

`NROS_XRCE_SUBSCRIBER_RING_DEPTH` is deliberately NOT derivable — no inventory
knows how deep a burst a subscriber must survive — but it does now have a
Kconfig symbol. It did not before issue 1033, which made it unsettable on
Zephyr however it was spelled: a Zephyr cargo build sees only the knobs
`nros_cargo_build.cmake` forwards, and shell exports do not survive that.

#### `NROS_XRCE_BUFFER_SIZE` is the XRCE receive ceiling

A subscriber's receive ring entry is a fixed `uint8_t data[XRCE_BUFFER_SIZE]`,
so **a subscription's own buffer size cannot raise it**: asking for
`create_subscription_sized::<M, 16384>` still stops at 1024, because the 16384
buffer is the destination of a copy that never happens. A sample that does not
fit is refused on take with `MessageTooLarge`.

The static cost is `XRCE_SUBSCRIBER_RING_DEPTH x XRCE_BUFFER_SIZE` per
subscriber, which is why the default is small — raise it deliberately, and
consider lowering the ring depth at the same time.

Raising it stops helping at the transport MTU: at 4096 samples arrive
*corrupted* rather than refused, with no error and no counter. That is
[issue 0819](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/0819-xrce-payloads-near-the-mtu-arrive-corrupted.md),
open at time of writing. Keep payloads comfortably below the MTU.

#### Compile-time only (not environment variables)

These are `#define`s in `packages/rmw/xrce/nros-rmw-xrce/src/internal.h` with no
env or Kconfig knob. Changing them means editing that header.

| Define                            | Description                             | Default |
|-----------------------------------|-----------------------------------------|---------|
| `XRCE_ENTITY_CREATION_TIMEOUT_MS` | Timeout for entity creation             | `1000`  |
| `XRCE_SERVICE_REPLY_TIMEOUT_MS`   | Per-attempt service reply timeout       | `50`    |
| `XRCE_SERVICE_REPLY_TOTAL_MS`     | Total service reply budget              | `5000`  |
| `XRCE_SESSION_FLUSH_TIMEOUT_MS`   | Session flush timeout                   | `100`   |
| `XRCE_SESSION_CREATION_RETRIES`   | Session creation retries                | `3`     |
| `XRCE_MAX_PENDING_REPLIES`        | In-flight service replies               | `4`     |
| `XRCE_SERVICE_REQUEST_RING_DEPTH` | Queued service requests per server      | `4`     |
| `XRCE_DEFAULT_AGENT_PORT`         | Default agent port                      | `2018`  |

### Core (`NROS_*`)

| Variable                        | Description                                                                              | Default | Crate       |
|---------------------------------|------------------------------------------------------------------------------------------|---------|-------------|
| `NROS_EXECUTOR_MAX_CBS`         | Max executor callback slots (compile-time fixed array size)                              | `4`     | nros-node   |
| `NROS_EXECUTOR_ARENA_SIZE`      | Executor arena size in bytes (compile-time fixed array size)                             | `4096`  | nros-node   |
| `NROS_SUBSCRIPTION_BUFFER_SIZE` | Default subscription/service buffer size (bytes)                                         | `1024`  | nros-node   |
| `NROS_LET_BUFFER_SIZE`          | Buffer size for LET semantics per handle                                                 | `512`   | nros-c      |
| `NROS_MAX_CONCURRENT_GOALS`     | Max concurrent goals per action server (compile-time constant, not env-var configurable) | `4`     | nros-c      |
| `NROS_MAX_PARAMETERS`           | Max parameters in parameter server                                                       | `32`    | nros-params |
| `NROS_MAX_PARAM_NAME_LEN`       | Max parameter name length                                                                | `64`    | nros-params |
| `NROS_MAX_STRING_VALUE_LEN`     | Max string parameter value length                                                        | `256`   | nros-params |
| `NROS_MAX_ARRAY_LEN`            | Max parameter array length                                                               | `32`    | nros-params |
| `NROS_MAX_BYTE_ARRAY_LEN`       | Max byte array parameter length                                                          | `256`   | nros-params |
| `NROS_MAX_PARAM_DESCRIPTION_LEN` | Max parameter description length; longer is truncated and logged, 0 = none (max 256)   | `256`   | nros-params |
