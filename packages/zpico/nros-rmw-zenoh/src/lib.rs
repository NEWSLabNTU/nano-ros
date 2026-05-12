//! nros-rmw-zenoh: Zenoh-pico RMW backend for nros
//!
//! This crate provides the zenoh-pico transport implementation,
//! combining the safe Rust API over zenoh-pico FFI with the
//! transport layer that implements nros-rmw traits.
//!
//! # Platform Backends
//!
//! Select one backend via feature flags:
//! - `platform-posix` - Uses POSIX threads, for desktop testing
//! - `platform-zephyr` - Uses Zephyr RTOS threads
//! - `platform-bare-metal` - Uses polling (bare-metal platforms)
//! - `platform-freertos` - Uses FreeRTOS threads + lwIP sockets
//! - `platform-threadx` - Uses ThreadX threads + NetX Duo sockets

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
    feature = "platform-orin-spe",
))]
pub(crate) mod config;
pub mod keyexpr;
pub mod zpico;

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
    feature = "platform-orin-spe",
))]
pub mod shim;

// Re-export zpico types (always available)
pub use zpico::{ZenohId, ZpicoError};

// Re-export platform-gated zpico types
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
    feature = "platform-orin-spe",
))]
pub use zpico::{
    Context, LivelinessToken, Publisher as ZpicoPublisher, Queryable, Subscriber as ZpicoSubscriber,
};

// Re-export shim types when platform feature is enabled
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
    feature = "platform-orin-spe",
))]
pub use shim::{
    MessageInfo, RMW_GID_SIZE, RmwAttachment, Ros2Liveliness, SERVICE_BUFFER_SIZE,
    SUBSCRIBER_BUFFER_SIZE, ZenohPublisher, ZenohRmw, ZenohServiceClient, ZenohServiceServer,
    ZenohSession, ZenohSubscriber, ZenohTransport,
};

// Re-export std-only executor wake functions
#[cfg(all(
    feature = "std",
    any(
        feature = "platform-posix",
        feature = "platform-zephyr",
        feature = "platform-bare-metal"
    )
))]
pub use shim::{signal_executor_wake, wait_for_executor_wake};

// Re-export extension traits
pub use keyexpr::{QosKeyExpr, ServiceKeyExpr, TopicKeyExpr};

// Re-export safety types when feature is enabled
#[cfg(feature = "safety-e2e")]
pub use nros_rmw::{IntegrityStatus, SafetyValidator, crc32};

// ============================================================================
// Phase 115.M.3 — C-vtable register entry (folded in from the
// retired `nros-rmw-zenoh-cffi` crate).
// ============================================================================
//
// The vtable IS the cross-language boundary. Once registered, runtime
// dispatch goes Rust→vtable→… directly; backends never `use` each
// other's trait surface. So the register fn lives next to the trait
// impl, and the legacy `*-cffi` two-crate split goes away.

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
    feature = "platform-orin-spe",
))]
mod cffi_register {
    use core::ffi::c_int;

    use nros_rmw_cffi::{NROS_RMW_RET_OK, NrosRmwRet, RustBackendAdapter};

    use crate::ZenohRmw;

    /// C entry — installs the zenoh-pico vtable into the cffi
    /// runtime. Returns `NROS_RMW_RET_OK` (0) on success.
    /// Idempotent — the runtime's atomic vtable slot accepts the
    /// most-recently-registered value, so re-calls are no-ops.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_rmw_zenoh_register() -> NrosRmwRet {
        RustBackendAdapter::<ZenohRmw>::register()
    }

    /// Failure mode for the safe Rust wrapper.
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct RegisterError(pub c_int);

    /// Safe Rust wrapper around [`nros_rmw_zenoh_register`]. Returns
    /// `Err(RegisterError(rc))` when the runtime rejects the vtable.
    pub fn register() -> Result<(), RegisterError> {
        let rc = nros_rmw_zenoh_register();
        if rc == NROS_RMW_RET_OK {
            Ok(())
        } else {
            Err(RegisterError(rc))
        }
    }
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
    feature = "platform-orin-spe",
))]
pub use cffi_register::{RegisterError, nros_rmw_zenoh_register, register};
