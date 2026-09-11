//! # Configuration
//!
//! ## Runtime environment variables
//!
//! [`ExecutorConfig::from_env()`](crate::env::ExecutorConfigEnvExt::from_env) reads these at startup:
//!
//! | Variable | Description | Default |
//! |----------|-------------|---------|
//! | `ROS_DOMAIN_ID` | ROS 2 domain ID | `0` |
//! | `ZENOH_LOCATOR` | Router address (`tcp/…`, `udp/…`, or `tls/…`) | `tcp/127.0.0.1:7447` |
//! | `ZENOH_MODE` | Session mode: `client` or `peer` | `client` |
//! | `ZENOH_TLS_ROOT_CA_CERTIFICATE` | Path to CA certificate (PEM) | — |
//! | `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64` | Base64-encoded CA cert (bare-metal) | — |
//!
//! ## Buffer tuning (build-time)
//!
//! Set these environment variables **before** `cargo build`.  After
//! changing a value, run `cargo clean -p zpico-sys` (or `xrce-sys`) to
//! force a rebuild.
//!
//! **Zenoh backend (`ZPICO_*`):**
//!
//! | Variable | Description | Posix | Embedded |
//! |----------|-------------|-------|----------|
//! | `ZPICO_FRAG_MAX_SIZE` | Max reassembled message size | 65536 | 2048 |
//! | `ZPICO_BATCH_UNICAST_SIZE` | Max unicast batch before fragmentation | 65536 | 1024 |
//! | `ZPICO_BATCH_MULTICAST_SIZE` | Max multicast batch size | 8192 | 1024 |
//! | `NROS_SUBSCRIBER_BUFFER_SIZE` | Per-subscriber buffer in zenoh shim | 1024 | 1024 |
//! | `ZPICO_SERVICE_BUFFER_SIZE` | Per-service-server buffer in zenoh shim | 1024 | 1024 |
//!
//! **XRCE-DDS backend (`XRCE_*`):**
//!
//! | Variable | Description | Posix | Embedded |
//! |----------|-------------|-------|----------|
//! | `XRCE_TRANSPORT_MTU` | Transport MTU (also sizes stream buffers) | 4096 | 512 |
//! | `XRCE_BUFFER_SIZE` | Default of the three receive families below | 1024 | 1024 |
//! | `XRCE_SUBSCRIBER_BUFFER_SIZE` | Subscriber ring entry | `XRCE_BUFFER_SIZE` | ditto |
//! | `XRCE_SERVICE_REQUEST_BUFFER_SIZE` | Service-server request entry | `XRCE_BUFFER_SIZE` | ditto |
//! | `XRCE_SERVICE_REPLY_BUFFER_SIZE` | Service-client reply slot | `XRCE_BUFFER_SIZE` | ditto |
//! | `XRCE_STREAM_HISTORY` | Reliable stream history depth (>= 2) | 4 | 4 |
//!
//! The three families were one number until phase-454 W6.b, so a subscriber
//! ring entry paid for the largest type a SERVICE carried, 32 x 8 times over.
//! Two of them are also DERIVED from what the image declares when it declares
//! it (RFC-0100): the subscriber entry takes the largest wire bound over the
//! declared subscriptions and `XRCE_STREAM_HISTORY` drops to the protocol
//! floor of 4 when no endpoint declares `reliability = reliable`. Stating
//! either here still wins.
//!
//! **Core (`NROS_*`, C API only):**
//!
//! | Variable | Description | Default |
//! |----------|-------------|---------|
//! | `NROS_MAX_PARAMETERS` | Max parameters in parameter server | 32 |
//!
//! Example — increase zenoh defrag to 128 KB for large point clouds:
//!
//! ```bash
//! ZPICO_FRAG_MAX_SIZE=131072 cargo build --features rmw-zenoh,platform-posix
//! ```
