//! nros C++ API — header-only C++14 library + Rust FFI staticlib.
//!
//! This crate provides `extern "C"` functions designed for the nros-cpp
//! C++ headers. Unlike `nros-c` (which erases types into opaque handles),
//! `nros-cpp` preserves type information through the FFI boundary — each
//! message/service/action type gets its own FFI function.
//!
//! # Architecture
//!
//! ```text
//! C++ (nros-cpp headers)  →  extern "C"  →  nros-cpp (Rust)  →  nros-node
//! ```
//!
//! The C++ side provides inline opaque storage for all entity handles
//! (publisher, subscription, service, guard condition, executor, action).
//! No heap allocation required — fully alloc-free.
//!
//! All serialization/deserialization happens on the runtime.

#![no_std]
#![allow(non_camel_case_types)]
#![allow(dead_code)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "panic-halt")]
use panic_halt as _;

#[cfg(feature = "rmw-xrce-cffi")]
extern crate nros_rmw_xrce_cffi;

// Opt-in RTOS heap-usage tracking (issue #6). A single shared `HeapStats`
// counter instruments whichever RTOS global allocator is active (exactly one
// platform feature is on at a time). `STATS` sees the Rust global allocator's
// footprint only — zenoh-pico's direct C-side z_malloc/pvPortMalloc traffic is
// not counted, so it under-reports true heap pressure.
//
// Phase 230 1b.3 / RFC-0034 D7 — the platform ABI exposes the TRUE *unified*
// heap figures (`nros_platform_heap_used_bytes` / `_total_bytes`), where the
// platform owns one kernel heap shared by the C side and the Rust
// `#[global_allocator]`. Design: keep `nros_heap_used_bytes()` /
// `nros_heap_peak_bytes()` as the Rust-footprint view (unchanged semantics, so
// callers tracking only the Rust allocator keep their meaning) and add
// `nros_heap_platform_used_bytes()` + `nros_heap_total_bytes()` that forward to
// the platform query for the unified figure. Both return `0` on ports that
// don't instrument their heap.
#[cfg(feature = "alloc-stats")]
mod heap_stats {
    pub static STATS: zpico_alloc::HeapStats = zpico_alloc::HeapStats::new();

    // Canonical platform heap query (RFC-0034 D7). Resolved at the final
    // C-binary link step from the linked `nros-platform-<rtos>` cffi shim.
    unsafe extern "C" {
        fn nros_platform_heap_used_bytes() -> usize;
        fn nros_platform_heap_total_bytes() -> usize;
    }

    /// Bytes currently outstanding through the Rust global allocator.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_heap_used_bytes() -> usize {
        STATS.used()
    }

    /// Peak outstanding bytes through the Rust global allocator since boot.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_heap_peak_bytes() -> usize {
        STATS.peak()
    }

    /// Bytes currently outstanding from the platform's *unified* heap — the
    /// true figure spanning both the Rust global allocator and the C side
    /// (zenoh-pico etc.), where the port owns one shared kernel heap. `0` if
    /// the port does not instrument heap usage.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_heap_platform_used_bytes() -> usize {
        unsafe { nros_platform_heap_used_bytes() }
    }

    /// Total managed heap size in bytes (used + free) reported by the
    /// platform, or `0` if unknown.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_heap_total_bytes() -> usize {
        unsafe { nros_platform_heap_total_bytes() }
    }
}

// FreeRTOS global allocator: wraps pvPortMalloc/vPortFree for alloc on no_std.
// FreeRTOS heap_4 returns 8-byte aligned pointers, sufficient for all nros types.
#[cfg(all(feature = "alloc", not(feature = "std"), feature = "platform-freertos"))]
mod freertos_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    // Phase 230 1d / RFC-0034 D6 — route through the platform ABI
    // (`nros_platform_alloc` → `pvPortMalloc`) so the C++ API Rust heap
    // shares the one funnel with zenoh-pico's C side.
    unsafe extern "C" {
        fn nros_platform_alloc(size: usize) -> *mut core::ffi::c_void;
        fn nros_platform_dealloc(ptr: *mut core::ffi::c_void);
    }

    struct FreeRtosAllocator;

    unsafe impl GlobalAlloc for FreeRtosAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { nros_platform_alloc(layout.size()) as *mut u8 };
            #[cfg(feature = "alloc-stats")]
            if !p.is_null() {
                crate::heap_stats::STATS.on_alloc(layout.size());
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { nros_platform_dealloc(ptr as *mut core::ffi::c_void) }
            #[cfg(feature = "alloc-stats")]
            crate::heap_stats::STATS.on_dealloc(_layout.size());
        }
    }

    #[global_allocator]
    static ALLOCATOR: FreeRtosAllocator = FreeRtosAllocator;
}

// Zephyr global allocator: wraps Zephyr's k_malloc/k_free, backed by
// CONFIG_HEAP_MEM_POOL_SIZE. Required for the C++ API path on Zephyr
// targets that don't bring zephyr-lang-rust's static_alloc with them
// (e.g. qemu_cortex_a9 with the DDS RMW backend). Phase 71.6.
#[cfg(all(feature = "alloc", not(feature = "std"), feature = "platform-zephyr"))]
mod zephyr_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    // Phase 230 1d / RFC-0034 D6 — route through the platform ABI.
    unsafe extern "C" {
        fn nros_platform_alloc(size: usize) -> *mut core::ffi::c_void;
        fn nros_platform_dealloc(ptr: *mut core::ffi::c_void);
    }

    struct ZephyrAllocator;

    unsafe impl GlobalAlloc for ZephyrAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { nros_platform_alloc(layout.size()) as *mut u8 };
            #[cfg(feature = "alloc-stats")]
            if !p.is_null() {
                crate::heap_stats::STATS.on_alloc(layout.size());
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { nros_platform_dealloc(ptr as *mut core::ffi::c_void) }
            #[cfg(feature = "alloc-stats")]
            crate::heap_stats::STATS.on_dealloc(_layout.size());
        }
    }

    #[global_allocator]
    static ALLOCATOR: ZephyrAllocator = ZephyrAllocator;

    // Minimal panic handler for the no_std + platform-zephyr build.
    #[panic_handler]
    fn panic(_info: &core::panic::PanicInfo) -> ! {
        loop {
            core::hint::spin_loop();
        }
    }
}

