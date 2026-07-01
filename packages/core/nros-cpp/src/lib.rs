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

// Phase 241.D3-rev (W12) × phase-248 — nros-cpp bundles `nros-c` as a HARD dependency,
// and nros-c (behind the platform vtable) owns the no_std `#[global_allocator]`
// (`platform_alloc`), the `#[panic_handler]`, AND the `critical_section::set_impl!`
// (`platform_critical_section`). The single-runtime umbrella permits exactly ONE of each
// across the whole crate graph, so nros-cpp defines NONE of them here — nros-c's serve
// the entire `libnros_cpp.a` (all route through the same `nros_platform_*` vtable). This
// crate's `platform-*` features forward to `nros-c/platform-*` (which enable nros-c's
// `global-allocator` / `critical-section`) and `panic-halt` → `nros-c/panic-halt`.

use core::ffi::{c_char, c_int, c_void};

// Phase 241.D3-rev — force-link the selected backend into `libnros_cpp.a` (the C++
// umbrella's staticlib root) + auto-register it before `main`. nros-c's twin anchor
// is DCE'd as a dependency, so the root carries its own. See `rmw_backend`.
#[cfg(any(feature = "rmw-zenoh-cffi", feature = "rmw-xrce-cffi"))]
mod rmw_backend;

// Phase 241.D3-rev — pull nros-c's FULL `#[no_mangle]` C surface into libnros_cpp.a.
// nros-cpp bundles nros-c as an rlib and links only libnros_cpp.a, so rustc DCEs any
// C entry point the C++ FFI itself never references (e.g. nros_param_server_fini) —
// yet a C++ binary may call it via the C ABI. nros-c's own `#[used]` anchor is DCE'd
// as a dependency; referencing it from THIS staticlib root keeps it + the entry
// points it names.
#[used]
static _KEEP_C_SURFACE: &[unsafe extern "C" fn()] = &nros_c::c_surface_anchor::C_SURFACE_ANCHOR;

// Phase 241 W11 (Option D) — `pub mod cpp_surface_anchor { … CPP_SURFACE_ANCHOR }`,
// generated by build.rs (this crate's own ungated `nros_cpp_*` no_mangle surface).
include!(concat!(env!("OUT_DIR"), "/cpp_surface_anchor.rs"));

// W11 backend anchor — the selected cffi backend's auto-register fn, so a downstream
// staticlib root pulls its register closure into the archive. Empty with no cffi backend.
#[cfg(any(feature = "rmw-zenoh-cffi", feature = "rmw-xrce-cffi"))]
const _BACKEND_ANCHOR: &[unsafe extern "C" fn()] = &[rmw_backend::auto_register];
#[cfg(all(
    feature = "rmw-cffi",
    not(any(feature = "rmw-zenoh-cffi", feature = "rmw-xrce-cffi"))
))]
const _BACKEND_ANCHOR: &[unsafe extern "C" fn()] = &[];

/// Phase 241 W11 (Option D) — combined force-link anchor for a downstream staticlib root
/// (the per-entry `<entry>_runtime` crate). When nros-cpp is bundled as a dependency
/// rlib, its own `#[used]` anchors are DCE'd before the runtime staticlib is emitted; the
/// runtime root references THIS with its own `#[used]` to re-pull the full ABI surface —
/// nros-c's C API + nros-cpp's C++ FFI + the selected backend's register closure.
#[cfg(feature = "rmw-cffi")]
pub static FORCE_LINK_ANCHOR: &[&[unsafe extern "C" fn()]] = &[
    &nros_c::c_surface_anchor::C_SURFACE_ANCHOR,
    &cpp_surface_anchor::CPP_SURFACE_ANCHOR,
    _BACKEND_ANCHOR,
];

// W11 — re-export the backend auto-register so the runtime root can install its OWN
// `.init_array` ctor (this crate's ctor, in a dep rlib, is DCE'd). Pull `register()`
// before `main` on hosted targets that honor `.init_array`.
#[cfg(any(feature = "rmw-zenoh-cffi", feature = "rmw-xrce-cffi"))]
pub use rmw_backend::auto_register as nros_cpp_auto_register_backend;

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

// Phase 269 (W0) — executor-shim: lifecycle + parameter FFI over the CppContext handle.
mod lifecycle_shim;
mod params_shim;

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
/// Parameter not found in the executor's store.
pub const NROS_CPP_RET_NOT_FOUND: nros_cpp_ret_t = -8;
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
    // Phase 241.D3-rev — the selected backend is auto-registered before `main` by
    // the `nros-c` umbrella's `rmw_backend` `.init_array` ctor (bundled into
    // `libnros_cpp.a`). `nros_app_register_backends()` above stays as the weak
    // board-override hook; the explicit per-backend `register()` calls are gone
    // (idempotent re-register, and they referenced backend crates nros-cpp no
    // longer deps directly).
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

