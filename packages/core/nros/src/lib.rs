//! # nros
//!
//! A lightweight ROS 2 client library for embedded systems.
//!
//! This crate provides a unified API for building ROS 2 nodes in Rust,
//! with support for `no_std` environments and embedded targets.
//!
//! ## Features
//!
//! - **no_std compatible**: Works on bare-metal and RTOS targets
//! - **Zero-copy where possible**: Minimizes memory allocations
//! - **Type-safe**: Compile-time verification of message types
//! - **ROS 2 compatible**: Interoperates with standard ROS 2 nodes via rmw_zenoh
//!
//! ## Quick Start
//!
//! ```ignore
//! use nros::prelude::*;
//! use std_msgs::msg::Int32;
//!
//! let config = ExecutorConfig::from_env().node_name("my_node");
//! let mut executor = Executor::open(&config)?;
//!
//! let node = executor.node_builder("my_node").build()?;
//! let publisher = executor.node_mut(node).create_publisher::<Int32>("/my_topic")?;
//! publisher.publish(&Int32 { data: 42 })?;
//!
//! executor.node_mut(node).create_subscription::<Int32, _>("/topic", |msg: &Int32| {
//!     println!("Received: {}", msg.data);
//! })?;
//!
//! executor.spin_blocking(SpinOptions::default());
//! ```
//!
//! ## Executor Sizing
//!
//! The executor's static memory layout is controlled via environment variables
//! at build time:
//!
//! - **`NROS_EXECUTOR_MAX_CBS`** (default 4) — maximum number of registered
//!   callbacks (subscriptions + timers + services + guard conditions).
//! - **`NROS_EXECUTOR_ARENA_SIZE`** (default 4096) — byte budget for storing
//!   callback closures inline.
//!
//! For messages larger than the default 1024-byte receive buffer, size the
//! subscription via the builder's `.rx_buffer::<N>()` knob (e.g.
//! `node_mut(id).subscription(t).typed::<M>().rx_buffer::<4096>().build(cb)`).
//!
//! ## Transport Backends
//!
//! Phase 248 C5c — `nros` is RMW- and platform-AGNOSTIC. It carries only the
//! `rmw-cffi` vtable; the concrete backend (zenoh / xrce / cyclonedds) enters
//! the link graph via the board crate (embedded), the board-less app's own
//! `nros-rmw-*` dep (native), or the `nros-c`/`nros-cpp` staticlib root (D3),
//! and self-registers through the `RMW_INIT_ENTRIES` walker at `Executor::open`.
//! The concrete session type is resolved automatically; advanced users can
//! access it via `nros::internals::RmwSession`.
//!
//! ## Crate Features
//!
//! `nros` exposes only FUNCTIONAL features — `std`/`alloc`, the `rmw-cffi`
//! vtable, `lending`, `bridge`/`config`, `param-services`,
//! `lifecycle-services`, `safety-e2e`, `stream`, `ffi-sync`, and the ROS
//! edition (`ros-humble`/`ros-iron`). There are NO `platform-*` or concrete
//! `rmw-*` selector features (Phase 248 C7). Platform + RMW are selected by the
//! board / staticlib root via dependencies, not `nros` features. The
//! `zephyr_component_main!` entry macro is gated only on `rmw-cffi` (it's
//! framework entry codegen, like `nros::main!`), not a platform feature.
//!
//! **ROS version** (select one):
//! - `ros-humble` - ROS 2 Humble
//! - `ros-iron` - ROS 2 Iron
//!
//! **Other**:
//! - `std` (default) - Enable standard library support
//! - `alloc` - Enable heap allocation without full std
//!
//! ## Further Reading
//!
//! - [`guide`] — tutorials: getting started, services, configuration,
//!   ROS 2 interop, and troubleshooting
//! - [Message Generation](https://github.com/jerry73204/nano-ros/blob/main/docs/guides/message-generation.md)
//!   — codegen reference (all options, output structure, bundled interfaces)
//! - [Environment Variables](https://github.com/jerry73204/nano-ros/blob/main/docs/reference/environment-variables.md)
//!   — complete buffer tuning reference
//! - [ROS 2 Interop](https://github.com/jerry73204/nano-ros/blob/main/docs/reference/rmw_zenoh_interop.md)
//!   — protocol details (key expressions, liveliness, attachments)
//! - [Examples](https://github.com/jerry73204/nano-ros/tree/main/examples)
//!   — working examples by platform (native, QEMU, ESP32, Zephyr)