// critical-section impl backed by Zephyr's nros_zephyr_irq_lock /
// nros_zephyr_irq_unlock. portable-atomic requires this whenever the
// Zephyr C++ staticlib is linked without the zephyr-lang-rust crate, including
// native_sim std builds.
#[cfg(feature = "platform-zephyr")]
mod zephyr_critical_section {
    unsafe extern "C" {
        fn nros_zephyr_irq_lock() -> u32;
        fn nros_zephyr_irq_unlock(key: u32);
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe fn acquire_irq_key() -> critical_section::RawRestoreState {
        unsafe { nros_zephyr_irq_lock() }
    }

    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    unsafe fn acquire_irq_key() -> critical_section::RawRestoreState {
        let _ = unsafe { nros_zephyr_irq_lock() };
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe fn release_irq_key(token: critical_section::RawRestoreState) {
        unsafe { nros_zephyr_irq_unlock(token) }
    }

    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    unsafe fn release_irq_key(_token: critical_section::RawRestoreState) {
        unsafe { nros_zephyr_irq_unlock(0) }
    }

    struct ZephyrCs;
    critical_section::set_impl!(ZephyrCs);

    unsafe impl critical_section::Impl for ZephyrCs {
        unsafe fn acquire() -> critical_section::RawRestoreState {
            unsafe { acquire_irq_key() }
        }

        unsafe fn release(token: critical_section::RawRestoreState) {
            unsafe { release_irq_key(token) }
        }
    }
}

// ThreadX global allocator (C++ API path). Phase 230 1d / RFC-0034 D6 —
// route through the platform ABI directly (`nros_platform_alloc` →
// `tx_byte_allocate`); was `z_malloc`, the alias forwarding to the same.
#[cfg(all(feature = "alloc", not(feature = "std"), feature = "platform-threadx"))]
mod threadx_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    unsafe extern "C" {
        fn nros_platform_alloc(size: usize) -> *mut core::ffi::c_void;
        fn nros_platform_dealloc(ptr: *mut core::ffi::c_void);
    }

    struct ThreadXAllocator;

    unsafe impl GlobalAlloc for ThreadXAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { nros_platform_alloc(layout.size()) as *mut u8 };
            #[cfg(feature = "alloc-stats")]
            if !p.is_null() {
                crate::heap_stats::STATS.on_alloc(layout.size());
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { nros_platform_dealloc(ptr as *mut core::ffi::c_void) }
            #[cfg(feature = "alloc-stats")]
            crate::heap_stats::STATS.on_dealloc(_layout.size());
        }
    }

    #[global_allocator]
    static ALLOCATOR: ThreadXAllocator = ThreadXAllocator;
}

use core::ffi::{c_char, c_int, c_void};

// Phase 161 — mirror nros-c's Phase 134.fix. Declaring
// `nros_rmw_zenoh_register` as a plain `extern "C"` symbol keeps the
// public surface (downstream C/C++ glue may resolve this) without
// pulling `nros-rmw-zenoh` into `libnros_cpp.a`'s Rust dep graph. The
// linker resolves the symbol at the C-binary link step from
// `libnros_rmw_zenoh.a` (the standalone staticlib).
#[cfg(feature = "rmw-zenoh-cffi")]
unsafe extern "C" {
    pub fn nros_rmw_zenoh_register() -> i32;
}

// ── Core entity modules (alloc-free — caller provides inline storage) ──
#[cfg(feature = "rmw-cffi")]
mod guard_condition;
#[cfg(feature = "rmw-cffi")]
mod publisher;
#[cfg(feature = "rmw-cffi")]
mod service;
#[cfg(feature = "rmw-cffi")]
mod subscription;
#[cfg(feature = "rmw-cffi")]
mod timer;

// ── Action module (alloc-free — caller provides inline storage) ──
#[cfg(feature = "rmw-cffi")]
mod action;

// Phase 115.D — runtime-pluggable custom transport. Always-on (no
// rmw-* gate) because the registration is platform-side, not RMW-side.
mod transport;

// ── Tick-time client dispatch (Phase 212.M-F.4.c) ──
//
// Mirror of the Rust substrate's `TickCtx::call_raw` /
// `TickCtx::send_goal_raw` seams added in Phase 212.M-F.4 (`d15565efe`).
// Always-on (no rmw-cffi gate) because the stub error path is independent
// of any RMW backend — the symbols exist + return `NROS_CPP_RET_ERROR`
// until the codegen-side `GenClientDispatch` impl lands (M-F.4.a).
mod tick_ctx;

// ============================================================================
// Error codes (mirror nros-c for consistency)
// ============================================================================

/// Return type for nros C++ FFI functions.
pub type nros_cpp_ret_t = c_int;

/// Success.
pub const NROS_CPP_RET_OK: nros_cpp_ret_t = 0;
/// Generic error.
pub const NROS_CPP_RET_ERROR: nros_cpp_ret_t = -1;
/// Timeout.
pub const NROS_CPP_RET_TIMEOUT: nros_cpp_ret_t = -2;
/// Invalid argument.
pub const NROS_CPP_RET_INVALID_ARGUMENT: nros_cpp_ret_t = -3;
/// Not initialized.
pub const NROS_CPP_RET_NOT_INIT: nros_cpp_ret_t = -4;
/// Resource limit reached.
pub const NROS_CPP_RET_FULL: nros_cpp_ret_t = -5;
/// Try again — operation not ready yet.
pub const NROS_CPP_RET_TRY_AGAIN: nros_cpp_ret_t = -6;
/// Reentrant call detected — executor is already spinning.
pub const NROS_CPP_RET_REENTRANT: nros_cpp_ret_t = -7;
/// Transport / connection error.
pub const NROS_CPP_RET_TRANSPORT_ERROR: nros_cpp_ret_t = -100;

/// Phase 108 — operation not implemented by the active backend.
pub const NROS_CPP_RET_UNSUPPORTED: nros_cpp_ret_t = -16;

// ============================================================================
// Inline opaque storage sizes (in u64 units)
// ============================================================================
//
// These constants define the inline storage for internal C++ FFI wrapper
// structs (CppPublisher, CppSubscription, etc.). The C++ side allocates
// buffers of this size; the runtime writes directly into them.
// Compile-time assertions in each module verify the storage is large enough.

