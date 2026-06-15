//! Executor struct and core spin methods.

use core::{marker::PhantomData, mem::MaybeUninit};

use nros_core::{BorrowedMessage, RosMessage, RosService};
use nros_rmw::{QosSettings, ServiceInfo, Session, TopicInfo, TransportError};

use crate::{session, timer::TimerDuration};

#[cfg(feature = "safety-e2e")]
use super::arena::{
    SubSafetyEntry, sub_safety_has_data, sub_safety_pre_sample, sub_safety_try_process,
};
#[cfg(feature = "rmw-cffi")]
use super::types::ExecutorConfig;
#[cfg(feature = "std")]
use super::types::SpinOptions;
use super::{
    arena::{
        BufferStrategy, CallbackMeta, EntryKind, GuardConditionEntry, ServiceClientCallbackEntry,
        ServiceClientRawArenaEntry, ServiceClientSendHeader, SrvEntry, SrvRawEntry,
        SubBufferedBorrowedEntry, SubBufferedEntry, SubBufferedRawCEntry, SubBufferedRawEntry,
        SubBufferedRawInfoCEntry, SubBufferedRawInfoEntry, SubInfoEntry, SubInplaceEntry,
        TimerEntry, TimerHeader, always_ready, buffered_region_size, drop_entry, guard_has_data,
        guard_try_process, no_pre_sample, service_client_callback_try_process,
        service_client_raw_try_process, srv_has_data, srv_raw_has_data, srv_raw_try_process,
        srv_try_process, sub_buffered_borrowed_has_data, sub_buffered_borrowed_try_process,
        sub_buffered_has_data, sub_buffered_raw_c_has_data, sub_buffered_raw_c_try_process,
        sub_buffered_raw_has_data, sub_buffered_raw_info_c_has_data,
        sub_buffered_raw_info_c_try_process, sub_buffered_raw_info_has_data,
        sub_buffered_raw_info_try_process, sub_buffered_raw_try_process, sub_buffered_try_process,
        sub_info_has_data, sub_info_pre_sample, sub_info_try_process, sub_inplace_has_data,
        sub_inplace_try_process, timer_try_process,
    },
    node::NodeHandle,
    spsc_ring::SpscRing,
    triple_buffer::TripleBuffer,
    types::{
        ExecutorSemantics, GuardConditionHandle, HandleId, InvocationMode, NodeError,
        RawResponseCallback, RawServiceCallback, RawSubscriptionCallback,
        RawSubscriptionInfoCallback, ReadinessSnapshot, SpinOnceResult, SpinPeriodPollingResult,
        Trigger,
    },
};

// ============================================================================
// Executor::open() factory method
// ============================================================================

#[cfg(feature = "rmw-cffi")]
impl Executor {
    /// Open a new executor session using the active RMW backend.
    ///
    /// Phase 115.M.4 — auto-registers the cffi vtable for whichever
    /// backend the build was configured for, mirroring the C++ side's
    /// `#ifdef NROS_RMW_<NAME>` fan-out in `<nros/node.hpp>`. The
    /// runtime's atomic vtable slot is idempotent: a re-call of any
    /// backend's `register()` is a no-op, so the fan-out below is safe
    /// to invoke on every `Executor::open` (cheaper than a `Once` and
    /// doesn't pull in `std::sync` for no_std targets).
    ///
    /// Connects to the middleware at the locator specified in `config`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = ExecutorConfig::from_env().node_name("my_node");
    /// let mut executor = Executor::open(&config)?;
    /// ```
    pub fn open(config: &ExecutorConfig<'_>) -> Result<Self, NodeError> {
        use nros_rmw::Rmw;

        // Phase 128.A.3 / 249 P4b.1 — manifest-driven backend selection.
        //
        // Every linked backend self-registered via its `.init_array`
        // ctor before `main` (RFC-0042 §D3.3), so the registry is
        // already populated — no runtime section walk.
        //
        // 1. Consult `$NROS_RMW` (when std/env is available) for
        //    explicit override, mirroring ROS 2's `RMW_IMPLEMENTATION`.
        // 2. With no selector, pick the unique registered backend.
        //    Zero registered → `NoBackend`; more than one →
        //    `Ambiguous` (user must set `$NROS_RMW` or use
        //    `Executor::open_multi`).
        let selector = read_rmw_selector_env();
        // `as_deref()` on `Option<Vec<u8>>` yields `Option<&[u8]>`;
        // on the no_std `Option<&'static [u8]>` variant it's a
        // no-op the lint catches but the std signature still
        // requires the call. Allowed locally.
        #[allow(clippy::needless_option_as_deref)]
        let sel_ref = selector.as_deref();
        match nros_rmw_cffi::resolve_backend(sel_ref) {
            nros_rmw_cffi::BackendResolution::Single(_) => {}
            // Map every non-`Single` outcome to a transport
            // ConnectionFailed for now; the more granular ret codes
            // (NO_BACKEND / AMBIGUOUS / UNKNOWN) are exposed to C
            // callers via `nros_init`'s return value (Phase 128.C.2).
            _ => return Err(NodeError::Transport(TransportError::ConnectionFailed)),
        }

        let rmw_config = nros_rmw::RmwConfig {
            locator: config.locator,
            mode: config.mode,
            domain_id: config.domain_id,
            node_name: config.node_name,
            namespace: config.namespace,
            properties: &[],
        };
        let session = if let Some(name) = sel_ref {
            // Selector path: route to the specific named backend so
            // the env-var-disambiguated outcome matches what the
            // resolver above identified.
            nros_rmw_cffi::CffiRmw::open_with_rmw(
                core::str::from_utf8(name).unwrap_or(""),
                &rmw_config,
            )
        } else {
            nros_rmw_cffi::CffiRmw.open(&rmw_config)
        }
        .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))?;
        let mut executor = Self::from_session(session);
        #[cfg(not(feature = "std"))]
        {
            executor.clock_us_fn = config.clock_us;
            executor.last_spin_end_us = config.clock_us.map(|clock| clock());
        }
        executor.set_node_identity(config.node_name, config.namespace);
        #[cfg(all(feature = "std", feature = "rmw-cffi"))]
        executor.install_wake_signal_on_primary();
        #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
        executor.install_wake_signal_on_primary_alloc();
        Ok(executor)
    }

    /// Phase 128.F.1 — explicit per-backend session declaration for
    /// bridge mode. `specs[0]` becomes the primary session; `specs[1..]`
    /// open as extras keyed by RMW name. After construction, every
    /// `create_node_on(name, rmw)` call dispatches to whichever
    /// session was opened under that RMW name (or, when the rmw name
    /// matches the primary, the primary session itself).
    ///
    /// Single-backend callers should keep using
    /// [`open`](Self::open) — this entry costs an extra
    /// `open_with_rmw` per spec and adds no value when only one
    /// backend is linked.
    ///
    /// `$NROS_RMW` env is ignored: bridge mode wants explicit names.
    #[cfg(feature = "rmw-cffi")]
    pub fn open_multi(specs: &[SessionSpec<'_>]) -> Result<Self, NodeError> {
        // Phase 249 P4b.1 — backends self-registered via their
        // `.init_array` ctor before `main`; no runtime section walk.
        let primary = specs
            .first()
            .ok_or(NodeError::Transport(TransportError::ConnectionFailed))?;
        let primary_session =
            nros_rmw_cffi::CffiRmw::open_with_rmw(primary.rmw, &primary.to_rmw_config())
                .map_err(NodeError::Transport)?;
        let mut executor = Self::from_session(primary_session);
        executor.set_node_identity("", "/");
        // Phase 156 — see `Executor::open` for primary-identity
        // recording rationale.
        let _ = executor.primary_rmw_name.push_str(primary.rmw);
        let _ = executor.primary_locator.push_str(primary.locator);
        #[cfg(all(feature = "std", feature = "rmw-cffi"))]
        executor.install_wake_signal_on_primary();
        #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
        executor.install_wake_signal_on_primary_alloc();

        for spec in specs.iter().skip(1) {
            let session = nros_rmw_cffi::CffiRmw::open_with_rmw(spec.rmw, &spec.to_rmw_config())
                .map_err(NodeError::Transport)?;
            executor
                .extra_sessions
                .push(session)
                .map_err(|_| NodeError::NodeTableFull)?;
            #[cfg(feature = "std")]
            {
                let idx = executor.extra_sessions.len() - 1;
                executor.install_wake_signal_on_extra(idx);
            }
            #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
            {
                let idx = executor.extra_sessions.len() - 1;
                executor.install_wake_signal_on_extra_alloc(idx);
            }
        }

        Ok(executor)
    }

    /// Phase 104.C.1 — open the Executor against a specific RMW
    /// backend by name. Selects from the named registry (Phase
    /// 104.B.2). `rmw_name` must match one of the names a backend
    /// registered under (`"zenoh"`, `"cyclonedds"`, `"xrce"`, …).
    ///
    /// Equivalent to [`Executor::open`] when the registry has exactly
    /// one backend (the default-backend fast path). Use this entry
    /// point in multi-backend builds where `Executor::open` would
    /// pick the first-registered slot.
    ///
    /// Single-Executor multi-Node multi-RMW (the long-term Design X
    /// from `docs/roadmap/phase-104-multi-backend-bridges.md`) is
    /// follow-up work — Phase 104.C.2 + C.3.
    #[cfg(feature = "rmw-cffi")]
    pub fn open_with_rmw(rmw_name: &str, config: &ExecutorConfig<'_>) -> Result<Self, NodeError> {
        if !nros_rmw_cffi::backend_registered() {
            return Err(NodeError::Transport(TransportError::ConnectionFailed));
        }

        let rmw_config = nros_rmw::RmwConfig {
            locator: config.locator,
            mode: config.mode,
            domain_id: config.domain_id,
            node_name: config.node_name,
            namespace: config.namespace,
            properties: &[],
        };
        let session = nros_rmw_cffi::CffiRmw::open_with_rmw(rmw_name, &rmw_config)
            .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))?;
        let mut executor = Self::from_session(session);
        #[cfg(not(feature = "std"))]
        {
            executor.clock_us_fn = config.clock_us;
            executor.last_spin_end_us = config.clock_us.map(|clock| clock());
        }
        executor.set_node_identity(config.node_name, config.namespace);
        // Phase 156 — record primary identity for the session-
        // cache hit path. See `Executor::open` for the rationale.
        let _ = executor.primary_rmw_name.push_str(rmw_name);
        let _ = executor.primary_locator.push_str(config.locator);
        #[cfg(all(feature = "std", feature = "rmw-cffi"))]
        executor.install_wake_signal_on_primary();
        #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
        executor.install_wake_signal_on_primary_alloc();
        Ok(executor)
    }
}

/// Phase 128.F.1 — per-backend session declaration for
/// [`Executor::open_multi`]. Each spec names an RMW backend (must
/// match one a backend registered under via
/// `nros_rmw_cffi_register_named` / the `RMW_INIT_ENTRIES` linker
/// section) and the locator + domain id to open against it.
#[cfg(feature = "rmw-cffi")]
#[derive(Clone, Copy)]
pub struct SessionSpec<'cfg> {
    pub rmw: &'cfg str,
    pub locator: &'cfg str,
    pub domain_id: u32,
    pub node_name: &'cfg str,
    pub namespace: &'cfg str,
}

#[cfg(feature = "rmw-cffi")]
impl<'cfg> SessionSpec<'cfg> {
    /// Minimal spec — just RMW name + locator. Domain id defaults to
    /// 0; node name and namespace are empty.
    pub const fn new(rmw: &'cfg str, locator: &'cfg str) -> Self {
        Self {
            rmw,
            locator,
            domain_id: 0,
            node_name: "",
            namespace: "",
        }
    }

    pub const fn domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }

    pub const fn node_name(mut self, name: &'cfg str) -> Self {
        self.node_name = name;
        self
    }

    pub const fn namespace(mut self, ns: &'cfg str) -> Self {
        self.namespace = ns;
        self
    }

    fn to_rmw_config(self) -> nros_rmw::RmwConfig<'cfg> {
        nros_rmw::RmwConfig {
            locator: self.locator,
            mode: nros_rmw::SessionMode::Client,
            domain_id: self.domain_id,
            node_name: self.node_name,
            namespace: self.namespace,
            properties: &[],
        }
    }
}

// Phase 128.A.3 — selector for the single-backend resolution path.
//
// On hosted (`std`) builds, read `$NROS_RMW`; mirrors ROS 2's
// `RMW_IMPLEMENTATION`. Returns the name as a byte vector so the
// caller can pass it to `nros_rmw_cffi::resolve_backend` and (when
// `Some`) to `CffiRmw::open_with_rmw`.
//
// On `no_std` / bare-metal builds, environment variables are not
// available; resolution always falls through to the single-backend
// or ambiguous path. Embedded users with multiple backends use the
// bridge surface `Executor::open_multi` instead.
#[cfg(all(feature = "std", feature = "rmw-cffi"))]
fn read_rmw_selector_env() -> Option<alloc::vec::Vec<u8>> {
    let raw = std::env::var_os("NROS_RMW")?;
    let bytes = raw.as_encoded_bytes();
    if bytes.is_empty() {
        return None;
    }
    Some(bytes.to_vec())
}

#[cfg(all(not(feature = "std"), feature = "rmw-cffi"))]
fn read_rmw_selector_env() -> Option<&'static [u8]> {
    None
}

// ============================================================================
// SessionStore — owned or borrowed session
// ============================================================================

/// Session storage: owned or borrowed via raw pointer.
///
/// The C API creates a session in `nros_support_init()` before the
/// executor. `Borrowed` lets the executor use that session without owning it.
#[allow(clippy::large_enum_variant)]
pub(crate) enum SessionStore {
    Owned(session::ConcreteSession),
    Borrowed(*mut session::ConcreteSession),
}

impl core::ops::Deref for SessionStore {
    type Target = session::ConcreteSession;
    fn deref(&self) -> &session::ConcreteSession {
        match self {
            SessionStore::Owned(s) => s,
            SessionStore::Borrowed(ptr) => unsafe { &**ptr },
        }
    }
}

impl core::ops::DerefMut for SessionStore {
    fn deref_mut(&mut self) -> &mut session::ConcreteSession {
        match self {
            SessionStore::Owned(s) => s,
            SessionStore::Borrowed(ptr) => unsafe { &mut **ptr },
        }
    }
}

/// Phase 228.E — an opaque, `Send` handle to an [`Executor`]'s RMW session.
///
/// In the per-tier model the boot executor opens the one session and hands each
/// spawned tier task a handle (not a borrow) so the task opens its own
/// [`Executor`] over that *same* session across the RTOS task boundary. Wrapping
/// the `pub(crate)` session pointer lets board crates (`nros-board-posix`,
/// `nros-board-freertos`, …) name + move the handle without naming the session
/// type. Obtain via [`Executor::session_handle`]; consume via
/// [`Executor::open_with_session_handle`].
#[cfg(any(has_rmw, test))]
pub struct SessionHandle(*mut session::ConcreteSession);

// SAFETY: the per-tier model deliberately shares one session across RTOS tasks;
// concurrent access is serialized by the RMW backend's internal locks (the RTOS
// targets build zenoh-pico `Z_FEATURE_MULTI_THREAD=1` — RFC-0032 §5.0). The
// boot executor owns the session and outlives every tier task.
#[cfg(any(has_rmw, test))]
unsafe impl Send for SessionHandle {}

/// Phase 228.C — pure callback-group filter decision. `None` = wildcard (accept
/// every group); `Some` = accept only listed groups. Backs
/// [`Executor::group_active`]; split out so the logic is unit-testable without a
/// live session.
pub(crate) fn group_filter_accepts<const N: usize, const M: usize>(
    active: &Option<heapless::Vec<heapless::String<N>, M>>,
    group: &str,
) -> bool {
    match active {
        None => true,
        Some(v) => v.iter().any(|g| g.as_str() == group),
    }
}

#[cfg(test)]
mod group_filter_tests {
    use super::group_filter_accepts;

    type Groups = heapless::Vec<heapless::String<32>, { crate::config::MAX_NODES }>;

    #[test]
    fn wildcard_accepts_all() {
        let none: Option<Groups> = None;
        assert!(group_filter_accepts(&none, "anything"));
    }

    #[test]
    fn set_accepts_only_listed_groups() {
        let mut v: Groups = heapless::Vec::new();
        let mut s = heapless::String::new();
        s.push_str("ctrl").unwrap();
        v.push(s).unwrap();
        let active = Some(v);
        assert!(group_filter_accepts(&active, "ctrl"));
        assert!(!group_filter_accepts(&active, "telem"));
    }
}

// ============================================================================
// Executor
// ============================================================================

/// Backend-agnostic executor that owns a session.
///
/// Provides `create_node()` for entity creation and `drive_io()` for polling.
///
/// # Callback Mode
///
/// The executor supports arena-based callback registration via the
/// `node_mut(id).subscription(t)` builder and
/// [`register_service()`](Self::register_service), with dispatch via
/// [`spin_once()`](Self::spin_once). No heap allocation is needed.
///
/// The sizes are set via `NROS_EXECUTOR_MAX_CBS` (default 4) and
/// `NROS_EXECUTOR_ARENA_SIZE` (default 4096) environment variables at build time.
///
/// Phase 124.B.2 — opaque context handed to the runtime wake
/// callback. Backends store the raw pointer + invoke the callback;
/// the callback decodes back to `&WakeCtx`.
#[cfg(all(feature = "std", feature = "rmw-cffi"))]
pub(crate) struct WakeCtx {
    pub(crate) flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) cv: std::sync::Arc<std::sync::Condvar>,
    #[allow(dead_code)] // Held by spin_once's wait predicate (124.B.4).
    pub(crate) mu: std::sync::Arc<std::sync::Mutex<()>>,
    /// Phase 130.3 — Zephyr+std uses the k_sem wake primitive; the
    /// runtime cb signals both this and the std cv so a future
    /// migration to a single primitive flips one branch instead
    /// of two.
    pub(crate) node_wake: Option<std::sync::Arc<super::node_wake::NodeWake>>,
}

/// Phase 124.B.2 — runtime wake callback.
///
/// RT-context contract:
///
/// * **Thread-safe**: callable from any thread. The cb is lock-free
///   on the cv path — no mutex held during `notify_all`. Lost-wakeup
///   is prevented by the waiter checking `wake_flag` under
///   `wake_mu` via the `wait_timeout_while` predicate.
/// * **NOT async-signal-safe on POSIX**: `pthread_cond_signal`
///   isn't on the POSIX async-signal-safe function list. For POSIX
///   signal handler wake, use a `signalfd` + select pattern in a
///   thread that owns the wake duty.
/// * **RTOS ISR**: per-RTOS platform layer wraps the cv with an
///   ISR-safe primitive (`xSemaphoreGiveFromISR`,
///   `tx_event_flags_set` from ISR, `k_sem_give` from ISR on
///   Zephyr). Backend's ISR caller routes through the platform's
///   `signal_from_isr` API instead of this cb directly.
/// * **Bounded execution time**: O(1) — atomic store + cv notify.
///   No allocation, no contended lock.
///
/// The cb is the symbol backends invoke from their async wake path
/// (datagram arrival, worker-thread enqueue, etc.). It does
/// flag-write + condvar-signal in that order, lock-free.
#[cfg(all(feature = "std", feature = "rmw-cffi"))]
pub(crate) unsafe extern "C" fn nros_rmw_runtime_wake_cb(ctx: *mut core::ffi::c_void) {
    if ctx.is_null() {
        return;
    }
    // Phase 141.B.2 — capture T0 at cb entry. No-op when the
    // probe feature is off or no cycle reader is installed.
    #[cfg(feature = "wake-latency-probe")]
    super::wake_probe::on_wake();
    // SAFETY: ctx points at a `WakeCtx` owned by an Executor still
    // alive at the time of the call. Executor::drop must clear the
    // callback via `set_wake_callback(None, _)` on all sessions
    // before dropping wake_ctx; this happens in `install_wake_*`
    // teardown path.
    let wake = unsafe { &*(ctx as *const WakeCtx) };
    wake.flag.store(true, std::sync::atomic::Ordering::SeqCst);
    // Lock-free notify. The waiter observes wake_flag under wake_mu
    // in its wait_timeout_while predicate — flag.store with SeqCst
    // happens-before any subsequent acquire in the waiter, so the
    // waiter cannot miss the signal even though we don't hold mu
    // here. Standard pthread cond-var idiom.
    wake.cv.notify_all();
    // Phase 130.3 — Zephyr+std waits on `NodeWake` (k_sem) instead
    // of the std cv. Signal both so the cb keeps working whichever
    // wait primitive spin_once is using.
    if let Some(nw) = wake.node_wake.as_ref() {
        nw.signal();
    }
}

/// Phase 124.B.7.c — POSIX signalfd worker.
///
/// Owns a Linux `eventfd` plus a worker thread that `read()`s the
/// fd and forwards via `wake_ctx.cv.notify_all()`. The eventfd
/// write side is async-signal-safe per the kernel contract
/// (`write(2)` to an eventfd is permitted from signal handlers),
/// closing the gap that `pthread_cond_signal` leaves open on POSIX.
///
/// Lifecycle:
///   * Constructed lazily in `Executor::signal_fd()` on first
///     caller request.
///   * `Drop` writes a shutdown sentinel + joins the worker.
///
/// Caller flow (signal handler):
///   1. Get fd via `Executor::signal_fd()` before installing the
///      handler.
///   2. Handler does `eventfd_write(fd, 1)` (equivalently,
///      `write(fd, &1u64, 8)`).
///   3. Worker thread reads the fd, signals wake_cv. spin_once
///      blocked in cv.wait_timeout_while sees flag=true and exits.
#[cfg(all(feature = "signal-fd-wake", target_os = "linux"))]
pub struct WakeSignalFd {
    fd: core::ffi::c_int,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(all(feature = "signal-fd-wake", target_os = "linux"))]
impl WakeSignalFd {
    /// Spawn the worker. `wake_ctx_ptr` is the `*const WakeCtx`
    /// produced by `Executor::wake_ctx_ptr` — same value the
    /// runtime wake cb decodes.
    fn new(wake_ctx_ptr: *const WakeCtx) -> Result<Self, std::io::Error> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }

        let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shutdown_clone = std::sync::Arc::clone(&shutdown);

        // Pass wake_ctx pointer as usize so the closure is Send.
        // SAFETY: the pointer is valid for the Executor's lifetime
        // (WakeCtx is owned by the Executor's `wake_ctx: Arc<WakeCtx>`
        // field which outlives this worker thread — we join in Drop).
        let ctx_addr = wake_ctx_ptr as usize;
        let worker = std::thread::Builder::new()
            .name("nros-wakefd".into())
            .spawn(move || {
                let ctx = ctx_addr as *const WakeCtx;
                loop {
                    let mut buf = [0u8; 8];
                    let n =
                        unsafe { libc::read(fd, buf.as_mut_ptr() as *mut core::ffi::c_void, 8) };
                    if n <= 0 {
                        // EINTR / EOF — re-check shutdown then loop.
                        if shutdown_clone.load(std::sync::atomic::Ordering::Acquire) {
                            return;
                        }
                        continue;
                    }
                    if shutdown_clone.load(std::sync::atomic::Ordering::Acquire) {
                        return;
                    }
                    // Same effect as nros_rmw_runtime_wake_cb. We
                    // can't call it directly because it dereferences
                    // ctx as &WakeCtx which would race with Executor
                    // drop unless we hold a guarantee — the
                    // shutdown_flag check above + Drop's join gives it.
                    unsafe {
                        let w = &*ctx;
                        w.flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        w.cv.notify_all();
                    }
                }
            })
            .map_err(|e| {
                unsafe { libc::close(fd) };
                std::io::Error::other(alloc::format!("spawn nros-wakefd worker: {e}"))
            })?;

        Ok(Self {
            fd,
            shutdown,
            worker: Some(worker),
        })
    }

    /// Returns the writable eventfd. The caller (typically a POSIX
    /// signal handler) writes any non-zero 8-byte value to trigger
    /// a wake. `write(2)` on an eventfd is async-signal-safe per
    /// `eventfd(2)` man page.
    pub fn fd(&self) -> core::ffi::c_int {
        self.fd
    }
}

#[cfg(all(feature = "signal-fd-wake", target_os = "linux"))]
impl Drop for WakeSignalFd {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Release);
        // Wake the worker so it re-checks shutdown.
        let one: u64 = 1;
        unsafe {
            libc::write(self.fd, &one as *const u64 as *const core::ffi::c_void, 8);
        }
        if let Some(j) = self.worker.take() {
            let _ = j.join();
        }
        unsafe { libc::close(self.fd) };
    }
}