#![no_std]

// ── Feature validation (mutual exclusivity) ─────────────────────────────
// Phase 248 C5c/C7 — `nros` carries NO `platform-*` selector features, so the
// platform mutual-exclusion `compile_error!` is gone. The platform is selected
// by the board / staticlib root via an `nros-platform` dep, and nros-node picks
// the kernel primitive at runtime (C2 wake-probe).
// Only `rmw-cffi` is exposed at this layer; the cffi shim selects the
// concrete backend at the C ABI level via the `RMW_INIT_ENTRIES` walker.

// At most one ROS edition.
#[cfg(all(feature = "ros-humble", feature = "ros-iron"))]
compile_error!("`ros-humble` and `ros-iron` are mutually exclusive — select one ROS edition.");

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

// Phase 216.A.5 — the `nros::node!()` proc-macro emits absolute paths
// under `::nros::*` (so downstream Node pkgs only need a single `nros`
// dep). For the in-crate macro-expansion test in `node.rs`, alias the
// `nros` crate name to itself so those absolute paths resolve. Gated on
// `cfg(test)` to keep the alias out of normal builds.
#[cfg(test)]
extern crate self as nros;

// Phase 248 C5c — the umbrella's force-link statics
// (`__FORCE_LINK_{PLATFORM_CFFI,ZENOH,XRCE,CYCLONEDDS_SYS}`) are REMOVED along
// with `nros`'s concrete-backend deps. `nros` no longer references any concrete
// RMW or platform crate, so it has nothing to force-link. Registration + the
// `nros_platform_*` link anchor now live with whoever owns the concrete crate:
//   * embedded   — the BOARD crate force-links its backend + calls
//     `<backend>::register()` in its boot path (C5a);
//   * board-less native — the APP owns `nros-rmw-*` + a `#[used]` force-link in
//     its `main.rs`, and `nros-platform-cffi[posix-c-port]` anchors the C symbols;
//   * C/C++ staticlib — `nros-c`/`nros-cpp` bundle one backend (D3) and anchor
//     `nros-platform` themselves.

// Phase 249 P1 — `__register_linked_rmw()` (a Phase 248 C5c no-op kept only so the
// `nros::main!` framework's call sites compiled) is REMOVED along with those call
// sites. Backend registration never routed through the backend-agnostic `nros` crate:
// hosted auto-registers via the `RMW_INIT_ENTRIES` walk at `Executor::open`; embedded
// boards perform the explicit `<backend>::register()` in their boot path (C5a). One
// Rust trigger = the board/app explicit register (phase-249).

pub mod dispatch_tag;
pub mod guide;
pub mod node;
pub mod node_metadata;
/// Phase 212.M.5.a.2 — executor-backed component runtime.
///
/// Binds [`Node`] / [`ExecutableNode`] to a live
/// [`Executor`] so a Node pkg can actually run (versus
/// [`MetadataRecorder`](node_metadata::MetadataRecorder) which
/// is the planner-side metadata sink).
///
/// Gated on `rmw-cffi`; the underlying [`Executor`] is only present
/// when an RMW backend is linked.
#[cfg(feature = "rmw-cffi")]
pub mod node_runtime;

/// Phase 212.L.5 — top-level init API.
///
/// Re-exported flat at the crate root: `nros::init()`,
/// `nros::init_with_launch_auto()`, `nros::init_with_launch(path)`,
/// `nros::init_with_args(args)`, `nros::Context`, `nros::InitError`.
#[cfg(feature = "std")]
pub mod init;

#[cfg(feature = "std")]
pub use init::{
    Context, ContextSource, InitError, init, init_with_args, init_with_launch,
    init_with_launch_auto,
};

/// Compile-time opaque storage sizes for FFI consumers.
///
/// See [`sizes`] for the `export_size!` pattern used to expose these values
/// to `nros-c` / `nros-cpp` at build time.
pub mod sizes;

/// CDR encapsulation constants and helpers for FFI layers that handle raw
/// CDR bytes (e.g. nros-c, nros-cpp action and service paths).
pub mod cdr {
    pub use nros_serdes::{
        CDR_BE_HEADER, CDR_HEADER_LEN, CDR_LE_HEADER, strip_cdr_header, write_cdr_le_header,
    };
}