// Opaque storage sizes computed from size_of at compile time — always exact.
// When no RMW backend is enabled (workspace-level check), placeholder values
// are used. The placeholders are never used at runtime.

const fn u64s_for<T>() -> usize {
    core::mem::size_of::<T>().div_ceil(8)
}

// With RMW backend: exact sizes from actual types.
// Phase 87.6: `CppPublisher` removed — the FFI stores an `RmwPublisher`
// handle directly, sized via `NROS_PUBLISHER_SIZE` from the `nros` probe
// (see packages/core/nros-cpp/build.rs).
// Phase 87.6: `CppSubscription` removed — the FFI stores an
// `RmwSubscriber` handle directly, sized via `NROS_SUBSCRIBER_SIZE` from
// the `nros` probe.
// Phase 87.6: `CppServiceServer` and `CppServiceClient` removed — the FFI
// stores `RmwServiceServer` / `RmwServiceClient` handles directly, sized
// via `NROS_SERVICE_SERVER_SIZE` / `NROS_SERVICE_CLIENT_SIZE` from the
// `nros` probe.
// Phase 87.11: `CPP_ACTION_SERVER_OPAQUE_U64S` and
// `CPP_ACTION_CLIENT_OPAQUE_U64S` removed. ActionServer/ActionClient
// storage sizes are now sourced from `nros::sizes::CppActionServerLayout`
// / `CppActionClientLayout` via the probe; see action.rs for the
// layout-mirror equality asserts.

// Phase 87.6: `CPP_GUARD_HANDLE_OPAQUE_U64S` removed — the C++
// `nros::GuardCondition` class sizes its `storage_` from
// `NROS_GUARD_CONDITION_SIZE` (`size_of::<GuardConditionHandle>()`
// probed from the nros rlib).

// ============================================================================
// QoS types (passed from C++ to Rust by value)
// ============================================================================

/// QoS reliability policy.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_cpp_qos_reliability_t {
    NROS_CPP_QOS_RELIABLE = 0,
    NROS_CPP_QOS_BEST_EFFORT = 1,
}

/// QoS durability policy.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_cpp_qos_durability_t {
    NROS_CPP_QOS_VOLATILE = 0,
    NROS_CPP_QOS_TRANSIENT_LOCAL = 1,
}

/// QoS history policy.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_cpp_qos_history_t {
    NROS_CPP_QOS_KEEP_LAST = 0,
    NROS_CPP_QOS_KEEP_ALL = 1,
}

/// QoS liveliness policy. Phase 108.B.7 — matches DDS `LIVELINESS`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_cpp_qos_liveliness_t {
    NROS_CPP_QOS_LIVELINESS_NONE = 0,
    NROS_CPP_QOS_LIVELINESS_AUTOMATIC = 1,
    NROS_CPP_QOS_LIVELINESS_MANUAL_BY_TOPIC = 2,
    NROS_CPP_QOS_LIVELINESS_MANUAL_BY_NODE = 3,
}

/// QoS settings (passed by value from C++).
///
/// Phase 108.B.7 — full DDS-shaped QoS surface. The four core fields
/// (`reliability`, `durability`, `history`, `depth`) plus extended
/// policies (`liveliness_kind`, `deadline_ms`, `lifespan_ms`,
/// `liveliness_lease_ms`, `avoid_ros_namespace_conventions`) match
/// `nros_qos_t` (C API) and `QosSettings` (Rust API).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_qos_t {
    pub reliability: nros_cpp_qos_reliability_t,
    pub durability: nros_cpp_qos_durability_t,
    pub history: nros_cpp_qos_history_t,
    pub liveliness_kind: nros_cpp_qos_liveliness_t,
    pub depth: c_int,
    /// Subscriber max-inter-arrival / publisher offered-rate, ms.
    /// `0` = infinite (no deadline check).
    pub deadline_ms: u32,
    /// Sample expiry, ms. `0` = infinite.
    pub lifespan_ms: u32,
    /// Liveliness lease, ms. `0` = infinite.
    pub liveliness_lease_ms: u32,
    /// If non-zero, topic-name encoding skips the `/rt/` ROS prefix.
    pub avoid_ros_namespace_conventions: u8,
}

impl nros_cpp_qos_t {
    pub(crate) fn to_qos_settings(self) -> nros_rmw::QosSettings {
        use nros_rmw::{
            QosDurabilityPolicy, QosHistoryPolicy, QosLivelinessPolicy, QosReliabilityPolicy,
        };

        nros_rmw::QosSettings {
            reliability: match self.reliability {
                nros_cpp_qos_reliability_t::NROS_CPP_QOS_RELIABLE => QosReliabilityPolicy::Reliable,
                nros_cpp_qos_reliability_t::NROS_CPP_QOS_BEST_EFFORT => {
                    QosReliabilityPolicy::BestEffort
                }
            },
            durability: match self.durability {
                nros_cpp_qos_durability_t::NROS_CPP_QOS_VOLATILE => QosDurabilityPolicy::Volatile,
                nros_cpp_qos_durability_t::NROS_CPP_QOS_TRANSIENT_LOCAL => {
                    QosDurabilityPolicy::TransientLocal
                }
            },
            history: match self.history {
                nros_cpp_qos_history_t::NROS_CPP_QOS_KEEP_LAST => QosHistoryPolicy::KeepLast,
                nros_cpp_qos_history_t::NROS_CPP_QOS_KEEP_ALL => QosHistoryPolicy::KeepAll,
            },
            liveliness_kind: match self.liveliness_kind {
                nros_cpp_qos_liveliness_t::NROS_CPP_QOS_LIVELINESS_NONE => {
                    QosLivelinessPolicy::None
                }
                nros_cpp_qos_liveliness_t::NROS_CPP_QOS_LIVELINESS_AUTOMATIC => {
                    QosLivelinessPolicy::Automatic
                }
                nros_cpp_qos_liveliness_t::NROS_CPP_QOS_LIVELINESS_MANUAL_BY_TOPIC => {
                    QosLivelinessPolicy::ManualByTopic
                }
                nros_cpp_qos_liveliness_t::NROS_CPP_QOS_LIVELINESS_MANUAL_BY_NODE => {
                    QosLivelinessPolicy::ManualByNode
                }
            },
            depth: self.depth as u32,
            deadline_ms: self.deadline_ms,
            lifespan_ms: self.lifespan_ms,
            liveliness_lease_ms: self.liveliness_lease_ms,
            avoid_ros_namespace_conventions: self.avoid_ros_namespace_conventions != 0,
        }
    }
}

