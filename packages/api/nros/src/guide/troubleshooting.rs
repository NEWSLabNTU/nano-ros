//! # Troubleshooting
//!
//! ## Message too large / truncated
//!
//! Messages pass through multiple buffer layers.  A message must fit every
//! layer to be delivered intact:
//!
//! | Layer | Env var | Posix default |
//! |-------|---------|---------------|
//! | Defragmentation | `ZPICO_FRAG_MAX_SIZE` | 65536 |
//! | Batch size | `ZPICO_BATCH_UNICAST_SIZE` | 65536 |
//! | Shim buffer | `ZPICO_SUBSCRIBER_BUFFER_SIZE` | 1024 |
//! | User receive buffer | — (const generic) | 1024 |
//!
//! For large messages, increase the transport limits (set before `cargo
//! build`) and size the per-entity buffer via the builder's `.rx_buffer::<N>()`:
//!
//! ```ignore
//! // 4 KB receive buffer
//! executor.node_mut(node).subscription("/topic").typed::<MyMsg>()
//!     .rx_buffer::<4096>().build(|msg| { ... })?;
//! ```
//!
//! ## zenoh version mismatch
//!
//! zenoh-pico and zenohd must be the same version (1.6.2).  Symptoms:
//! `z_publisher_put failed: -100` (`_Z_ERR_TRANSPORT_TX_FAILED`) followed
//! by `-73` (`_Z_ERR_SESSION_CLOSED`).
//!
//! The router is the one ROS ships: `ros2 run rmw_zenoh_cpp rmw_zenohd`.
//! nano-ros vendors none (RFC-0075), and the old `just build-zenohd` recipe
//! was retired with the vendored copy.
//!
//! ## Build issues
//!
//! - **Submodule not found** — init that ONE submodule, scoped:
//!   `git submodule update --init packages/rmw/zenoh/zpico-sys/zenoh-pico`.
//!   Never `--recursive` on nano-ros — the transitive closure is large and
//!   mostly irrelevant to any one task. `nros setup <board> --rmw zenoh`
//!   provisions it for you, shallow, and is the preferred route.
//! - **CMake cache stale** (changed env vars not taking effect) — run
//!   `cargo clean -p zpico-sys` then rebuild
//!
//! ## zenoh-pico error codes
//!
//! | Code | Name | Meaning |
//! |------|------|---------|
//! | -3 | `_Z_ERR_TRANSPORT_OPEN_FAILED` | Cannot connect to router |
//! | -73 | `_Z_ERR_SESSION_CLOSED` | Session closed after failure |
//! | -78 | `_Z_ERR_SYSTEM_OUT_OF_MEMORY` | Allocation failed |
//! | -100 | `_Z_ERR_TRANSPORT_TX_FAILED` | Transport transmission failed |
//! | -128 | `_Z_ERR_GENERIC` | Generic error |