/// Phase 124.B.7.b — ISR / interrupt-context wake callback.
///
/// Same semantics as [`nros_rmw_runtime_wake_cb`] but constrained to
/// async-signal-safe / ISR-safe primitives.
///
/// Per-platform routing:
///
/// * **POSIX (std)**: `pthread_cond_signal` is NOT on the POSIX
///   async-signal-safe function list. Calling from a SIGUSR1
///   handler is technically UB. Real fix (Phase 124.B.7.c) routes
///   via `signalfd`/`eventfd` + a runtime worker thread; until that
///   lands, signal-handler callers MUST use
///   `nros_guard_condition_trigger` from a **separate thread** (not
///   from the handler itself), OR set the wake_flag and rely on
///   the next poll deadline. This cb currently aliases the regular
///   `wake_cb` and is safe only from non-signal-handler ISR-like
///   contexts (e.g. timer thread, kernel callback).
///
/// * **RTOS no_std (Zephyr/FreeRTOS/ThreadX)**: routes through the
///   platform-cffi `condvar_signal_from_isr` slot. Each backend
///   uses its ISR-safe variant — `xSemaphoreGiveFromISR`,
///   `tx_semaphore_put`, `k_condvar_signal`.
///
/// `ctx` semantics identical to [`nros_rmw_runtime_wake_cb`].
#[cfg(all(feature = "std", feature = "rmw-cffi"))]
#[allow(dead_code)] // Public exposure pending B.7.c signalfd worker.
pub(crate) unsafe extern "C" fn nros_rmw_runtime_wake_cb_from_isr(ctx: *mut core::ffi::c_void) {
    // Today: alias regular wake_cb. POSIX signal-handler safety
    // pending B.7.c (signalfd worker-thread forward). Documented in
    // the contract above so callers know the boundary.
    unsafe { nros_rmw_runtime_wake_cb(ctx) };
}

/// Phase 216 follow-up — per-Node dispatch trampoline registered with
/// [`Executor::register_dispatch_slot`].
///
/// The board-side dispatch task (RTIC `__nros_run` / Embassy
/// `__nros_run_task`) dequeues a `nros_platform::SignaledCallback`
/// envelope and forwards `(cb_id, ctx_ptr)` into
/// [`Executor::dispatch_callback`]; that method linear-scans this
/// slot table and invokes every registered `on_callback` with the
/// owning Node's per-Node `state` blob. Each Node's
/// `__nros_node_<pkg>_on_callback` self-filters on its own
/// `CallbackId` tag set, so a slot whose Node doesn't own this
/// callback is a cheap no-op string compare.
///
/// The shape mirrors the per-pkg `__nros_node_<pkg>_on_callback`
/// extern "C" trampoline emitted by the `nros::node!()` macro
/// (see `packages/core/nros-macros/src/lib.rs` Phase 216.A.5).
///
/// # Why not `linkme`
///
/// `linkme::distributed_slice` hangs on bare-metal Cortex-M /
/// RISC-V because `cortex_m_rt`'s link script doesn't provide the
/// `__start_/__stop_` section anchors in a shape that lets the
/// iterator terminate (see
/// `packages/core/nros-rmw-cffi/src/section.rs` Phase 142). Since
/// stm32f4 RTIC / Embassy boards are the whole point of Phase 216,
/// the registry uses the explicit `register()` pattern from Phase
/// 104.A.
#[derive(Clone, Copy)]
pub struct DispatchSlot {
    /// Owning Node's `State` blob — produced by the macro-emitted
    /// `i()` and round-tripped through
    /// `nros::__private_node_state_into_raw`. Opaque to the
    /// executor.
    pub state: *mut core::ffi::c_void,
    /// Per-Node `extern "C"` trampoline; signature matches the
    /// `__nros_node_<pkg>_on_callback` symbol the `nros::node!()`
    /// macro emits.
    pub on_callback: unsafe extern "C" fn(
        state: *mut core::ffi::c_void,
        cb_id_ptr: *const u8,
        cb_id_len: usize,
        ctx: *mut core::ffi::c_void,
    ),
}

// SAFETY: `DispatchSlot` carries two raw pointers (`state` + an
// extern "C" fn pointer). The fn pointer is `Send`/`Sync` by
// definition; the `state` pointer's `Send`/`Sync` story matches the
// owning `Executor` (which is `unsafe impl Send`). Treating the
// slot itself as `Send` keeps the existing `Executor` Send impl
// intact — see `unsafe impl Send for Executor {}` later in this
// file.
unsafe impl Send for DispatchSlot {}
unsafe impl Sync for DispatchSlot {}

pub struct Executor {
    pub(crate) session: SessionStore,
    pub(crate) arena: [MaybeUninit<u8>; crate::config::ARENA_SIZE],
    pub(crate) arena_used: usize,
    pub(crate) entries: [Option<CallbackMeta>; crate::config::MAX_CBS],
    /// Phase 110.B — registered scheduling contexts. Slot 0 is
    /// auto-populated with a `Fifo` SC at construction; every entry
    /// without an explicit binding maps to it via
    /// `sched_context_bindings`.
    pub(crate) sched_contexts: [Option<super::sched_context::SchedContext>; crate::config::MAX_SC],
    /// Per-entry SC binding parallel to `entries`. Defaults to
    /// `SchedContextId(0)` (the auto-created Fifo SC).
    pub(crate) sched_context_bindings:
        [super::sched_context::SchedContextId; crate::config::MAX_CBS],
    /// Phase 110.E — user-space sporadic-server budget state per
    /// Sporadic-class SC. Slot indices match `sched_contexts`; non-
    /// Sporadic slots stay `None`.
    pub(crate) sporadic_states:
        [Option<super::sched_context::SporadicState>; crate::config::MAX_SC],
    /// Phase 110.E.b — atomic sporadic state + opaque platform-timer
    /// handle for ISR-driven refill. Populated by
    /// `register_sporadic_timer`; dropped on Executor `Drop` via the
    /// stored `destroy_fn`.
    #[cfg(feature = "alloc")]
    pub(crate) sporadic_atomic_states: [Option<(
        portable_atomic_util::Arc<super::sched_context::AtomicSporadicState>,
        OpaqueTimerHandle,
    )>; crate::config::MAX_SC],
    /// Phase 110.G — major-frame length for time-triggered dispatch.
    /// `0` (default) disables the TT gate entirely; non-zero enables
    /// gating per
    /// `SchedContext.tt_window_offset_us / tt_window_duration_us`.
    pub(crate) major_frame_us: u32,
    /// Phase 110.F — per-OS-priority worker pool. Lazily populated
    /// on first dispatch routing to a non-zero `os_pri`. Lives
    /// behind `feature = "scheduler-os-priority"` + `feature =
    /// "std"` because workers need `std::thread` + `mpsc`.
    #[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
    pub(crate) os_priority_workers: std::collections::HashMap<u8, OsPriorityWorker>,
    /// Phase 110.F — caller-supplied `apply_policy` function pointer
    /// each worker invokes at startup to elevate its OS priority.
    /// `None` = the worker pool is disabled; entries bound to non-
    /// zero `os_pri` SCs fall back to the cooperative path.
    /// Mirrors `Executor::open_threaded`'s `apply_policy: fn(...)`
    /// shape — keeps Executor non-generic over Platform.
    #[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
    pub(crate) os_priority_apply_policy:
        Option<fn(nros_platform_api::SchedPolicy) -> Result<(), nros_platform_api::SchedError>>,
    pub(crate) trigger: Trigger,
    pub(crate) semantics: ExecutorSemantics,
    /// Node name for entities created via `register_subscription`/`register_service`.
    /// Empty means unset — no liveliness tokens will be declared.
    pub(crate) node_name: heapless::String<64>,
    /// Phase 228.C — per-tier callback-group filter. `None` = wildcard (register
    /// every callback — the single-tier degenerate case + today's behaviour).
    /// `Some(groups)` = this tier's executor accepts only callbacks whose
    /// `.callback_group()` is in the set; others are skipped at registration.
    pub(crate) active_groups:
        Option<heapless::Vec<heapless::String<32>, { crate::config::MAX_NODES }>>,
    /// Node namespace (default: "/").
    pub(crate) namespace: heapless::String<64>,
    /// Phase 104.C.2 — rclcpp-style `add_node` table. Holds the
    /// per-Node metadata (name, namespace, rmw, locator, default
    /// SchedContext) for every Node attached to this Executor. The
    /// implicit "primary" Node (NodeId(0)) mirrors `node_name` +
    /// `namespace` above and is auto-populated on first use.
    pub(crate) nodes: heapless::Vec<super::node_record::NodeRecord, { crate::config::MAX_NODES }>,
    /// Phase 216 follow-up — per-Node dispatch trampoline registry.
    ///
    /// Populated by [`Executor::register_dispatch_slot`]; walked by
    /// [`Executor::dispatch_callback`] each time the board-side
    /// dispatch task hands off a `SignaledCallback` envelope.
    /// Sized by `MAX_NODES` because the upper-bound is one slot per
    /// Node pkg deployed on this executor (the same upper bound used
    /// by `nodes` and `extra_sessions`). `MAX_NODES` is driven by the
    /// `NROS_EXECUTOR_MAX_NODES` build-script env var (default 4);
    /// boards that deploy more Node pkgs raise it at build time.
    ///
    /// Default is `heapless::Vec::new()` (empty) — Nodes register
    /// themselves explicitly via the `register_dispatch_slot` API.
    /// The fallback shape avoids the `linkme` hazard on bare-metal
    /// Cortex-M / RISC-V (see `DispatchSlot` doc).
    pub(crate) dispatch_slots: heapless::Vec<DispatchSlot, { crate::config::MAX_NODES }>,
    /// Phase 104.C.3 — extra sessions opened by `node_builder.rmw()`
    /// calls that named a backend different from the Executor's
    /// primary session. Indexed by `NodeRecord.session_idx`
    /// (1..=N maps to `extra_sessions[N-1]`; idx 0 is the primary
    /// `self.session`). Sized by `NROS_EXECUTOR_MAX_NODES` since one
    /// extra session per Node is the worst case.
    pub(crate) extra_sessions:
        heapless::Vec<session::ConcreteSession, { crate::config::MAX_NODES }>,
    /// Phase 156 — primary session's rmw name + locator, captured
    /// at `open*` time so `NodeBuilder::resolve_session_slot`'s
    /// cache lookup can detect when a `.rmw(name).locator(loc)`
    /// matches the primary (slot 0) instead of falling through to
    /// `CffiRmw::open_with_rmw` and trying to open a SECOND
    /// session against the same backend. zenoh-pico's global state
    /// is a process singleton; opening twice fails. Empty when
    /// constructed via `from_session(_ptr)` without `open*`
    /// recording the metadata; in that case the cache check
    /// degrades to "always miss" (today's behaviour).
    pub(crate) primary_rmw_name: heapless::String<32>,
    pub(crate) primary_locator: heapless::String<128>,
    #[cfg(feature = "std")]
    pub(crate) halt_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Phase 104.C.6 — shared executor wake flag. Any source of work
    /// (foreign thread handing off a callback, signal handler, future
    /// per-session vtable wake hook) sets this; `spin_once` swaps it to
    /// `false` on entry and, if it was `true`, polls every session with
    /// a 0-ms timeout instead of blocking. Lets one notification wake
    /// the executor regardless of which session the user is currently
    /// blocked on (the multi-RMW bridge case).
    #[cfg(feature = "std")]
    pub(crate) wake_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Phase 124.B.2 — wake condvar paired with `wake_flag`. The
    /// runtime-supplied wake callback (`nros_rmw_runtime_wake_cb` in
    /// nros-rmw-cffi) writes `wake_flag = true` AND signals
    /// `wake_cv` atomically under `wake_mu`. `spin_once` blocks on
    /// the cv with a deadline instead of calling `drive_io` with the
    /// user's timeout — sub-poll-period wake latency.
    ///
    /// Poll-only backends (NULL `set_wake_callback` slot) leave the
    /// cb uninstalled; the cv wait still fires on its deadline,
    /// then drive_io(0) drains whatever the backend's internal
    /// poll has buffered.
    #[cfg(feature = "std")]
    #[allow(dead_code)] // Wired by spin_once after 124.B.4.
    pub(crate) wake_cv: std::sync::Arc<std::sync::Condvar>,
    #[cfg(feature = "std")]
    #[allow(dead_code)]
    pub(crate) wake_mu: std::sync::Arc<std::sync::Mutex<()>>,
    /// Phase 130.3 — Zephyr+std uses `nros_platform_wake_*` (k_sem)
    /// instead of `std::sync::Condvar` because Zephyr's libc
    /// `pthread_cond_timedwait` hangs past its deadline. `None`
    /// when the platform provider didn't link a wake primitive
    /// (e.g. test builds with `rmw-cffi` but no `platform-*`
    /// feature); spin_once falls back to driving the transport
    /// for the full timeout in that case.
    #[cfg(all(feature = "std", feature = "rmw-cffi"))]
    pub(crate) node_wake: Option<std::sync::Arc<super::node_wake::NodeWake>>,
    /// Phase 130.4 — true when at least one session's backend
    /// installed the wake callback. Drives whether `spin_once`
    /// uses the wake-primitive wait (`NodeWake` / `Condvar`) or
    /// just `drive_io(timeout_ms)`. Poll-only backends
    /// (XRCE-DDS-Client, current Cyclone/dust-DDS shims) leave
    /// this `false`; the wait then becomes a no-op sleep that
    /// starves reliable retransmission (Phase 127.C.4 root
    /// cause: server's `send_reply` flushes 100 ms once, then a
    /// blind `wait_ms(100)` sleeps with zero session activity, so
    /// the agent's ACK arrives into a stalled session and reliable
    /// redelivery never fires).
    #[cfg(all(feature = "std", feature = "rmw-cffi"))]
    pub(crate) has_async_wake: bool,
    /// Phase 124.B.2 — opaque context Arc handed to backends via
    /// `set_wake_callback`. Lazy-allocated on first install; stays
    /// alive for the Executor's lifetime so the raw pointer stored
    /// in backends remains valid.
    #[cfg(all(feature = "std", feature = "rmw-cffi"))]
    pub(crate) wake_ctx: Option<std::sync::Arc<WakeCtx>>,
    // Phase 141.A.3 — alloc-mode (no_std RTOS) mirror of the wake
    // state above. Same semantics: `wake_flag_alloc` is set SeqCst
    // by the runtime cb + cleared by spin_once on entry;
    // `node_wake_alloc` is the kernel-native binary semaphore
    // (lifted to alloc cfg in e36ee8cf) the cb signals;
    // `wake_ctx_alloc` is the Arc handed to backends via
    // `set_wake_callback(Some(cb), Arc::as_ptr(ctx) as *mut _)`.
    // `has_async_wake_alloc` is `true` after the first session
    // accepts the wake-cb install (`supports_wake_callback`).
    // Drives the no_std spin_once wait branch to block on
    // `node_wake_alloc.wait_ms(deadline)` instead of relying on
    // `drive_io`'s transport-blocking recv for the full timeout.
    #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
    pub(crate) wake_flag_alloc: portable_atomic_util::Arc<portable_atomic::AtomicBool>,
    #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
    pub(crate) node_wake_alloc: Option<portable_atomic_util::Arc<super::node_wake::NodeWake>>,
    #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
    pub(crate) wake_ctx_alloc: Option<portable_atomic_util::Arc<super::wake_alloc::WakeCtxAlloc>>,
    #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
    pub(crate) has_async_wake_alloc: bool,
    /// Phase 124.B.7.c — lazily-allocated POSIX signalfd worker.
    /// Owned by the Executor; spawned on first `signal_fd()` call.
    /// Drop joins the worker thread and closes the fd.
    #[cfg(all(feature = "signal-fd-wake", target_os = "linux"))]
    pub(crate) signal_fd: Option<WakeSignalFd>,
    #[cfg(feature = "param-services")]
    pub(crate) params: Option<alloc::boxed::Box<crate::parameter_services::ParamState>>,
    #[cfg(feature = "lifecycle-services")]
    pub(crate) lifecycle:
        Option<alloc::boxed::Box<crate::lifecycle_services::LifecycleRuntimeState>>,
    /// Sub-millisecond wall-clock residual carried across `spin_once` calls
    /// so timers tick at true wall-clock rate even when `drive_io` returns
    /// in well under 1 ms (e.g. zenoh-pico condvar wakeups under load).
    #[cfg(feature = "std")]
    pub(crate) spin_residual_us: u64,
    /// Sub-millisecond residual for no_std wall-clock timer accounting.
    #[cfg(not(feature = "std"))]
    pub(crate) spin_residual_us: u64,
    /// Wall-clock instant at which the previous `spin_once` exited. The
    /// timer delta on the next call is measured from this point so any
    /// time the caller spent between `spin_once` invocations (e.g. an
    /// explicit `thread::sleep`) counts toward timer accumulation just
    /// like time spent inside `drive_io`.
    #[cfg(feature = "std")]
    pub(crate) last_spin_end: Option<std::time::Instant>,
    /// Monotonic clock endpoint for no_std timer accounting.
    #[cfg(not(feature = "std"))]
    pub(crate) last_spin_end_us: Option<u64>,
    /// Optional platform clock hook supplied by `ExecutorConfig`.
    #[cfg(not(feature = "std"))]
    pub(crate) clock_us_fn: Option<fn() -> u64>,
}