// Re-export core types
pub use nros_core::{
    CdrReader, CdrWriter, Clock, ClockType, DeserError, Deserialize, Duration, Logger, MessageInfo,
    PUBLISHER_GID_SIZE, RawMessageInfo, RosMessage, RosService, SerError, Serialize, Time,
};

// Re-export heapless for generated message types and examples
pub use nros_core::heapless;

// Re-export component-mode API
#[cfg(feature = "rmw-cffi")]
pub use node::NodeExecutorRuntime;
// Phase 212.M.5.a.2 — executor-backed runtime entry points.
// (`component_register_symbol` retired in the Phase 212.N.7 closing
// sweep — the helper had no live callers after the BSP baker + macro
// extern emit were deleted.)
pub use node::{
    ActionExecutor, Callback, CallbackCtx, CallbackEffects, ClientDispatch, DeclaredNode,
    DeclaredNodeRuntime, ExecutableNode, MISSING_NODE_EXPORT_ERROR, Node, NodeActionClient,
    NodeActionServer, NodeContext, NodeDeclError, NodeOptions, NodeParameter, NodePublisher,
    NodeResult, NodeRuntime, NodeRuntimeAdapter, NodeServiceClient, NodeServiceServer,
    NodeSubscription, NodeTimer, PublisherResolver, RuntimeNodeRecord, TickCtx,
    record_node_metadata, register_node,
};
// Phase 212.M.5.a.4 — internal helper consumed by `nros::node!()`
// for the BSP dispatch path. Public-but-doc-hidden so the macro expand
// resolves it as `::nros::__private_node_state_into_raw`.
#[cfg(feature = "alloc")]
#[doc(hidden)]
pub use node::__private_node_state_into_raw;
#[cfg(feature = "std")]
pub use node_metadata::SourceMetadataExport;
pub use node_metadata::{
    CallbackEffectKind, CallbackEffectMetadata, EntityKind, EntityMetadata, MetadataRecorder,
    MetadataString, NodeMetadata, NodeMetadataError, ParameterDefault, SourceLocationMetadata,
    SourceNameKind,
};
#[doc(hidden)]
pub use node_metadata::{CallbackId, EntityId, NodeId};
// Phase 216.A.4 — opaque tag types Node authors hold on `Self::State`
// and match against the `Callback<'_>` delivered to
// `ExecutableNode::on_callback`.
pub use dispatch_tag::{ActionTag, ServiceTag, SubscriptionTag};
#[cfg(feature = "rmw-cffi")]
pub use node_runtime::{
    ExecutorError,
    ExecutorNodeRuntime,
    RegisteredNode,
    // Phase 257 (W0-B) — the uniform cross-language component-install seam backing
    // `__nros_component_<pkg>_install` (nros::node!): register an ExecutableNode on the
    // shared executor a foreign typed entry hands in. (`register_node_borrowed` stays
    // crate-internal — it returns the private `ComponentCell`.)
    install_node_typed,
    // W4a — same seam, seeding the node's NodeContext with launch-baked `<param>` initials.
    install_node_typed_with_params,
};

/// Phase 257 (W0-B) — `install_node_typed` stub for builds without the cffi runtime.
/// The typed-entry install seam needs the `rmw-cffi` executor; a `nros::node!()` pkg
/// compiled without `rmw-cffi` still emits `__nros_component_<pkg>_install` (the macro
/// can't see the umbrella's feature), so this stub keeps it linkable — it returns `-1`
/// (no real executor to install on). The real impl is `node_runtime::install_node_typed`.
///
/// # Safety
/// Signature parity with the real impl; the stub dereferences nothing.
#[cfg(not(feature = "rmw-cffi"))]
#[doc(hidden)]
pub unsafe fn install_node_typed<C: node::ExecutableNode + 'static>(
    _executor: *mut core::ffi::c_void,
) -> i32
where
    C::State: 'static,
{
    -1
}

