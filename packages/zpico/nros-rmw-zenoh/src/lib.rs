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

    #[cfg(not(feature = "lending"))]
    use nros_rmw_cffi::RustBackendAdapter;
    use nros_rmw_cffi::{NROS_RMW_RET_OK, NrosRmwRet};

    #[cfg(not(feature = "lending"))]
    use crate::ZenohRmw;

    /// C entry — installs the zenoh-pico vtable into the cffi
    /// runtime under the canonical name `"zenoh"`. Returns
    /// `NROS_RMW_RET_OK` (0) on success. Idempotent — duplicate
    /// `("zenoh", vtable)` registrations are in-place overwrites.
    ///
    /// Phase 124.A.4.b — when the `lending` feature is on, install
    /// a vtable that overrides `pub_loan/_commit/_discard` with
    /// zenoh-pico-specific trampolines (zero-copy aliased publish).
    /// Without `lending`, fall back to the generic adapter vtable
    /// whose loan slots are NULL — runtime arena fallback applies.
    #[cfg(not(feature = "lending"))]
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_rmw_zenoh_register() -> NrosRmwRet {
        unsafe { RustBackendAdapter::<ZenohRmw>::register_named(c"zenoh".as_ptr()) }
    }

    #[cfg(feature = "lending")]
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_rmw_zenoh_register() -> NrosRmwRet {
        unsafe {
            nros_rmw_cffi::nros_rmw_cffi_register_named(
                c"zenoh".as_ptr(),
                &super::loan_trampolines::ZENOH_VTABLE,
            )
        }
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

    // Phase 104.A — POSIX auto-registration. `.init_array` is walked
    // by libc startup before `main`; the entry points to
    // `auto_register_ctor` which calls `nros_rmw_zenoh_register`. Pure-
    // Rust binaries that depend on this crate pick up registration
    // automatically; users don't need an explicit `register()` call.
    //
    // Bare-metal targets (RTIC, FreeRTOS, NuttX, ThreadX, Zephyr,
    // orin-spe) typically skip `.init_array` walking — those callers
    // continue to invoke `register()` from `main`. The cfg below
    // gates on POSIX-class hosts (`target_os = "linux" |
    // "macos" | "freebsd"`) where the section is honoured.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    #[used]
    #[unsafe(link_section = ".init_array")]
    static AUTO_REGISTER_CTOR: extern "C" fn() = auto_register_ctor;

    #[cfg(target_os = "macos")]
    #[used]
    #[unsafe(link_section = "__DATA,__mod_init_func")]
    static AUTO_REGISTER_CTOR: extern "C" fn() = auto_register_ctor;

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    extern "C" fn auto_register_ctor() {
        // SAFETY: idempotent vtable install. The atomic slot accepts
        // re-registration; if the user has already called register()
        // manually, this is a no-op. Errors are ignored — the next
        // `Executor::open` will fail with a clear message via
        // `nros_rmw_cffi::backend_registered()`.
        let _ = nros_rmw_zenoh_register();
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

// ============================================================================
// Phase 124.A.4.b — zenoh-pico cffi loan trampolines
// ============================================================================
//
// When the `lending` feature is on, the cffi register installs a
// vtable whose `pub_loan/_commit/_discard` slots call into
// `ZenohPublisher`'s native single-slot arena + aliased-publish path
// (Phase 99.F). C/C++ callers get the same zero-copy semantics Rust
// callers have through the `SlotLending` trait — no staging-buffer
// memcpy in the cffi fallback.
//
// Storage discipline (mirrors `RustBackendAdapter`):
//   - `NrosRmwPublisher::backend_data` was set by `create_publisher`
//     to `Box::into_raw(Box<ZenohPublisher>)`. Trampolines cast back
//     to `&ZenohPublisher`.
//   - The loan trampoline boxes a lifetime-erased `ZenohSlot<'static>`
//     and stows the raw pointer in `*out_token`. Commit / discard /
//     drop reclaim the box.
#[cfg(all(
    feature = "lending",
    any(
        feature = "platform-posix",
        feature = "platform-zephyr",
        feature = "platform-bare-metal",
        feature = "platform-freertos",
        feature = "platform-nuttx",
        feature = "platform-threadx",
        feature = "platform-orin-spe",
    ),
))]
mod loan_trampolines {
    extern crate alloc;
    use alloc::boxed::Box;
    use core::ffi::c_void;