/// Phase 266 (W5b/W6) — named variant of [`nros_board_native_run_components`].
///
/// `session_name` sets the primary session / node name visible via `ros2 node list`
/// (the #98 fix for C entries). NULL or empty → falls back to `"node"` (the
/// unified compiled default — same as the Rust `nros::main!` resolver compiled
/// default and the C++ 2-arg `nros::init` default after this phase).
///
/// The generated typed C entry (`nros codegen entry --lang c --typed`) calls this
/// from `main`, passing `nros_boot_config_node_name(&NROS_BOOT_CONFIG)` which
/// resolves to the launch node name for single-node entries (or NULL for multi-node,
/// where the "node" default applies).
///
/// # Safety
/// `session_name` must be NULL or a valid null-terminated string.
/// `setup` must be a valid function pointer; it is invoked once with the executor
/// handle (a `*mut CppContext`) before the spin loop.
#[cfg(all(feature = "rmw-cffi", feature = "std"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_board_native_run_components_named(
    session_name: *const c_char,
    setup: Option<unsafe extern "C" fn(executor: *mut c_void) -> i32>,
) -> i32 {
    let setup = match setup {
        Some(f) => f,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };

    // Resolve session name: null / empty → "node" (unified default, phase 266).
    let name_resolved: &core::ffi::CStr = if session_name.is_null() {
        c"node"
    } else {
        let s = unsafe { core::ffi::CStr::from_ptr(session_name) };
        if s.is_empty() { c"node" } else { s }
    };

    // Env overlay (mirrors the C++ `nros::init()` hosted fallback).
    let locator = std::env::var("NROS_LOCATOR")
        .ok()
        .filter(|s| !s.is_empty())
        .and_then(|s| std::ffi::CString::new(s).ok());
    let domain_id: u8 = std::env::var("ROS_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&d| d <= 232)
        .unwrap_or(0) as u8;

    // Executor storage lives for the whole run (init writes a CppContext here).
    let mut storage = core::mem::MaybeUninit::<CppContext>::uninit();
    let sptr = storage.as_mut_ptr() as *mut c_void;
    let locator_ptr = locator.as_ref().map_or(core::ptr::null(), |c| c.as_ptr());
    let rc = unsafe {
        nros_cpp_init(
            locator_ptr,
            domain_id,
            name_resolved.as_ptr(),
            core::ptr::null(),
            sptr,
        )
    };
    if rc != NROS_CPP_RET_OK {
        return rc as i32;
    }

    let setup_rc = unsafe { setup(sptr) };
    if setup_rc != 0 {
        unsafe { nros_cpp_fini(sptr) };
        return setup_rc;
    }

    let bound_ms: u64 = std::env::var("NROS_ENTRY_SPIN_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let start_ns = nros_cpp_time_ns();
    let mut ret = 0;
    loop {
        let last = unsafe { nros_cpp_spin_once(sptr, 10) };
        if last != NROS_CPP_RET_OK {
            ret = last as i32;
            break;
        }
        if bound_ms != 0 {
            let elapsed_ms = (nros_cpp_time_ns() - start_ns) / 1_000_000;
            if elapsed_ms >= bound_ms {
                break;
            }
        }
    }

    unsafe { nros_cpp_fini(sptr) };
    ret
}

/// Phase 257 (W0-A, RFC-0043) — typed C Entry lifecycle (unnamed variant).
///
/// Delegates to [`nros_board_native_run_components_named`] with a NULL session
/// name, which resolves to the unified default `"node"` (phase 266 default
/// change). Kept for ABI back-compatibility with callers that do not pass a
/// name; new generated entries call `nros_board_native_run_components_named`
/// directly with the baked boot-config node name.
///
/// # Safety
/// `setup` must be a valid function pointer.
#[cfg(all(feature = "rmw-cffi", feature = "std"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_board_native_run_components(
    setup: Option<unsafe extern "C" fn(executor: *mut c_void) -> i32>,
) -> i32 {
    unsafe { nros_board_native_run_components_named(core::ptr::null(), setup) }
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

    // Phase 211.H (issue #52) — per-topic QoS overrides the deploy plan lowered
    // from `qos_overrides.<topic>.<role>.<policy>` launch params. Set by
    // `nros_cpp_node_set_qos_overrides`; folded into each entity's QoS at
    // publisher/subscription create time. Appended at the END so existing field
    // offsets (the C++ ABI) are unchanged; null/0 = no overrides (legacy).
    /// Pointer to a `&'static`-lifetime array of [`nros_cpp_qos_override_t`], or
    /// null. The generated/hand-written entry owns the storage for the node's
    /// lifetime.
    pub qos_overrides: *const nros_cpp_qos_override_t,
    /// Number of entries in `qos_overrides`. 0 = none.
    pub qos_overrides_len: usize,
}

/// Phase 211.H (issue #52) — one per-topic QoS override, the C++-FFI mirror of
/// Rust's `nros_rmw::QosOverride` (and nros-c's `nros_qos_override_t`). The
/// deploy plan lowers a `qos_overrides.<topic>.<role>.<policy>` launch param
/// into a `&'static` array of these, which the entry installs on the node via
/// [`nros_cpp_node_set_qos_overrides`]; the node folds matching `(topic, role)`
/// entries into each entity's QoS at create time, before the backend-compat
/// check. Plain scalar fields only (no `#[repr(C)]` enums) → trivially stable
/// cbindgen output.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_cpp_qos_override_t {
    /// Resolved (remapped) topic, NUL-terminated UTF-8 (e.g. `"/chatter"`).
    pub topic: *const c_char,
    /// `0` = publisher, `1` = subscription.
    pub role: u8,
    /// `0` = reliability, `1` = durability, `2` = history, `3` = depth.
    pub policy: u8,
    /// Policy-specific value: reliability `0`=best_effort/`1`=reliable;
    /// durability `0`=volatile/`1`=transient_local; history
    /// `0`=keep_last/`1`=keep_all; depth = the KeepLast depth.
    pub value: u32,
}