/// W4a — `install_node_typed_with_params` stub for builds without the cffi runtime.
/// Signature parity with `node_runtime::install_node_typed_with_params`; returns `-1`.
///
/// # Safety
/// The stub dereferences nothing.
#[cfg(not(feature = "rmw-cffi"))]
#[doc(hidden)]
pub unsafe fn install_node_typed_with_params<C: node::ExecutableNode + 'static>(
    _executor: *mut core::ffi::c_void,
    _params: &[(&str, &str)],
) -> i32
where
    C::State: 'static,
{
    -1
}
// Phase 212.N.12 — canonical `nros::node!()` macro. Replaces the legacy
// `nros::node!()` macro (retired in the N.12 hard rename — both the
// proc-macro forwarder and the Cargo metadata key are gone).
pub use nros_macros::node;
// Phase 212.N.9 — `nros::main!()` proc-macro family. One-line Entry-pkg
// `main.rs` (replaces the legacy `build.rs + include!()` shape). See
// `docs/design/0024-multi-node-workspace-layout.md` §11.6.
pub use nros_macros::main;

/// Define Zephyr's `rust_main` for a self-bringup Rust component package.
///
/// The macro is intended for `rust_cargo_application()` apps whose crate
/// already invokes `nros::node!()`. It opens a Zephyr executor, registers
/// the supplied component through [`ExecutorNodeRuntime`], and spins forever.
// Phase 248 C7 (Method A) — gated on `rmw-cffi` only (needs `Executor`), NOT a
// `platform-*` feature. This is a framework ENTRY macro (same category as
// `nros::main!`'s zephyr `rust_main` codegen) — `#[macro_export]` so it emits
// nothing unless a Zephyr example invokes it; the body's `::zephyr::*` /
// `::nros_platform::zephyr::wait_network` resolve only in that zephyr-build
// context (the example deps the `zephyr` crate + `nros-platform[platform-zephyr]`).
#[cfg(feature = "rmw-cffi")]
#[macro_export]
macro_rules! zephyr_component_main {
    ($node:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn rust_main() {
            unsafe {
                zephyr::set_logger().ok();
            }
            // Phase 248 C7 step 1 — relocated helper (was `$crate::platform::zephyr`).
            let _ = ::nros_platform::zephyr::wait_network(2000);
            // Phase 249 P1 — RMW register is board/platform-owned (Phase 248 C5a);
            // the backend-agnostic `nros` crate cannot register (no backend dep), so
            // the former no-op `$crate::__register_linked_rmw()` emit is removed. The
            // Zephyr entry's explicit backend register lands via the board/platform
            // boot path (verify under the phase-249 Zephyr e2e gate).
            // Locator: `default_const()` = EMPTY locator → zenoh-pico
            // multicast scouting, which native_sim NSOS can't satisfy.
            // Bake `NROS_LOCATOR` at compile time (the example `build.rs`
            // re-exports `CONFIG_NROS_ZENOH_LOCATOR` from Kconfig into that
            // env). No baked value → falls back to the empty locator.
            const BAKED_LOCATOR: ::core::option::Option<&str> = ::core::option_env!("NROS_LOCATOR");
            let config = match BAKED_LOCATOR {
                ::core::option::Option::Some(loc) if !loc.is_empty() => {
                    $crate::ExecutorConfig::new(loc).node_name(<$node as $crate::Node>::NAME)
                }
                _ => {
                    $crate::ExecutorConfig::default_const().node_name(<$node as $crate::Node>::NAME)
                }
            };
            let executor = match $crate::Executor::open(&config) {
                Ok(executor) => executor,
                Err(_) => return,
            };
            let mut runtime = $crate::ExecutorNodeRuntime::from_executor(executor);
            if runtime.register_node::<$node>().is_err() {
                return;
            }
            // Readiness marker. The C/C++ Zephyr listeners print
            // "Waiting for messages..." from their `main()` before the spin
            // loop; the e2e harness polls for that substring to know the
            // subscriber has declared before starting the talker (Phase 89.12).
            // The Rust path's spin loop lives in this macro (the node only owns
            // callbacks), so emit the same canonical marker here — without it a
            // fully-working Rust listener never signals readiness and the e2e
            // times out at 30 s (issue #35: the zenoh native_sim rust pubsub /
            // service / action failures were this missing marker, not a
            // transport fault — `Executor::open` + `register_node` had already
            // succeeded).
            ::log::info!("Waiting for messages");
            loop {
                let _ = runtime.spin_once(::core::time::Duration::from_millis(10));
            }
        }
    };
}