// ============================================================================
// Build-time configuration
// ============================================================================

mod executor_config {
    include!(concat!(env!("OUT_DIR"), "/nros_cpp_ffi_config.rs"));
}
pub use executor_config::CPP_EXECUTOR_OPAQUE_U64S;

// Compile-time asserts that the auto-generated C-side STORAGE macros
// are large enough for their Rust counterparts. If a Rust type grows
// past the estimate emitted by build.rs, compilation fails with a
// clear error instead of silently overflowing caller-provided storage.
#[cfg(feature = "rmw-cffi")]
const _: () = {
    // Phase 87.6: `CppPublisher`, `CppSubscription`, `CppServiceServer`,
    // and `CppServiceClient` assertions removed — all four now use
    // thin-wrapper storage sized from the Rust SSoT (`NROS_*_SIZE`
    // probes in the generated header).
    // Phase 87.6: `GuardConditionHandle` assertion removed — storage
    // sized from `NROS_GUARD_CONDITION_SIZE` (probed).
};

// ============================================================================
// Executor handle (alloc-free — caller provides inline storage)
// ============================================================================

/// The concrete nros-node executor type used by the C++ FFI.
#[cfg(feature = "rmw-cffi")]
pub(crate) type CppExecutor = nros_node::Executor;

/// Context wrapping the executor and the domain ID.
///
/// The executor doesn't store domain_id itself — it's consumed during
/// session open. We keep it here so publisher/subscription creation
/// can pass the correct value to `TopicInfo::with_domain()`.
#[cfg(feature = "rmw-cffi")]
pub(crate) struct CppContext {
    pub(crate) executor: CppExecutor,
    pub(crate) domain_id: u32,
}

// Compile-time assertion: inline storage must fit CppContext.
#[cfg(feature = "rmw-cffi")]
const _: () = assert!(
    core::mem::size_of::<CppContext>() <= CPP_EXECUTOR_OPAQUE_U64S * core::mem::size_of::<u64>(),
    "CPP_EXECUTOR_OPAQUE_U64S too small for CppContext — increase NROS_EXECUTOR_ARENA_SIZE \
     or NROS_EXECUTOR_MAX_CBS, or adjust the overhead in build.rs"
);

// ============================================================================
// Init / Fini
// ============================================================================