impl Executor {
    /// Create an executor from an already-opened session.
    // The `arena` field intentionally lives inline so embedded callers can
    // place an `Executor` in `static` storage without an allocator. The
    // construction expression is large but is RVO'd into its destination
    // by the optimiser; the clippy lint flags it because it can't prove
    // the move-elision.
    #[allow(clippy::large_stack_arrays)]
    pub fn from_session(session: session::ConcreteSession) -> Self {
        // SAFETY: MaybeUninit::uninit() is always safe; these bytes are only
        // accessed through properly-typed ptr::write / ptr::read via the
        // dispatch function pointers stored in `entries`.
        Self {
            session: SessionStore::Owned(session),
            arena: [MaybeUninit::uninit(); crate::config::ARENA_SIZE],
            arena_used: 0,
            entries: [None; crate::config::MAX_CBS],
            sched_contexts: {
                let mut s = [None; crate::config::MAX_SC];
                s[0] = Some(super::sched_context::SchedContext::default());
                s
            },
            sched_context_bindings: [super::sched_context::SchedContextId(0);
                crate::config::MAX_CBS],
            sporadic_states: [None; crate::config::MAX_SC],
            #[cfg(feature = "alloc")]
            sporadic_atomic_states: [const { None }; crate::config::MAX_SC],
            major_frame_us: 0,
            #[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
            os_priority_workers: std::collections::HashMap::new(),
            #[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
            os_priority_apply_policy: None,
            trigger: Trigger::Any,
            semantics: ExecutorSemantics::RclcppExecutor,
            node_name: heapless::String::new(),
            active_groups: None,
            nodes: heapless::Vec::new(),
            dispatch_slots: heapless::Vec::new(),
            extra_sessions: heapless::Vec::new(),
            primary_rmw_name: heapless::String::new(),
            primary_locator: heapless::String::new(),
            namespace: {
                let mut ns = heapless::String::new();
                let _ = ns.push_str("/");
                ns
            },
            #[cfg(feature = "std")]
            halt_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "std")]
            wake_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "std")]
            wake_cv: std::sync::Arc::new(std::sync::Condvar::new()),
            #[cfg(feature = "std")]
            wake_mu: std::sync::Arc::new(std::sync::Mutex::new(())),
            #[cfg(all(feature = "std", feature = "rmw-cffi"))]
            node_wake: super::node_wake::NodeWake::new().map(std::sync::Arc::new),
            #[cfg(all(feature = "std", feature = "rmw-cffi"))]
            wake_ctx: None,
            #[cfg(all(feature = "std", feature = "rmw-cffi"))]
            has_async_wake: false,
            // Phase 141.A.3 — alloc-mode wake state init. Constructed
            // eagerly (NodeWake allocation) so the runtime cb can be
            // installed lazily on first session without a fallible
            // alloc inside spin_once. `None` when the platform
            // provider reports the primitive unavailable (matches
            // the std-RTOS path's `node_wake: Option<...>`).
            #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
            wake_flag_alloc: portable_atomic_util::Arc::new(portable_atomic::AtomicBool::new(
                false,
            )),
            #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
            node_wake_alloc: super::node_wake::NodeWake::new().map(portable_atomic_util::Arc::new),
            #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
            wake_ctx_alloc: None,
            #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
            has_async_wake_alloc: false,
            #[cfg(all(feature = "signal-fd-wake", target_os = "linux"))]
            signal_fd: None,
            #[cfg(feature = "param-services")]
            params: None,
            #[cfg(feature = "lifecycle-services")]
            lifecycle: None,
            #[cfg(feature = "std")]
            spin_residual_us: 0,
            #[cfg(not(feature = "std"))]
            spin_residual_us: 0,
            // Initialise the spin endpoint to construction time so the
            // very first `spin_once` credits time the caller spent
            // *before* it (e.g. setup, an explicit pre-spin sleep) just
            // like time spent between later calls.
            #[cfg(feature = "std")]
            last_spin_end: Some(std::time::Instant::now()),
            #[cfg(not(feature = "std"))]
            last_spin_end_us: None,
            #[cfg(not(feature = "std"))]
            clock_us_fn: None,
        }
    }

    /// Create an executor from a borrowed session pointer.
    ///
    /// # Safety
    /// - `session_ptr` must point to a valid, initialized session that lives at
    ///   least as long as this executor.
    /// - The caller must not move or drop the session while the executor exists.
    // See `from_session` for the lint rationale.
    #[allow(clippy::large_stack_arrays)]
    pub unsafe fn from_session_ptr(session_ptr: *mut session::ConcreteSession) -> Self {
        Self {
            session: SessionStore::Borrowed(session_ptr),
            arena: [MaybeUninit::uninit(); crate::config::ARENA_SIZE],
            arena_used: 0,
            entries: [None; crate::config::MAX_CBS],
            sched_contexts: {
                let mut s = [None; crate::config::MAX_SC];
                s[0] = Some(super::sched_context::SchedContext::default());
                s
            },
            sched_context_bindings: [super::sched_context::SchedContextId(0);
                crate::config::MAX_CBS],
            sporadic_states: [None; crate::config::MAX_SC],
            #[cfg(feature = "alloc")]
            sporadic_atomic_states: [const { None }; crate::config::MAX_SC],
            major_frame_us: 0,
            #[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
            os_priority_workers: std::collections::HashMap::new(),
            #[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
            os_priority_apply_policy: None,
            trigger: Trigger::Any,
            semantics: ExecutorSemantics::RclcppExecutor,
            node_name: heapless::String::new(),
            active_groups: None,
            nodes: heapless::Vec::new(),
            dispatch_slots: heapless::Vec::new(),
            extra_sessions: heapless::Vec::new(),
            primary_rmw_name: heapless::String::new(),
            primary_locator: heapless::String::new(),
            namespace: {
                let mut ns = heapless::String::new();
                let _ = ns.push_str("/");
                ns
            },
            #[cfg(feature = "std")]
            halt_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "std")]
            wake_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "std")]
            wake_cv: std::sync::Arc::new(std::sync::Condvar::new()),
            #[cfg(feature = "std")]
            wake_mu: std::sync::Arc::new(std::sync::Mutex::new(())),
            #[cfg(all(feature = "std", feature = "rmw-cffi"))]
            node_wake: super::node_wake::NodeWake::new().map(std::sync::Arc::new),
            #[cfg(all(feature = "std", feature = "rmw-cffi"))]
            wake_ctx: None,
            #[cfg(all(feature = "std", feature = "rmw-cffi"))]
            has_async_wake: false,
            // Phase 141.A.3 — alloc-mode wake state init. Constructed
            // eagerly (NodeWake allocation) so the runtime cb can be
            // installed lazily on first session without a fallible
            // alloc inside spin_once. `None` when the platform
            // provider reports the primitive unavailable (matches
            // the std-RTOS path's `node_wake: Option<...>`).
            #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
            wake_flag_alloc: portable_atomic_util::Arc::new(portable_atomic::AtomicBool::new(
                false,
            )),
            #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
            node_wake_alloc: super::node_wake::NodeWake::new().map(portable_atomic_util::Arc::new),
            #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
            wake_ctx_alloc: None,
            #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
            has_async_wake_alloc: false,
            #[cfg(all(feature = "signal-fd-wake", target_os = "linux"))]
            signal_fd: None,
            #[cfg(feature = "param-services")]
            params: None,
            #[cfg(feature = "lifecycle-services")]
            lifecycle: None,
            #[cfg(feature = "std")]
            spin_residual_us: 0,
            #[cfg(not(feature = "std"))]
            spin_residual_us: 0,
            // Initialise the spin endpoint to construction time so the
            // very first `spin_once` credits time the caller spent
            // *before* it (e.g. setup, an explicit pre-spin sleep) just
            // like time spent between later calls.
            #[cfg(feature = "std")]
            last_spin_end: Some(std::time::Instant::now()),
            #[cfg(not(feature = "std"))]
            last_spin_end_us: None,
            #[cfg(not(feature = "std"))]
            clock_us_fn: None,
        }
    }

    /// Phase 228.B (RFC-0015) — construct a tier task's executor that **shares**
    /// a session opened once by the orchestration `main()`.
    ///
    /// In the per-tier execution model `main()` opens one RMW session, then
    /// spawns one RTOS task per priority tier; each task calls this to get an
    /// [`Executor`] over the *same* session (the `Borrowed` session store — this
    /// executor neither owns nor closes it), registers its tier's callback
    /// groups, and spins. Thin alias over [`Executor::from_session_ptr`].
    ///
    /// # Safety
    /// `session` must outlive every executor/task built from it (the
    /// orchestration `main()` holds it and never returns / WFIs), and must not
    /// be mutated except through these executors' spin calls.
    pub unsafe fn open_with_session(session: *mut session::ConcreteSession) -> Self {
        unsafe { Self::from_session_ptr(session) }
    }

    /// Raw pointer to this executor's RMW session, for the per-tier model:
    /// the boot task opens the one session via [`Executor::open`] (the RMW
    /// session is a process-wide singleton — opening twice fails), then hands
    /// this pointer to each spawned tier task's
    /// [`Executor::open_with_session`]. The boot task's executor owns the
    /// session and outlives every borrower, so the pointer stays valid for the
    /// program's life. Works for both `Owned` and `Borrowed` stores.
    ///
    /// # Safety
    /// The returned pointer aliases `self.session`. Callers must keep `self`
    /// alive (not moved/dropped) for as long as any tier executor uses the
    /// pointer, and must only touch the session through executor spin calls
    /// (the RMW backend serializes concurrent access through its own locks).
    pub fn session_ptr(&mut self) -> *mut session::ConcreteSession {
        &mut *self.session as *mut session::ConcreteSession
    }

    /// Opaque, `Send` form of [`session_ptr`](Self::session_ptr) — the per-tier
    /// model hands this to each spawned tier task (it can cross the RTOS task /
    /// thread boundary, which a bare `*mut` cannot). See [`SessionHandle`].
    ///
    /// # Safety
    /// Same contract as [`session_ptr`](Self::session_ptr): `self` (the session
    /// owner) must outlive every executor built from the handle.
    pub fn session_handle(&mut self) -> SessionHandle {
        SessionHandle(self.session_ptr())
    }

    /// Open an [`Executor`] over the session a [`SessionHandle`] refers to (the
    /// `Borrowed` store — neither owns nor closes it). The tier-task counterpart
    /// to [`session_handle`](Self::session_handle).
    ///
    /// # Safety
    /// The handle's session must still be alive (its owning executor not moved
    /// or dropped); access only through executor spin calls.
    pub unsafe fn open_with_session_handle(handle: SessionHandle) -> Self {
        unsafe { Self::open_with_session(handle.0) }
    }

    /// Phase 228.C — set this tier executor's active callback-group filter. The
    /// generated per-tier task calls this before registering nodes; afterwards
    /// only callbacks whose `.callback_group()` is in `groups` register here.
    /// An empty slice (or never calling it) leaves the wildcard — register all
    /// callbacks (the single-tier degenerate case + today's behaviour).
    pub fn set_active_groups(&mut self, groups: &[&str]) {
        if groups.is_empty() {
            self.active_groups = None;
            return;
        }
        let mut v = heapless::Vec::new();
        for g in groups {
            let mut s = heapless::String::new();
            if s.push_str(g).is_ok() {
                let _ = v.push(s);
            }
        }
        self.active_groups = Some(v);
    }

    /// Phase 228.C — whether a callback in `group` should register in this
    /// executor under the current filter. Wildcard (`None`) accepts everything.
    pub fn group_active(&self, group: &str) -> bool {
        group_filter_accepts(&self.active_groups, group)
    }

    /// Set the node name and namespace used for liveliness tokens.
    ///
    /// Called by `open()` to propagate config values. When `register_subscription`
    /// or `register_service` creates entities, these values are attached to the
    /// Phase 156 — record the primary session's backend identity
    /// (rmw name + locator) so `NodeBuilder::resolve_session_slot`
    /// can detect when a `.rmw(name)` matches the primary instead
    /// of opening a SECOND backend session against the same
    /// singleton (zenoh-pico's `g_session` is process-wide;
    /// opening twice fails). `Executor::open*` calls this
    /// automatically; the C surface (`nros_executor_init`) calls
    /// it manually because it constructs via `from_session_ptr`
    /// which doesn't know the open metadata. Empty strings = "no
    /// primary identity tracked"; the cache check degrades to
    /// always-miss.
    pub fn set_primary_identity(&mut self, rmw_name: &str, locator: &str) {
        self.primary_rmw_name.clear();
        let _ = self.primary_rmw_name.push_str(rmw_name);
        self.primary_locator.clear();
        let _ = self.primary_locator.push_str(locator);
    }

    /// `TopicInfo`/`ServiceInfo` so the zenoh backend can declare liveliness.
    pub fn set_node_identity(&mut self, node_name: &str, namespace: &str) {
        self.node_name.clear();
        let _ = self.node_name.push_str(node_name);
        if !namespace.is_empty() {
            self.namespace.clear();
            let _ = self.namespace.push_str(namespace);
        }
    }

    // =========================================================================
    // Phase 110.B — SchedContext API
    // =========================================================================

    /// Identifier of the auto-created default `Fifo`-class scheduling
    /// context. Every callback registered without an explicit
    /// [`bind_handle_to_sched_context`] binds to this SC.
    pub fn default_sched_context_id(&self) -> super::sched_context::SchedContextId {
        super::sched_context::SchedContextId(0)
    }

    /// Register a new scheduling context. Returns a [`SchedContextId`]
    /// callers pass to [`bind_handle_to_sched_context`] to attach
    /// callbacks. Phase 110.B.
    pub fn create_sched_context(
        &mut self,
        sc: super::sched_context::SchedContext,
    ) -> Result<super::sched_context::SchedContextId, NodeError> {
        // Slot 0 is reserved for the default Fifo SC; search 1..MAX_SC.
        for (i, slot) in self.sched_contexts.iter_mut().enumerate().skip(1) {
            if slot.is_none() {
                *slot = Some(sc);
                // Phase 110.E — Sporadic-class SCs get a sibling
                // `SporadicState` entry that the spin_once dispatch
                // path consults each cycle to refill the budget at
                // period boundaries and skip dispatch when budget
                // is exhausted.
                if matches!(sc.class, super::sched_context::SchedClass::Sporadic) {
                    let budget = sc.budget_us.get().map(|nz| nz.get()).unwrap_or(u32::MAX);
                    let period = sc.period_us.get().map(|nz| nz.get()).unwrap_or(u32::MAX);
                    self.sporadic_states[i] =
                        Some(super::sched_context::SporadicState::new(budget, period));
                }
                return Ok(super::sched_context::SchedContextId(i as u8));
            }
        }
        Err(NodeError::NoSchedContextSlot)
    }

    /// Bind a registered callback to a scheduling context. The next
    /// `spin_once` cycle dispatches the callback through that SC's
    /// queue (FIFO bitmap or EDF heap). Phase 110.B.
    pub fn bind_handle_to_sched_context(
        &mut self,
        handle: HandleId,
        sc_id: super::sched_context::SchedContextId,
    ) -> Result<(), NodeError> {
        let i = handle.0;
        if i >= crate::config::MAX_CBS {
            return Err(NodeError::InvalidSchedContextBinding);
        }
        if self.entries[i].is_none() {
            return Err(NodeError::InvalidSchedContextBinding);
        }
        let sc_idx = sc_id.0 as usize;
        if sc_idx >= crate::config::MAX_SC || self.sched_contexts[sc_idx].is_none() {
            return Err(NodeError::InvalidSchedContextBinding);
        }
        self.sched_context_bindings[i] = sc_id;
        Ok(())
    }

    /// Phase 110.F — opt in to per-callback OS-priority dispatch.
    /// Once registered, every `spin_once` cycle routes ready entries
    /// whose bound SC has `os_pri > 0` onto a worker thread the OS
    /// scheduler has elevated to that numeric priority. Workers are
    /// spawned lazily on first use and self-halt when the Executor
    /// drops.
    ///
    /// `apply_policy` is the same `fn(SchedPolicy) -> Result<(),
    /// SchedError>` shape `open_threaded` takes — keeps the
    /// Executor non-generic over Platform.
    ///
    /// Calling this with `apply_policy = noop` is fine for testing
    /// (workers spawn but don't actually elevate priority); real
    /// hard-RT use needs `CAP_SYS_NICE` on Linux or the equivalent
    /// kernel config on RTOSes.
    #[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
    pub fn register_os_priority_dispatcher(
        &mut self,
        apply_policy: fn(
            nros_platform_api::SchedPolicy,
        ) -> Result<(), nros_platform_api::SchedError>,
    ) {
        self.os_priority_apply_policy = Some(apply_policy);
    }

    /// Phase 110.G — enable time-triggered dispatch by setting the
    /// executor's major-frame length. Once set, every `spin_once`
    /// cycle gates dispatch through each entry's bound SC's
    /// `tt_window_offset_us` / `tt_window_duration_us` fields:
    /// dispatch only fires when the current monotonic time falls
    /// inside the window `[off, off + duration) mod major_frame`.
    ///
    /// `major_frame_us = 0` disables the TT gate (default state).
    /// Setting a non-zero major frame after callbacks are already
    /// registered is allowed — TT gates take effect on the next
    /// `spin_once` cycle.
    pub fn register_time_triggered_dispatcher(&mut self, major_frame_us: u32) {
        self.major_frame_us = major_frame_us;
    }

    /// Phase 110.G — apply a declarative cyclic schedule.
    ///
    /// One-shot helper that wraps the underlying primitives:
    /// validates the schedule (`major_frame > 0`, no overlapping
    /// windows, every window fits inside the major frame), sets the
    /// executor's major-frame length, then materialises one
    /// `SchedContext` per window with `class = TimeTriggered` +
    /// the window's offset / duration. Returns the per-window
    /// [`SchedContextId`] array so callers can immediately
    /// `bind_handle_to_sched_context(handle, sc_id)` for their
    /// subscription / timer handles.
    ///
    /// `N` is the schedule's *declared* maximum window count;
    /// `schedule.window_count` gates how many SCs are actually
    /// created. Unused trailing slots return
    /// `SchedContextId::default()` (sentinel — callers must respect
    /// `window_count`).
    pub fn apply_time_triggered_schedule<const N: usize>(
        &mut self,
        schedule: &super::sched_context::TimeTriggeredSchedule<N>,
    ) -> Result<
        [super::sched_context::SchedContextId; N],
        super::sched_context::TimeTriggeredScheduleError,
    > {
        schedule.validate()?;
        self.major_frame_us = schedule.major_frame_us;
        // SC slot 0 is the auto-created default; reusing it as a
        // sentinel for unused trailing slots is safe because the
        // caller respects `schedule.window_count`.
        let mut ids: [super::sched_context::SchedContextId; N] =
            [super::sched_context::SchedContextId(0); N];
        for (i, window) in schedule.windows[..schedule.window_count].iter().enumerate() {
            // Deprecation note on `SchedClass::TimeTriggered`: TT
            // is implemented as a per-SC *window gate* on top of
            // the existing class-based dispatch (Fifo here keeps
            // the EDF / Sporadic budgets out of the picture for
            // pure cyclic schedules). The window-gate fields set
            // below are what `spin_once`'s 110.G runtime gate
            // actually reads.
            let sc = super::sched_context::SchedContext {
                tt_window_offset_us: super::sched_context::OptUs::from_us(window.offset_us),
                tt_window_duration_us: super::sched_context::OptUs::from_us(window.duration_us),
                ..super::sched_context::SchedContext::new_fifo()
            };
            ids[i] = self.create_sched_context(sc).map_err(|_| {
                super::sched_context::TimeTriggeredScheduleError::WindowCountOverflow
            })?;
        }
        Ok(ids)
    }

    /// Phase 110.E.b — register an ISR-driven refill timer for an
    /// already-created Sporadic SC. The caller invokes their
    /// platform's `PlatformTimer::create_periodic` with the returned
    /// `Arc<AtomicSporadicState>` as `user_data` and the
    /// `atomic_sporadic_refill_thunk` as the callback, then hands
    /// the resulting platform handle to this method via
    /// `OpaqueTimerHandle::new(handle, destroy_fn)`.
    ///
    /// The Executor stores both the Arc and the handle so Drop can
    /// clean them up. Calling this on a non-Sporadic SC returns
    /// `Err(InvalidSchedContextBinding)`.
    #[cfg(feature = "alloc")]
    pub fn register_sporadic_timer(
        &mut self,
        sc_id: super::sched_context::SchedContextId,
        timer: OpaqueTimerHandle,
    ) -> Result<portable_atomic_util::Arc<super::sched_context::AtomicSporadicState>, NodeError>
    {
        let i = sc_id.0 as usize;
        if i >= crate::config::MAX_SC {
            return Err(NodeError::InvalidSchedContextBinding);
        }
        let sc = self.sched_contexts[i]
            .as_ref()
            .ok_or(NodeError::InvalidSchedContextBinding)?;
        if !matches!(sc.class, super::sched_context::SchedClass::Sporadic) {
            return Err(NodeError::InvalidSchedContextBinding);
        }
        let budget = sc.budget_us.get().map(|nz| nz.get()).unwrap_or(u32::MAX);
        let period = sc.period_us.get().map(|nz| nz.get()).unwrap_or(u32::MAX);
        let state = portable_atomic_util::Arc::new(super::sched_context::AtomicSporadicState::new(
            budget, period,
        ));
        self.sporadic_atomic_states[i] = Some((portable_atomic_util::Arc::clone(&state), timer));
        Ok(state)
    }

    /// Inspect a registered scheduling context. Phase 110.B.
    pub fn sched_context(
        &self,
        sc_id: super::sched_context::SchedContextId,
    ) -> Option<&super::sched_context::SchedContext> {
        self.sched_contexts.get(sc_id.0 as usize)?.as_ref()
    }

    /// Phase 104.C.2 — start a rclcpp-style Node builder for this
    /// Executor. The returned [`NodeBuilder`](super::node_record::NodeBuilder)
    /// is chainable:
    ///
    /// ```ignore
    /// let id = exec.node_builder("ingress")
    ///     .rmw("zenoh")
    ///     .locator("tcp/127.0.0.1:7447")
    ///     .sched(my_sc_id)
    ///     .build()?;
    /// ```
    ///
    /// In Phase 104.C.2 the Node table is storage-only — all
    /// registered Nodes share the Executor's primary session. Per-
    /// Node session binding (the bridge feature) lands in Phase
    /// 104.C.3 when the session cache is wired.
    pub fn node_builder<'a, 'cfg>(
        &'a mut self,
        name: &'cfg str,
    ) -> super::node_record::NodeBuilder<'a, 'cfg> {
        super::node_record::NodeBuilder {
            executor: self,
            name,
            namespace: None,
            rmw_name: None,
            locator: None,
            domain_id: None,
            sched: None,
            session_idx: None,
        }
    }

    /// Return the Node table — Phase 104.C.2 read accessor.
    pub fn nodes(&self) -> &[super::node_record::NodeRecord] {
        &self.nodes
    }

    /// Borrow a Node's metadata by id, returning `None` if the id
    /// is out of range.
    pub fn node(&self, id: super::node_record::NodeId) -> Option<&super::node_record::NodeRecord> {
        self.nodes.get(id.index())
    }

    /// Phase 189.M1 — an executor-borrowing node handle for the entity builders
    /// (`exec.node_mut(id).subscription(t)...` / `.create_subscription(...)`).
    /// A short-lived `&mut Executor` borrow — use one at a time; entity handles
    /// are owned and outlive it (see `NodeCtx`).
    pub fn node_mut(&mut self, id: super::node_record::NodeId) -> super::node::NodeCtx<'_> {
        super::node::NodeCtx::new(self, id)
    }

    /// Phase 104.C.3 — resolve a session-slot index to a mutable
    /// session reference. Slot 0 = the Executor's primary session;
    /// slots 1..=N = the `extra_sessions` vec opened by
    /// `node_builder.rmw(name)` calls that named a backend
    /// different from the primary.
    pub(crate) fn session_at_mut(&mut self, idx: u8) -> Option<&mut session::ConcreteSession> {
        if idx == 0 {
            Some(&mut *self.session)
        } else {
            self.extra_sessions.get_mut((idx - 1) as usize)
        }
    }

    /// Phase 104.C.9.b — resolve the per-Node session for direct
    /// entity creation paths (C++ FFI publisher / subscription /
    /// service that bypass the `register_*_on` arena dispatch).
    /// Returns `None` when `node_id` is out of range or the Node's
    /// `session_idx` lands outside the executor's session table.
    pub fn node_session_mut(
        &mut self,
        node_id: super::node_record::NodeId,
    ) -> Option<&mut session::ConcreteSession> {
        let session_idx = self.nodes.get(node_id.index())?.session_idx;
        self.session_at_mut(session_idx)
    }

    /// Phase 189.M1 — create a typed publisher bound to a node's session.
    /// Backs `node.publisher(t).typed::<M>().build()` on the
    /// executor-borrowing [`NodeCtx`](super::node::NodeCtx); the returned
    /// handle is owned and outlives the `NodeCtx`.
    pub fn create_publisher_on<M: crate::cyclonedds_register::MessageForRmw>(
        &mut self,
        node_id: super::node_record::NodeId,
        topic_name: &str,
        qos: QosSettings,
    ) -> Result<crate::executor::handles::EmbeddedPublisher<M>, NodeError> {
        // Phase 212.K.7.6.b — register `M`'s cyclonedds descriptor before
        // creating the underlying publisher handle. No-op for other RMWs.
        crate::cyclonedds_register::register_type::<M>()?;
        let handle = self.create_raw_publisher_handle_on(
            node_id,
            topic_name,
            <M as RosMessage>::TYPE_NAME,
            <M as RosMessage>::TYPE_HASH,
            qos,
        )?;
        Ok(crate::executor::handles::EmbeddedPublisher {
            handle,
            event_regs: crate::executor::handles::empty_event_regs(),
            _phantom: PhantomData,
        })
    }

    /// Phase 189.M1 — create a generic (type-erased) publisher bound to a
    /// node's session. Backs `node.publisher(t).generic(ty, hash).build()`;
    /// the bridge re-publishes through this handle on the dest session.
    pub fn create_publisher_raw_on(
        &mut self,
        node_id: super::node_record::NodeId,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
    ) -> Result<crate::executor::handles::EmbeddedRawPublisher, NodeError> {
        let handle =
            self.create_raw_publisher_handle_on(node_id, topic_name, type_name, type_hash, qos)?;
        Ok(crate::executor::handles::EmbeddedRawPublisher {
            handle,
            arena: crate::executor::handles::TxArena::new(),
            event_regs: crate::executor::handles::empty_event_regs(),
        })
    }

    /// Shared prelude for the publisher-on-node paths: resolve the node's
    /// identity + session slot, build the [`TopicInfo`], validate QoS, and
    /// create the backend publisher handle. Mirrors
    /// `register_subscription_buffered_raw_on`'s session resolution so a
    /// bridge's source sub + dest pub agree on topic construction.
    fn create_raw_publisher_handle_on(
        &mut self,
        node_id: super::node_record::NodeId,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
    ) -> Result<session::RmwPublisher, NodeError> {
        let (node_name, ns, session_idx) = {
            let r = self
                .nodes
                .get(node_id.index())
                .ok_or(NodeError::InvalidSchedContextBinding)?;
            (r.name.clone(), r.namespace.clone(), r.session_idx)
        };
        let mut topic = TopicInfo::new(topic_name, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let session = self
            .session_at_mut(session_idx)
            .ok_or(NodeError::BackendMismatch)?;
        qos.validate_against(Session::supported_qos_policies(session))
            .map_err(NodeError::Transport)?;
        session
            .create_publisher(&topic, qos)
            .map_err(|_| NodeError::Transport(TransportError::PublisherCreationFailed))
    }

    /// Phase 124.B.1 — install the executor's wake callback onto the
    /// primary session. Best-effort: backends that don't override
    /// `Session::set_wake_callback` (poll-only XRCE, bare-metal)
    /// ignore the call and continue to be drained on the executor's
    /// deadline-bound cv-wait boundary.
    #[cfg(all(feature = "std", feature = "rmw-cffi"))]
    fn install_wake_signal_on_primary(&mut self) {
        use nros_rmw::Session as _;
        let ctx = self.wake_ctx_ptr();
        // SAFETY: `ctx` points at executor-owned wake state that outlives
        // the session callback installation and is cleared on executor drop.
        unsafe {
            self.session
                .set_wake_callback(Some(nros_rmw_runtime_wake_cb), ctx);
        }
        if self.session.supports_wake_callback() {
            self.has_async_wake = true;
        }
    }

    /// Phase 124.B.1 — install the wake callback onto an extra
    /// session opened by `node_builder.rmw(...)`. Called from
    /// `NodeBuilder::build()` right after `extra_sessions.push(...)`.
    #[cfg(all(feature = "std", feature = "rmw-cffi"))]
    pub(crate) fn install_wake_signal_on_extra(&mut self, idx: usize) {
        use nros_rmw::Session as _;
        let ctx = self.wake_ctx_ptr();
        if let Some(s) = self.extra_sessions.get_mut(idx) {
            // SAFETY: same executor-owned wake state as the primary session;
            // the extra session is owned by this executor.
            unsafe {
                s.set_wake_callback(Some(nros_rmw_runtime_wake_cb), ctx);
            }
            if s.supports_wake_callback() {
                self.has_async_wake = true;
            }
        }
    }

    /// Phase 124.B.2 — opaque context pointer the runtime wake
    /// callback receives. Encodes `(flag, mu, cv)` as a borrowed
    /// `&WakeCtx` reference; the callback decodes via
    /// `*const WakeCtx`.
    ///
    /// Lifetime: tied to the Executor instance. WakeCtx storage
    /// lives inside Executor (lazy-allocated on first install), so
    /// the pointer stays valid as long as the Executor is.
    /// Phase 124.B.7.c — POSIX signal-handler-safe wake fd.
    ///
    /// Returns a Linux `eventfd` that callers (typically POSIX
    /// signal handlers) can `write(fd, &1u64, 8)` to from any
    /// context, including signal handlers. A runtime-owned worker
    /// thread reads the fd and signals `wake_cv`, unblocking
    /// `spin_once`.
    ///
    /// The worker thread is spawned lazily on first call and
    /// joined on Executor drop. Linux-only and gated behind
    /// `feature = "signal-fd-wake"`; binaries that don't install
    /// signal handlers shouldn't enable it.
    ///
    /// Returns the raw fd. The Executor retains ownership; do not
    /// `close()` it from the caller.
    #[cfg(all(feature = "signal-fd-wake", feature = "rmw-cffi", target_os = "linux"))]
    pub fn signal_fd(&mut self) -> std::io::Result<core::ffi::c_int> {
        let ctx_ptr = self.wake_ctx_ptr() as *const WakeCtx;
        if self.signal_fd.is_none() {
            self.signal_fd = Some(WakeSignalFd::new(ctx_ptr)?);
        }
        Ok(self.signal_fd.as_ref().expect("just set").fd())
    }

    #[cfg(all(feature = "std", feature = "rmw-cffi"))]
    fn wake_ctx_ptr(&mut self) -> *mut core::ffi::c_void {
        if self.wake_ctx.is_none() {
            self.wake_ctx = Some(std::sync::Arc::new(WakeCtx {
                flag: std::sync::Arc::clone(&self.wake_flag),
                cv: std::sync::Arc::clone(&self.wake_cv),
                mu: std::sync::Arc::clone(&self.wake_mu),
                node_wake: self.node_wake.as_ref().map(std::sync::Arc::clone),
            }));
        }
        let arc = self.wake_ctx.as_ref().expect("just set");
        std::sync::Arc::as_ptr(arc) as *mut core::ffi::c_void
    }

    // Phase 141.A.3 — alloc-mode (no_std RTOS) mirror of
    // `install_wake_signal_on_primary` /
    // `install_wake_signal_on_extra` / `wake_ctx_ptr`. Same
    // best-effort install contract: backends that don't override
    // `Session::set_wake_callback` ignore the call and continue
    // to be drained on the executor's deadline-bound wait.
    #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
    fn wake_ctx_alloc_ptr(&mut self) -> Option<*mut core::ffi::c_void> {
        // Without a NodeWake there's no kernel primitive to signal;
        // skip the install + let spin_once fall back to drive_io
        // for the full timeout. Mirrors the std-RTOS path's
        // `if let Some(wake) = self.node_wake.as_ref()` predicate.
        let node_wake = self.node_wake_alloc.as_ref()?;
        if self.wake_ctx_alloc.is_none() {
            self.wake_ctx_alloc = Some(portable_atomic_util::Arc::new(
                super::wake_alloc::WakeCtxAlloc {
                    flag: portable_atomic_util::Arc::clone(&self.wake_flag_alloc),
                    node_wake: portable_atomic_util::Arc::clone(node_wake),
                },
            ));
        }
        let arc = self.wake_ctx_alloc.as_ref().expect("just set");
        Some(portable_atomic_util::Arc::as_ptr(arc) as *mut core::ffi::c_void)
    }

    #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
    fn install_wake_signal_on_primary_alloc(&mut self) {
        use nros_rmw::Session as _;
        let Some(ctx) = self.wake_ctx_alloc_ptr() else {
            return;
        };
        // SAFETY: `ctx` is the raw pointer of an Arc<WakeCtxAlloc>
        // owned by the Executor (`self.wake_ctx_alloc`); the Arc
        // lives as long as the Executor and is cleared on drop.
        unsafe {
            self.session
                .set_wake_callback(Some(super::wake_alloc::nros_rmw_runtime_wake_cb), ctx);
        }
        if self.session.supports_wake_callback() {
            self.has_async_wake_alloc = true;
        }
    }

    #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
    pub(crate) fn install_wake_signal_on_extra_alloc(&mut self, idx: usize) {
        use nros_rmw::Session as _;
        let Some(ctx) = self.wake_ctx_alloc_ptr() else {
            return;
        };
        if let Some(s) = self.extra_sessions.get_mut(idx) {
            // SAFETY: same wake state as the primary session; the
            // extra session is owned by this Executor.
            unsafe {
                s.set_wake_callback(Some(super::wake_alloc::nros_rmw_runtime_wake_cb), ctx);
            }
            if s.supports_wake_callback() {
                self.has_async_wake_alloc = true;
            }
        }
    }

    /// Phase 104.C.4 — apply a Node's default SchedContext to a
    /// freshly-registered handle. Called from every `_inner`
    /// register variant after the entry slot is committed. No-op
    /// when `node_id` is None (legacy path), when the Node is
    /// out of range, or when the Node's `default_sched` is the
    /// auto-created Fifo slot (0) which matches the executor's
    /// default binding already.
    ///
    /// Handles can still override per-call via
    /// `bind_handle_to_sched_context(handle, sc_id)` post-register.
    pub(crate) fn apply_node_default_sched(
        &mut self,
        slot: usize,
        node_id: Option<super::node_record::NodeId>,
    ) {
        let Some(id) = node_id else { return };
        let Some(rec) = self.nodes.get(id.index()) else {
            return;
        };
        let sc = rec.default_sched;
        if sc.0 == 0 {
            return;
        }
        if slot >= crate::config::MAX_CBS {
            return;
        }
        let sc_idx = sc.0 as usize;
        if sc_idx >= crate::config::MAX_SC || self.sched_contexts[sc_idx].is_none() {
            return;
        }
        self.sched_context_bindings[slot] = sc;
    }

    /// Phase 104.C.3.2 — scoped Node-handle access. The closure
    /// receives a [`Node`] bound to the requested [`NodeId`]'s
    /// session + identity. Use the standard `Node::create_publisher`,
    /// `create_subscription`, etc. APIs inside.
    ///
    /// rclcpp-aligned bridge pattern:
    ///
    /// ```ignore
    /// let node_in = exec.node_builder("ingress").rmw("zenoh").build()?;
    /// let node_out = exec.node_builder("egress").rmw("xrce").build()?;
    ///
    /// let pub_out = exec.with_node(node_out, |n| {
    ///     n.create_publisher::<Int32>("/fwd")
    /// })??;
    ///
    /// exec.with_node(node_in, |n| {
    ///     n.create_subscription_buffered::<Int32, _, 1024>(
    ///         "/src", qos(), move |m| { let _ = pub_out.publish(m); }
    ///     )
    /// })??;
    /// ```
    ///
    /// The closure can return any type; double-`?` unwraps the
    /// outer `Result<R, NodeError>` from `with_node` and the inner
    /// result returned by the closure.
    /// Phase 104.C.3.3.d — flat-Result variant of
    /// [`with_node`](Self::with_node). When the closure already
    /// returns `Result<R, NodeError>`, this avoids the double-`?`:
    ///
    /// ```ignore
    /// // Without `with_node_try`:
    /// let pub_ = exec.with_node(id, |n| n.create_publisher(...))??;
    ///
    /// // With `with_node_try`:
    /// let pub_ = exec.with_node_try(id, |n| n.create_publisher(...))?;
    /// ```
    pub fn with_node_try<R>(
        &mut self,
        id: super::node_record::NodeId,
        f: impl FnOnce(&mut NodeHandle<'_>) -> Result<R, NodeError>,
    ) -> Result<R, NodeError> {
        self.with_node(id, f)?
    }

    pub fn with_node<R>(
        &mut self,
        id: super::node_record::NodeId,
        f: impl FnOnce(&mut NodeHandle<'_>) -> R,
    ) -> Result<R, NodeError> {
        let (name, ns, session_idx) = {
            let r = self
                .nodes
                .get(id.index())
                .ok_or(NodeError::InvalidSchedContextBinding)?;
            (r.name.clone(), r.namespace.clone(), r.session_idx)
        };
        let session = self
            .session_at_mut(session_idx)
            .ok_or(NodeError::BackendMismatch)?;
        // SAFETY: short-lived scoped reference. `Node::new` takes
        // `&mut ConcreteSession`; lifetime is bound to this fn's
        // body via the closure's borrow of `node`.
        let mut node = NodeHandle::new(name, ns, session, 0);
        Ok(f(&mut node))
    }

    /// Find a registered executor node by final name and namespace.
    pub fn node_id_by_name(
        &self,
        name: &str,
        namespace: &str,
    ) -> Option<super::node_record::NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .find(|(_, node)| node.name.as_str() == name && node.namespace.as_str() == namespace)
            .map(|(index, _)| super::node_record::NodeId::from_raw(index as u8))
    }

    /// Create a node on this executor.
    pub fn create_node(&mut self, name: &str) -> Result<NodeHandle<'_>, NodeError> {
        if name.len() > 64 {
            return Err(NodeError::NameTooLong);
        }

        let mut node_name = heapless::String::<64>::new();
        node_name
            .push_str(name)
            .map_err(|_| NodeError::NameTooLong)?;

        Ok(NodeHandle::new(
            node_name,
            self.namespace.clone(),
            &mut self.session,
            0,
        ))
    }

    /// Phase 128.F.2 — bridge-mode node factory. Registers a Node
    /// bound to the named RMW backend by opening (or reusing) an
    /// extra session via `node_builder().rmw(rmw).build()`, then
    /// returns a [`Node`] borrowing that session. Use when the
    /// binary intentionally links more than one backend and a Node
    /// must speak a specific one.
    ///
    /// The single-backend common case should keep using
    /// [`create_node`](Self::create_node) — this entry costs an
    /// extra session lookup and serves no purpose when only one
    /// backend is registered.
    #[cfg(feature = "rmw-cffi")]
    pub fn create_node_on(&mut self, name: &str, rmw: &str) -> Result<NodeHandle<'_>, NodeError> {
        if name.len() > 64 {
            return Err(NodeError::NameTooLong);
        }
        // Register the Node (opens an extra session under `rmw` if
        // none exists yet for that backend).
        let id = self.node_builder(name).rmw(rmw).build()?;
        let session_idx = self.node(id).ok_or(NodeError::NodeTableFull)?.session_idx;

        let mut node_name = heapless::String::<64>::new();
        node_name
            .push_str(name)
            .map_err(|_| NodeError::NameTooLong)?;
        let namespace = self.namespace.clone();
        let session = self
            .session_at_mut(session_idx)
            .ok_or(NodeError::NodeTableFull)?;
        Ok(NodeHandle::new(node_name, namespace, session, 0))
    }

    /// Drive transport I/O (poll network, dispatch callbacks).
    #[allow(dead_code)]
    pub(crate) fn drive_io(&mut self, timeout_ms: i32) -> Result<(), NodeError> {
        self.session
            .drive_io(timeout_ms)
            .map_err(|_| NodeError::Transport(TransportError::PollFailed))
    }

    /// Close the underlying session.
    pub fn close(&mut self) -> Result<(), NodeError> {
        self.session
            .close()
            .map_err(|_| NodeError::Transport(TransportError::ConnectionFailed))
    }

    /// Phase 216 follow-up — register a per-Node dispatch trampoline.
    ///
    /// The board-side Entry pkg (or the macro-emitted
    /// `register_dispatch(executor)` wrapper, once wired) calls this
    /// once per deployed Node pkg, handing in the
    /// `__nros_node_<pkg>_on_callback` symbol + the Node's per-pkg
    /// `state` blob. [`Executor::dispatch_callback`] then linear-scans
    /// the registered slots when the dispatch task hands off a
    /// `SignaledCallback`.
    ///
    /// Returns `Err(())` when the registry is full (`MAX_NODES`
    /// entries — raise via `NROS_EXECUTOR_MAX_NODES` at build time).
    ///
    /// # Safety
    ///
    /// `state` must outlive the executor (the typical shape is a
    /// `*mut State` produced by
    /// `nros::__private_node_state_into_raw` from the
    /// macro-emitted `i()`; that pointer's lifetime IS the
    /// `Executor`'s by construction). `on_callback` must be safe to
    /// invoke with `(state, cb_id_ptr, cb_id_len, ctx)` matching the
    /// per-Node `__nros_node_<pkg>_on_callback` ABI emitted by the
    /// `nros::node!()` macro (Phase 216.A.5).
    #[allow(clippy::result_unit_err)]
    pub fn register_dispatch_slot(
        &mut self,
        state: *mut core::ffi::c_void,
        on_callback: unsafe extern "C" fn(
            *mut core::ffi::c_void,
            *const u8,
            usize,
            *mut core::ffi::c_void,
        ),
    ) -> Result<(), ()> {
        self.dispatch_slots
            .push(DispatchSlot { state, on_callback })
            .map_err(|_| ())
    }

    /// Phase 216 follow-up — current registered dispatch-slot count.
    /// Diagnostic / test surface.
    pub fn dispatch_slot_count(&self) -> usize {
        self.dispatch_slots.len()
    }

    /// Phase 216 final dispatch hook — stable entry point the
    /// framework's dispatch task (RTIC `__nros_run` /
    /// Embassy `__nros_run_task`) calls for each `SignaledCallback`
    /// envelope it dequeues from the board-side SPSC / Embassy
    /// channel.
    ///
    /// ## Signature shape
    ///
    /// `nros-node` sits below `nros` in the dep graph, so the typed
    /// `nros::CallbackId<'_>` / `nros::CallbackCtx<'_>` types
    /// referenced in the Phase 216 design notes cannot appear in the
    /// signature here. The macro emit translates the dequeued
    /// envelope to the layer-clean `(cb_id: &str, ctx: *mut c_void)`
    /// pair before calling this method; the per-Node `on_callback`
    /// trampoline ABI (Phase 216.A.5,
    /// `__nros_node_<pkg>_on_callback(state, cb_id_ptr, cb_id_len,
    /// ctx)`) uses the same untyped shape on the other side of the
    /// fence, so the round-trip stays type-consistent.
    ///
    /// ## Body — linear scan of the dispatch registry
    ///
    /// Each registered [`DispatchSlot`] holds an
    /// `__nros_node_<pkg>_on_callback` fn pointer + the owning Node's
    /// `state` blob. The macro-emitted trampoline body
    /// `match`es on `CallbackId` tags the Node declared and is a
    /// no-op for non-matching `cb_id`s — at most one Node per
    /// `cb_id` actually acts, the rest are cheap string-compare
    /// no-ops. This mirrors the strategy
    /// `ExecutorNodeRuntime::dispatch_callback` uses in
    /// `packages/core/nros/src/node_runtime.rs:470`.
    ///
    /// ## What's NOT auto-wired today
    ///
    /// The `nros::node!()` macro doesn't yet emit a
    /// `register_dispatch(executor)` wrapper that pushes the per-pkg
    /// `(state, on_callback)` into this registry. Until that wiring
    /// lands (Phase 216 follow-up — see commit msg), downstream
    /// consumers (board's `init_hardware`, or the codegen-emitted
    /// `run_plan`) must call
    /// [`Executor::register_dispatch_slot`] explicitly with the
    /// `__nros_node_<pkg>_on_callback` symbol + a `state` blob from
    /// the macro-emitted `i()`.
    //
    // `ctx` is an opaque FFI cookie forwarded verbatim to each slot's
    // `on_callback`; this fn never dereferences it (the registered callback
    // does, under the `register_dispatch_slot` safety contract), so it is sound
    // to call from safe code.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn dispatch_callback(&mut self, cb_id: &str, ctx: *mut core::ffi::c_void) {
        let cb_id_ptr = cb_id.as_ptr();
        let cb_id_len = cb_id.len();
        // Snapshot pointer + length to avoid an outstanding borrow
        // across the unsafe fn calls below; each `DispatchSlot` is
        // `Copy`, so iterating by value sidesteps any aliasing
        // worry the borrow checker would flag if a slot's
        // `on_callback` re-entered the executor.
        for slot in self.dispatch_slots.iter().copied() {
            // SAFETY: caller of `register_dispatch_slot` guaranteed
            // `state` outlives the executor + `on_callback` matches
            // the per-Node `__nros_node_<pkg>_on_callback` ABI;
            // `cb_id_ptr`/`cb_id_len` describe the live `&str` the
            // caller passed in.
            unsafe {
                (slot.on_callback)(slot.state, cb_id_ptr, cb_id_len, ctx);
            }
        }
    }

    /// Get a reference to the underlying session.
    pub fn session(&self) -> &session::ConcreteSession {
        &self.session
    }

    /// Get a mutable reference to the underlying session.
    pub fn session_mut(&mut self) -> &mut session::ConcreteSession {
        &mut self.session
    }

    /// Phase 124.F.3 — session-level connectivity probe. Wire-level
    /// round-trip "is the peer / agent / router still reachable?"
    /// — cheaper than the service-availability probe (no discovery
    /// state required).
    ///
    /// Returns `Ok(())` on reply within `timeout_ms`,
    /// `Err(NodeError::Transport(Timeout))` on no reply,
    /// `Err(NodeError::Transport(Unsupported))` when the active
    /// backend can't probe.
    ///
    /// Mirrors micro-ROS's `rmw_uros_ping_agent`. Useful for
    /// reconnect-on-link-loss patterns: bare-metal code can call
    /// `ping(100)` periodically and tear down / re-open the session
    /// on timeout.
    pub fn ping(&mut self, timeout_ms: i32) -> Result<(), NodeError> {
        use nros_rmw::Session;
        self.session
            .ping_session(timeout_ms)
            .map_err(NodeError::Transport)
    }

    /// Get a mutable reference to an action client core in the arena by entry index.
    ///
    /// # Safety
    /// The caller must ensure that `entry_index` refers to an `ActionClientRawArenaEntry`.
    pub unsafe fn action_client_core_mut(
        &mut self,
        entry_index: usize,
    ) -> Option<&mut super::action_core::ActionClientCore> {
        let meta = self.entries.get(entry_index)?.as_ref()?;
        if !matches!(meta.kind, EntryKind::ActionClient) {
            return None;
        }
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        unsafe {
            let entry_ptr = arena_ptr.add(meta.offset)
                as *mut super::arena::ActionClientRawArenaEntry<
                    { crate::config::DEFAULT_RX_BUF_SIZE },
                    { crate::config::DEFAULT_RX_BUF_SIZE },
                    { crate::config::DEFAULT_RX_BUF_SIZE },
                >;
            Some(&mut (*entry_ptr).core)
        }
    }

    /// Get a mutable reference to a service-client arena entry (Phase 82).
    ///
    /// Returns `None` if `entry_index` doesn't refer to a service client
    /// entry. The default reply buffer size is assumed because the C API
    /// always uses the default — the entry was registered via
    /// `register_service_client_raw_sized::<DEFAULT_RX_BUF_SIZE>`.
    ///
    /// # Safety
    /// `entry_index` must refer to a `ServiceClientRawArenaEntry`.
    pub unsafe fn service_client_entry_mut(
        &mut self,
        entry_index: usize,
    ) -> Option<&mut super::arena::ServiceClientRawArenaEntry<{ crate::config::DEFAULT_RX_BUF_SIZE }>>
    {
        let meta = self.entries.get(entry_index)?.as_ref()?;
        if !matches!(meta.kind, EntryKind::ServiceClient) {
            return None;
        }
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        unsafe {
            let entry_ptr = arena_ptr.add(meta.offset)
                as *mut super::arena::ServiceClientRawArenaEntry<
                    { crate::config::DEFAULT_RX_BUF_SIZE },
                >;
            Some(&mut *entry_ptr)
        }
    }

    /// Set the executor-level trigger condition.
    ///
    /// Controls which handles must be ready before `spin_once` dispatches
    /// callbacks. Defaults to [`Trigger::AnyReady`](crate::Trigger).
    pub fn set_trigger(&mut self, trigger: Trigger) {
        self.trigger = trigger;
    }

    /// Set the executor data communication semantics.
    ///
    /// Choose between `Direct` (process in place) and `LET`
    /// (snapshot-then-process) semantics. See [`ExecutorSemantics`].
    pub fn set_semantics(&mut self, semantics: ExecutorSemantics) {
        self.semantics = semantics;
    }

    /// Set the invocation mode for a specific handle.
    ///
    /// Controls whether the callback fires on every spin
    /// ([`Always`](InvocationMode::Always)) or only when new data
    /// arrives ([`OnNewData`](InvocationMode::OnNewData), the default).
    pub fn set_invocation(&mut self, id: HandleId, mode: InvocationMode) {
        if let Some(Some(meta)) = self.entries.get_mut(id.0) {
            meta.invocation = mode;
        }
    }

    // ========================================================================
    // Arena-based callback registration
    // ========================================================================

    /// Bump-allocate space for `T` in the arena. Returns the byte offset.
    pub(crate) fn arena_alloc<T>(&mut self) -> Result<usize, NodeError> {
        let align = core::mem::align_of::<T>();
        let size = core::mem::size_of::<T>();
        let aligned_offset = (self.arena_used + align - 1) & !(align - 1);
        let new_used = aligned_offset + size;
        if new_used > crate::config::ARENA_SIZE {
            return Err(NodeError::BufferTooSmall);
        }
        self.arena_used = new_used;
        Ok(aligned_offset)
    }

    /// Bump-allocate space for `T` plus `trailing_bytes` extra bytes.
    ///
    /// Returns `(entry_offset, trailing_offset)`. The trailing region starts
    /// immediately after `T` (aligned to 8 bytes).
    pub(crate) fn arena_alloc_with_trailing<T>(
        &mut self,
        trailing_bytes: usize,
    ) -> Result<(usize, usize), NodeError> {
        let align = core::mem::align_of::<T>();
        let entry_size = core::mem::size_of::<T>();
        let entry_offset = self.arena_used.next_multiple_of(align);
        // Trailing region starts on an 8-byte (u64) boundary after the entry.
        let trailing_offset =
            (entry_offset + entry_size).next_multiple_of(core::mem::align_of::<u64>());
        let new_used = trailing_offset + trailing_bytes;
        if new_used > crate::config::ARENA_SIZE {
            return Err(NodeError::BufferTooSmall);
        }
        self.arena_used = new_used;
        Ok((entry_offset, trailing_offset))
    }

    /// Find the next free entry slot index.
    pub(crate) fn next_entry_slot(&self) -> Result<usize, NodeError> {
        self.entries
            .iter()
            .position(|e| e.is_none())
            .ok_or(NodeError::BufferTooSmall)
    }

    /// Typed buffered subscription core (the `node_mut(id).subscription(t)
    /// .typed::<M>()` builder lowers here). Routes the typed subscription
    /// through the [`NodeId`]'s session + identity (rclcpp `add_node` pattern).
    pub(crate) fn register_subscription_buffered_on<M, F, const RX_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        topic_name: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: crate::cyclonedds_register::MessageForRmw + 'static,
        F: FnMut(&M) + 'static,
    {
        type Entry<M, F> = SubBufferedEntry<M, F>;

        // Phase 212.K.7.6.b — see `create_publisher_on`.
        crate::cyclonedds_register::register_type::<M>()?;

        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = {
            let r = self
                .nodes
                .get(node_id.index())
                .ok_or(NodeError::InvalidSchedContextBinding)?;
            (r.name.clone(), r.namespace.clone(), r.session_idx)
        };
        let mut topic = TopicInfo::new(
            topic_name,
            <M as RosMessage>::TYPE_NAME,
            <M as RosMessage>::TYPE_HASH,
        )
        .with_namespace(&ns)
        // Phase 231 (RFC-0038) — hand the backend the receive-buffer size so it
        // can size-class its receive storage (zenoh-pico: small vs large).
        .with_rx_buffer_hint(RX_BUF);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            session
                .create_subscriber(&topic, qos)
                .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?
        };

        // Phase 231 Wave 0.2 (RFC-0038) — in-place dispatch when the backend
        // advertises it: deserialize straight from the borrowed receive slot,
        // no arena buffer (copy #1 removed). Else the buffered path below.
        {
            use nros_rmw::Subscriber as _;
            if handle.supports_process_in_place() {
                let entry_offset = self.arena_alloc::<SubInplaceEntry<M, F>>()?;
                unsafe {
                    let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
                    let entry_ptr = arena_ptr.add(entry_offset) as *mut SubInplaceEntry<M, F>;
                    core::ptr::write(
                        entry_ptr,
                        SubInplaceEntry {
                            handle,
                            callback,
                            _phantom: PhantomData,
                        },
                    );
                }
                self.entries[slot] = Some(CallbackMeta {
                    offset: entry_offset,
                    kind: EntryKind::Subscription,
                    try_process: sub_inplace_try_process::<M, F>,
                    has_data: sub_inplace_has_data::<M, F>,
                    pre_sample: no_pre_sample,
                    invocation: InvocationMode::OnNewData,
                    drop_fn: drop_entry::<SubInplaceEntry<M, F>>,
                });
                self.apply_node_default_sched(slot, Some(node_id));
                return Ok(HandleId(slot));
            }
        }

        let (_slot_count, trailing_bytes) = buffered_region_size(qos.depth, RX_BUF);

        let (entry_offset, trailing_offset) =
            self.arena_alloc_with_trailing::<Entry<M, F>>(trailing_bytes)?;

        let buf_ptr = unsafe { (self.arena.as_mut_ptr() as *mut u8).add(trailing_offset) };

        let buffer = if qos.depth <= 1 {
            BufferStrategy::Triple(unsafe { TripleBuffer::init(buf_ptr, RX_BUF) })
        } else {
            BufferStrategy::Ring(unsafe { SpscRing::init(buf_ptr, RX_BUF, qos.depth as usize) })
        };

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(entry_offset) as *mut Entry<M, F>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer,
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset: entry_offset,
            kind: EntryKind::Subscription,
            try_process: sub_buffered_try_process::<M, F>,
            has_data: sub_buffered_has_data::<M, F>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<M, F>>,
        });
        // Phase 104.C.4 — apply Node's default SchedContext.
        self.apply_node_default_sched(slot, Some(node_id));
        Ok(HandleId(slot))
    }

    /// Generic (type-erased) buffered subscription core (the
    /// `node_mut(id).subscription(t).generic(ty, hash)` builder lowers here).
    /// Routes the subscriber creation through the [`NodeId`]'s
    /// session + identity (rclcpp `add_node` pattern).
    ///
    /// Use this in bridge code where two Nodes bind to different RMW
    /// backends:
    ///
    /// ```ignore
    /// let node_in = exec.node_builder("ingress").rmw("zenoh").build()?;
    /// let pub_out = exec.with_node(node_out, |n| {
    ///     n.create_publisher_raw("/fwd", TYPE, HASH)
    /// })??;
    /// exec.register_subscription_buffered_raw_on::<_, 1024>(
    ///     node_in, "/src", TYPE, HASH, qos(),
    ///     move |bytes: &[u8]| { let _ = pub_out.publish_raw(bytes); },
    /// )?;
    /// ```
    pub(crate) fn register_subscription_buffered_raw_on<F, const RX_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        F: FnMut(&[u8]) + 'static,
    {
        // Pull the Node's identity + session slot out first so the
        // mutable session borrow doesn't conflict with the arena
        // alloc inside `add_arena_subscription_callback`.
        let (node_name, ns, session_idx) = {
            let r = self
                .nodes
                .get(node_id.index())
                .ok_or(NodeError::InvalidSchedContextBinding)?;
            (r.name.clone(), r.namespace.clone(), r.session_idx)
        };
        let mut topic = TopicInfo::new(topic_name, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            session
                .create_subscriber(&topic, qos)
                .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?
        };
        let handle_id = self.add_arena_subscription_callback::<F, RX_BUF>(handle, qos, callback)?;
        // Phase 104.C.4 — apply Node's default SchedContext.
        self.apply_node_default_sched(handle_id.0, Some(node_id));
        Ok(handle_id)
    }

    /// Register a borrowed (zero-copy) buffered subscription (Phase 229.6,
    /// issue 0007 / RFC-0033 `borrowed` mode).
    ///
    /// `B` is the code-generated borrowed-message marker (e.g. `ImageBorrow`)
    /// implementing [`BorrowedMessage`](nros_core::BorrowedMessage); the
    /// callback receives `&B::View<'a>` — a lifetime-carrying message whose
    /// unbounded sequence/string fields borrow directly from the receive buffer
    /// (no `heapless::Vec` copy). The view is valid only for the callback's
    /// duration.
    ///
    /// **Triple-buffer only.** A borrowed view must reference exactly one
    /// well-defined buffer slot for the callback's duration; an SPSC ring
    /// (`qos.depth > 1`) keeps several samples in flight with no single such
    /// slot. `qos.depth > 1` is therefore rejected with
    /// [`TransportError::Unsupported`].
    pub(crate) fn register_subscription_buffered_borrowed_on<B, F, const RX_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        topic_name: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        B: nros_core::BorrowedMessage + 'static,
        F: for<'a> FnMut(&B::View<'a>) + 'static,
    {
        type Entry<B, F> = SubBufferedBorrowedEntry<B, F>;

        // Borrowed views require a single well-defined slot (triple buffer).
        if qos.depth > 1 {
            return Err(NodeError::Transport(TransportError::Unsupported));
        }

        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = {
            let r = self
                .nodes
                .get(node_id.index())
                .ok_or(NodeError::InvalidSchedContextBinding)?;
            (r.name.clone(), r.namespace.clone(), r.session_idx)
        };
        let mut topic = TopicInfo::new(
            topic_name,
            <B as BorrowedMessage>::TYPE_NAME,
            <B as BorrowedMessage>::TYPE_HASH,
        )
        .with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            session
                .create_subscriber(&topic, qos)
                .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?
        };

        let (_slot_count, trailing_bytes) = buffered_region_size(qos.depth, RX_BUF);
        let (entry_offset, trailing_offset) =
            self.arena_alloc_with_trailing::<Entry<B, F>>(trailing_bytes)?;
        let buf_ptr = unsafe { (self.arena.as_mut_ptr() as *mut u8).add(trailing_offset) };

        // depth <= 1 guaranteed above → always triple buffer.
        let buffer = BufferStrategy::Triple(unsafe { TripleBuffer::init(buf_ptr, RX_BUF) });

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(entry_offset) as *mut Entry<B, F>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer,
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset: entry_offset,
            kind: EntryKind::Subscription,
            try_process: sub_buffered_borrowed_try_process::<B, F>,
            has_data: sub_buffered_borrowed_has_data::<B, F>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<B, F>>,
        });
        self.apply_node_default_sched(slot, Some(node_id));
        Ok(HandleId(slot))
    }

    /// Register a raw (type-erased) buffered subscription whose callback
    /// also receives a [`RawMessageInfo`](nros_core::RawMessageInfo)
    /// carrying the sample's wire **attachment** (Phase 189.M1).
    ///
    /// Backs the `node.subscription(t).generic(..).message_info().build(cb)`
    /// builder — the cross-RMW bridge reads the `bridge_origin` tag from
    /// `info.attachment()` for echo suppression. One sample per
    /// `spin_once`; the attachment is staged in a flat per-entry buffer
    /// (cap [`RAW_INFO_ATT_CAP`](super::arena::RAW_INFO_ATT_CAP)).
    pub fn register_subscription_buffered_raw_info_on<F, const RX_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        F: FnMut(&[u8], &nros_core::RawMessageInfo) + 'static,
    {
        type Entry<F, const N: usize> = SubBufferedRawInfoEntry<F, N>;

        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = {
            let r = self
                .nodes
                .get(node_id.index())
                .ok_or(NodeError::InvalidSchedContextBinding)?;
            (r.name.clone(), r.namespace.clone(), r.session_idx)
        };
        let mut topic = TopicInfo::new(topic_name, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            session
                .create_subscriber(&topic, qos)
                .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?
        };

        let offset = self.arena_alloc::<Entry<F, RX_BUF>>()?;
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<F, RX_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer: [0u8; RX_BUF],
                    att: [0u8; super::arena::RAW_INFO_ATT_CAP],
                    callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Subscription,
            try_process: sub_buffered_raw_info_try_process::<F, RX_BUF>,
            has_data: sub_buffered_raw_info_has_data::<F, RX_BUF>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<F, RX_BUF>>,
        });
        self.apply_node_default_sched(slot, Some(node_id));
        Ok(HandleId(slot))
    }

    /// Phase 250 (Wave 2) — register a generic (type-erased) raw subscription
    /// that surfaces E2E [`IntegrityStatus`](nros_rmw::IntegrityStatus) (CRC +
    /// sequence gap/dup) alongside the raw CDR bytes
    /// (`FnMut(&[u8], &IntegrityStatus)`). The type-erased analog of
    /// [`register_subscription_with_safety_sized_inner`]: the validator lives in
    /// the `RmwSubscriber` (`try_recv_validated`), so the subscriber is created
    /// plainly and no `register_type::<M>()` is needed (the declarative `Node`
    /// path is generic). Used by the declarative runtime's `.safety()` opt-in.
    #[cfg(feature = "safety-e2e")]
    pub fn register_subscription_buffered_raw_safety_on<F, const RX_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        F: FnMut(&[u8], &nros_rmw::IntegrityStatus) + 'static,
    {
        use super::arena::{
            SubBufferedRawSafetyEntry, sub_buffered_raw_safety_has_data,
            sub_buffered_raw_safety_try_process,
        };
        type Entry<F, const N: usize> = SubBufferedRawSafetyEntry<F, N>;

        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = {
            let r = self
                .nodes
                .get(node_id.index())
                .ok_or(NodeError::InvalidSchedContextBinding)?;
            (r.name.clone(), r.namespace.clone(), r.session_idx)
        };
        let mut topic = TopicInfo::new(topic_name, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            session
                .create_subscriber(&topic, qos)
                .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?
        };

        let offset = self.arena_alloc::<Entry<F, RX_BUF>>()?;
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<F, RX_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer: [0u8; RX_BUF],
                    callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Subscription,
            try_process: sub_buffered_raw_safety_try_process::<F, RX_BUF>,
            has_data: sub_buffered_raw_safety_has_data::<F, RX_BUF>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<F, RX_BUF>>,
        });
        self.apply_node_default_sched(slot, Some(node_id));
        Ok(HandleId(slot))
    }

    /// Register a raw byte-shaped callback against a pre-built
    /// `RmwSubscriber` handle.
    ///
    /// Backend-agnostic primitive — the caller is responsible for
    /// obtaining the handle by whatever route the active backend
    /// supports:
    ///
    /// - **Generic ROS-typed flow**: call `Session::create_subscriber`
    ///   on `self.session_mut()` with a [`TopicInfo`]. The
    ///   `node_mut(id).subscription(t).generic(ty, hash)` builder is the
    ///   convenience wrapper for this path.
    /// - **Backend-specific flow** (e.g. uORB needs `&'static orb_metadata`):
    ///   reach into the concrete session via [`Self::session_mut`] and
    ///   call its backend-specific create method, then hand the handle
    ///   here. `nros-px4::uorb::create_subscription_with_callback` is
    ///   the example.
    ///
    /// The arena-store + vtable wiring is identical to
    /// `register_subscription_buffered_raw`; the only thing that varies is
    /// where the handle came from. Callback fires on every message
    /// delivery during [`spin_once`](Self::spin_once); bytes are
    /// passed as `&[u8]`.
    pub fn add_arena_subscription_callback<F, const RX_BUF: usize>(
        &mut self,
        handle: session::RmwSubscriber,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        F: FnMut(&[u8]) + 'static,
    {
        type Entry<F> = SubBufferedRawEntry<F>;

        let slot = self.next_entry_slot()?;
        let (_slot_count, trailing_bytes) = buffered_region_size(qos.depth, RX_BUF);

        let (entry_offset, trailing_offset) =
            self.arena_alloc_with_trailing::<Entry<F>>(trailing_bytes)?;

        let buf_ptr = unsafe { (self.arena.as_mut_ptr() as *mut u8).add(trailing_offset) };

        let buffer = if qos.depth <= 1 {
            BufferStrategy::Triple(unsafe { TripleBuffer::init(buf_ptr, RX_BUF) })
        } else {
            BufferStrategy::Ring(unsafe { SpscRing::init(buf_ptr, RX_BUF, qos.depth as usize) })
        };

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(entry_offset) as *mut Entry<F>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer,
                    callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset: entry_offset,
            kind: EntryKind::Subscription,
            try_process: sub_buffered_raw_try_process::<F>,
            has_data: sub_buffered_raw_has_data::<F>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<F>>,
        });
        Ok(HandleId(slot))
    }

    pub(crate) fn register_subscription_with_info_sized_inner<M, F, const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: crate::cyclonedds_register::MessageForRmw + 'static,
        F: FnMut(&M, Option<&nros_core::MessageInfo>) + 'static,
    {
        type Entry<M, F, const N: usize> = SubInfoEntry<M, F, N>;

        // Phase 212.K.7.6.b — see `create_publisher_on`.
        crate::cyclonedds_register::register_type::<M>()?;

        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = match node_id {
            Some(id) => {
                let r = self
                    .nodes
                    .get(id.index())
                    .ok_or(NodeError::InvalidSchedContextBinding)?;
                (r.name.clone(), r.namespace.clone(), r.session_idx)
            }
            None => (self.node_name.clone(), self.namespace.clone(), 0u8),
        };
        let mut topic = TopicInfo::new(
            topic_name,
            <M as RosMessage>::TYPE_NAME,
            <M as RosMessage>::TYPE_HASH,
        )
        .with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            session
                .create_subscriber(&topic, qos)
                .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?
        };

        let offset = self.arena_alloc::<Entry<M, F, RX_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<M, F, RX_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer: [0u8; RX_BUF],
                    sampled_len: 0,
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Subscription,
            try_process: sub_info_try_process::<M, F, RX_BUF>,
            has_data: sub_info_has_data::<M, F, RX_BUF>,
            pre_sample: sub_info_pre_sample::<M, F, RX_BUF>,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<M, F, RX_BUF>>,
        });
        self.apply_node_default_sched(slot, node_id);
        Ok(HandleId(slot))
    }

    #[cfg(feature = "safety-e2e")]
    pub(crate) fn register_subscription_with_safety_sized_inner<M, F, const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        M: crate::cyclonedds_register::MessageForRmw + 'static,
        F: FnMut(&M, &nros_rmw::IntegrityStatus) + 'static,
    {
        type Entry<M, F, const N: usize> = SubSafetyEntry<M, F, N>;

        // Phase 212.K.7.6.b — see `create_publisher_on`.
        crate::cyclonedds_register::register_type::<M>()?;

        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = match node_id {
            Some(id) => {
                let r = self
                    .nodes
                    .get(id.index())
                    .ok_or(NodeError::InvalidSchedContextBinding)?;
                (r.name.clone(), r.namespace.clone(), r.session_idx)
            }
            None => (self.node_name.clone(), self.namespace.clone(), 0u8),
        };
        let mut topic = TopicInfo::new(
            topic_name,
            <M as RosMessage>::TYPE_NAME,
            <M as RosMessage>::TYPE_HASH,
        )
        .with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            session
                .create_subscriber(&topic, qos)
                .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?
        };

        let offset = self.arena_alloc::<Entry<M, F, RX_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<M, F, RX_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer: [0u8; RX_BUF],
                    sampled_len: 0,
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Subscription,
            try_process: sub_safety_try_process::<M, F, RX_BUF>,
            has_data: sub_safety_has_data::<M, F, RX_BUF>,
            pre_sample: sub_safety_pre_sample::<M, F, RX_BUF>,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<M, F, RX_BUF>>,
        });
        self.apply_node_default_sched(slot, node_id);
        Ok(HandleId(slot))
    }

    /// Register a service callback with the default buffer size.
    ///
    /// The callback is stored in the arena and invoked during [`spin_once()`](Self::spin_once).
    pub fn register_service<Svc, F>(
        &mut self,
        service_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        Svc: RosService + 'static,
        Svc::Request: crate::cyclonedds_register::MessageForRmw,
        Svc::Reply: crate::cyclonedds_register::MessageForRmw,
        F: FnMut(&Svc::Request) -> Svc::Reply + 'static,
    {
        self.register_service_sized::<Svc, F, { crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }>(service_name, callback)
    }

    /// Register a service callback with custom request/reply buffer sizes.
    pub fn register_service_sized<Svc, F, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        Svc: RosService + 'static,
        Svc::Request: crate::cyclonedds_register::MessageForRmw,
        Svc::Reply: crate::cyclonedds_register::MessageForRmw,
        F: FnMut(&Svc::Request) -> Svc::Reply + 'static,
    {
        type Entry<Svc, F, const RQ: usize, const RP: usize> = SrvEntry<Svc, F, RQ, RP>;

        // Phase 212.K.7.7.b — register both halves of the service round-trip
        // under cyclonedds. No-op for other RMWs. Mirrors the K.7.6.b hook
        // on `Node::create_service_sized`.
        crate::cyclonedds_register::register_type::<Svc::Request>()?;
        crate::cyclonedds_register::register_type::<Svc::Reply>()?;

        let slot = self.next_entry_slot()?;
        let node_name: heapless::String<64> = self.node_name.clone();
        let ns: heapless::String<64> = self.namespace.clone();
        let mut info = ServiceInfo::new(service_name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
            .with_namespace(&ns);
        if !node_name.is_empty() {
            info = info.with_node_name(&node_name);
        }
        let handle = self
            .session
            .create_service_server(&info, QosSettings::services_default())
            .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))?;

        let offset = self.arena_alloc::<Entry<Svc, F, REQ_BUF, REPLY_BUF>>()?;

        // SAFETY: same guarantees as register_subscription_sized.
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<Svc, F, REQ_BUF, REPLY_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    req_buffer: [0u8; REQ_BUF],
                    reply_buffer: [0u8; REPLY_BUF],
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Service,
            try_process: srv_try_process::<Svc, F, REQ_BUF, REPLY_BUF>,
            has_data: srv_has_data::<Svc, F, REQ_BUF, REPLY_BUF>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<Svc, F, REQ_BUF, REPLY_BUF>>,
        });
        Ok(HandleId(slot))
    }

    /// Phase 104.C.3.3.a — Node-aware variant of
    /// [`register_service_sized`](Self::register_service_sized).
    pub fn register_service_sized_on<Svc, F, const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        service_name: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        Svc: RosService + 'static,
        Svc::Request: crate::cyclonedds_register::MessageForRmw,
        Svc::Reply: crate::cyclonedds_register::MessageForRmw,
        F: FnMut(&Svc::Request) -> Svc::Reply + 'static,
    {
        type Entry<Svc, F, const RQ: usize, const RP: usize> = SrvEntry<Svc, F, RQ, RP>;

        // Phase 212.K.7.7.b — see `register_service_sized`.
        crate::cyclonedds_register::register_type::<Svc::Request>()?;
        crate::cyclonedds_register::register_type::<Svc::Reply>()?;

        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = {
            let r = self
                .nodes
                .get(node_id.index())
                .ok_or(NodeError::InvalidSchedContextBinding)?;
            (r.name.clone(), r.namespace.clone(), r.session_idx)
        };
        let mut info = ServiceInfo::new(service_name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
            .with_namespace(&ns);
        if !node_name.is_empty() {
            info = info.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            // Phase 193.5 — validate against the backend's supported policies
            // (no silent downgrade); request/reply effectively requires RELIABLE.
            qos.validate_against(session.supported_qos_policies())
                .map_err(NodeError::Transport)?;
            session
                .create_service_server(&info, qos)
                .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))?
        };

        let offset = self.arena_alloc::<Entry<Svc, F, REQ_BUF, REPLY_BUF>>()?;
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<Svc, F, REQ_BUF, REPLY_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    req_buffer: [0u8; REQ_BUF],
                    reply_buffer: [0u8; REPLY_BUF],
                    callback,
                    _phantom: PhantomData,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Service,
            try_process: srv_try_process::<Svc, F, REQ_BUF, REPLY_BUF>,
            has_data: srv_has_data::<Svc, F, REQ_BUF, REPLY_BUF>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<Svc, F, REQ_BUF, REPLY_BUF>>,
        });
        self.apply_node_default_sched(slot, Some(node_id));
        Ok(HandleId(slot))
    }

    /// Phase 104.C.3.3.a — Node-aware variant of
    /// [`register_service`](Self::register_service).
    pub fn register_service_on<Svc, F>(
        &mut self,
        node_id: super::node_record::NodeId,
        service_name: &str,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        Svc: RosService + 'static,
        Svc::Request: crate::cyclonedds_register::MessageForRmw,
        Svc::Reply: crate::cyclonedds_register::MessageForRmw,
        F: FnMut(&Svc::Request) -> Svc::Reply + 'static,
    {
        self.register_service_sized_on::<
            Svc,
            F,
            { crate::config::DEFAULT_RX_BUF_SIZE },
            { crate::config::DEFAULT_RX_BUF_SIZE },
        >(node_id, service_name, QosSettings::services_default(), callback)
    }

    // ========================================================================
    // Timer registration
    // ========================================================================

    /// Register a repeating timer callback.
    ///
    /// The callback fires every `period` milliseconds during [`spin_once()`](Self::spin_once).
    /// The timer delta is approximated by the `timeout_ms` argument to `spin_once`.
    pub fn register_timer<F>(
        &mut self,
        period: TimerDuration,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        F: FnMut() + 'static,
    {
        let slot = self.next_entry_slot()?;
        let offset = self.arena_alloc::<TimerEntry<F>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut TimerEntry<F>;
            core::ptr::write(
                entry_ptr,
                TimerEntry {
                    period_ms: period.as_millis(),
                    elapsed_ms: 0,
                    oneshot: false,
                    fired: false,
                    cancelled: false,
                    callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Timer,
            try_process: timer_try_process::<F>,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            drop_fn: drop_entry::<TimerEntry<F>>,
        });
        Ok(HandleId(slot))
    }

    /// Register a one-shot timer callback.
    ///
    /// The callback fires once after `delay` milliseconds, then becomes inert.
    pub fn register_timer_oneshot<F>(
        &mut self,
        delay: TimerDuration,
        callback: F,
    ) -> Result<HandleId, NodeError>
    where
        F: FnMut() + 'static,
    {
        let slot = self.next_entry_slot()?;
        let offset = self.arena_alloc::<TimerEntry<F>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut TimerEntry<F>;
            core::ptr::write(
                entry_ptr,
                TimerEntry {
                    period_ms: delay.as_millis(),
                    elapsed_ms: 0,
                    oneshot: true,
                    fired: false,
                    cancelled: false,
                    callback,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Timer,
            try_process: timer_try_process::<F>,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            drop_fn: drop_entry::<TimerEntry<F>>,
        });
        Ok(HandleId(slot))
    }

    // ========================================================================
    // Raw callback registration (for C API)
    // ========================================================================

    /// The kept C-FFI subscription core (Phase 189.M2.b): registers a
    /// raw `RawSubscriptionCallback` fn-ptr + `context` against an
    /// optional node's session. The Rust ergonomic surface is the
    /// `node.subscription(t)` builder (closures); this is the single
    /// primitive the `nros-c` thin wrapper lowers to. `node_id == None`
    /// is the legacy single-node path.
    #[allow(clippy::too_many_arguments)]
    pub fn add_arena_subscription_c_callback<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
        callback: RawSubscriptionCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = match node_id {
            Some(id) => {
                let r = self
                    .nodes
                    .get(id.index())
                    .ok_or(NodeError::InvalidSchedContextBinding)?;
                (r.name.clone(), r.namespace.clone(), r.session_idx)
            }
            None => (self.node_name.clone(), self.namespace.clone(), 0u8),
        };
        let mut topic = TopicInfo::new(topic_name, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            session
                .create_subscriber(&topic, qos)
                .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?
        };

        let (_slot_count, trailing_bytes) = buffered_region_size(qos.depth, RX_BUF);

        let (entry_offset, trailing_offset) =
            self.arena_alloc_with_trailing::<SubBufferedRawCEntry>(trailing_bytes)?;

        let buf_ptr = unsafe { (self.arena.as_mut_ptr() as *mut u8).add(trailing_offset) };

        let buffer = if qos.depth <= 1 {
            BufferStrategy::Triple(unsafe { TripleBuffer::init(buf_ptr, RX_BUF) })
        } else {
            BufferStrategy::Ring(unsafe { SpscRing::init(buf_ptr, RX_BUF, qos.depth as usize) })
        };

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(entry_offset) as *mut SubBufferedRawCEntry;
            core::ptr::write(
                entry_ptr,
                SubBufferedRawCEntry {
                    handle,
                    buffer,
                    callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset: entry_offset,
            kind: EntryKind::Subscription,
            try_process: sub_buffered_raw_c_try_process,
            has_data: sub_buffered_raw_c_has_data,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<SubBufferedRawCEntry>,
        });
        self.apply_node_default_sched(slot, node_id);
        Ok(HandleId(slot))
    }

    /// Phase 189.M3.4 — register a raw C-fn-ptr subscription whose callback
    /// also receives the sample's wire **attachment**
    /// ([`RawSubscriptionInfoCallback`]: `(data, len, attachment, att_len,
    /// context)`) — the C analog of the Rust
    /// `node.subscription(t).generic(..).message_info()` builder. Backs the C
    /// FFI `nros_executor_register_subscription_raw_with_info`. Flat per-entry
    /// payload + attachment buffers (cap [`RAW_INFO_ATT_CAP`](super::arena::RAW_INFO_ATT_CAP));
    /// one sample per `spin_once`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_arena_subscription_c_info_callback<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QosSettings,
        callback: RawSubscriptionInfoCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        type Entry<const N: usize> = SubBufferedRawInfoCEntry<N>;

        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = match node_id {
            Some(id) => {
                let r = self
                    .nodes
                    .get(id.index())
                    .ok_or(NodeError::InvalidSchedContextBinding)?;
                (r.name.clone(), r.namespace.clone(), r.session_idx)
            }
            None => (self.node_name.clone(), self.namespace.clone(), 0u8),
        };
        let mut topic = TopicInfo::new(topic_name, type_name, type_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            topic = topic.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            session
                .create_subscriber(&topic, qos)
                .map_err(|_| NodeError::Transport(TransportError::SubscriberCreationFailed))?
        };

        let offset = self.arena_alloc::<Entry<RX_BUF>>()?;
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut Entry<RX_BUF>;
            core::ptr::write(
                entry_ptr,
                Entry {
                    handle,
                    buffer: [0u8; RX_BUF],
                    att: [0u8; super::arena::RAW_INFO_ATT_CAP],
                    callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Subscription,
            try_process: sub_buffered_raw_info_c_try_process::<RX_BUF>,
            has_data: sub_buffered_raw_info_c_has_data::<RX_BUF>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<Entry<RX_BUF>>,
        });
        self.apply_node_default_sched(slot, node_id);
        Ok(HandleId(slot))
    }

    /// Register a raw (untyped) service callback.
    ///
    /// Register a raw (untyped) service callback with the default buffer size.
    ///
    /// The callback receives and produces CDR bytes without typed
    /// deserialization/serialization. Used by the C API wrapper.
    pub fn register_service_raw(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.register_service_raw_sized::<{ crate::config::DEFAULT_RX_BUF_SIZE }, { crate::config::DEFAULT_RX_BUF_SIZE }>(
            service_name,
            service_type,
            service_hash,
            QosSettings::services_default(),
            callback,
            context,
        )
    }

    /// Register a raw (untyped) service callback with custom buffer sizes + QoS.
    ///
    /// `REQ_BUF` and `REPLY_BUF` set the stack-allocated CDR buffers
    /// for the request and reply respectively. Increase for services
    /// with large payloads (e.g., parameter services). `qos` applies to both
    /// the request + reply endpoints (Phase 193.2c).
    #[allow(clippy::too_many_arguments)]
    pub fn register_service_raw_sized<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QosSettings,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.register_service_raw_sized_inner::<REQ_BUF, REPLY_BUF>(
            None,
            service_name,
            service_type,
            service_hash,
            qos,
            callback,
            context,
        )
    }

    /// Phase 104.C.3.3.a — Node-aware variant of
    /// [`register_service_raw_sized`]. C-FFI path.
    #[allow(clippy::too_many_arguments)]
    pub fn register_service_raw_sized_on<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QosSettings,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.register_service_raw_sized_inner::<REQ_BUF, REPLY_BUF>(
            Some(node_id),
            service_name,
            service_type,
            service_hash,
            qos,
            callback,
            context,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn register_service_raw_sized_inner<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QosSettings,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = match node_id {
            Some(id) => {
                let r = self
                    .nodes
                    .get(id.index())
                    .ok_or(NodeError::InvalidSchedContextBinding)?;
                (r.name.clone(), r.namespace.clone(), r.session_idx)
            }
            None => (self.node_name.clone(), self.namespace.clone(), 0u8),
        };
        let mut info =
            ServiceInfo::new(service_name, service_type, service_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            info = info.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            // Phase 193.5 — validate against the backend's supported policies
            // (no silent downgrade); request/reply effectively requires RELIABLE.
            qos.validate_against(session.supported_qos_policies())
                .map_err(NodeError::Transport)?;
            session
                .create_service_server(&info, qos)
                .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))?
        };

        let offset = self.arena_alloc::<SrvRawEntry<REQ_BUF, REPLY_BUF>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut SrvRawEntry<REQ_BUF, REPLY_BUF>;
            core::ptr::write(
                entry_ptr,
                SrvRawEntry {
                    handle,
                    req_buffer: [0u8; REQ_BUF],
                    reply_buffer: [0u8; REPLY_BUF],
                    callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::Service,
            try_process: srv_raw_try_process::<REQ_BUF, REPLY_BUF>,
            has_data: srv_raw_has_data::<REQ_BUF, REPLY_BUF>,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::OnNewData,
            drop_fn: drop_entry::<SrvRawEntry<REQ_BUF, REPLY_BUF>>,
        });
        self.apply_node_default_sched(slot, node_id);
        Ok(HandleId(slot))
    }

    // ========================================================================
    // Raw service client registration (Phase 82)
    // ========================================================================

    /// Register a raw (untyped) service client with the default reply
    /// buffer size.
    ///
    /// The client is owned by the executor's arena. Each `spin_once`
    /// dispatch polls the in-flight reply slot via `try_recv_reply_raw`
    /// and fires the registered callback when the response arrives.
    /// Used by the C API thin wrapper — see Phase 82.
    pub fn register_service_client_raw(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.register_service_client_raw_sized::<{ crate::config::DEFAULT_RX_BUF_SIZE }>(
            service_name,
            service_type,
            service_hash,
            QosSettings::services_default(),
            callback,
            context,
        )
    }

    /// Register a raw service client with a custom reply buffer size + QoS.
    ///
    /// `qos` applies to the client's request + reply endpoints (Phase 193.3b);
    /// defaults to [`QosSettings::services_default`] via the convenience
    /// wrapper.
    #[allow(clippy::too_many_arguments)]
    pub fn register_service_client_raw_sized<const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QosSettings,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.register_service_client_raw_sized_inner::<REPLY_BUF>(
            None,
            service_name,
            service_type,
            service_hash,
            qos,
            callback,
            context,
        )
    }

    /// Phase 104.C.3.3.a — Node-aware variant of
    /// [`register_service_client_raw_sized`]. Routes the client
    /// creation through the named Node's session.
    #[allow(clippy::too_many_arguments)]
    pub fn register_service_client_raw_sized_on<const REPLY_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QosSettings,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        self.register_service_client_raw_sized_inner::<REPLY_BUF>(
            Some(node_id),
            service_name,
            service_type,
            service_hash,
            qos,
            callback,
            context,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn register_service_client_raw_sized_inner<const REPLY_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QosSettings,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = match node_id {
            Some(id) => {
                let r = self
                    .nodes
                    .get(id.index())
                    .ok_or(NodeError::InvalidSchedContextBinding)?;
                (r.name.clone(), r.namespace.clone(), r.session_idx)
            }
            None => (self.node_name.clone(), self.namespace.clone(), 0u8),
        };
        let mut info =
            ServiceInfo::new(service_name, service_type, service_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            info = info.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            // Phase 193.5 — validate against the backend's supported policies
            // (no silent downgrade); request/reply effectively requires RELIABLE.
            qos.validate_against(session.supported_qos_policies())
                .map_err(NodeError::Transport)?;
            session
                .create_service_client(&info, qos)
                .map_err(|_| NodeError::Transport(TransportError::ServiceClientCreationFailed))?
        };

        let offset = self.arena_alloc::<ServiceClientRawArenaEntry<REPLY_BUF>>()?;
        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut ServiceClientRawArenaEntry<REPLY_BUF>;
            core::ptr::write(
                entry_ptr,
                ServiceClientRawArenaEntry {
                    handle,
                    reply_buffer: [0u8; REPLY_BUF],
                    pending: false,
                    reply_ready: core::sync::atomic::AtomicBool::new(false),
                    callback,
                    context,
                },
            );
        }

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::ServiceClient,
            try_process: service_client_raw_try_process::<REPLY_BUF>,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            drop_fn: drop_entry::<ServiceClientRawArenaEntry<REPLY_BUF>>,
        });
        self.apply_node_default_sched(slot, node_id);
        Ok(HandleId(slot))
    }

    /// RFC-0041 / Phase 239.1 — register a **typed callback** service client.
    /// The reply is eager-drained at `spin_once` and dispatched to `callback` as
    /// a deserialized `Svc::Reply`. Returns the scheduling [`HandleId`] and a
    /// `*mut` to the arena entry's send header (used to build the typed
    /// [`ServiceClientCallback`](super::handles::ServiceClientCallback)).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn register_service_client_callback<Svc, F, const REPLY_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QosSettings,
        callback: F,
    ) -> Result<(HandleId, *mut ServiceClientSendHeader<REPLY_BUF>), NodeError>
    where
        Svc: nros_core::RosService + 'static,
        F: FnMut(&Svc::Reply) + 'static,
    {
        let slot = self.next_entry_slot()?;
        let (node_name, ns, session_idx) = match node_id {
            Some(id) => {
                let r = self
                    .nodes
                    .get(id.index())
                    .ok_or(NodeError::InvalidSchedContextBinding)?;
                (r.name.clone(), r.namespace.clone(), r.session_idx)
            }
            None => (self.node_name.clone(), self.namespace.clone(), 0u8),
        };
        let mut info =
            ServiceInfo::new(service_name, service_type, service_hash).with_namespace(&ns);
        if !node_name.is_empty() {
            info = info.with_node_name(&node_name);
        }
        let handle = {
            let session = self
                .session_at_mut(session_idx)
                .ok_or(NodeError::BackendMismatch)?;
            qos.validate_against(session.supported_qos_policies())
                .map_err(NodeError::Transport)?;
            session
                .create_service_client(&info, qos)
                .map_err(|_| NodeError::Transport(TransportError::ServiceClientCreationFailed))?
        };

        let offset = self.arena_alloc::<ServiceClientCallbackEntry<Svc, F, REPLY_BUF>>()?;
        let hdr_ptr = unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr =
                arena_ptr.add(offset) as *mut ServiceClientCallbackEntry<Svc, F, REPLY_BUF>;
            core::ptr::write(
                entry_ptr,
                ServiceClientCallbackEntry {
                    hdr: ServiceClientSendHeader {
                        handle,
                        reply_buffer: [0u8; REPLY_BUF],
                        pending: false,
                        reply_ready: core::sync::atomic::AtomicBool::new(false),
                    },
                    callback,
                    _phantom: core::marker::PhantomData,
                },
            );
            &mut (*entry_ptr).hdr as *mut ServiceClientSendHeader<REPLY_BUF>
        };

        self.entries[slot] = Some(CallbackMeta {
            offset,
            kind: EntryKind::ServiceClient,
            try_process: service_client_callback_try_process::<Svc, F, REPLY_BUF>,
            has_data: always_ready,
            pre_sample: no_pre_sample,
            invocation: InvocationMode::Always,
            drop_fn: drop_entry::<ServiceClientCallbackEntry<Svc, F, REPLY_BUF>>,
        });
        self.apply_node_default_sched(slot, node_id);
        Ok((HandleId(slot), hdr_ptr))
    }

    // ========================================================================
    // Guard condition registration
    // ========================================================================

    /// Register a guard condition with a callback.
    ///
    /// Returns both the [`HandleId`] for trigger configuration and a
    /// [`GuardConditionHandle`] for triggering from other threads.
    pub fn register_guard_condition<F>(
        &mut self,
        callback: F,
    ) -> Result<(HandleId, GuardConditionHandle), NodeError>
    where
        F: FnMut() + 'static,
    {
        let slot = self.next_entry_slot()?;
        let offset = self.arena_alloc::<GuardConditionEntry<F>>()?;

        unsafe {
            let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
            let entry_ptr = arena_ptr.add(offset) as *mut GuardConditionEntry<F>;
            core::ptr::write(
                entry_ptr,
                GuardConditionEntry {
                    flag: portable_atomic::AtomicBool::new(false),
                    callback,
                },
            );

            // Create a handle pointing to the flag in the arena
            let flag_ptr = &(*entry_ptr).flag as *const portable_atomic::AtomicBool;
            #[allow(unused_mut)]
            let mut guard_handle = GuardConditionHandle::new(flag_ptr);
            // Phase 124.B.5 — wire the wake callback so trigger()
            // also signals the executor's wake_cv.
            #[cfg(all(feature = "std", feature = "rmw-cffi"))]
            {
                let ctx = self.wake_ctx_ptr();
                guard_handle.set_wake_cb(nros_rmw_runtime_wake_cb, ctx);
            }

            self.entries[slot] = Some(CallbackMeta {
                offset,
                kind: EntryKind::GuardCondition,
                try_process: guard_try_process::<F>,
                has_data: guard_has_data::<F>,
                pre_sample: no_pre_sample,
                invocation: InvocationMode::OnNewData,
                drop_fn: drop_entry::<GuardConditionEntry<F>>,
            });

            Ok((HandleId(slot), guard_handle))
        }
    }

    // ========================================================================
    // Timer control methods
    // ========================================================================

    /// Cancel a timer. A cancelled timer will not fire but still accumulates
    /// elapsed time. The timer can be restarted with [`reset_timer()`](Self::reset_timer).
    pub fn cancel_timer(&mut self, id: HandleId) -> Result<(), NodeError> {
        let meta = self
            .entries
            .get(id.0)
            .and_then(|e| e.as_ref())
            .ok_or(NodeError::BufferTooSmall)?;
        if !matches!(meta.kind, EntryKind::Timer) {
            return Err(NodeError::BufferTooSmall);
        }
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        // SAFETY: meta.offset points to a valid TimerEntry<F> which shares
        // layout with TimerHeader for its initial fields (both #[repr(C)]).
        let header = unsafe { &mut *(arena_ptr.add(meta.offset) as *mut TimerHeader) };
        header.cancelled = true;
        Ok(())
    }

    /// Reset a timer. Clears the cancelled state and resets the elapsed time
    /// to zero, so the timer starts a fresh period.
    pub fn reset_timer(&mut self, id: HandleId) -> Result<(), NodeError> {
        let meta = self
            .entries
            .get(id.0)
            .and_then(|e| e.as_ref())
            .ok_or(NodeError::BufferTooSmall)?;
        if !matches!(meta.kind, EntryKind::Timer) {
            return Err(NodeError::BufferTooSmall);
        }
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        let header = unsafe { &mut *(arena_ptr.add(meta.offset) as *mut TimerHeader) };
        header.cancelled = false;
        header.elapsed_ms = 0;
        Ok(())
    }

    /// Check if a timer is cancelled.
    pub fn timer_is_cancelled(&self, id: HandleId) -> bool {
        let meta = match self.entries.get(id.0).and_then(|e| e.as_ref()) {
            Some(m) if matches!(m.kind, EntryKind::Timer) => m,
            _ => return false,
        };
        let arena_ptr = self.arena.as_ptr() as *const u8;
        let header = unsafe { &*(arena_ptr.add(meta.offset) as *const TimerHeader) };
        header.cancelled
    }

    /// Get the period of a timer in milliseconds, or `None` if the handle
    /// is not a valid timer.
    pub fn timer_period_ms(&self, id: HandleId) -> Option<u64> {
        let meta = self
            .entries
            .get(id.0)
            .and_then(|e| e.as_ref())
            .filter(|m| matches!(m.kind, EntryKind::Timer))?;
        let arena_ptr = self.arena.as_ptr() as *const u8;
        let header = unsafe { &*(arena_ptr.add(meta.offset) as *const TimerHeader) };
        Some(header.period_ms)
    }

    // ========================================================================
    // spin_once (three-phase: readiness -> trigger -> dispatch)
    // ========================================================================

    /// Drive I/O and dispatch registered callbacks once.
    ///
    /// Three-phase execution:
    /// 1. **Readiness scan** — query each handle's `has_data()`.
    /// 2. **Trigger evaluation** — check if the executor-level trigger passes.
    /// 3. **Dispatch** — invoke callbacks according to their `InvocationMode`.
    ///
    /// Returns a [`SpinOnceResult`] with counts of processed items and errors.
    ///
    /// # Arguments
    /// * `timeout` — upper bound on the I/O wait. Saturated at
    ///   `i32::MAX` ms (~24 days) for the underlying transport call.
    ///
    /// Phase 84.D7: unified on `core::time::Duration`. The previous
    /// `timeout_ms: i32` signature had a latent footgun where
    /// `spin_once(-1)` silently froze timers while still polling I/O;
    /// `Duration` has no negative sentinel.
    pub fn spin_once(&mut self, timeout: core::time::Duration) -> SpinOnceResult {
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;

        // Phase 110.0 — cap against the backend's next internal-event
        // deadline (lease keepalive, heartbeat, ACK-NACK timeout, ...).
        // Default backend impl returns `None`, so this is a no-op
        // unless the active backend opts in.
        #[allow(unused_variables)]
        let timeout_ms = match self.session.next_deadline_ms() {
            Some(next) => timeout_ms.min(next.min(i32::MAX as u32) as i32),
            None => timeout_ms,
        };

        // Wall-clock-accurate timer accumulation. Measure real time
        // since the previous `spin_once` exited (or, on the first call,
        // since `drive_io` started). Two failure modes the requested
        // `timeout_ms` doesn't capture:
        //  1. `drive_io` returns early — e.g. zenoh-pico's condvar wakes
        //     on data arrival, well under 1 ms.
        //  2. The caller spends time outside `spin_once` (explicit sleep,
        //     ROS-2 cooperative scheduling, etc.) and that time should
        //     still count toward timers.
        // Crediting the requested timeout to timers in either case ticks
        // them faster than wall-clock — observed as a 30 Hz control loop
        // overshooting to >200 Hz under sustained traffic. Carry the
        // sub-ms remainder across calls so precision is preserved.
        #[cfg(feature = "std")]
        let spin_start = std::time::Instant::now();
        #[cfg(not(feature = "std"))]
        let spin_start_us = self.clock_us_fn.map(|clock| clock());

        // Phase 104.C.6 — shared executor wake. Swap-and-clear the
        // wake flag; if it was set before this `spin_once` entered,
        // skip the blocking wait on the primary session and poll
        // every session non-blockingly. Lets a wake signal from any
        // thread (or, post-104.C.6.b, any backend's vtable hook)
        // pre-empt whichever session the executor would otherwise
        // sleep on. Cost on the no-wake path is one atomic swap.
        #[cfg(feature = "std")]
        #[allow(unused_variables)]
        let was_woken = self
            .wake_flag
            .swap(false, std::sync::atomic::Ordering::SeqCst);

        // Phase 124.B.4 — condvar-blocked wait.
        //
        // RT contract:
        //  * cv.wait_timeout_while: bounded by `timeout_ms`.
        //    Predicate is O(1) — one atomic swap + Instant::now.
        //    No allocation. PI-mutex consideration: wake_mu held
        //    only during predicate check (microseconds);
        //    contended worst-case = notify_all execution time
        //    (~10s of µs).
        //  * Backend's `set_wake_callback`-installed cb is called
        //    on async data arrival from its transport-notify path
        //    (worker thread, ISR-safe variant via 124.B.7). The
        //    runtime cb writes wake_flag + signals wake_cv,
        //    unblocking this loop sub-poll-period.
        //  * Poll-only backends (XRCE, bare-metal) leave the slot
        //    NULL; the cv wait still fires on its deadline, then
        //    drive_io(0) drains whatever the backend's internal
        //    poll has buffered. Equivalent to their pre-124
        //    behaviour minus the blocking wait inside drive_io.
        //
        // Lost-wakeup safe: SeqCst flag write happens-before
        // notify, and the waiter checks the flag under wake_mu in
        // the predicate. If wake fires between drain and cv.wait
        // entry, the predicate sees flag=true on first eval and
        // exits immediately.
        // Phase 130.4 — only sleep in the wake-primitive wait when a
        // backend actually installed `set_wake_callback`. Poll-only
        // backends (XRCE, current Cyclone / dust-DDS) leave the
        // vtable slot NULL → `has_async_wake == false` → drive_io
        // for the caller's full timeout instead of sleeping in a
        // never-signaled wait that starves reliable retransmission
        // (Phase 127.C.4 root cause: server's send_reply flushes
        // 100 ms once, then NodeWake.wait_ms(100) sleeps 100 ms
        // with zero session activity, so the agent's ACK arrives
        // into a stalled session and reliable redelivery never
        // fires). RTOS std builds with an event-driven backend
        // installed still use `NodeWake` (kernel-native binary
        // semaphore — honors its deadline, dodges Zephyr's libc
        // `pthread_cond_timedwait` hang); POSIX/macOS std keep
        // the existing `std::Condvar` path.
        // Phase 248 (C2) — platform-agnostic wake wait. The choice of
        // wait primitive is made at runtime from the platform vtable's
        // wake probe, NOT a compile-time per-RTOS `cfg`:
        //
        //   * `node_wake.is_some()` → a kernel-native binary semaphore
        //     (`nros_platform_wake_*`) is linked; block on it. It honors
        //     its deadline (dodging e.g. Zephyr's libc
        //     `pthread_cond_timedwait` hang) and `nros_rmw_runtime_wake_cb`
        //     signals it on transport arrival.
        //   * `node_wake.is_none()` → no platform wake primitive; fall
        //     back to the std `Condvar` wake pair.
        //
        // Either way, only sleep in the wake-primitive/cv wait when a
        // backend actually installed `set_wake_callback`
        // (`has_async_wake`); poll-only backends (XRCE, current Cyclone)
        // leave the slot NULL and get a full-timeout `drive_io` so
        // reliable retransmission isn't starved (Phase 127.C.4).
        #[cfg(all(feature = "std", feature = "rmw-cffi"))]
        let primary_drive_timeout_ms = if let Some(wake) = self.node_wake.as_ref() {
            if !was_woken && self.has_async_wake {
                let _ = wake.wait_ms(timeout_ms as u32);
                // Clear any pending flag the cb set while we were
                // waiting; mirrors the std cv predicate's flag drain.
                let _ = self
                    .wake_flag
                    .swap(false, std::sync::atomic::Ordering::SeqCst);
                0
            } else {
                timeout_ms
            }
        } else {
            if !was_woken && self.has_async_wake {
                let dur = core::time::Duration::from_millis(timeout_ms as u64);
                // SAFETY-invariant: `wake_mu` guards `()` — it is purely the
                // companion mutex for `wake_cv`, protecting no shared state. A
                // poison (another thread panicked while holding it) cannot have
                // corrupted anything, so recover the guard rather than aborting
                // this hot spin loop.
                let g = self.wake_mu.lock().unwrap_or_else(|e| e.into_inner());
                let _ = self.wake_cv.wait_timeout_while(g, dur, |_| {
                    !self
                        .wake_flag
                        .swap(false, std::sync::atomic::Ordering::SeqCst)
                });
            }
            // drive_io is non-blocking when the cv-wait above ran;
            // full-timeout otherwise so the transport's blocking recv
            // yields the thread instead of busy-spinning.
            if self.has_async_wake { 0 } else { timeout_ms }
        };

        // std builds without rmw-cffi (mock-session tests, future
        // alternative backends) keep the original "drive_io is
        // non-blocking" assumption.
        #[cfg(all(feature = "std", not(feature = "rmw-cffi")))]
        let primary_drive_timeout_ms = 0;

        // Phase 248 (C2) — no_std + alloc + rmw-cffi path. When a backend
        // installed the wake-cb (`has_async_wake_alloc`) and a platform
        // wake primitive is available (`node_wake_alloc.is_some()` — the
        // runtime vtable probe), block on `node_wake_alloc.wait_ms` so the
        // executor unblocks on transport arrival rather than relying on
        // drive_io's blocking recv for the full timeout. Then drive_io(0)
        // drains whatever the backend's poll path buffered. Platforms with
        // no wake primitive (bare-metal) fall through to the full timeout.
        #[cfg(all(feature = "alloc", not(feature = "std"), feature = "rmw-cffi"))]
        let primary_drive_timeout_ms = {
            let was_woken_alloc = self
                .wake_flag_alloc
                .swap(false, portable_atomic::Ordering::SeqCst);
            if !was_woken_alloc
                && self.has_async_wake_alloc
                && let Some(wake) = self.node_wake_alloc.as_ref()
            {
                let _ = wake.wait_ms(timeout_ms as u32);
                // Drain any flag the cb set while we were waiting.
                let _ = self
                    .wake_flag_alloc
                    .swap(false, portable_atomic::Ordering::SeqCst);
                0
            } else {
                timeout_ms
            }
        };

        // no_std without (alloc + rmw-cffi) keeps the legacy
        // full-timeout drive_io call.
        #[cfg(all(
            not(feature = "std"),
            not(all(feature = "alloc", feature = "rmw-cffi"))
        ))]
        let primary_drive_timeout_ms = timeout_ms;

        let _ = self.session.drive_io(primary_drive_timeout_ms);
        for extra in self.extra_sessions.iter_mut() {
            let _ = extra.drive_io(0);
        }

        #[cfg(feature = "std")]
        let delta_ms = {
            let now = std::time::Instant::now();
            // `last_spin_end` is seeded at construction time, so this
            // path always has a Some(_) on every call.
            let prev = self.last_spin_end.unwrap_or(spin_start);
            let elapsed = now.saturating_duration_since(prev);
            self.last_spin_end = Some(now);
            let total_us = self
                .spin_residual_us
                .saturating_add(elapsed.as_micros() as u64);
            let ms = total_us / 1000;
            self.spin_residual_us = total_us % 1000;
            ms
        };
        #[cfg(not(feature = "std"))]
        let delta_ms = if let Some(clock) = self.clock_us_fn {
            let now = clock();
            let prev = self
                .last_spin_end_us
                .unwrap_or_else(|| spin_start_us.unwrap_or(now));
            self.last_spin_end_us = Some(now);
            let elapsed_us = now.saturating_sub(prev);
            let total_us = self.spin_residual_us.saturating_add(elapsed_us);
            let ms = total_us / 1000;
            self.spin_residual_us = total_us % 1000;
            ms
        } else {
            timeout_ms as u64
        };
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;

        // Phase 1: Readiness scan (Phase 110.A.b — backed by FifoReadySet).
        //
        // `bits` carries data-readiness only (used by trigger eval +
        // by `InvocationMode::OnNewData`). `always_mask` carries the
        // `InvocationMode::Always` entries that fire regardless of
        // data presence. The dispatcher drains
        // `FifoReadySet(bits | always_mask)` after the trigger
        // passes; `pop_next` yields registration order (lowest bit
        // first) so behavior is bit-identical to the pre-refactor
        // `for (i, meta) in entries.iter().enumerate()` loop.
        let mut bits: u64 = 0;
        let mut count: usize = 0;
        let mut non_timer_mask: u64 = 0;
        let mut always_mask: u64 = 0;

        for (i, meta) in self.entries.iter().enumerate() {
            if let Some(meta) = meta {
                let data_ptr = unsafe { arena_ptr.add(meta.offset) as *const u8 };
                if unsafe { (meta.has_data)(data_ptr) } {
                    bits |= 1u64 << i;
                }
                if !matches!(meta.kind, EntryKind::Timer | EntryKind::GuardCondition) {
                    non_timer_mask |= 1u64 << i;
                }
                if matches!(meta.invocation, InvocationMode::Always) {
                    always_mask |= 1u64 << i;
                }
                count += 1;
            }
        }

        let snapshot = ReadinessSnapshot { bits, count };

        // Phase 2: Trigger evaluation
        let trigger_passes = match &self.trigger {
            Trigger::Any => bits & non_timer_mask != 0 || non_timer_mask == 0,
            Trigger::All => bits & non_timer_mask == non_timer_mask,
            Trigger::One(id) => snapshot.is_ready(*id),
            Trigger::AllOf(set) => snapshot.all_ready(*set),
            Trigger::AnyOf(set) => snapshot.any_ready(*set),
            Trigger::Always => true,
            Trigger::Predicate(f) => f(&snapshot),
            Trigger::RawPredicate { callback, context } => {
                // Convert ReadinessSnapshot bitmask to a bool array for the C callback
                let mut ready_array = [false; 64];
                for (i, slot) in ready_array
                    .iter_mut()
                    .enumerate()
                    .take(snapshot.count.min(64))
                {
                    *slot = snapshot.bits & (1u64 << i) != 0;
                }
                // SAFETY: The callback and context are provided by the C API caller.
                // The ready_array is valid for snapshot.count elements.
                unsafe { callback(ready_array.as_ptr(), snapshot.count, *context) }
            }
        };

        if !trigger_passes {
            // Timers still need delta accumulation even when trigger doesn't pass
            for meta in self.entries.iter().flatten() {
                if matches!(meta.kind, EntryKind::Timer) {
                    let data_ptr = unsafe { arena_ptr.add(meta.offset) };
                    let _ = unsafe { (meta.try_process)(data_ptr, delta_ms) };
                }
            }

            // Parameter services live outside the arena and must be processed
            // regardless of trigger state, otherwise ROS 2 param queries time out.
            #[cfg(feature = "param-services")]
            if let Some(params) = &mut self.params {
                {
                    let crate::parameter_services::ParamState {
                        server, services, ..
                    } = &mut **params;
                    let _ = services.process_services(server);
                }
                // Phase 172.H — persist any runtime override applied this tick.
                crate::parameter_services::flush_param_store(params);
            }

            // Same treatment for lifecycle services — `ros2 lifecycle get`
            // must succeed even when no callbacks fired this tick.
            // SAFETY: see the matching invariant on the later call site.
            #[cfg(feature = "lifecycle-services")]
            if let Some(lc) = &mut self.lifecycle {
                let crate::lifecycle_services::LifecycleRuntimeState {
                    state_machine,
                    services,
                } = &mut **lc;
                let _ = unsafe { services.process_services(state_machine) };
            }

            return SpinOnceResult::new();
        }

        // Phase 2.5: LET pre-sample (only when LogicalExecutionTime)
        //
        // Sample all subscription data into entry buffers BEFORE dispatching
        // any callbacks. This ensures all callbacks in this cycle see a
        // consistent snapshot of data from the same point in time.
        // Services are NOT pre-sampled (request-reply is sequential).
        if matches!(self.semantics, ExecutorSemantics::LogicalExecutionTime) {
            for meta in self.entries.iter().flatten() {
                if matches!(meta.kind, EntryKind::Subscription) {
                    let data_ptr = unsafe { arena_ptr.add(meta.offset) };
                    unsafe { (meta.pre_sample)(data_ptr) };
                }
            }
        }

        // Phase 3: Dispatch (Phase 110.C — bucketed by SC.priority).
        //
        // Two ready-set families, each split across `Priority::COUNT`
        // buckets (Critical / Normal / BestEffort). Per-entry SC
        // `class` selects FIFO bitmap vs EDF heap; SC `priority`
        // selects the bucket within. Drain order:
        //   for each bucket in priority order (Critical first):
        //     drain EDF heap (deadline-priority), then FIFO bitmap
        //     (registration-order)
        // Default workloads — every entry on the auto-default Fifo SC
        // (Normal priority) — populate only `fifo[Normal]`, so
        // dispatch order is bit-identical to 110.B.b for those.
        const NB: usize = super::sched_context::Priority::COUNT;
        let mut result = SpinOnceResult::new();
        let mut fifo: super::ready_set::BucketedFifoSet<NB, { crate::config::MAX_CBS }> =
            super::ready_set::BucketedFifoSet::new();
        let mut edf: super::ready_set::BucketedEdfSet<NB, { crate::config::MAX_CBS }> =
            super::ready_set::BucketedEdfSet::new();
        let active_mask = bits | always_mask;

        // Phase 110.E — refill any Sporadic SC budgets at period
        // boundaries before deciding what to dispatch this cycle.
        // Refill is polled (not ISR-driven) — coarse but correct
        // upper-bound bandwidth limiter.
        #[cfg(feature = "std")]
        {
            // Monotonic ms relative to a process-static epoch so the
            // refill clock survives wall-clock jumps.
            use std::sync::OnceLock;
            static EPOCH: OnceLock<std::time::Instant> = OnceLock::new();
            let now_ms = std::time::Instant::now()
                .saturating_duration_since(*EPOCH.get_or_init(std::time::Instant::now))
                .as_millis() as u64;
            // Use the cycle's `delta_ms` as the per-SC consumption
            // estimate — worst-case attribution. Per-callback
            // measurement lands with a higher-precision clock hook.
            let delta_us = (delta_ms as u32).saturating_mul(1000);
            for slot in self.sporadic_states.iter_mut().flatten() {
                let _ = slot.tick(now_ms, delta_us);
            }
        }

        for i in 0..crate::config::MAX_CBS {
            if active_mask & (1u64 << i) == 0 {
                continue;
            }
            let sc_idx = self.sched_context_bindings[i].0 as usize;
            let sc_class_priority_deadline = self
                .sched_contexts
                .get(sc_idx)
                .and_then(|s| s.as_ref())
                .map(|sc| {
                    (
                        sc.class,
                        sc.priority.index(),
                        sc.deadline_us.get().map(|nz| nz.get()).unwrap_or(u32::MAX),
                    )
                });
            let (sc_class, bucket, deadline_us) = sc_class_priority_deadline.unwrap_or((
                super::sched_context::SchedClass::Fifo,
                super::sched_context::Priority::Normal.index(),
                u32::MAX,
            ));
            // Phase 110.E — Sporadic SC dispatch is suppressed when
            // its budget is exhausted. Atomic path (110.E.b PlatformTimer
            // refill) takes precedence when registered; polled path
            // (cycle-level delta_us attribution) handles the unregistered
            // case. Either way, exhausted budget skips dispatch.
            if matches!(sc_class, super::sched_context::SchedClass::Sporadic) {
                #[cfg(feature = "alloc")]
                let atomic_has_budget = self
                    .sporadic_atomic_states
                    .get(sc_idx)
                    .and_then(|s| s.as_ref())
                    .map(|(state, _)| state.has_budget());
                #[cfg(not(feature = "alloc"))]
                let atomic_has_budget: Option<bool> = None;
                let has_budget = match atomic_has_budget {
                    Some(b) => b,
                    None => self
                        .sporadic_states
                        .get(sc_idx)
                        .and_then(|s| s.as_ref())
                        .map(|s| s.budget_remaining_us > 0)
                        .unwrap_or(true),
                };
                if !has_budget {
                    continue;
                }
                // Phase 110.E.b follow-up — per-callback runtime
                // accounting (replaces this cycle-level attribution)
                // is applied at dispatch time below via
                // `consume_dispatch_runtime_us`. We only update the
                // polled-path `SporadicState` (no_std fallback) here
                // because the atomic path now records actual
                // wall-clock per-callback runtime. The
                // `delta_us` over-attribution that previously hit the
                // atomic state was a worst-case bandwidth limiter;
                // per-callback measurement is strictly tighter.
                #[cfg(not(feature = "alloc"))]
                {
                    let _ = sc_idx; // polled-state path lives in
                    // `sporadic_states`; this branch is a no-op when
                    // the atomic path is enabled.
                }
            }
            // Phase 110.F — per-callback OS priority routing. Entries
            // bound to an SC with `os_pri > 0` dispatch onto a worker
            // thread the OS has elevated to that priority; the
            // cooperative path is skipped for those entries. Workers
            // are spawned lazily.
            #[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
            {
                let os_pri = self
                    .sched_contexts
                    .get(sc_idx)
                    .and_then(|s| s.as_ref())
                    .map(|sc| sc.os_pri)
                    .unwrap_or(0);
                if os_pri > 0
                    && let Some(apply_policy) = self.os_priority_apply_policy
                {
                    let worker = self
                        .os_priority_workers
                        .entry(os_pri)
                        .or_insert_with(|| OsPriorityWorker::spawn(os_pri, apply_policy));
                    if let Some(meta) = self.entries[i].as_ref() {
                        let _ = worker.try_dispatch(WorkItem {
                            arena_base: arena_ptr as usize,
                            arena_offset: meta.offset,
                            try_process: meta.try_process,
                            delta_ms,
                        });
                    }
                    continue;
                }
            }
            // Phase 110.G — TT window gate, orthogonal to class.
            // Skips dispatch when the SC has a TT window AND the
            // current monotonic time is outside it. Both gates apply
            // independently — a Sporadic SC with a TT window must
            // pass both.
            if self.major_frame_us > 0 {
                let sc_opt = self.sched_contexts.get(sc_idx).and_then(|s| s.as_ref());
                if let Some(sc) = sc_opt {
                    let off = sc.tt_window_offset_us.get().map(|nz| nz.get()).unwrap_or(0);
                    let dur = sc
                        .tt_window_duration_us
                        .get()
                        .map(|nz| nz.get())
                        .unwrap_or(0);
                    if dur > 0 {
                        // Compute current phase within the major
                        // frame using the accumulated `delta_ms` clock
                        // (std-only precise; no_std uses `delta_ms`
                        // approximation from spin cadence).
                        #[cfg(feature = "std")]
                        let now_us = {
                            use std::sync::OnceLock;
                            static EPOCH: OnceLock<std::time::Instant> = OnceLock::new();
                            std::time::Instant::now()
                                .saturating_duration_since(
                                    *EPOCH.get_or_init(std::time::Instant::now),
                                )
                                .as_micros() as u64
                        };
                        #[cfg(not(feature = "std"))]
                        let now_us = delta_ms.saturating_mul(1000);
                        let phase = (now_us % self.major_frame_us as u64) as u32;
                        let in_window = if off + dur <= self.major_frame_us {
                            phase >= off && phase < off + dur
                        } else {
                            // Window wraps the major frame boundary.
                            let end = (off as u64 + dur as u64) % self.major_frame_us as u64;
                            phase >= off || (phase as u64) < end
                        };
                        if !in_window {
                            continue;
                        }
                    }
                }
            }
            let is_edf = matches!(sc_class, super::sched_context::SchedClass::Edf);
            let job = super::types::ActiveJob {
                sort_key: if is_edf { deadline_us } else { i as u32 },
                desc_idx: i as super::types::DescIdx,
            };
            if is_edf {
                let _ = edf.insert_into(bucket, job);
            } else {
                let _ = fifo.insert_into(bucket, job);
            }
        }

        // SAFETY: each `desc_idx` we pop was set above only when the
        // corresponding `entries[i]` slot was `Some`; no Executor
        // mutation happens between that scan and this dispatch.
        let dispatch_one = |meta: &CallbackMeta,
                            arena_ptr: *mut u8,
                            delta_ms: u64,
                            result: &mut SpinOnceResult| {
            // Phase 141.B.2 — capture T1 at subscription dispatch
            // entry. Probe pairs it with the most recent T0 from
            // `nros_rmw_runtime_wake_cb` (std + alloc variants)
            // and pushes `T1 - T0` onto the ring buffer 141.C
            // drains. No-op when the probe feature is off or
            // no cycle reader is installed. Other entry kinds
            // (Service / Timer / GuardCondition) skip the probe
            // because the 141 acceptance is specifically
            // wake-to-subscription-dispatch latency.
            #[cfg(feature = "wake-latency-probe")]
            if matches!(meta.kind, EntryKind::Subscription) {
                super::wake_probe::on_dispatch();
            }
            let data_ptr = unsafe { arena_ptr.add(meta.offset) };
            match unsafe { (meta.try_process)(data_ptr, delta_ms) } {
                Ok(true) => match meta.kind {
                    EntryKind::Subscription => result.subscriptions_processed += 1,
                    EntryKind::Service
                    | EntryKind::ServiceClient
                    | EntryKind::ActionServer
                    | EntryKind::ActionClient => result.services_handled += 1,
                    EntryKind::Timer => result.timers_fired += 1,
                    EntryKind::GuardCondition => {}
                },
                Ok(false) => {}
                Err(_) => match meta.kind {
                    EntryKind::Subscription => result.subscription_errors += 1,
                    EntryKind::Service
                    | EntryKind::ServiceClient
                    | EntryKind::ActionServer
                    | EntryKind::ActionClient => result.service_errors += 1,
                    EntryKind::Timer | EntryKind::GuardCondition => {}
                },
            }
        };

        // Phase 110.E.b follow-up — per-callback runtime accounting
        // for Sporadic SCs. Wall-clock-measure each dispatch and
        // consume the elapsed microseconds from the bound SC's
        // atomic budget. This replaces the cycle-level over-
        // attribution that previously charged the FULL `delta_us`
        // against every Sporadic SC regardless of which entries
        // actually fired — accurate per-callback measurement is the
        // shape the design doc's per-callback runtime acceptance
        // calls out. The closure is `feature = "std"`-gated because
        // it needs a `core::time::Instant`-equivalent monotonic
        // clock; the no_std fallback continues to use the polled
        // `SporadicState` path (cycle delta_us) until a board-side
        // monotonic-microsecond accessor lands.
        #[cfg(feature = "std")]
        let consume_dispatch_runtime_us =
            |desc_idx: usize,
             elapsed_us: u32,
             sched_context_bindings: &[super::sched_context::SchedContextId;
                  crate::config::MAX_CBS],
             sched_contexts: &[Option<super::sched_context::SchedContext>;
                  crate::config::MAX_SC],
             #[cfg(feature = "alloc")] sporadic_atomic_states: &[Option<(
                portable_atomic_util::Arc<super::sched_context::AtomicSporadicState>,
                OpaqueTimerHandle,
            )>;
                  crate::config::MAX_SC]| {
                let sc_idx = sched_context_bindings[desc_idx].0 as usize;
                let sc_class = sched_contexts
                    .get(sc_idx)
                    .and_then(|s| s.as_ref())
                    .map(|sc| sc.class)
                    .unwrap_or(super::sched_context::SchedClass::Fifo);
                if !matches!(sc_class, super::sched_context::SchedClass::Sporadic) {
                    return;
                }
                #[cfg(feature = "alloc")]
                if let Some((state, _)) =
                    sporadic_atomic_states.get(sc_idx).and_then(|s| s.as_ref())
                {
                    state.consume(elapsed_us);
                    // Phase 110.E.b — overrun detection. Cooperative
                    // single-thread can't preempt a runaway callback,
                    // so post-dispatch wall-clock comparison delivers
                    // the same observable signal as the design's
                    // oneshot-IRQ-and-cancel pattern, without needing
                    // a separate timer per SC. `budget_capacity_us` is
                    // the per-period budget the SC was sized against;
                    // any callback exceeding that has run past its
                    // bandwidth allotment.
                    if elapsed_us > state.budget_capacity_us {
                        state.record_overrun(elapsed_us - state.budget_capacity_us);
                    }
                }
                #[cfg(not(feature = "alloc"))]
                {
                    let _ = (sc_idx, elapsed_us);
                }
            };

        // For each priority bucket (Critical → Normal → BestEffort),
        // drain EDF first then FIFO so an EDF callback in this bucket
        // beats a FIFO peer at the same priority, but no lower-priority
        // entry runs while a higher-priority bucket has work pending.
        // Strict static priority across buckets; non-preemptive within
        // an in-flight callback (see Phase 110.D).
        for bucket in 0..NB {
            while let Some(job) = edf.pop_from(bucket) {
                let i = job.desc_idx as usize;
                if let Some(meta) = self.entries[i].as_ref() {
                    #[cfg(feature = "std")]
                    let start = std::time::Instant::now();
                    dispatch_one(meta, arena_ptr, delta_ms, &mut result);
                    #[cfg(feature = "std")]
                    {
                        let elapsed_us = start.elapsed().as_micros().min(u32::MAX as u128) as u32;
                        consume_dispatch_runtime_us(
                            i,
                            elapsed_us,
                            &self.sched_context_bindings,
                            &self.sched_contexts,
                            #[cfg(feature = "alloc")]
                            &self.sporadic_atomic_states,
                        );
                    }
                }
            }
            while let Some(job) = fifo.pop_from(bucket) {
                let i = job.desc_idx as usize;
                if let Some(meta) = self.entries[i].as_ref() {
                    #[cfg(feature = "std")]
                    let start = std::time::Instant::now();
                    dispatch_one(meta, arena_ptr, delta_ms, &mut result);
                    #[cfg(feature = "std")]
                    {
                        let elapsed_us = start.elapsed().as_micros().min(u32::MAX as u128) as u32;
                        consume_dispatch_runtime_us(
                            i,
                            elapsed_us,
                            &self.sched_context_bindings,
                            &self.sched_contexts,
                            #[cfg(feature = "alloc")]
                            &self.sporadic_atomic_states,
                        );
                    }
                }
            }
        }

        // Process parameter services (outside the arena)
        #[cfg(feature = "param-services")]
        if let Some(params) = &mut self.params {
            {
                let crate::parameter_services::ParamState {
                    server, services, ..
                } = &mut **params;
                if let Ok(n) = services.process_services(server) {
                    result.services_handled += n;
                }
            }
            // Phase 172.H — persist any runtime override applied this tick.
            crate::parameter_services::flush_param_store(params);
        }

        // Process lifecycle services (outside the arena).
        //
        // SAFETY: `change_state` dispatches a user-supplied C callback through a
        // raw function pointer stored in `LifecyclePollingNodeCtx`. The caller
        // of `register_lifecycle_services` guarantees the callback/context pair
        // stays live for as long as the executor (see that method's docs).
        #[cfg(feature = "lifecycle-services")]
        if let Some(lc) = &mut self.lifecycle {
            let crate::lifecycle_services::LifecycleRuntimeState {
                state_machine,
                services,
            } = &mut **lc;
            if let Ok(n) = unsafe { services.process_services(state_machine) } {
                result.services_handled += n;
            }
        }

        result
    }

    /// Drive I/O and dispatch callbacks in an infinite loop.
    ///
    /// Each iteration calls [`spin_once(timeout_ms)`](Self::spin_once),
    /// which pumps the transport and dispatches all registered callbacks.
    ///
    /// This is the primary run loop for embedded applications:
    ///
    /// ```ignore
    /// let mut executor = Executor::open(&config)?;
    /// executor.register_subscription::<Int32, _>("/topic", |msg| { /* ... */ })?;
    /// executor.spin(10); // never returns
    /// ```
    pub fn spin(&mut self, timeout: core::time::Duration) -> ! {
        loop {
            self.spin_once(timeout);
        }
    }

    /// Phase 104.C.3.3.c — rclcpp-`spin()`-shape no-arg variant.
    /// Defaults the per-iteration timeout to 50 ms, which keeps
    /// idle binaries from busy-spinning while staying responsive
    /// enough for default-QoS messaging.
    pub fn spin_default(&mut self) -> ! {
        self.spin(core::time::Duration::from_millis(50))
    }

    /// Drive I/O and dispatch callbacks asynchronously.
    ///
    /// Runs forever, yielding between poll cycles so that other async tasks
    /// (e.g., [`Promise`](super::handles::Promise)) can make progress.
    ///
    /// Uses only `core::future` — no external async runtime dependency.
    ///
    /// # Usage patterns
    ///
    /// ```ignore
    /// // Pattern 1: select with a promise (embassy-futures)
    /// use embassy_futures::select::{select, Either};
    /// let promise = client.call(&req)?;
    /// let Either::Second(reply) = select(executor.spin_async(), promise).await
    ///     else { unreachable!() };
    ///
    /// // Pattern 2: manual polling (no async runtime)
    /// let mut promise = client.call(&req)?;
    /// loop {
    ///     executor.spin_once(core::time::Duration::from_millis(10));
    ///     if let Ok(Some(r)) = promise.try_recv() { break r; }
    /// }
    /// ```
    pub async fn spin_async(&mut self) -> ! {
        loop {
            self.spin_once(core::time::Duration::from_millis(1));
            core::future::poll_fn::<(), _>(|cx| {
                cx.waker().wake_by_ref();
                core::task::Poll::Pending
            })
            .await;
        }
    }

    // ========================================================================
    // spin_one_period (no_std)
    // ========================================================================

    /// Process one iteration and return remaining sleep time.
    ///
    /// This is `no_std` compatible — the caller is responsible for the actual
    /// delay using platform-specific sleep.
    ///
    /// # Arguments
    /// * `period_ms` - Target period in milliseconds
    /// * `elapsed_ms` - Time elapsed since last call (used for timer ticking)
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     let r = executor.spin_one_period(10, elapsed_ms);
    ///     platform_sleep_ms(r.remaining_ms);
    /// }
    /// ```
    pub fn spin_one_period(&mut self, period_ms: u64, elapsed_ms: u64) -> SpinPeriodPollingResult {
        let result = self.spin_once(core::time::Duration::from_millis(elapsed_ms));
        SpinPeriodPollingResult {
            work: result,
            remaining_ms: period_ms.saturating_sub(elapsed_ms),
        }
    }
}

