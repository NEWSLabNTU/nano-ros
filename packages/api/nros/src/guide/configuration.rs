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
//! | `NROS_SERVICE_INBOX_BYTES` | Request slot of a user service server's inbox ring (zenoh); `ZPICO_SERVICE_BUFFER_SIZE` is its one-release alias | 1024 | 1024 |
//! | `NROS_SERVICE_INBOX_DEPTH` | Requests a user service server holds before dropping the newest | 4 | 4 |
//! | `NROS_ACTION_INBOX_BYTES` | Request slot of an action server queryable's inbox ring | `NROS_SERVICE_INBOX_BYTES` | ditto |
//! | `NROS_ACTION_INBOX_DEPTH` | Requests an action server queryable holds (the twin of `ZPICO_MAX_PENDING_REPLIES`) | 4 | 4 |
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
//! The zenoh service inbox is per FAMILY since phase-461 W1 (issue 1352): a
//! user service and an action server queryable each take a slot size and a
//! ring depth of their own, and the parameter and lifecycle services bring
//! their own ring, sized by `nros-node` from the contract's declared
//! parameters. An image that states nothing gets the one table it had.
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