/// Initialize an nros executor session.
///
/// Opens a middleware connection and writes the executor context directly
/// into caller-provided storage (no heap allocation).
///
/// # Parameters
/// * `locator` — Middleware locator (e.g., `"tcp/127.0.0.1:7447"`), or NULL for default.
/// * `domain_id` — ROS domain ID (0–232).
/// * `node_name` — Node name (null-terminated string). Must not be NULL.
/// * `namespace` — Node namespace (null-terminated string), or NULL for `"/"`.
/// * `storage` — Pointer to caller-provided storage (at least `CPP_EXECUTOR_OPAQUE_U64S * 8` bytes,
///   aligned to 8 bytes). The executor is written directly into this buffer.
///
/// # Safety
/// * `node_name` must be a valid null-terminated string.
/// * `locator` and `namespace` must be valid null-terminated strings or NULL.
/// * `storage` must be a valid pointer to appropriately sized and aligned storage.
///
/// # Returns
/// `NROS_CPP_RET_OK` on success, error code otherwise.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_init(
    locator: *const c_char,
    domain_id: u8,
    node_name: *const c_char,
    namespace: *const c_char,
    storage: *mut c_void,
) -> nros_cpp_ret_t {
    if node_name.is_null() || storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    unsafe extern "C" {
        fn nros_app_register_backends();
    }
    unsafe {
        nros_app_register_backends();
    }
    #[cfg(feature = "rmw-xrce-cffi")]
    {
        let _ = nros_rmw_xrce_cffi::register();
    }
    // Phase 161 — drop the redundant `nros_rmw_zenoh::register()` call;
    // `nros_app_register_backends()` above already calls
    // `nros_rmw_zenoh_register()` via the CMake-emitted strong stub
    // (`cmake/NanoRosLink.cmake:62-117`). Keeping a second registration
    // path used to pull `nros-rmw-zenoh` into the Rust dep graph and
    // produced the dual zenoh-pico instance bug — see Cargo.toml for
    // the full diagnosis.
    let node_name_str = match unsafe { cstr_to_str(node_name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    let ns_str = if namespace.is_null() {
        "/"
    } else {
        match unsafe { cstr_to_str(namespace) } {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    };

    let locator_str = if locator.is_null() {
        "tcp/127.0.0.1:7447"
    } else {
        match unsafe { cstr_to_str(locator) } {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    };

    let config = nros_node::ExecutorConfig::new(locator_str)
        .domain_id(domain_id as u32)
        .node_name(node_name_str)
        .namespace(ns_str);

    match CppExecutor::open(&config) {
        Ok(executor) => {
            let ctx = CppContext {
                executor,
                domain_id: domain_id as u32,
            };
            // Write directly into caller-provided storage — no heap allocation.
            unsafe { core::ptr::write(storage as *mut CppContext, ctx) };
            NROS_CPP_RET_OK
        }
        // Phase 155.C — surface the inner `NodeError` variant as a
        // specific `NROS_CPP_RET_*` code instead of collapsing every
        // backend failure to TRANSPORT_ERROR. Mirrors the C-side
        // `transport_error_to_ret` mapping from Phase 155.B so the
        // next `nros::init -> -X` log line in the FreeRTOS / RV64
        // C++ tests identifies which precondition the backend
        // rejected.
        Err(e) => node_error_to_cpp_ret(e),
    }
}

/// Phase 155.C — map `NodeError` to the closest `NROS_CPP_RET_*` code.
/// Unknown variants stay TRANSPORT_ERROR (-100) — the legacy catch-all.
#[cfg(feature = "rmw-cffi")]
fn node_error_to_cpp_ret(err: nros_node::NodeError) -> nros_cpp_ret_t {
    use nros_node::NodeError as E;
    use nros_rmw::TransportError as T;
    match err {
        E::NameTooLong => NROS_CPP_RET_INVALID_ARGUMENT,
        E::Serialization | E::Deserialization => NROS_CPP_RET_ERROR,
        E::BufferTooSmall => NROS_CPP_RET_FULL,
        E::Timeout => NROS_CPP_RET_TIMEOUT,
        E::NotInitialized => NROS_CPP_RET_NOT_INIT,
        E::RequestInFlight => NROS_CPP_RET_REENTRANT,
        E::Transport(t) => match t {
            T::ConnectionFailed | T::Disconnected => NROS_CPP_RET_TRANSPORT_ERROR,
            T::Timeout | T::WouldBlock => NROS_CPP_RET_TIMEOUT,
            T::InvalidConfig => NROS_CPP_RET_INVALID_ARGUMENT,
            T::BufferTooSmall | T::MessageTooLarge | T::TooLarge => NROS_CPP_RET_FULL,
            _ => NROS_CPP_RET_TRANSPORT_ERROR,
        },
        _ => NROS_CPP_RET_TRANSPORT_ERROR,
    }
}

/// Shut down an nros executor session.
///
/// Drops the executor in-place within the caller's storage.
///
/// # Safety
/// `storage` must point to a live `CppContext` written by `nros_cpp_init()`, or NULL (no-op).
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_fini(storage: *mut c_void) -> nros_cpp_ret_t {
    if storage.is_null() {
        return NROS_CPP_RET_OK;
    }

    unsafe {
        let ctx = &mut *(storage as *mut CppContext);
        let _ = ctx.executor.close();
        core::ptr::drop_in_place(storage as *mut CppContext);
    }

    NROS_CPP_RET_OK
}

// ============================================================================
// Node
// ============================================================================

/// Opaque node handle.
///
/// A node is a lightweight view into the executor: it borrows the
/// executor for its lifetime. The C++ FFI stores the executor pointer
/// plus the node name/namespace and re-creates the borrow when needed.
#[repr(C)]
pub struct nros_cpp_node_t {
    /// Pointer to the parent executor handle (not owned).
    pub executor: *mut c_void,
    /// Node name (null-terminated, max 64 bytes including null).
    pub name: [u8; NROS_CPP_NAME_LEN],
    /// Node namespace (null-terminated, max 64 bytes including null).
    pub namespace: [u8; NROS_CPP_NAMESPACE_LEN],
    /// Phase 104.C.9.b — opaque NodeId returned by
    /// `Executor::node_builder(...).build()`. `0` = primary Node
    /// (legacy single-Session creation path); non-zero values route
    /// publisher / subscription / service creation through the
    /// per-Node session resolved via
    /// `Executor::node_session_mut(NodeId)`.
    pub node_id: u8,
    /// Reserved for future use; pad to next u64 boundary.
    pub _reserved: [u8; NROS_CPP_NODE_RESERVED],
}

/// Maximum RMW backend name length for `nros_cpp_node_options_t`.
/// Mirrors `BACKEND_NAME_MAX` in `nros-rmw-cffi`.
pub const NROS_CPP_RMW_NAME_LEN: usize = 32;

/// Maximum per-Node locator override length for `nros_cpp_node_options_t`.
pub const NROS_CPP_LOCATOR_LEN: usize = 128;

/// Maximum namespace length for `nros_cpp_node_options_t`. Matches the
/// inline buffer in `nros_cpp_node_t`.
pub const NROS_CPP_NAMESPACE_LEN: usize = 64;

/// Maximum node name length for the inline buffer in `nros_cpp_node_t`. Phase
/// 192.5 — single source for what was an inlined `64` at every name-buffer site.
/// NOTE: `nros_node::limits` uses a larger namespace/name bound; this C++ ABI is
/// the deliberately-fixed 64-byte embedded inline — reconcile in a follow-up if
/// they are required to match (changing it is a `#[repr(C)]` ABI change).
pub const NROS_CPP_NAME_LEN: usize = 64;

/// Reserved padding bytes in `nros_cpp_node_t` (pad `node_id` to the next u64
/// boundary). Phase 192.5 — names the struct-layout `7`.
pub const NROS_CPP_NODE_RESERVED: usize = 7;

/// Sentinel value for `domain_id_override`. When set, the executor's
/// existing domain_id is used.
pub const NROS_CPP_DOMAIN_ID_INHERIT: u32 = u32::MAX;

/// Phase 104.C.9 — extended node-creation options (C++ FFI).
///
/// Mirrors `nros_node_options_t` in nros-c (Phase 104.C.8) — same field
/// shape, separate FFI surface so the two language wrappers can evolve
/// independently. Used by `nros_cpp_node_create_ex` and the C++
/// `NodeBuilder` wrapper in `nros/node.hpp`.
#[repr(C)]
pub struct nros_cpp_node_options_t {
    /// Namespace storage (UTF-8, NUL-terminated within `namespace_len`).
    pub namespace: [u8; NROS_CPP_NAMESPACE_LEN],
    /// Length of `namespace` in bytes (excluding NUL).
    pub namespace_len: usize,
    /// RMW backend name (e.g. "zenoh", "cyclonedds"). Empty selects first-registered.
    pub rmw_name: [u8; NROS_CPP_RMW_NAME_LEN],
    /// Length of `rmw_name`.
    pub rmw_name_len: usize,
    /// Optional per-Node locator override. Empty inherits the executor's.
    pub locator: [u8; NROS_CPP_LOCATOR_LEN],
    /// Length of `locator`.
    pub locator_len: usize,
    /// Per-Node domain ID. `NROS_CPP_DOMAIN_ID_INHERIT` = inherit.
    pub domain_id_override: u32,
    /// SchedContext slot to bind on handles created via this Node.
    /// 0 = executor default.
    pub sched_context_id: u8,
    /// Reserved for future use; must be zero.
    pub _reserved: [u8; 3],
}

impl Default for nros_cpp_node_options_t {
    fn default() -> Self {
        Self {
            namespace: [0u8; NROS_CPP_NAMESPACE_LEN],
            namespace_len: 0,
            rmw_name: [0u8; NROS_CPP_RMW_NAME_LEN],
            rmw_name_len: 0,
            locator: [0u8; NROS_CPP_LOCATOR_LEN],
            locator_len: 0,
            domain_id_override: NROS_CPP_DOMAIN_ID_INHERIT,
            sched_context_id: 0,
            _reserved: [0u8; 3],
        }
    }
}

/// Phase 104.C.9 — zero-initialised `nros_cpp_node_options_t`.
///
/// All length fields default to 0 ("inherit"); `domain_id_override` is
/// `NROS_CPP_DOMAIN_ID_INHERIT`. The C++ `NodeOptions` wrapper consumes
/// this via `Executor::node_builder(name)`.
#[unsafe(no_mangle)]
pub extern "C" fn nros_cpp_node_get_default_options() -> nros_cpp_node_options_t {
    nros_cpp_node_options_t::default()
}

/// Create a node on an executor.
///
/// Equivalent to populating an [`nros_cpp_node_options_t`] with the
/// supplied namespace + zero defaults and calling
/// `nros_cpp_node_create_ex`. Kept for source compatibility with
/// pre-Phase-104.C.9 callers.
///
/// # Parameters
/// * `executor_handle` — Opaque executor handle from `nros_cpp_init()`.
/// * `name` — Node name (null-terminated). Must not be NULL.
/// * `namespace` — Node namespace (null-terminated), or NULL for `"/"`.
/// * `out_node` — Receives the node handle on success.
///
/// # Safety
/// * `executor_handle` must be a valid handle from `nros_cpp_init()`.
/// * `name` must be a valid null-terminated string.
/// * `namespace` must be a valid null-terminated string or NULL.
/// * `out_node` must be a valid pointer to an `nros_cpp_node_t`.
///
/// # Returns
/// `NROS_CPP_RET_OK` on success, error code otherwise.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_create(
    executor_handle: *mut c_void,
    name: *const c_char,
    namespace: *const c_char,
    out_node: *mut nros_cpp_node_t,
) -> nros_cpp_ret_t {
    if executor_handle.is_null() || name.is_null() || out_node.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) if !s.is_empty() && s.len() < 64 => s,
        _ => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let ns_str = if namespace.is_null() {
        "/"
    } else {
        match unsafe { cstr_to_str(namespace) } {
            Some(s) if s.len() < NROS_CPP_NAMESPACE_LEN => s,
            _ => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    };

    let out = unsafe { &mut *out_node };
    out.executor = executor_handle;
    out.name = [0u8; NROS_CPP_NAME_LEN];
    out.name[..name_str.len()].copy_from_slice(name_str.as_bytes());
    out.namespace = [0u8; NROS_CPP_NAMESPACE_LEN];
    if !ns_str.is_empty() {
        out.namespace[..ns_str.len()].copy_from_slice(ns_str.as_bytes());
    }
    out.node_id = 0;
    out._reserved = [0u8; NROS_CPP_NODE_RESERVED];

    NROS_CPP_RET_OK
}

/// Phase 104.C.9 — create a node with extended options.
///
/// Thin C++ FFI wrapper over the Rust `Executor::node_builder(name)
/// .rmw(...).locator(...).domain_id(...).namespace(...).sched(...)
/// .build()` chain. The `options.rmw_name` selector binds the Node to
/// a registered RMW backend; subsequent handle creations on the Node
/// route through that backend's session.
///
/// Currently the per-Node SchedContext field and the multi-Session
/// `extra_sessions` plumbing land via a follow-up (Phase 104.C.9.b)
/// once the C++ executor surfaces `Executor::node_builder` directly.
/// The options struct round-trips into `nros_cpp_node_t` storage today
/// so users can write code against the final API surface.
///
/// # Parameters
/// * `executor_handle` — Opaque executor handle.
/// * `name` — Node name (null-terminated). Must not be NULL.
/// * `options` — Pointer to a populated `nros_cpp_node_options_t`. NULL
///   is rejected; use `nros_cpp_node_get_default_options()` to get a
///   zero-initialised instance.
/// * `out_node` — Receives the node handle on success.
///
/// # Safety
/// All pointer arguments must satisfy their per-parameter rules. The
/// options struct's length fields must not overrun their buffers.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_create_ex(
    executor_handle: *mut c_void,
    name: *const c_char,
    options: *const nros_cpp_node_options_t,
    out_node: *mut nros_cpp_node_t,
) -> nros_cpp_ret_t {
    if executor_handle.is_null() || name.is_null() || options.is_null() || out_node.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    if name_str.is_empty() || name_str.len() >= 64 {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let opts = unsafe { &*options };
    if opts.namespace_len > NROS_CPP_NAMESPACE_LEN
        || opts.rmw_name_len > NROS_CPP_RMW_NAME_LEN
        || opts.locator_len > NROS_CPP_LOCATOR_LEN
    {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    // Phase 104.C.9.b — drive Rust's `Executor::node_builder(name)
    // .rmw(...).locator(...).domain_id(...).namespace(...).sched(...).
    // build()` and store the returned NodeId on the C++ node handle.
    // Subsequent `nros_cpp_publisher_create` / `_subscription_create`
    // / `_service_*_create` calls observe `node_id != 0` and route
    // through `Executor::node_session_mut(NodeId)` instead of the
    // primary session.
    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let mut builder = ctx.executor.node_builder(name_str);
    if opts.rmw_name_len > 0 {
        let rmw = unsafe { core::str::from_utf8_unchecked(&opts.rmw_name[..opts.rmw_name_len]) };
        builder = builder.rmw(rmw);
    }
    if opts.locator_len > 0 {
        let loc = unsafe { core::str::from_utf8_unchecked(&opts.locator[..opts.locator_len]) };
        builder = builder.locator(loc);
    }
    if opts.domain_id_override != NROS_CPP_DOMAIN_ID_INHERIT {
        builder = builder.domain_id(opts.domain_id_override);
    }
    if opts.namespace_len > 0 {
        let ns = unsafe { core::str::from_utf8_unchecked(&opts.namespace[..opts.namespace_len]) };
        builder = builder.namespace(ns);
    }
    if opts.sched_context_id != 0 {
        builder = builder.sched(nros_node::executor::sched_context::SchedContextId(
            opts.sched_context_id,
        ));
    }
    let node_id = match builder.build() {
        Ok(id) => id,
        Err(_) => return NROS_CPP_RET_ERROR,
    };

    let out = unsafe { &mut *out_node };
    out.executor = executor_handle;
    out.name = [0u8; NROS_CPP_NAME_LEN];
    out.name[..name_str.len()].copy_from_slice(name_str.as_bytes());

    out.namespace = [0u8; NROS_CPP_NAMESPACE_LEN];
    if opts.namespace_len > 0 {
        out.namespace[..opts.namespace_len].copy_from_slice(&opts.namespace[..opts.namespace_len]);
    } else {
        out.namespace[..1].copy_from_slice(b"/");
    }
    out.node_id = node_id.raw();
    out._reserved = [0u8; NROS_CPP_NODE_RESERVED];

    NROS_CPP_RET_OK
}

/// Destroy a node.
///
/// Currently a no-op since the node is just metadata referencing the executor.
/// The executor owns all resources.
#[unsafe(no_mangle)]
pub extern "C" fn nros_cpp_node_destroy(_node: *mut nros_cpp_node_t) -> nros_cpp_ret_t {
    // Node is a lightweight view — nothing to free.
    NROS_CPP_RET_OK
}

/// Get the node name.
///
/// Returns a pointer to the null-terminated name string stored in the node handle.
/// The pointer is valid as long as the `nros_cpp_node_t` is alive.
///
/// # Safety
/// `node` must be a valid pointer to an initialized `nros_cpp_node_t`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_name(node: *const nros_cpp_node_t) -> *const c_char {
    if node.is_null() {
        return core::ptr::null();
    }
    unsafe { (*node).name.as_ptr() as *const c_char }
}

/// Get the node namespace.
///
/// Returns a pointer to the null-terminated namespace string stored in the node handle.
/// The pointer is valid as long as the `nros_cpp_node_t` is alive.
///
/// # Safety
/// `node` must be a valid pointer to an initialized `nros_cpp_node_t`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_namespace(
    node: *const nros_cpp_node_t,
) -> *const c_char {
    if node.is_null() {
        return core::ptr::null();
    }
    unsafe { (*node).namespace.as_ptr() as *const c_char }
}

/// Phase 88.12 — return the `nros_log::Logger` keyed on this node's
/// name. Opaque handle on the C++ side; pass to `NROS_LOG_*` macros.
///
/// # Safety
/// `node` must be a valid pointer to an initialized `nros_cpp_node_t`,
/// or NULL (in which case NULL is returned).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_logger(
    node: *const nros_cpp_node_t,
) -> *const core::ffi::c_void {
    if node.is_null() {
        return core::ptr::null();
    }
    let name_ptr = unsafe { (*node).name.as_ptr() };
    // Find the NUL terminator in the fixed-size `name` array to
    // build a `&str` for the intern table lookup.
    let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, (*node).name.len()) };
    let nul = name_bytes.iter().position(|&b| b == 0).unwrap_or(0);
    let name = core::str::from_utf8(&name_bytes[..nul]).unwrap_or("");
    let logger: &'static nros_log::Logger = nros_log::get_logger(name);
    (logger as *const nros_log::Logger).cast()
}