pub(crate) const NROS_CPP_QOS_OVERRIDE_ROLE_PUBLISHER: u8 = 0;
pub(crate) const NROS_CPP_QOS_OVERRIDE_ROLE_SUBSCRIPTION: u8 = 1;

/// Fold any overrides matching `(topic, role)` into `qos`. Mirrors
/// `nros_rmw::QosSettings::apply_overrides`: single linear scan,
/// last-write-wins, no alloc. `overrides` may be null (`len == 0` ⇒ no-op).
///
/// # Safety
/// `overrides` must be null or point to `len` valid `nros_cpp_qos_override_t`,
/// each `topic` null or a valid NUL-terminated UTF-8 C string for the call.
pub(crate) unsafe fn apply_qos_overrides(
    mut qos: nros_rmw::QosSettings,
    overrides: *const nros_cpp_qos_override_t,
    len: usize,
    topic: &str,
    role: u8,
) -> nros_rmw::QosSettings {
    use nros_rmw::{QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy};

    if overrides.is_null() || len == 0 {
        return qos;
    }
    let table = unsafe { core::slice::from_raw_parts(overrides, len) };
    for ovr in table {
        if ovr.role != role || ovr.topic.is_null() {
            continue;
        }
        let Ok(ovr_topic) = (unsafe { core::ffi::CStr::from_ptr(ovr.topic) }).to_str() else {
            continue;
        };
        if ovr_topic != topic {
            continue;
        }
        match ovr.policy {
            0 => {
                qos.reliability = if ovr.value == 0 {
                    QosReliabilityPolicy::BestEffort
                } else {
                    QosReliabilityPolicy::Reliable
                }
            }
            1 => {
                qos.durability = if ovr.value == 0 {
                    QosDurabilityPolicy::Volatile
                } else {
                    QosDurabilityPolicy::TransientLocal
                }
            }
            2 => {
                qos.history = if ovr.value == 0 {
                    QosHistoryPolicy::KeepLast
                } else {
                    QosHistoryPolicy::KeepAll
                }
            }
            3 => qos.depth = ovr.value,
            _ => {}
        }
    }
    qos
}