// ============================================================================
// Parameter services (cfg param-services)
// ============================================================================

#[cfg(feature = "param-services")]
impl Executor {
    /// Register the 6 ROS 2 parameter services for this node.
    ///
    /// Creates service servers for `get_parameters`, `set_parameters`,
    /// `set_parameters_atomically`, `list_parameters`, `describe_parameters`,
    /// and `get_parameter_types`.
    ///
    /// The service names follow the ROS 2 convention: `/{namespace}/{node_name}/{suffix}`.
    /// For the default namespace `/`, this becomes `/{node_name}/{suffix}` (e.g.
    /// `/sentinel/list_parameters`).
    ///
    /// Parameter services are stored outside the arena and don't consume
    /// callback slots.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = ExecutorConfig::from_env().node_name("talker");
    /// let mut executor = Executor::open(&config)?;
    /// executor.register_parameter_services()?;
    /// executor.declare_parameter("start_value", ParameterValue::Integer(0));
    /// ```
    pub fn register_parameter_services(&mut self) -> Result<(), NodeError> {
        use crate::parameter_services::{
            DescribeParameters, GetParameterTypes, GetParameters, ListParameters,
            PARAM_SERVICE_BUFFER_SIZE, ParameterServiceServers, SetParameters,
            SetParametersAtomically,
        };
        use nros_core::RosService;

        type PSrv<Svc> = super::handles::EmbeddedServiceServer<
            Svc,
            PARAM_SERVICE_BUFFER_SIZE,
            PARAM_SERVICE_BUFFER_SIZE,
        >;

        // Build the node FQN from namespace + node_name, following ROS 2 convention.
        // Default namespace "/" → "/{node_name}"; otherwise "/{namespace}/{node_name}".
        let mut node_fqn = heapless::String::<256>::new();
        let ns: &str = &self.namespace;
        let nn: &str = &self.node_name;
        if ns.is_empty() || ns == "/" {
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str(nn).map_err(|_| NodeError::NameTooLong)?;
        } else {
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn
                .push_str(ns.trim_matches('/'))
                .map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str(nn).map_err(|_| NodeError::NameTooLong)?;
        }

        /// Build a service name like `{node_fqn}/{suffix}` and create the server handle.
        fn create_param_srv<Svc: RosService>(
            session: &mut session::ConcreteSession,
            node_fqn: &str,
            namespace: &str,
            node_name: &str,
            suffix: &str,
        ) -> Result<session::RmwServiceServer, NodeError> {
            let mut name = heapless::String::<256>::new();
            name.push_str(node_fqn)
                .map_err(|_| NodeError::NameTooLong)?;
            name.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            name.push_str(suffix).map_err(|_| NodeError::NameTooLong)?;
            let mut info = ServiceInfo::new(&name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
                .with_namespace(namespace);
            if !node_name.is_empty() {
                info = info.with_node_name(node_name);
            }
            session
                .create_service_server(&info, QosSettings::services_default())
                .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))
        }