// ============================================================================
// Spin
// ============================================================================

/// Drive transport I/O and dispatch any registered callbacks.
///
/// Call this periodically so subscriptions can receive data.
///
/// # Parameters
/// * `handle` — Opaque executor handle from `nros_cpp_init()`.
/// * `timeout_ms` — Maximum time to block waiting for I/O (milliseconds).
///
/// # Safety
/// `handle` must be a valid handle returned by `nros_cpp_init()`.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_spin_once(
    handle: *mut c_void,
    timeout_ms: i32,
) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let ctx = unsafe { &mut *(handle as *mut CppContext) };
    let ms = timeout_ms.max(0) as u64;
    // Phase 127.C.4 — the prior Zephyr+std bypass (drive_io(0) + msleep)
    // starved reliable XRCE retransmission on the server side and
    // skipped arena dispatch on the client side; the underlying
    // condvar hang it worked around is gated off in
    // `Executor::spin_once` for Zephyr+std, so a normal spin runs the
    // transport for the full timeout via UDP recv and fires the arena
    // trampolines.
    let _ = ctx
        .executor
        .spin_once(core::time::Duration::from_millis(ms));
    NROS_CPP_RET_OK
}

/// Phase 124.F.3 — session-level connectivity probe.
///
/// Wire-level round-trip ("is the peer / agent / router reachable?")
/// with `timeout_ms` budget. Returns `NROS_CPP_RET_OK` on reply,
/// `NROS_CPP_RET_TIMEOUT` on no reply, `NROS_CPP_RET_UNSUPPORTED`
/// when the active backend can't probe. Mirrors micro-ROS's
/// `rmw_uros_ping_agent`.
///
/// # Safety
/// `handle` must be a valid `CppContext` from `nros_cpp_init()`.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_executor_ping(
    handle: *mut c_void,
    timeout_ms: i32,
) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(handle as *mut CppContext) };
    match ctx.executor.ping(timeout_ms) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(nros_node::NodeError::Transport(nros_rmw::TransportError::Timeout)) => {
            NROS_CPP_RET_TIMEOUT
        }
        Err(nros_node::NodeError::Transport(nros_rmw::TransportError::Unsupported)) => {
            NROS_CPP_RET_UNSUPPORTED
        }
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