/// Install the per-topic QoS override table on `node` (issue #52). Every entity
/// created afterwards folds the matching `(topic, role)` entries into its QoS —
/// the C++ mirror of Rust's `NodeHandle::set_qos_overrides`. `overrides` must
/// outlive the node. `len == 0` (or null) clears.
///
/// # Safety
/// `node` must point to an initialised `nros_cpp_node_t`; `overrides` null or
/// `len` valid entries living at least as long as the node.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_set_qos_overrides(
    node: *mut nros_cpp_node_t,
    overrides: *const nros_cpp_qos_override_t,
    len: usize,
) -> nros_cpp_ret_t {
    let Some(node) = (unsafe { node.as_mut() }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    node.qos_overrides = overrides;
    node.qos_overrides_len = len;
    NROS_CPP_RET_OK
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

    // Phase 268 (RFC-0046) — register the node through `Executor::node_builder`
    // (the one shared site both languages funnel through) so it gets a distinct
    // NodeId + NodeRecord carrying this name. Previously this left `node_id = 0`
    // (unregistered), so node-id-keyed entity paths — notably the raw arena
    // subscription register — fell back to the session's name. A multi-node
    // entry therefore collapsed every such entity onto the single session name
    // (`/node`) instead of its component (`/talker`, `/listener`). With a real
    // NodeId, `node_session_mut(node_id)` still resolves the shared primary
    // session (no rmw override → slot 0), so routing is unchanged — only the
    // node identity is now correct per component, matching the Rust + `_ex`
    // paths.
    let ctx = unsafe { &mut *(executor_handle as *mut CppContext) };
    let node_id = match ctx
        .executor
        .node_builder(name_str)
        .namespace(ns_str)
        .build()
    {
        Ok(id) => id,
        Err(_) => return NROS_CPP_RET_ERROR,
    };

    let out = unsafe { &mut *out_node };
    out.executor = executor_handle;
    out.name = [0u8; NROS_CPP_NAME_LEN];
    out.name[..name_str.len()].copy_from_slice(name_str.as_bytes());
    out.namespace = [0u8; NROS_CPP_NAMESPACE_LEN];
    if !ns_str.is_empty() {
        out.namespace[..ns_str.len()].copy_from_slice(ns_str.as_bytes());
    }
    out.node_id = node_id.raw();
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

/// Phase 272 (W2) — seed the `node_name → sched_context` table before the node
/// is built. Mirrors the W1 `Executor::bind_node_name_sched` via the C++ executor
/// handle; called by the emitted entry setup AFTER creating sched-context slots
/// and BEFORE constructing/configuring components (RFC-0047: seed before build).
///
/// Covers every component shape (configure-shape C/C++ and rclcpp IS-A-node) since
/// every node funnels through `Executor::node_builder(name).build()` (RFC-0046) and
/// the builder looks up the table there. This dissolves issue #124 at the emit level:
/// rclcpp-shape nodes are seeded here and pick up their tier in the builder.
///
/// # Safety
/// `handle` must be a context returned by `nros_cpp_init`.
/// `name` must be a valid null-terminated UTF-8 string.
/// `namespace_` may be NULL (defaults to `"/"`), otherwise must be a valid
/// null-terminated UTF-8 string.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_bind_node_name_sched(
    handle: *mut c_void,
    name: *const c_char,
    namespace_: *const c_char,
    sc_id: u8,
) -> nros_cpp_ret_t {
    if handle.is_null() || name.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(handle as *mut CppContext) };
    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let ns_str = if namespace_.is_null() {
        "/"
    } else {
        match unsafe { cstr_to_str(namespace_) } {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    };
    ctx.executor.bind_node_name_sched(
        name_str,
        ns_str,
        nros_node::executor::sched_context::SchedContextId(sc_id),
    );
    NROS_CPP_RET_OK
}

/// Phase 273 (W2) — seed the group → sched-context table for a specific
/// callback group of a named node. Call BEFORE the node is constructed (before
/// `nros_cpp_node_create`) so that the group's entities pick up the binding at
/// register time. Layering: group table > node-name table > default (RFC-0047
/// Precedence). Mirror of `nros_cpp_bind_node_name_sched` at finer granularity.
///
/// # Safety
/// `handle` must be a context returned by `nros_cpp_init`.
/// `name` must be a valid null-terminated UTF-8 string.
/// `namespace_` may be NULL (defaults to `"/"`), otherwise must be a valid
/// null-terminated UTF-8 string.
/// `group` must be a valid null-terminated UTF-8 string.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_bind_group_sched(
    handle: *mut c_void,
    name: *const c_char,
    namespace_: *const c_char,
    group: *const c_char,
    sc_id: u8,
) -> nros_cpp_ret_t {
    if handle.is_null() || name.is_null() || group.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(handle as *mut CppContext) };
    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let ns_str = if namespace_.is_null() {
        "/"
    } else {
        match unsafe { cstr_to_str(namespace_) } {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    };
    let group_str = match unsafe { cstr_to_str(group) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    ctx.executor.bind_group_sched(
        name_str,
        ns_str,
        group_str,
        nros_node::executor::sched_context::SchedContextId(sc_id),
    );
    NROS_CPP_RET_OK
}

// ============================================================================
// Phase 274.W1 — RFC-0015 Model 1 primitives (session ⊥ executor + gating FFI)
// ============================================================================

/// Phase 274.W1 (RFC-0015 Model 1) — get the session handle from an opened executor.
///
/// Returns an opaque pointer to the underlying RMW session. Pass this to
/// [`nros_cpp_executor_open_over_session`] to open additional executors that
/// share the same session (the per-tier borrowed-executor model).
///
/// The returned pointer is valid as long as the primary executor's storage
/// (`nros_cpp_init` / `out_storage`) lives. NULL is returned on null input.
///
/// # Safety
/// `executor` must be a valid pointer to a `CppContext` written by `nros_cpp_init()`.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_executor_session_handle(executor: *mut c_void) -> *mut c_void {
    if executor.is_null() {
        return core::ptr::null_mut();
    }
    let ctx = unsafe { &mut *(executor as *mut CppContext) };
    ctx.executor.session_handle().into_raw()
}