// Re-export node types
pub use nros_node::{NodeConfig, PublisherHandle, StandaloneNode, SubscriberHandle};

// Re-export publisher/subscriber options (topic + QoS; always available).
pub use nros_node::{PublisherOptions, SubscriberOptions};

// Re-export timer types
pub use nros_node::{TimerCallbackFn, TimerDuration, TimerHandle, TimerMode, TimerState};

// Re-export transport types (middleware-agnostic)
pub use nros_rmw::{
    Publisher, QosDurabilityPolicy, QosHistoryPolicy, QosLivelinessPolicy, QosOverride,
    QosOverrideRole, QosOverrideValue, QosPolicyMask, QosReliabilityPolicy, QosSettings, Rmw,
    RmwConfig, ServiceClientTrait, ServiceInfo, ServiceRequest, ServiceServerTrait, Session,
    SessionMode, Subscriber, TopicInfo, Transport, TransportConfig, TransportError,
};

/// Phase 108.B — standard ROS-2-equivalent QoS profiles. Match
/// upstream `rmw_qos_profile_default` etc. field-by-field. Backends
/// validate against these synchronously at create time; no silent
/// downgrade.
pub mod qos {
    use crate::{
        QosDurabilityPolicy, QosHistoryPolicy, QosLivelinessPolicy, QosReliabilityPolicy,
        QosSettings,
    };

    /// `rmw_qos_profile_default`-equivalent: reliable + volatile +
    /// keep-last(10), automatic liveliness, no deadline / lifespan.
    pub const DEFAULT: QosSettings = QosSettings {
        reliability: QosReliabilityPolicy::Reliable,
        durability: QosDurabilityPolicy::Volatile,
        history: QosHistoryPolicy::KeepLast,
        liveliness_kind: QosLivelinessPolicy::Automatic,
        depth: 10,
        deadline_ms: 0,
        lifespan_ms: 0,
        liveliness_lease_ms: 0,
        avoid_ros_namespace_conventions: false,
    };

    /// `rmw_qos_profile_sensor_data`-equivalent: best-effort +
    /// volatile + keep-last(5).
    pub const SENSOR_DATA: QosSettings = QosSettings {
        reliability: QosReliabilityPolicy::BestEffort,
        depth: 5,
        ..DEFAULT
    };

    /// `rmw_qos_profile_services_default`-equivalent.
    pub const SERVICES_DEFAULT: QosSettings = DEFAULT;

    /// `rmw_qos_profile_parameters`-equivalent: depth = 1000.
    pub const PARAMETERS: QosSettings = QosSettings {
        depth: 1000,
        ..DEFAULT
    };

    /// `rmw_qos_profile_system_default`-equivalent.
    pub const SYSTEM_DEFAULT: QosSettings = DEFAULT;
}

// Re-export safety types when feature is enabled
#[cfg(feature = "safety-e2e")]
pub use nros_rmw::{IntegrityStatus, SafetyValidator, crc32};

// Phase 248 C7 step 1 — the `nros::platform::zephyr` module (the
// `wait_for_network` FFI wrapper) RELOCATED to `nros-platform`
// (`nros_platform::zephyr::wait_network`); callers reference it via
// `::nros_platform::zephyr::wait_network`. nros no longer hosts a platform
// helper module. (The `zephyr_component_main!` macro relocation is C7 step 2.)
//
/// Backend-specific internal types.
///
/// These types are implementation details of the transport backends.
/// Most users should use the high-level APIs (`Executor`, etc.)
/// instead of these types directly.
///
/// The `Rmw*` type aliases resolve to whichever backend is active at compile time,
/// providing a backend-agnostic way to reference concrete transport types.
pub mod internals {
    // ── Backend-agnostic type aliases ────────────────────────────────────
    // These resolve to the concrete types of the active RMW backend.
    // Today the only exposed backend at this layer is the cffi shim.

    #[cfg(feature = "rmw-cffi")]
    pub type RmwSession = nros_rmw_cffi::CffiSession;
    #[cfg(feature = "rmw-cffi")]
    pub type RmwPublisher = nros_rmw_cffi::CffiPublisher;
    #[cfg(feature = "rmw-cffi")]
    pub type RmwSubscriber = nros_rmw_cffi::CffiSubscriber;
    #[cfg(feature = "rmw-cffi")]
    pub type RmwServiceServer = nros_rmw_cffi::CffiServiceServer;
    #[cfg(feature = "rmw-cffi")]
    pub type RmwServiceClient = nros_rmw_cffi::CffiServiceClient;