        let get_handle = create_param_srv::<GetParameters>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "get_parameters",
        )?;
        let set_handle = create_param_srv::<SetParameters>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "set_parameters",
        )?;
        let set_atomic_handle = create_param_srv::<SetParametersAtomically>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "set_parameters_atomically",
        )?;
        let list_handle = create_param_srv::<ListParameters>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "list_parameters",
        )?;
        let desc_handle = create_param_srv::<DescribeParameters>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "describe_parameters",
        )?;
        let types_handle = create_param_srv::<GetParameterTypes>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "get_parameter_types",
        )?;

        let servers = ParameterServiceServers::new(
            PSrv::<GetParameters> {
                handle: get_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<SetParameters> {
                handle: set_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<SetParametersAtomically> {
                handle: set_atomic_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<ListParameters> {
                handle: list_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<DescribeParameters> {
                handle: desc_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            PSrv::<GetParameterTypes> {
                handle: types_handle,
                req_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; PARAM_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
        );

        self.params = Some(alloc::boxed::Box::new(
            crate::parameter_services::ParamState {
                server: nros_params::ParameterServer::new(),
                services: alloc::boxed::Box::new(servers),
                store: alloc::boxed::Box::new(nros_params::NullParamStore),
            },
        ));

        Ok(())
    }

    /// Phase 172.H — attach a parameter-override persistence backend.
    ///
    /// Call this **after** [`register_parameter_services`](Self::register_parameter_services)
    /// and after declaring the plan's default parameters, so persisted
    /// overrides win over compile-time defaults. It immediately overlays any
    /// values the backend already holds onto the declared parameters, then
    /// keeps the store to flush future runtime `set_parameters` changes (the
    /// executor flushes from its spin loop whenever a value changed).
    ///
    /// Returns [`NodeError::NotInitialized`] if parameter services have not
    /// been registered yet.
    pub fn enable_parameter_persistence(
        &mut self,
        store: alloc::boxed::Box<dyn nros_params::ParamStore>,
    ) -> Result<(), NodeError> {
        let state = self.params.as_mut().ok_or(NodeError::NotInitialized)?;
        // Overlay persisted overrides onto the declared defaults.
        store.load(&mut |name, value| {
            let _ = state.server.set(name, value);
        });
        // Loading persisted state is restoration, not a new runtime change —
        // don't let it trigger an immediate re-flush.
        state.server.take_dirty();
        state.store = store;
        Ok(())
    }

    /// Phase 172.H — like [`enable_parameter_persistence`](Self::enable_parameter_persistence)
    /// but boxes the backend for you, so callers (and generated code) need no
    /// `Box` import.
    pub fn enable_parameter_persistence_with<S>(&mut self, store: S) -> Result<(), NodeError>
    where
        S: nros_params::ParamStore + 'static,
    {
        self.enable_parameter_persistence(alloc::boxed::Box::new(store))
    }
}

// ============================================================================
// Lifecycle services (cfg lifecycle-services)
// ============================================================================

#[cfg(feature = "lifecycle-services")]
impl Executor {
    /// Register the five REP-2002 lifecycle services on this executor.
    ///
    /// After this call, `ros2 lifecycle set|get|list|nodes` can drive the
    /// stored [`LifecyclePollingNodeCtx`](crate::lifecycle::LifecyclePollingNodeCtx)
    /// through the node's lifecycle. The state machine is created fresh
    /// (starting in `Unconfigured`); callers register their transition
    /// callbacks via [`Executor::lifecycle_state_machine_mut`].
    ///
    /// # Safety
    /// Registered callbacks on the state machine are C FFI function pointers.
    /// The caller must keep the callback code and any context it captures
    /// valid for as long as the executor processes services.
    pub fn register_lifecycle_services(&mut self) -> Result<(), NodeError> {
        use crate::{
            lifecycle::LifecyclePollingNodeCtx,
            lifecycle_services::{
                ChangeState, GetAvailableStates, GetAvailableTransitions, GetState,
                LIFECYCLE_SERVICE_BUFFER_SIZE, LifecycleRuntimeState, LifecycleServiceServers,
            },
        };
        use nros_core::RosService;

        type LcSrv<Svc> = super::handles::EmbeddedServiceServer<
            Svc,
            LIFECYCLE_SERVICE_BUFFER_SIZE,
            LIFECYCLE_SERVICE_BUFFER_SIZE,
        >;

        // Build the node FQN from namespace + node_name (same convention as
        // register_parameter_services).
        let mut node_fqn = heapless::String::<256>::new();
        let ns: &str = &self.namespace;
        let nn: &str = &self.node_name;
        if ns.is_empty() || ns == "/" {
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str(nn).map_err(|_| NodeError::NameTooLong)?;
        } else {
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn
                .push_str(ns.trim_matches('/'))
                .map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            node_fqn.push_str(nn).map_err(|_| NodeError::NameTooLong)?;
        }

        fn create_lc_srv<Svc: RosService>(
            session: &mut session::ConcreteSession,
            node_fqn: &str,
            namespace: &str,
            node_name: &str,
            suffix: &str,
        ) -> Result<session::RmwServiceServer, NodeError> {
            let mut name = heapless::String::<256>::new();
            name.push_str(node_fqn)
                .map_err(|_| NodeError::NameTooLong)?;
            name.push_str("/").map_err(|_| NodeError::NameTooLong)?;
            name.push_str(suffix).map_err(|_| NodeError::NameTooLong)?;
            let mut info = ServiceInfo::new(&name, Svc::SERVICE_NAME, Svc::SERVICE_HASH)
                .with_namespace(namespace);
            if !node_name.is_empty() {
                info = info.with_node_name(node_name);
            }
            session
                .create_service_server(&info, QosSettings::services_default())
                .map_err(|_| NodeError::Transport(TransportError::ServiceServerCreationFailed))
        }

        let cs_handle =
            create_lc_srv::<ChangeState>(&mut self.session, &node_fqn, ns, nn, "change_state")?;
        let gs_handle =
            create_lc_srv::<GetState>(&mut self.session, &node_fqn, ns, nn, "get_state")?;
        let gas_handle = create_lc_srv::<GetAvailableStates>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "get_available_states",
        )?;
        let gat_handle = create_lc_srv::<GetAvailableTransitions>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "get_available_transitions",
        )?;
        let gtg_handle = create_lc_srv::<GetAvailableTransitions>(
            &mut self.session,
            &node_fqn,
            ns,
            nn,
            "get_transition_graph",
        )?;

        let servers = LifecycleServiceServers::new(
            LcSrv::<ChangeState> {
                handle: cs_handle,
                req_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            LcSrv::<GetState> {
                handle: gs_handle,
                req_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            LcSrv::<GetAvailableStates> {
                handle: gas_handle,
                req_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            LcSrv::<GetAvailableTransitions> {
                handle: gat_handle,
                req_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
            LcSrv::<GetAvailableTransitions> {
                handle: gtg_handle,
                req_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                reply_buffer: [0u8; LIFECYCLE_SERVICE_BUFFER_SIZE],
                _phantom: core::marker::PhantomData,
            },
        );

        self.lifecycle = Some(alloc::boxed::Box::new(LifecycleRuntimeState {
            state_machine: LifecyclePollingNodeCtx::new(),
            services: alloc::boxed::Box::new(servers),
        }));

        Ok(())
    }

    /// Mutable access to the lifecycle state machine, if registered.
    ///
    /// Used to register transition callbacks before spinning and to read the
    /// current state from application code.
    pub fn lifecycle_state_machine_mut(
        &mut self,
    ) -> Option<&mut crate::lifecycle::LifecyclePollingNodeCtx> {
        self.lifecycle.as_mut().map(|lc| &mut lc.state_machine)
    }

    /// Immutable access to the lifecycle state machine, if registered.
    pub fn lifecycle_state_machine(&self) -> Option<&crate::lifecycle::LifecyclePollingNodeCtx> {
        self.lifecycle.as_ref().map(|lc| &lc.state_machine)
    }
}

// ============================================================================
// Parameter declaration API (cfg param-services)
// ============================================================================

#[cfg(feature = "param-services")]
impl Executor {
    /// Declare a parameter with a value. Returns `true` if successful.
    pub fn declare_parameter(&mut self, name: &str, value: nros_params::ParameterValue) -> bool {
        if let Some(params) = &mut self.params {
            params.server.declare(name, value)
        } else {
            false
        }
    }

    /// Declare a parameter with a value and descriptor. Returns `true` if successful.
    pub fn declare_parameter_with_descriptor(
        &mut self,
        name: &str,
        value: nros_params::ParameterValue,
        descriptor: nros_params::ParameterDescriptor,
    ) -> bool {
        if let Some(params) = &mut self.params {
            params
                .server
                .declare_with_descriptor(name, value, Some(descriptor))
        } else {
            false
        }
    }

    /// Get a parameter value by name.
    pub fn get_parameter(&self, name: &str) -> Option<&nros_params::ParameterValue> {
        self.params.as_ref()?.server.get(name)
    }

    /// Get an integer parameter value by name (convenience).
    pub fn get_parameter_integer(&self, name: &str) -> Option<i64> {
        self.params.as_ref()?.server.get_integer(name)
    }

    /// Get a reference to the parameter server (if registered).
    pub fn params(&self) -> Option<&nros_params::ParameterServer> {
        self.params.as_ref().map(|p| &p.server)
    }

    /// Get a mutable reference to the parameter server (if registered).
    pub fn params_mut(&mut self) -> Option<&mut nros_params::ParameterServer> {
        self.params.as_mut().map(|p| &mut p.server)
    }

    /// Create a typed parameter builder (rclrs-compatible API).
    ///
    /// Returns a [`ParameterBuilder`] for fluent parameter declaration with
    /// `.default()`, `.description()`, `.range()`, and terminal methods
    /// `.mandatory()`, `.optional()`, or `.read_only()`.
    ///
    /// Returns [`NodeError::NotInitialized`] if parameter services have
    /// not been registered yet — call [`register_parameter_services`]
    /// first.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let max_speed = executor.parameter::<f64>("max_speed")?
    ///     .default(25.0)
    ///     .description("Maximum velocity (m/s)")
    ///     .read_only()?;
    /// ```
    ///
    /// [`ParameterBuilder`]: nros_params::ParameterBuilder
    /// [`register_parameter_services`]: Self::register_parameter_services
    pub fn parameter<'a, T: nros_params::ParameterVariant>(
        &'a mut self,
        name: &'a str,
    ) -> Result<nros_params::ParameterBuilder<'a, T>, NodeError> {
        let server = self
            .params
            .as_mut()
            .map(|p| &mut p.server)
            .ok_or(NodeError::NotInitialized)?;
        Ok(nros_params::ParameterBuilder::new(server, name))
    }
}

// ============================================================================
// std-gated spin and halt methods
// ============================================================================

#[cfg(feature = "std")]
impl Executor {
    /// Blocking spin loop with configurable exit conditions.
    ///
    /// Runs until one of:
    /// - [`halt()`](Self::halt) is called (from another thread or signal handler)
    /// - Timeout expires (if set in options)
    /// - Max callbacks reached (if set in options)
    /// - `only_next` is true (single iteration)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Spin forever until halted
    /// executor.spin_blocking(SpinOptions::default())?;
    ///
    /// // Spin with 5-second timeout
    /// executor.spin_blocking(SpinOptions::new().timeout_ms(5000))?;
    ///
    /// // Single iteration
    /// executor.spin_blocking(SpinOptions::spin_once())?;
    /// ```
    pub fn spin_blocking(&mut self, opts: SpinOptions) -> Result<(), NodeError> {
        use std::time::{Duration, Instant};

        const POLL_INTERVAL: core::time::Duration = core::time::Duration::from_millis(10);

        let start = Instant::now();
        let timeout = opts.timeout_ms.map(Duration::from_millis);
        let mut total_callbacks = 0usize;

        self.halt_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);

        loop {
            if self.halt_flag.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }

            if timeout.is_some_and(|t| start.elapsed() >= t) {
                break;
            }

            let result = self.spin_once(POLL_INTERVAL);
            total_callbacks += result.total();

            if opts.max_callbacks.is_some_and(|max| total_callbacks >= max) {
                break;
            }

            if opts.only_next {
                break;
            }
        }

        Ok(())
    }

    /// Execute one period with wall-clock overrun detection.
    ///
    /// Calls [`spin_once()`](Self::spin_once), measures wall-clock time, sleeps
    /// for the remainder if under budget.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let period = std::time::Duration::from_millis(10);
    /// let result = executor.spin_one_period_timed(period);
    /// if result.overrun {
    ///     log::warn!("Period overrun: {:?}", result.elapsed);
    /// }
    /// ```
    pub fn spin_one_period_timed(
        &mut self,
        period: std::time::Duration,
    ) -> super::types::SpinPeriodResult {
        let start = std::time::Instant::now();
        let result = self.spin_once(period);
        let elapsed = start.elapsed();
        let overrun = elapsed > period;
        if !overrun {
            std::thread::sleep(period - elapsed);
        }
        super::types::SpinPeriodResult {
            work: result,
            overrun,
            elapsed,
        }
    }

    /// Spin at a fixed rate with drift compensation. Blocks until halted.
    ///
    /// Uses wall-clock time to maintain the target rate. The next invocation
    /// time is accumulated (not reset to `now + period`) to prevent cumulative
    /// drift.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // 100Hz control loop — blocks until halt() is called
    /// executor.spin_period(std::time::Duration::from_millis(10))?;
    /// ```
    pub fn spin_period(&mut self, period: std::time::Duration) -> Result<(), NodeError> {
        self.halt_flag
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let mut next_invocation = std::time::Instant::now() + period;

        loop {
            if self.halt_flag.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }

            self.spin_once(period);

            let now = std::time::Instant::now();
            if now < next_invocation {
                std::thread::sleep(next_invocation - now);
            }
            // Accumulate to prevent drift (not = now + period)
            next_invocation += period;
        }
        Ok(())
    }

    /// Request the executor to stop spinning.
    ///
    /// Sets a flag that causes [`spin_blocking()`](Self::spin_blocking) or
    /// [`spin_period()`](Self::spin_period) to exit on the next iteration.
    /// Safe to call from another thread or signal handler.
    ///
    /// Also raises the Phase 104.C.6 wake flag so a `spin_once` already
    /// blocked inside a backend's `drive_io` falls through to the halt
    /// check on its next loop iteration instead of waiting out its full
    /// `timeout_ms` first.
    pub fn halt(&self) {
        self.halt_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.wake_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Phase 110.D.b — move this Executor onto a fresh OS thread,
    /// apply a per-thread scheduling policy via the caller-supplied
    /// `apply_policy` function, and run the spin loop until
    /// [`ThreadHandle::halt`] fires.
    ///
    /// The function-pointer indirection on `apply_policy` lets the
    /// caller pass any platform's `PlatformScheduler::set_current_thread_policy`
    /// without forcing `Executor` to be generic over the platform —
    /// keeps the existing `Executor` type stable.
    ///
    /// Multi-executor preemption (the actual hard-RT win) comes from
    /// the OS scheduler — call `open_threaded` once per criticality
    /// tier, each with its own policy / priority. The kernel handles
    /// preemption across executors; within a single executor,
    /// dispatch remains non-preemptive (110.A–C bucketed sets).
    ///
    /// # Safety
    ///
    /// Moves `self` across thread boundaries. `Executor` contains a
    /// raw `*mut session::ConcreteSession` when constructed via
    /// `from_session_ptr`; the caller must ensure that pointer's
    /// referent stays valid across the lifetime of the spawned thread
    /// and that no other thread mutates the session concurrently.
    /// `from_session` (Owned) is safer — `ConcreteSession` ownership
    /// transfers cleanly into the thread.
    #[cfg(feature = "std")]
    pub unsafe fn open_threaded(
        self,
        policy: nros_platform_api::SchedPolicy,
        apply_policy: fn(
            nros_platform_api::SchedPolicy,
        ) -> Result<(), nros_platform_api::SchedError>,
        spin_period: core::time::Duration,
    ) -> ThreadHandle {
        let halt = std::sync::Arc::clone(&self.halt_flag);
        // SAFETY: Send is asserted via `unsafe impl Send for Executor`
        // below; the caller's safety contract on `from_session_ptr`
        // covers the pointer-validity invariant.
        let mut executor = self;
        let join = std::thread::spawn(move || {
            // Apply the requested OS scheduling policy to this fresh
            // thread. Failure is reported but not propagated — a
            // runtime that fails to lift to SCHED_FIFO still spins
            // correctly at SCHED_OTHER (just without RT guarantees).
            let _ = apply_policy(policy);
            while !executor.is_halted() {
                executor.spin_once(spin_period);
            }
        });
        ThreadHandle {
            join: Some(join),
            halt,
        }
    }

    /// Check if halt has been requested.
    pub fn is_halted(&self) -> bool {
        self.halt_flag.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get a clone of the halt flag for use in signal handlers or other threads.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let halt = executor.halt_flag();
    /// std::thread::spawn(move || {
    ///     std::thread::sleep(Duration::from_secs(5));
    ///     halt.store(true, Ordering::SeqCst);
    /// });
    /// executor.spin_blocking(SpinOptions::default())?;
    /// ```
    pub fn halt_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.halt_flag.clone()
    }

    /// Phase 104.C.6 — wake the executor from another thread / ISR /
    /// signal handler.
    ///
    /// Sets the shared `wake_flag`. The next `spin_once` swap-clears the
    /// flag, skips the blocking wait on the primary session, and polls
    /// every session non-blockingly so whatever queued the wake is
    /// observed in a single iteration. Idempotent — multiple `wake()`
    /// calls collapse into one observed wake per `spin_once`.
    pub fn wake(&self) {
        self.wake_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Phase 104.C.6 — clone of the shared wake flag for cross-thread
    /// use (signal handlers, foreign threads, future per-backend vtable
    /// wake hooks).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let wake = executor.wake_handle();
    /// std::thread::spawn(move || {
    ///     // ... compute something ...
    ///     // hand off to executor by setting the flag.
    ///     wake.store(true, Ordering::SeqCst);
    /// });
    /// loop { executor.spin_once(Duration::from_millis(100)); }
    /// ```
    pub fn wake_handle(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.wake_flag.clone()
    }
}

/// Phase 110.E.b — opaque per-platform timer handle. Stores the
/// raw platform handle (POSIX `timer_t` boxed via `PosixTimerHandle`,
/// FreeRTOS `TimerHandle_t`, etc.) plus a destroy thunk so the
/// Executor can clean up without being generic over the platform.
///
/// Caller of `register_sporadic_timer` builds this via
/// `OpaqueTimerHandle::new(handle, destroy_fn)` after their
/// `PlatformTimer::create_periodic` call returns.
#[cfg(feature = "alloc")]
pub struct OpaqueTimerHandle {
    handle: *mut core::ffi::c_void,
    destroy_fn: extern "C" fn(*mut core::ffi::c_void),
}

#[cfg(feature = "alloc")]
unsafe impl Send for OpaqueTimerHandle {}
#[cfg(feature = "alloc")]
unsafe impl Sync for OpaqueTimerHandle {}

#[cfg(feature = "alloc")]
impl OpaqueTimerHandle {
    /// # Safety
    /// `handle` must be a live platform-specific timer handle that
    /// `destroy_fn` knows how to drop. Caller surrenders ownership
    /// of the underlying handle to the Executor.
    pub unsafe fn new(
        handle: *mut core::ffi::c_void,
        destroy_fn: extern "C" fn(*mut core::ffi::c_void),
    ) -> Self {
        Self { handle, destroy_fn }
    }
}

#[cfg(feature = "alloc")]
impl Drop for OpaqueTimerHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            (self.destroy_fn)(self.handle);
            self.handle = core::ptr::null_mut();
        }
    }
}