/// Phase 274.W1 (RFC-0015 Model 1) — open a new `Borrowed` executor over a shared session.
///
/// Opens an executor that **does not own or close** the session on drop (the
/// `Borrowed` session store). This is the per-tier task primitive: the primary
/// executor opened the session once via `nros_cpp_init`; each tier task calls this
/// with the primary's session handle to get its own executor over the same session.
///
/// `node_name` sets the borrowed executor's node identity for graph naming; NULL
/// leaves it unnamed. `domain_id` is stored in the new context (it is consumed
/// during session open for the primary — the borrowed executor inherits the same
/// transport config from the shared session).
///
/// # Safety
/// - `session_handle` must be a valid non-null pointer from
///   [`nros_cpp_executor_session_handle`] on a live primary executor.
/// - `out_storage` must be valid caller-provided storage of at least
///   `CPP_EXECUTOR_OPAQUE_U64S * 8` bytes, 8-byte aligned, uninitialised.
/// - The primary executor's storage MUST outlive every borrowed executor built from it.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_executor_open_over_session(
    session_handle: *mut c_void,
    node_name: *const c_char,
    domain_id: u32,
    out_storage: *mut c_void,
) -> nros_cpp_ret_t {
    if session_handle.is_null() || out_storage.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    // Reconstruct the SessionHandle from the opaque pointer.
    // SAFETY: caller guarantees this came from nros_cpp_executor_session_handle
    // on a still-live primary executor.
    let handle = unsafe { nros_node::SessionHandle::from_raw(session_handle) };

    // Open a new executor that Borrows the session — does NOT open a new RMW
    // session and does NOT close it on drop. Same as the Rust tier-task pattern.
    // SAFETY: the handle's session is alive (caller's contract); access only
    // through executor spin calls (RMW backend's internal locks serialize).
    let mut executor = unsafe { CppExecutor::open_with_session_handle(handle) };

    // Set node identity for graph naming (liveliness key expressions etc.).
    if let Some(name_str) = unsafe { cstr_to_str(node_name) } {
        executor.set_node_identity(name_str, "/");
    }

    let ctx = CppContext {
        executor,
        domain_id,
    };
    // Write directly into caller-provided storage — no heap allocation.
    unsafe { core::ptr::write(out_storage as *mut CppContext, ctx) };
    NROS_CPP_RET_OK
}

/// Phase 274.W1 (RFC-0015 Model 1) — gate this executor to a set of named callback groups.
///
/// After this call, only callbacks whose `.callback_group()` is in `groups` will
/// register on this executor. Pass `n == 0` or `groups == NULL` to clear the
/// filter (wildcard — accept all groups, which is the default).
///
/// In the per-tier model: call this on a borrowed executor BEFORE registering
/// callbacks so only the tier's groups land here. Mirrors
/// `Executor::set_active_groups`.
///
/// # Safety
/// - `executor` must be a valid pointer to a `CppContext`.
/// - `groups` must be NULL or point to `n` valid null-terminated UTF-8 C strings.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_executor_set_active_groups(
    executor: *mut c_void,
    groups: *const *const c_char,
    n: usize,
) -> nros_cpp_ret_t {
    if executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let ctx = unsafe { &mut *(executor as *mut CppContext) };

    if n == 0 || groups.is_null() {
        // Empty / NULL ⇒ wildcard (clear filter, accept all groups).
        ctx.executor.set_active_groups(&[]);
        return NROS_CPP_RET_OK;
    }

    // Collect group names from the C string pointer array onto the stack.
    // Bounded at 16 entries (generous for tier gating; silently truncates extras).
    const MAX_GROUPS_FFI: usize = 16;
    let mut group_strs = [""; MAX_GROUPS_FFI];
    let mut count = 0usize;

    let ptr_slice = unsafe { core::slice::from_raw_parts(groups, n.min(MAX_GROUPS_FFI)) };
    for &raw_ptr in ptr_slice {
        if let Some(s) = unsafe { cstr_to_str(raw_ptr) }
            && !s.is_empty()
        {
            group_strs[count] = s;
            count += 1;
        }
    }

    ctx.executor.set_active_groups(&group_strs[..count]);
    NROS_CPP_RET_OK
}