    /// Phase 124.A — zero-copy publisher slot type. Lives in the
    /// `internals` module so `nros-c` can construct + transmute the
    /// lifetime when boxing the slot for the C-side `_loan` /
    /// `_commit` / `_discard` token plumbing.
    #[cfg(all(feature = "rmw-cffi", feature = "lending"))]
    pub type RmwSlot<'a> = nros_rmw_cffi::CffiSlot<'a>;

    /// Phase 124.A — zero-copy subscriber view type.
    #[cfg(all(feature = "rmw-cffi", feature = "lending"))]
    pub type RmwView<'a> = nros_rmw_cffi::CffiView<'a>;

    /// Open a new middleware session.
    ///
    /// Wraps the backend-specific session constructor behind a common signature.
    /// Used by the C API (`nros-c`); Rust users should prefer `Executor::open()`.
    ///
    /// Phase 156 — consults `$NROS_RMW` (when std + the env var is set)
    /// to pin the primary backend by name, mirroring what `Executor::open`
    /// does for Rust callers. Without this, C bridges built with two
    /// linked backends (e.g. xrce + dds) get whichever ctor fires
    /// first via linkme — non-deterministic across link orderings +
    /// often the wrong backend for the bridge's intended primary.
    #[cfg(feature = "rmw-cffi")]
    pub fn open_session(
        locator: &str,
        mode: nros_rmw::SessionMode,
        domain_id: u32,
        node_name: &str,
    ) -> Result<RmwSession, nros_rmw::TransportError> {
        use nros_rmw::Rmw;

        // Phase 249 P4b.1 — every linked backend self-registered via
        // its `.init_array` ctor before `main` (RFC-0042 §D3.3); no
        // runtime section walk.

        let config = nros_rmw::RmwConfig {
            locator,
            mode,
            domain_id,
            node_name,
            namespace: "",
            properties: &[],
        };
        // Phase 156 — honor `$NROS_RMW` env-var primary selector
        // when present so C bridges built with multiple linked
        // backends (e.g. xrce + dds) pin the primary deterministically
        // instead of taking whichever linkme ctor fires first.
        // Phase 155.B — propagate the real `TransportError` instead of
        // collapsing every backend failure to `ConnectionFailed`. The
        // C-side `nros_support_init` decodes the variant into a
        // specific `NROS_RET_*` code so "init -> -X" tells the user
        // which precondition the backend rejected.
        #[cfg(feature = "std")]
        if let Some(name) = std::env::var("NROS_RMW").ok().filter(|s| !s.is_empty()) {
            return nros_rmw_cffi::CffiRmw::open_with_rmw(&name, &config);
        }
        nros_rmw_cffi::CffiRmw.open(&config)
    }

    /// Drive middleware I/O for pull-based backends.
    ///
    /// Delegates to [`Session::drive_io()`](nros_rmw::Session::drive_io),
    /// which each backend implements appropriately (no-op for push-based,
    /// poll for pull-based).
    ///
    /// Used by the C API executor before polling handles.
    #[cfg(feature = "rmw-cffi")]
    pub fn drive_session_io(session: &mut RmwSession, timeout_ms: i32) {
        use nros_rmw::Session;
        let _ = session.drive_io(timeout_ms);
    }
}

// Re-export types that don't depend on RMW (always available)
pub use nros_node::{
    ExecutorConfig, ExecutorSemantics, GuardConditionHandle, HandleId, HandleSet, InvocationMode,
    NodeError, RawCancelCallback, RawGoalCallback, RawServiceCallback, RawSubscriptionCallback,
    ReadinessSnapshot, SpinOnceResult, SpinOptions, SpinPeriodPollingResult, Trigger,
};