// =============================================================================
// Phase 110.B / 110.C — SchedContext FFI for the C++ wrapper
// =============================================================================

/// `nros::SchedClass` mirror. Phase 110.B.
#[cfg(feature = "rmw-cffi")]
#[repr(u8)]
pub enum nros_cpp_sched_class_t {
    Fifo = 0,
    Edf = 1,
    Sporadic = 2,
    BestEffort = 3,
    TimeTriggered = 4,
}

/// `nros::Priority` mirror. Phase 110.C.
#[cfg(feature = "rmw-cffi")]
#[repr(u8)]
pub enum nros_cpp_priority_t {
    Critical = 0,
    Normal = 1,
    BestEffort = 2,
}

/// `nros::DeadlinePolicy` mirror. Phase 110.B.
#[cfg(feature = "rmw-cffi")]
#[repr(u8)]
pub enum nros_cpp_deadline_policy_t {
    Released = 0,
    Activated = 1,
    Inherited = 2,
}

/// `nros::SchedContext` mirror passed to
/// [`nros_cpp_create_sched_context`]. Time fields use `0` as
/// "absent" sentinel (mirrors the Rust `OptUs` newtype).
#[cfg(feature = "rmw-cffi")]
#[repr(C)]
pub struct nros_cpp_sched_context_t {
    pub class: nros_cpp_sched_class_t,
    pub priority: nros_cpp_priority_t,
    pub deadline_policy: nros_cpp_deadline_policy_t,
    pub period_us: u32,
    pub budget_us: u32,
    pub deadline_us: u32,
    /// Phase 110.F — opt-in OS-level priority for per-callback dispatch.
    pub os_pri: u8,
    /// Phase 110.G — TT-window offset within the executor's major frame.
    pub tt_window_offset_us: u32,
    /// Phase 110.G — TT-window length in microseconds.
    pub tt_window_duration_us: u32,
}