// ============================================================================
// Phase 274.W2 — RFC-0015 Model 1: native multi-tier entry (C-ABI seam)
// ============================================================================

/// Per-tier specification for [`nros_board_native_run_tiers`].
///
/// Mirrors `nros_platform::TierSpec` in C-ABI form. `groups` must point to
/// an array of `n_groups` null-terminated UTF-8 strings; NULL / 0 means
/// "accept all groups" (wildcard — degenerate single-tier).
///
/// `setup` is called once on the tier's thread, with the tier's borrowed
/// executor handle, AFTER `set_active_groups` — so only the tier's groups'
/// callbacks register. The boot tier (index 0) uses the owning executor.
///
/// `priority` is a raw POSIX nice-level adjustment (advisory on Linux
/// without elevated privileges). `stack_bytes` is informational on native
/// (`std::thread` manages the stack). `spin_period_us` is the sleep between
/// `spin_once` calls; 0 uses a 1 ms floor.
///
/// # Safety
///
/// `name` must be NULL or a valid null-terminated string.
/// `groups` must be NULL or point to `n_groups` valid null-terminated strings.
/// `setup` must be a valid function pointer or NULL (NULL skips setup — only
/// useful for tiers that register no nodes of their own).
#[cfg(all(feature = "rmw-cffi", feature = "std"))]
#[repr(C)]
pub struct NativeTierSpecC {
    pub name: *const c_char,
    pub groups: *const *const c_char,
    pub n_groups: usize,
    pub priority: i64,
    pub stack_bytes: usize,
    pub spin_period_us: u64,
    pub setup: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

/// Phase 274.W2 (RFC-0015 Model 1) — run a native multi-tier entry over one
/// shared RMW session.
///
/// Opens ONE session on the calling (boot) thread; spawns `n_tiers - 1`
/// threads each opening a **borrowed** executor (no second RMW session, no
/// double-close). Each thread:
///   1. `nros_cpp_executor_open_over_session` — open borrowed executor.
///   2. `nros_cpp_executor_set_active_groups` — gate to the tier's groups.
///   3. `setup(executor)` — create + configure nodes (only the tier's
///      groups' callbacks register).
///   4. `spin_once` loop at `spin_period_us` until shutdown flag.
///
/// The boot thread runs the first (highest-priority) tier on the owning
/// executor; it respects the `$NROS_ENTRY_SPIN_MS` bound for test/CI use.
/// When the boot thread exits its spin loop it signals the other tiers (via
/// `Arc<AtomicBool>`) and joins them before closing the session.
///
/// # Safety
/// `tiers` must be a valid pointer to `n_tiers` [`NativeTierSpecC`] entries,
/// valid for the duration of the call. `session_name` is NULL or a valid
/// null-terminated string.
#[cfg(all(feature = "rmw-cffi", feature = "std"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_board_native_run_tiers(
    session_name: *const c_char,
    tiers: *const NativeTierSpecC,
    n_tiers: usize,
) -> i32 {
    use std::{
        string::String,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        vec::Vec,
    };

    if tiers.is_null() || n_tiers == 0 {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }

    let tier_slice = unsafe { core::slice::from_raw_parts(tiers, n_tiers) };

    // Resolve session name: null / empty → "node".
    let name_resolved: &core::ffi::CStr = if session_name.is_null() {
        c"node"
    } else {
        let s = unsafe { core::ffi::CStr::from_ptr(session_name) };
        if s.is_empty() { c"node" } else { s }
    };

    // Env overlays.
    let locator = std::env::var("NROS_LOCATOR")
        .ok()
        .filter(|s| !s.is_empty())
        .and_then(|s| std::ffi::CString::new(s).ok());
    let domain_id: u8 = std::env::var("ROS_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&d| d <= 232)
        .unwrap_or(0) as u8;

    // Open primary (session-owning) executor on the boot thread.
    let mut boot_storage = core::mem::MaybeUninit::<CppContext>::uninit();
    let sptr = boot_storage.as_mut_ptr() as *mut c_void;
    let locator_ptr = locator.as_ref().map_or(core::ptr::null(), |c| c.as_ptr());
    let rc = unsafe {
        nros_cpp_init(
            locator_ptr,
            domain_id,
            name_resolved.as_ptr(),
            core::ptr::null(),
            sptr,
        )
    };
    if rc != NROS_CPP_RET_OK {
        return rc as i32;
    }

    // Get the shared session handle for borrowed-executor tier threads.
    let session_handle: usize = unsafe { nros_cpp_executor_session_handle(sptr) } as usize;

    // Boot tier — apply active_groups + run setup on the owning executor.
    let boot_tier = &tier_slice[0];
    if !boot_tier.groups.is_null() && boot_tier.n_groups > 0 {
        unsafe {
            nros_cpp_executor_set_active_groups(sptr, boot_tier.groups, boot_tier.n_groups);
        }
    }
    if let Some(setup_fn) = boot_tier.setup {
        let setup_rc = unsafe { setup_fn(sptr) };
        if setup_rc != 0 {
            unsafe { nros_cpp_fini(sptr) };
            return setup_rc;
        }
    }

    std::eprintln!(
        "nros: multi-tier run — {} tier(s) over one session",
        n_tiers
    );

    // Shared shutdown flag — boot thread sets it; tier threads poll it.
    let shutdown = Arc::new(AtomicBool::new(false));

    // Wrapper to make *const/*mut Send across thread spawn.
    struct SendUsize(usize);
    unsafe impl Send for SendUsize {}

    // Spawn one std::thread per non-boot tier.
    let mut thread_handles: Vec<std::thread::JoinHandle<()>> = Vec::with_capacity(n_tiers - 1);
    for tier in &tier_slice[1..] {
        let shutdown_clone = Arc::clone(&shutdown);
        let period_us = tier.spin_period_us;
        let n_groups = tier.n_groups;
        let groups_usize = SendUsize(tier.groups as usize);
        let session_usize = SendUsize(session_handle);
        let setup_fn = tier.setup;
        let domain_id_copy: u32 = domain_id as u32;
        let tier_name = if tier.name.is_null() {
            String::new()
        } else {
            unsafe { core::ffi::CStr::from_ptr(tier.name) }
                .to_string_lossy()
                .into_owned()
        };

        let handle = std::thread::Builder::new()
            .name(std::format!("nros-tier-{tier_name}"))
            .spawn(move || {
                // Open borrowed executor (shares the session — does NOT open
                // a new RMW session and does NOT close it on drop).
                let sh = session_usize.0 as *mut c_void;
                let groups_ptr = groups_usize.0 as *const *const c_char;

                let mut tier_storage = core::mem::MaybeUninit::<CppContext>::uninit();
                let tptr = tier_storage.as_mut_ptr() as *mut c_void;
                let rc = unsafe {
                    nros_cpp_executor_open_over_session(sh, core::ptr::null(), domain_id_copy, tptr)
                };
                if rc != NROS_CPP_RET_OK {
                    return;
                }

                // Gate to this tier's callback groups.
                if !groups_ptr.is_null() && n_groups > 0 {
                    unsafe { nros_cpp_executor_set_active_groups(tptr, groups_ptr, n_groups) };
                }

                // Run setup (creates + configures nodes for this tier).
                if let Some(setup) = setup_fn {
                    let setup_rc = unsafe { setup(tptr) };
                    if setup_rc != 0 {
                        // Drop borrowed executor (no session close).
                        unsafe { core::ptr::drop_in_place(tptr as *mut CppContext) };
                        return;
                    }
                }

                // Spin at the tier's period until the shutdown flag is set.
                let period = core::time::Duration::from_micros(period_us.max(1_000));
                while !shutdown_clone.load(Ordering::Relaxed) {
                    let rc = unsafe { nros_cpp_spin_once(tptr, 10) };
                    if rc != NROS_CPP_RET_OK {
                        break;
                    }
                    std::thread::sleep(period);
                }

                // Drop borrowed executor in place (does NOT close the shared session).
                unsafe { core::ptr::drop_in_place(tptr as *mut CppContext) };
            })
            .unwrap_or_else(|e| {
                std::eprintln!("nros: failed to spawn tier '{tier_name}': {e}");
                // Return a dummy thread that immediately exits.
                std::thread::spawn(|| {})
            });
        thread_handles.push(handle);
    }

    // Boot thread spin loop (tier[0] on the owning executor).
    let bound_ms: u64 = std::env::var("NROS_ENTRY_SPIN_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let start_ns = nros_cpp_time_ns();
    let boot_period = core::time::Duration::from_micros(boot_tier.spin_period_us.max(1_000));
    let mut ret = 0i32;
    loop {
        let last = unsafe { nros_cpp_spin_once(sptr, 10) };
        if last != NROS_CPP_RET_OK {
            ret = last as i32;
            break;
        }
        if bound_ms != 0 {
            let elapsed_ms = (nros_cpp_time_ns() - start_ns) / 1_000_000;
            if elapsed_ms >= bound_ms {
                break;
            }
        }
        std::thread::sleep(boot_period);
    }

    // Signal all tier threads to exit and wait for them.
    shutdown.store(true, Ordering::Relaxed);
    for h in thread_handles {
        let _ = h.join();
    }

    // Close the primary (session-owning) executor.
    unsafe { nros_cpp_fini(sptr) };
    ret
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
        // phase-243: the canonical platform µs clock, ns-scaled — for every no_std
        // platform incl. Zephyr (was a Zephyr-only `nros_platform_time_ns` extern,
        // an A-only symbol now retired).
        <nros_platform::ConcretePlatform as nros_platform::PlatformClock>::clock_us()
            .saturating_mul(1_000)
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

#[cfg(test)]
mod qos_override_tests {
    use super::*;
    use nros_rmw::{QosDurabilityPolicy, QosReliabilityPolicy};

    #[test]
    fn apply_qos_overrides_matches_topic_and_role() {
        let ovr = [nros_cpp_qos_override_t {
            topic: c"/chatter".as_ptr(),
            role: NROS_CPP_QOS_OVERRIDE_ROLE_PUBLISHER,
            policy: 0, // reliability
            value: 0,  // best_effort
        }];
        let base = nros_rmw::QosSettings::default(); // Reliable

        let got = unsafe {
            apply_qos_overrides(
                base,
                ovr.as_ptr(),
                ovr.len(),
                "/chatter",
                NROS_CPP_QOS_OVERRIDE_ROLE_PUBLISHER,
            )
        };
        assert_eq!(got.reliability, QosReliabilityPolicy::BestEffort);

        // Wrong role / topic / empty → untouched.
        let got = unsafe {
            apply_qos_overrides(
                base,
                ovr.as_ptr(),
                ovr.len(),
                "/chatter",
                NROS_CPP_QOS_OVERRIDE_ROLE_SUBSCRIPTION,
            )
        };
        assert_eq!(got.reliability, QosReliabilityPolicy::Reliable);
        let got = unsafe {
            apply_qos_overrides(
                base,
                ovr.as_ptr(),
                ovr.len(),
                "/other",
                NROS_CPP_QOS_OVERRIDE_ROLE_PUBLISHER,
            )
        };
        assert_eq!(got.reliability, QosReliabilityPolicy::Reliable);
        let got = unsafe {
            apply_qos_overrides(
                base,
                core::ptr::null(),
                0,
                "/chatter",
                NROS_CPP_QOS_OVERRIDE_ROLE_PUBLISHER,
            )
        };
        assert_eq!(got.reliability, QosReliabilityPolicy::Reliable);
    }

    #[test]
    fn apply_qos_overrides_durability_and_depth() {
        let ovr = [
            nros_cpp_qos_override_t {
                topic: c"/t".as_ptr(),
                role: NROS_CPP_QOS_OVERRIDE_ROLE_SUBSCRIPTION,
                policy: 1,
                value: 1,
            },
            nros_cpp_qos_override_t {
                topic: c"/t".as_ptr(),
                role: NROS_CPP_QOS_OVERRIDE_ROLE_SUBSCRIPTION,
                policy: 3,
                value: 42,
            },
        ];
        let got = unsafe {
            apply_qos_overrides(
                nros_rmw::QosSettings::default(),
                ovr.as_ptr(),
                ovr.len(),
                "/t",
                NROS_CPP_QOS_OVERRIDE_ROLE_SUBSCRIPTION,
            )
        };
        assert_eq!(got.durability, QosDurabilityPolicy::TransientLocal);
        assert_eq!(got.depth, 42);
    }
}