    use nros_rmw::SlotLending;
    use nros_rmw_cffi::{
        NROS_RMW_RET_ERROR, NROS_RMW_RET_OK, NROS_RMW_RET_WOULD_BLOCK, NrosRmwPublisher,
        NrosRmwRet, NrosRmwVtable, RustBackendAdapter,
    };

    use crate::{ZenohRmw, shim::publisher::ZenohSlot};

    type ZenohPublisher =
        <<ZenohRmw as nros_rmw::Rmw>::Session as nros_rmw::Session>::PublisherHandle;

    /// Static-lifetime alias backing the boxed token. The cffi
    /// runtime guarantees the publisher outlives every outstanding
    /// loan (commit / discard / Drop all run before publisher
    /// destruction); `'static` is the cheapest way to erase the
    /// borrow checker's perspective.
    type StaticSlot = ZenohSlot<'static>;

    unsafe extern "C" fn zenoh_pub_loan(
        publisher: *mut NrosRmwPublisher,
        requested_len: usize,
        out_buf: *mut *mut u8,
        out_cap: *mut usize,
        out_token: *mut *mut c_void,
    ) -> NrosRmwRet {
        if publisher.is_null()
            || out_buf.is_null()
            || out_cap.is_null()
            || out_token.is_null()
            || requested_len == 0
        {
            return nros_rmw_cffi::NROS_RMW_RET_INVALID_ARGUMENT;
        }
        let backend_data = unsafe { (*publisher).backend_data };
        if backend_data.is_null() {
            return nros_rmw_cffi::NROS_RMW_RET_INVALID_ARGUMENT;
        }
        let pub_handle = unsafe { &*(backend_data as *const ZenohPublisher) };
        match pub_handle.try_lend_slot(requested_len) {
            Ok(Some(mut slot)) => {
                let buf_ptr = slot.as_mut().as_mut_ptr();
                let cap = slot.as_mut().len();
                // SAFETY: erase lifetime — cffi-runtime contract
                // guarantees the publisher outlives the loan.
                let static_slot: StaticSlot =
                    unsafe { core::mem::transmute::<ZenohSlot<'_>, StaticSlot>(slot) };
                let boxed = Box::new(static_slot);
                unsafe {
                    *out_buf = buf_ptr;
                    *out_cap = cap;
                    *out_token = Box::into_raw(boxed) as *mut c_void;
                }
                NROS_RMW_RET_OK
            }
            Ok(None) => NROS_RMW_RET_WOULD_BLOCK,
            Err(_) => NROS_RMW_RET_ERROR,
        }
    }

    unsafe extern "C" fn zenoh_pub_commit(
        publisher: *mut NrosRmwPublisher,
        token: *mut c_void,
        actual_len: usize,
    ) -> NrosRmwRet {
        if publisher.is_null() || token.is_null() {
            return nros_rmw_cffi::NROS_RMW_RET_INVALID_ARGUMENT;
        }
        let backend_data = unsafe { (*publisher).backend_data };
        if backend_data.is_null() {
            return nros_rmw_cffi::NROS_RMW_RET_INVALID_ARGUMENT;
        }
        let pub_handle = unsafe { &*(backend_data as *const ZenohPublisher) };
        let mut slot: Box<StaticSlot> = unsafe { Box::from_raw(token as *mut StaticSlot) };
        slot.truncate(actual_len);
        match pub_handle.commit_slot(*slot) {
            Ok(()) => NROS_RMW_RET_OK,
            Err(_) => NROS_RMW_RET_ERROR,
        }
    }

    unsafe extern "C" fn zenoh_pub_discard(_publisher: *mut NrosRmwPublisher, token: *mut c_void) {
        if token.is_null() {
            return;
        }
        // SAFETY: token came from `Box::into_raw(Box<StaticSlot>)` in
        // `zenoh_pub_loan`. Reconstitute and drop — ZenohSlot::drop
        // releases the arena.
        let _slot: Box<StaticSlot> = unsafe { Box::from_raw(token as *mut StaticSlot) };
    }

    /// Customised zenoh vtable: base = generic `RustBackendAdapter`
    /// trampolines for all standard slots; loan slots overridden to
    /// route through zenoh-pico's aliased-publish path.
    pub(super) static ZENOH_VTABLE: NrosRmwVtable = NrosRmwVtable {
        pub_loan: Some(zenoh_pub_loan),
        pub_commit: Some(zenoh_pub_commit),
        pub_discard: Some(zenoh_pub_discard),
        ..RustBackendAdapter::<ZenohRmw>::VTABLE
    };
}