// Re-export RMW-dependent types (require an active transport backend)
#[cfg(feature = "rmw-cffi")]
pub use nros_node::{
    ActionClient, ActionClientCore, ActionServer, ActionServerCore, ActionServerHandle,
    ActionServerRawHandle, ActiveGoal, CompletedGoal, EmbeddedPublisher, EmbeddedRawPublisher,
    EmbeddedServiceClient, EmbeddedServiceServer, Executor, FeedbackStream, GoalFeedbackStream,
    LoanError, NodeHandle, Promise, PublishLoan, RawActionClientSpec, RawActionServerSpec,
    RawActiveGoal, RawSubscription, RecvView, SessionHandle, SessionSpec, Subscription,
};

// Phase 173.5 — board config traits. `BoardConfig` (read locator /
// domain) + `BoardTransportConfig` (the generator writes nros.toml
// `[[transport]]` IP / baud into a NanoRosOwned board `Config`).
// Named `BoardTransportConfig` to avoid colliding with the
// transport-layer `TransportConfig` already re-exported above.
pub use nros_platform::{BoardConfig, BoardTransportConfig};

// Phase 216.A.1 — `DispatchStrategy` enum. User-visible at
// `nros::DispatchStrategy`; the canonical home is `nros_platform::
// board::dispatch` so the C ABI symbol the `nros::node!()` macro emits
// (`__nros_node_<pkg>_dispatch_strategy() -> u8`) lives next to the
// other board-side trampolines.
pub use nros_platform::DispatchStrategy;

/// Implementation detail — used by `nros::node!()` macro expansion.
///
/// Re-exports `nros_platform` so the macro's emitted trampoline can
/// reference `RuntimeCtx` / `RuntimeError` / the `Node*Fn`
/// fn-pointer aliases without forcing every consumer Node pkg's
/// `Cargo.toml` to carry an explicit `nros-platform` dep on top of
/// `nros`. Phase 212.M-F.13 path (b).
///
/// Not part of the public API — paths under this module may change at
/// any time. End users should depend on `nros` alone and invoke
/// `nros::node!()`; the macro routes through here automatically.
#[doc(hidden)]
pub mod __macro_support {
    pub use ::nros_platform;
}

// Phase 110.B / 110.G — scheduling-context API surface. Consumers
// of the Phase 110 cyclic / TT scheduler need these types to
// describe schedules and bind handles; re-exporting them here
// keeps user code free of `nros_node::executor::sched_context`
// path noise. Gated on `rmw-cffi`: the source module is
// `#[cfg(any(has_rmw, test))]` in nros-node, so it only exists once
// an RMW backend is linked (matches the re-export block above).
#[cfg(feature = "rmw-cffi")]
pub use nros_node::executor::sched_context::{
    DeadlinePolicy, OptUs, Priority, SchedClass, SchedContext, SchedContextId,
    TimeTriggeredSchedule, TimeTriggeredScheduleError, TimeTriggeredWindow,
};

#[cfg(all(feature = "std", feature = "rmw-cffi"))]
pub use nros_node::SpinPeriodResult;

// Re-export service types
pub use nros_core::{ServiceClient, ServiceServer};

// Re-export action types
pub use nros_core::{
    CancelResponse, GoalId, GoalInfo, GoalResponse, GoalStatus, GoalStatusStamped, RosAction,
};

// Re-export lifecycle types (always available, no_std compatible)
pub use nros_core::{LifecycleState, LifecycleTransition, TransitionResult};
pub use nros_node::{LifecycleCallbackFn, LifecycleError, LifecyclePollingNode};

/// Re-export of the full lifecycle module so examples can reach
/// `LifecycleCallbackSlot`, `LifecyclePollingNodeCtx`, etc.
pub mod lifecycle {
    pub use nros_core::lifecycle::{LifecycleState, LifecycleTransition, TransitionResult};
    pub use nros_node::lifecycle::*;
}

// Phase 128.G — bridge surface re-exports. Gated behind the
// `bridge` / `config` umbrella features so single-backend builds
// don't pull in `nros-bridge` (or, for `config`, the TOML stack).
#[cfg(feature = "bridge")]
pub use nros_bridge as bridge;

#[cfg(feature = "config")]
pub use nros_bridge::run_from_config;

// Re-export parameter types
pub use nros_params::{
    MandatoryParameter, OptionalParameter, Parameter, ParameterBuilder, ParameterDescriptor,
    ParameterError, ParameterServer, ParameterType, ParameterValue, ParameterVariant,
    ReadOnlyParameter, SetParameterResult,
};
// Phase 172.H — runtime parameter-override persistence backends.
/// Hosted file-backed parameter store (the only built-in backend today).
#[cfg(feature = "std")]
pub use nros_params::FileParamStore;
pub use nros_params::{NullParamStore, ParamStore, ParamStoreError};

