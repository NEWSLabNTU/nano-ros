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
//! build`) and use `_sized` method variants for larger per-entity buffers:
//!
//! ```ignore
//! // 4 KB receive buffer
//! executor.add_subscription_sized::<MyMsg, _, 4096>("/topic", |msg| { ... })?;
//! ```
//!
//! ## zenoh version mismatch
//!
//! zenoh-pico and zenohd must be the same version (1.6.2).  Symptoms:
//! `z_publisher_put failed: -100` (`_Z_ERR_TRANSPORT_TX_FAILED`) followed
//! by `-73` (`_Z_ERR_SESSION_CLOSED`).
//!
//! Build zenohd from the pinned submodule (`just build-zenohd`) or install
//! the matching version.
//!
//! ## Build issues
//!
//! - **Submodule not found** — run `git submodule update --init --recursive`
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