/// Identifier of the auto-created default `Fifo` SC. Phase 110.B.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub extern "C" fn nros_cpp_default_sched_context_id() -> u8 {
    0
}

/// Register a new scheduling context. Phase 110.B.
///
/// On success writes the new SC id through `out_sc_id` and returns
/// `NROS_CPP_RET_OK`. Returns `NROS_CPP_RET_INVALID_ARGUMENT` for null
/// pointers, `NROS_CPP_RET_ERROR` if `MAX_SC` is exhausted.
///
/// # Safety
/// All pointers must be valid; `handle` must be a context returned by
/// `nros_cpp_init`.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_create_sched_context(
    handle: *mut c_void,
    cfg: *const nros_cpp_sched_context_t,
    out_sc_id: *mut u8,
) -> nros_cpp_ret_t {
    if handle.is_null() || cfg.is_null() || out_sc_id.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    use nros_node::executor::sched_context::{
        DeadlinePolicy, OptUs, Priority, SchedClass, SchedContext,
    };
    let ctx = unsafe { &mut *(handle as *mut CppContext) };
    let cfg = unsafe { &*cfg };
    #[allow(deprecated)]
    let sc = SchedContext {
        class: match cfg.class {
            nros_cpp_sched_class_t::Fifo => SchedClass::Fifo,
            nros_cpp_sched_class_t::Edf => SchedClass::Edf,
            nros_cpp_sched_class_t::Sporadic => SchedClass::Sporadic,
            nros_cpp_sched_class_t::BestEffort => SchedClass::BestEffort,
            // Phase 110.G refactor — TimeTriggered is now an
            // orthogonal slot annotation; route to Fifo + populate
            // tt_window_*.
            nros_cpp_sched_class_t::TimeTriggered => SchedClass::Fifo,
        },
        priority: match cfg.priority {
            nros_cpp_priority_t::Critical => Priority::Critical,
            nros_cpp_priority_t::Normal => Priority::Normal,
            nros_cpp_priority_t::BestEffort => Priority::BestEffort,
        },
        deadline_policy: match cfg.deadline_policy {
            nros_cpp_deadline_policy_t::Released => DeadlinePolicy::Released,
            nros_cpp_deadline_policy_t::Activated => DeadlinePolicy::Activated,
            nros_cpp_deadline_policy_t::Inherited => DeadlinePolicy::Inherited,
        },
        period_us: OptUs::from_us(cfg.period_us),
        budget_us: OptUs::from_us(cfg.budget_us),
        deadline_us: OptUs::from_us(cfg.deadline_us),
        os_pri: cfg.os_pri,
        tt_window_offset_us: OptUs::from_us(cfg.tt_window_offset_us),
        tt_window_duration_us: OptUs::from_us(cfg.tt_window_duration_us),
    };
    match ctx.executor.create_sched_context(sc) {
        Ok(id) => {
            unsafe { *out_sc_id = id.0 };
            NROS_CPP_RET_OK
        }
        Err(_) => NROS_CPP_RET_FULL,
    }
}

/// Bind a registered callback to a scheduling context. Phase 110.B.
///
/// `handle` is the executor context; `callback_handle` is the index
/// returned from a previous `add_*` call; `sc_id` is from
/// [`nros_cpp_create_sched_context`].
///
/// # Safety
/// `handle` must be a context returned by `nros_cpp_init`.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_bind_handle_to_sched_context(
    handle: *mut c_void,
    callback_handle: usize,
    sc_id: u8,
) -> nros_cpp_ret_t {
    if handle.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(handle as *mut CppContext) };
    let h = nros_node::executor::HandleId(callback_handle);
    let id = nros_node::executor::sched_context::SchedContextId(sc_id);
    match ctx.executor.bind_handle_to_sched_context(h, id) {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_INVALID_ARGUMENT,
    }
}

/// Get current monotonic time in nanoseconds.
///
/// Used by `nros::Future::wait()` (header-side) to budget its spin loop by
/// wall-clock rather than iteration count, so that an early-returning
/// `spin_once` on a signaled condvar doesn't collapse the nominal timeout
/// into microseconds. Phase 89.2.
#[unsafe(no_mangle)]
pub extern "C" fn nros_cpp_time_ns() -> u64 {
    #[cfg(feature = "std")]
    {
        use std::time::Instant;
        static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        let epoch = EPOCH.get_or_init(Instant::now);
        Instant::now().duration_since(*epoch).as_nanos() as u64
    }
    #[cfg(not(feature = "std"))]
    {
        // Use the canonical platform clock instead of depending on a
        // backend-specific shim symbol such as zenoh-pico's z_clock_now.
        #[cfg(feature = "platform-zephyr")]
        {
            unsafe extern "C" {
                fn nros_platform_time_ns() -> u64;
            }
            unsafe { nros_platform_time_ns() }
        }
        #[cfg(not(feature = "platform-zephyr"))]
        {
            <nros_platform::ConcretePlatform as nros_platform::PlatformClock>::clock_us()
                .saturating_mul(1_000)
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Convert a C null-terminated string to a `&str`.
///
/// Returns `None` if the pointer is null or the bytes are not valid UTF-8.
pub(crate) unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    // Find null terminator
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
            if len > 4096 {
                return None; // safety bound
            }
        }
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    core::str::from_utf8(bytes).ok()
}