/// Handle returned from [`Executor::open_threaded`]. Holds the
/// spawned thread's join handle and a clone of the executor's halt
/// flag. Drop runs `halt() + join()` so the thread can't outlive the
/// handle.
#[cfg(feature = "std")]
pub struct ThreadHandle {
    join: Option<std::thread::JoinHandle<()>>,
    halt: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(feature = "std")]
impl ThreadHandle {
    /// Signal the spawned executor thread to stop. The thread exits
    /// on its next `spin_once` iteration.
    pub fn halt(&self) {
        self.halt.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Wait for the spawned thread to exit. Returns the join result.
    /// After `join`, calling it again is a no-op (returns `Ok(())`).
    pub fn join(mut self) -> std::thread::Result<()> {
        self.halt();
        match self.join.take() {
            Some(j) => j.join(),
            None => Ok(()),
        }
    }
}

#[cfg(feature = "std")]
impl Drop for ThreadHandle {
    fn drop(&mut self) {
        self.halt.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

// SAFETY: Phase 110.D.b — `Executor` contains a raw `*mut
// session::ConcreteSession` only on the `from_session_ptr` (Borrowed)
// path; the `from_session` (Owned) path is plain Send-able. The
// `unsafe fn open_threaded` entry point documents the safety
// contract for Borrowed sessions; for Owned sessions the Send claim
// is unconditional.
#[cfg(feature = "std")]
unsafe impl Send for Executor {}

// =============================================================================
// Phase 110.F — `OsPriorityWorker` + `WorkItem`
// =============================================================================

/// One worker thread per distinct `SchedContext.os_pri` value used
/// across registered SCs. Self-elevates via the executor's stored
/// `apply_policy` fn pointer at startup; drains a bounded mpsc
/// mailbox of `WorkItem`s. Phase 110.F.
#[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
pub(crate) struct OsPriorityWorker {
    sender: std::sync::mpsc::Sender<WorkItem>,
    halt: std::sync::Arc<std::sync::atomic::AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

#[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
struct WorkItem {
    arena_base: usize,
    arena_offset: usize,
    try_process: unsafe fn(*mut u8, u64) -> Result<bool, nros_rmw::TransportError>,
    delta_ms: u64,
}

// SAFETY: Phase 110.F per-DescIdx exclusive-access invariant — the
// activator scan in `spin_once` only sends a `WorkItem` for a given
// `arena_offset` to one worker per cycle, and won't re-send the same
// offset until the worker drains the previous one (`os_pri` dispatch
// is the worker's exclusive path; cooperative dispatch is skipped
// for SCs with non-zero `os_pri`). The fn pointer is Send-clean.
#[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
unsafe impl Send for WorkItem {}

#[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
impl OsPriorityWorker {
    fn spawn(
        os_pri: u8,
        apply_policy: fn(
            nros_platform_api::SchedPolicy,
        ) -> Result<(), nros_platform_api::SchedError>,
    ) -> Self {
        use std::sync::atomic::Ordering;
        let (tx, rx) = std::sync::mpsc::channel::<WorkItem>();
        let halt = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let halt_w = std::sync::Arc::clone(&halt);
        let join = std::thread::Builder::new()
            .name(alloc::format!("nros-os-pri-{os_pri}"))
            .spawn(move || {
                // Self-elevate. Failure is logged but doesn't stop
                // the worker — running at SCHED_OTHER is still
                // correct, just without the priority guarantee.
                let _ = apply_policy(nros_platform_api::SchedPolicy::Fifo { os_pri });
                while !halt_w.load(Ordering::Acquire) {
                    match rx.recv_timeout(core::time::Duration::from_millis(10)) {
                        Ok(item) => {
                            // SAFETY: arena_base + arena_offset point
                            // into the executor's arena, which
                            // outlives the worker per Drop ordering
                            // (Executor::Drop halts + joins workers
                            // before the arena is freed).
                            let data = (item.arena_base as *mut u8).wrapping_add(item.arena_offset);
                            let _ = unsafe { (item.try_process)(data, item.delta_ms) };
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            // SAFETY-invariant: spawn failure means the OS refused a new
            // thread (resource exhaustion). This runs once per priority
            // level at lazy worker setup — not on any hot/spin path — and
            // a runtime that cannot create its priority worker has no
            // correct way to continue, so fail fast at the setup point.
            .expect("os-priority worker spawn");
        Self {
            sender: tx,
            halt,
            join: Some(join),
        }
    }

    fn try_dispatch(&self, item: WorkItem) -> bool {
        self.sender.send(item).is_ok()
    }
}

#[cfg(all(feature = "std", feature = "scheduler-os-priority"))]
impl Drop for OsPriorityWorker {
    fn drop(&mut self) {
        self.halt.store(true, std::sync::atomic::Ordering::Release);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;
        for meta in self.entries.iter().flatten() {
            // SAFETY: each entry was written by `ptr::write` in `add_*` and
            // has not been dropped yet. `drop_fn` matches the concrete type.
            unsafe {
                let data_ptr = arena_ptr.add(meta.offset);
                (meta.drop_fn)(data_ptr);
            }
        }
    }
}

#[cfg(all(test, not(feature = "rmw-cffi")))]
mod dispatch_registry_tests {
    //! Phase 216 follow-up — `Executor::register_dispatch_slot` +
    //! `Executor::dispatch_callback` round-trip.
    //!
    //! Uses `MockSession` (same pattern as
    //! `lifecycle_services::tests::mock_integration`) so the test
    //! doesn't need a live RMW backend. Gated `not(feature =
    //! "rmw-cffi")` because under `rmw-cffi` the `ConcreteSession`
    //! type alias resolves to the cffi session, which `MockSession`
    //! can't impersonate.

    extern crate alloc;

    use super::Executor;
    use crate::mock::MockSession;
    use std::sync::Mutex;

    static CAPTURED: Mutex<alloc::vec::Vec<(usize, alloc::vec::Vec<u8>, usize)>> =
        Mutex::new(alloc::vec::Vec::new());

    /// Test trampoline matching the per-Node
    /// `__nros_node_<pkg>_on_callback` ABI shape (Phase 216.A.5).
    unsafe extern "C" fn recording_on_callback(
        state: *mut core::ffi::c_void,
        cb_id_ptr: *const u8,
        cb_id_len: usize,
        ctx: *mut core::ffi::c_void,
    ) {
        // SAFETY: caller (test body below) holds storage live;
        // `cb_id_ptr..len` points into a `&str` literal.
        let cb_id_bytes = unsafe { core::slice::from_raw_parts(cb_id_ptr, cb_id_len).to_vec() };
        let mut guard = CAPTURED.lock().expect("CAPTURED poisoned");
        guard.push((state as usize, cb_id_bytes, ctx as usize));
    }

    #[test]
    fn register_dispatch_slot_round_trip() {
        let session = MockSession::new();
        let mut executor: Executor = Executor::from_session(session);

        // Pre-condition: empty registry.
        assert_eq!(executor.dispatch_slot_count(), 0);

        // Two distinct "states" so we prove every slot gets called
        // with its OWN state.
        let mut state_blob_a: u32 = 0xABCD_0001;
        let mut state_blob_b: u32 = 0xABCD_0002;
        let state_a_ptr = &mut state_blob_a as *mut u32 as *mut core::ffi::c_void;
        let state_b_ptr = &mut state_blob_b as *mut u32 as *mut core::ffi::c_void;

        executor
            .register_dispatch_slot(state_a_ptr, recording_on_callback)
            .expect("register slot A");
        executor
            .register_dispatch_slot(state_b_ptr, recording_on_callback)
            .expect("register slot B");
        assert_eq!(executor.dispatch_slot_count(), 2);

        let mut ctx_blob: u32 = 0xFEED_BEEF;
        let ctx_ptr = &mut ctx_blob as *mut u32 as *mut core::ffi::c_void;
        let cb_id = "/talker/timer/publish";

        CAPTURED.lock().expect("CAPTURED poisoned").clear();
        executor.dispatch_callback(cb_id, ctx_ptr);

        let captured = CAPTURED.lock().expect("CAPTURED poisoned").clone();
        assert_eq!(
            captured.len(),
            2,
            "every registered slot must be invoked — linear scan, \
             no self-filter at the registry layer"
        );
        // heapless::Vec iterates in insertion order.
        assert_eq!(captured[0].0, state_a_ptr as usize, "slot A's state");
        assert_eq!(captured[1].0, state_b_ptr as usize, "slot B's state");
        for (idx, capture) in captured.iter().enumerate() {
            assert_eq!(
                capture.1.as_slice(),
                cb_id.as_bytes(),
                "slot {idx} cb_id bytes round-trip"
            );
            assert_eq!(
                capture.2, ctx_ptr as usize,
                "slot {idx} ctx pointer round-trip"
            );
        }
    }

    #[test]
    fn register_dispatch_slot_capacity_full() {
        let session = MockSession::new();
        let mut executor: Executor = Executor::from_session(session);

        let mut state_blob: u32 = 0;
        let state_ptr = &mut state_blob as *mut u32 as *mut core::ffi::c_void;

        // `MAX_NODES` slots fit; the next one must error.
        for _ in 0..crate::config::MAX_NODES {
            executor
                .register_dispatch_slot(state_ptr, recording_on_callback)
                .expect("under-capacity push must succeed");
        }
        assert_eq!(executor.dispatch_slot_count(), crate::config::MAX_NODES);
        let overflow = executor.register_dispatch_slot(state_ptr, recording_on_callback);
        assert!(
            overflow.is_err(),
            "over-capacity push must return Err(()) — raise \
             NROS_EXECUTOR_MAX_NODES at build time to grow the registry"
        );
    }
}