/// Prelude module for convenient imports
///
/// Import everything you need with a single statement:
/// ```
/// use nros::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        CdrReader, CdrWriter, Deserialize, Logger, MessageInfo, NodeConfig, PublisherHandle,
        QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy, QosSettings, RosMessage,
        RosService, Serialize, StandaloneNode, SubscriberHandle, TopicInfo,
    };

    // Re-export component-mode API.
    #[cfg(feature = "rmw-cffi")]
    pub use crate::NodeExecutorRuntime;
    #[cfg(feature = "std")]
    pub use crate::SourceMetadataExport;
    pub use crate::{
        ActionTag, Callback, CallbackEffectKind, CallbackEffects, DeclaredNode,
        DeclaredNodeRuntime, EntityKind, MetadataRecorder, Node, NodeActionClient,
        NodeActionServer, NodeContext, NodeDeclError, NodeOptions, NodeParameter, NodePublisher,
        NodeResult, NodeRuntime, NodeRuntimeAdapter, NodeServiceClient, NodeServiceServer,
        NodeSubscription, NodeTimer, ParameterDefault, RuntimeNodeRecord, ServiceTag,
        SourceLocationMetadata, SourceNameKind, SubscriptionTag, node, record_node_metadata,
        register_node,
    };

    // Re-export lifecycle types
    pub use crate::{
        LifecycleCallbackFn, LifecycleError, LifecyclePollingNode, LifecycleState,
        LifecycleTransition, TransitionResult,
    };

    // Re-export executor config + handle types (always available)
    pub use crate::{
        ExecutorConfig, GuardConditionHandle, HandleId, HandleSet, InvocationMode, NodeError,
        SessionMode, SpinOnceResult, SpinOptions, SpinPeriodPollingResult, TransportError, Trigger,
    };

    // Re-export RMW-dependent executor + handle types
    #[cfg(feature = "rmw-cffi")]
    pub use crate::{
        EmbeddedPublisher, EmbeddedServiceClient, Executor, FeedbackStream, NodeHandle, Promise,
        Subscription,
    };

    // Publisher/Subscriber options (topic + QoS).
    pub use crate::{PublisherOptions, SubscriberOptions};

    #[cfg(all(feature = "std", feature = "rmw-cffi"))]
    pub use crate::SpinPeriodResult;

    // Re-export parameter types
    pub use crate::{ParameterServer, ParameterType, ParameterValue};

    // Re-export typed parameter API (rclrs-compatible builder pattern)
    pub use crate::{
        MandatoryParameter, OptionalParameter, ParameterBuilder, ParameterError, ParameterVariant,
        ReadOnlyParameter,
    };

    // Re-export action types
    pub use crate::{GoalId, GoalInfo, GoalResponse, GoalStatus, GoalStatusStamped, RosAction};

    // Re-export Time, Duration, Clock from core
    pub use nros_core::{Clock, ClockType, Duration, Time};

    // Re-export timer types
    pub use crate::{TimerCallbackFn, TimerDuration, TimerHandle, TimerMode};
}

/// Derive macros for message types
///
/// Use these macros to generate message serialization code.
/// These macros help you create custom message types that are compatible
/// with ROS 2's CDR serialization format.
pub mod derive {
    pub use nros_macros::RosMessage;
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_prelude_imports() {
        // This test just verifies that the prelude compiles
        use crate::prelude::*;

        let _ = NodeConfig::new("test_node", "/");
        let _ = QosSettings::BEST_EFFORT;
    }

    /// Verify the Node* canonical trait + context + result types
    /// resolve after the Component→Node hard rename. The Component*
    /// aliases were dropped in the same phase; their absence is
    /// enforced by the workspace audit (no live `Component*` ident
    /// remains in core / examples / tests).
    #[test]
    fn node_context_types_resolve() {
        // Canonical "Node*" trait + context names (post-rename).
        fn _take_node_ctx<N: crate::Node>(_: &mut crate::NodeContext<'_, dyn crate::NodeRuntime>) {}
        // Result type resolves.
        let _: crate::NodeResult<()> = Ok(());
    }
}
