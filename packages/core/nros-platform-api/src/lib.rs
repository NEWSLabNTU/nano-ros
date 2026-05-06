//! Platform capability sub-traits for nros.
//!
//! This crate exists to break the dependency cycle between
//! `nros-platform` (which depends on every platform crate via its
//! feature-gated `ConcretePlatform` resolver) and the platform crates
//! themselves (which need to implement these traits on their ZSTs).
//! It contains only trait definitions — no implementations, no
//! dependencies — so platform crates can take a build-time dep on it
//! without creating a cycle back through `nros-platform`.
//!
//! `nros-platform` re-exports everything from this crate, so downstream
//! code that writes `use nros_platform::PlatformClock;` continues to
//! work unchanged.
//!
//! Each trait covers an independent system capability. Platform
//! implementations pick which traits to implement based on what the
//! hardware/RTOS provides. RMW shim crates declare trait bounds for
//! the capabilities they need.

#![no_std]
//!
//! # Naming convention
//!
//! Method names drop redundant prefixes when the trait name already
//! supplies the namespace — e.g., `PlatformTcp::open` rather than
//! `PlatformTcp::tcp_open`. Dispatch is always through a qualified
//! path (`<ConcretePlatform as PlatformTcp>::open(...)`), so
//! trait-to-trait collisions (PlatformTcp::open vs PlatformUdp::open)
//! are disambiguated at the call site without needing a prefix on the
//! trait method itself.
//!
//! Three categories still keep sub-namespace prefixes internally:
//!
//! * `PlatformThreading` — `mutex_*`, `condvar_*`, `task_*` because the
//!   trait bundles three independent primitive families and unprefixed
//!   `init` / `drop` would be ambiguous *within* the trait itself.
//! * `PlatformUdpMulticast` — `mcast_*` because these methods have
//!   different signatures from `PlatformUdp`'s same-name methods; keeping
//!   the prefix makes call sites that use both traits self-documenting.
//! * The `close` method appears on both `PlatformTcp` and
//!   `PlatformSocketHelpers` — the first is TCP teardown, the second is
//!   zenoh-pico's generic "shutdown + close" helper. Both live unprefixed
//!   in their respective traits; call sites disambiguate via the
//!   qualified path.
//!
//! # Status (Phase 84.F4)
//!
//! The platform ZSTs (`PosixPlatform`, `ZephyrPlatform`, etc.) do **not**
//! currently implement these traits — every platform exposes its methods
//! as *inherent* `impl Platform { fn foo() {} }` blocks, and shim crates
//! dispatch by name match. 84.F4 migrates each platform to `impl
//! PlatformX for Platform { fn foo() {} }` one trait at a time, with the
//! shims switching to `<P as PlatformX>::foo()`. Until that work is
//! complete the traits here are a target specification.

use core::ffi::{c_int, c_void};

pub mod xorshift32;

// ============================================================================
// Clock (required by all RMW backends)
// ============================================================================

/// Monotonic clock.
///
/// The most critical platform primitive. Must be backed by a hardware timer
/// or OS tick — never by a software counter that only advances when polled.
pub trait PlatformClock {
    /// Returns monotonic time in milliseconds.
    fn clock_ms() -> u64;

    /// Returns monotonic time in microseconds.
    fn clock_us() -> u64;
}

// ============================================================================
// Heap allocation (zenoh-pico requires ~64 KB heap)
// ============================================================================

/// Heap memory allocation.
pub trait PlatformAlloc {
    /// Allocate `size` bytes. Returns null on failure.
    fn alloc(size: usize) -> *mut c_void;

    /// Reallocate a previously allocated block. Returns null on failure.
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void;

    /// Free a previously allocated block.
    fn dealloc(ptr: *mut c_void);
}

// ============================================================================
// Sleep / delay
// ============================================================================

/// Sleep primitives.
///
/// On bare-metal with smoltcp, implementations should poll the network
/// stack during busy-wait sleep to avoid missing packets.
pub trait PlatformSleep {
    /// Sleep for the given number of microseconds.
    fn sleep_us(us: usize);

    /// Sleep for the given number of milliseconds.
    fn sleep_ms(ms: usize);

    /// Sleep for the given number of seconds.
    fn sleep_s(s: usize);
}

// ============================================================================
// Cooperative yield
// ============================================================================

/// Scheduler yield primitive.
///
/// Used inside `socket_wait_event` and similar "let another task make
/// progress" points where the caller isn't actually waiting for I/O
/// readability — the background read task already owns that — it just
/// needs to relinquish the CPU so the real waiter can run.
///
/// Prior to Phase 77.22 each backend hand-rolled its own 1-ms busy
/// sleep (`libc::usleep(1000)`, `vTaskDelay(1)`, `tx_thread_sleep(1)`,
/// `k_usleep(1000)`, `select(.., 1 ms)`) — all with slightly different
/// units and no common home.
///
/// **ISR-safety**: on the hosted-RTOS backends (FreeRTOS / NuttX /
/// Zephyr / ThreadX) the underlying primitives panic or error when
/// invoked from an ISR. Don't call `yield_now()` from an interrupt
/// handler.
///
/// **Bare-metal has no real yield**: there's nothing to yield *to*.
/// The default bare-metal impl is `core::hint::spin_loop()` (a pure
/// CPU hint: emits `YIELD` / `PAUSE` / `WFE` depending on the arch,
/// safe everywhere). Board crates that have armed an IRQ source may
/// opt in to deep idle (`wfi`) via a separate `BoardIdle` hook — not
/// part of this trait because calling `wfi` without an IRQ source
/// deadlocks.
pub trait PlatformYield {
    /// Relinquish the CPU long enough for another task / thread to run.
    ///
    /// Non-blocking in the sleep sense — returns as soon as the
    /// scheduler has had an opportunity to pick a different runnable.
    fn yield_now();
}

// ============================================================================
// Phase 110.D — `PlatformScheduler` (per-thread OS scheduling policy)
// ============================================================================

/// Errors returned by [`PlatformScheduler`] entry points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedError {
    /// The active platform doesn't expose this control surface
    /// (e.g. bare-metal with no scheduler, or an RTOS without an
    /// affinity API).
    Unsupported,
    /// The requested policy is valid for this platform but the
    /// numeric arguments fall outside the platform's accepted range.
    OutOfRange,
    /// A platform-specific syscall / kernel call failed; check
    /// `errno` (POSIX) or the RTOS error code for details.
    KernelError,
}

/// Per-thread OS scheduling policy.
///
/// `Fifo` / `RoundRobin` / `Deadline` / `Sporadic` map to platform-
/// native scheduling classes when available. `Platform(...)` is the
/// escape hatch for RTOS-specific knobs (e.g. ThreadX preempt-
/// threshold) that don't map cleanly into the portable variants.
///
/// User-facing API (`Executor::open_threaded`) takes the abstract
/// [`Priority`-like](crate) values and `PlatformScheduler` translates
/// them into platform-native numerics — direction-flipped priority
/// (Zephyr / ThreadX low-numeric = high-priority vs POSIX / FreeRTOS
/// high-numeric = high-priority) handled internally. Phase 110.D.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedPolicy {
    /// POSIX `SCHED_FIFO`. `os_pri` is the platform-native numeric
    /// priority; out-of-range values surface `SchedError::OutOfRange`.
    Fifo { os_pri: u8 },
    /// POSIX `SCHED_RR` with `quantum_ms` time slice.
    RoundRobin { os_pri: u8, quantum_ms: u32 },
    /// Linux `SCHED_DEADLINE` (sched_setattr). All values nanoseconds.
    Deadline {
        runtime_ns: u64,
        period_ns: u64,
        deadline_ns: u64,
    },
    /// NuttX `SCHED_SPORADIC` server. Phase 110.E.
    Sporadic {
        budget_us: u32,
        period_us: u32,
        hi_pri: u8,
        lo_pri: u8,
    },
}

/// Per-thread OS-scheduling control surface.
///
/// Each platform implements as much of this trait as its kernel
/// supports. `bare-metal` returns [`SchedError::Unsupported`] from
/// every entry point — there is no scheduler to talk to.
///
/// Phase 110.D wires this for Linux + NuttX (via POSIX) at v1; Zephyr
/// / FreeRTOS / ThreadX impls land alongside per-RTOS bring-up.
pub trait PlatformScheduler {
    /// Apply the requested policy to the calling thread.
    ///
    /// Default returns [`SchedError::Unsupported`] so single-core bare-
    /// metal targets and RTOS impls without per-thread scheduling
    /// pickup the no-op behavior automatically. Platforms with a
    /// real scheduler override this.
    fn set_current_thread_policy(_p: SchedPolicy) -> Result<(), SchedError> {
        Err(SchedError::Unsupported)
    }

    /// Cooperative yield. Same semantics as
    /// [`PlatformYield::yield_now`]; mirrored here so consumers don't
    /// need to import both traits when only the scheduler control
    /// surface is in scope. Default is a `core::hint::spin_loop()`.
    fn yield_now() {
        core::hint::spin_loop();
    }

    /// Pin the calling thread to the CPUs whose bit is set in
    /// `cpu_mask`. Default returns [`SchedError::Unsupported`] —
    /// platforms with affinity APIs override.
    fn set_affinity(_cpu_mask: u32) -> Result<(), SchedError> {
        Err(SchedError::Unsupported)
    }
}

// ============================================================================
// Random number generation
// ============================================================================

/// Pseudo-random number generation.
///
/// A simple xorshift32 PRNG is sufficient. Seed with hardware entropy
/// (RNG peripheral, ADC noise, wall-clock time) during platform init.
pub trait PlatformRandom {
    fn random_u8() -> u8;
    fn random_u16() -> u16;
    fn random_u32() -> u32;
    fn random_u64() -> u64;

    /// Fill buffer with random bytes.
    fn random_fill(buf: *mut c_void, len: usize);
}

// ============================================================================
// Wall-clock time (for logging, not timing-critical)
// ============================================================================

/// Wall-clock / system time.
///
/// Used for logging timestamps and `z_time_now_as_str()`.
/// On bare-metal without an RTC, return monotonic time or zeros.
///
/// The two-function `time_since_epoch_*` split (instead of returning a
/// struct) was chosen to match the shape that zenoh-pico's C headers
/// want across the FFI boundary — zpico-platform-shim forwards each
/// of these directly to a `_z_time_*` symbol, so collapsing them into
/// a Rust struct would require the shim to decompose the struct on
/// every call.
pub trait PlatformTime {
    /// Returns system time in milliseconds.
    fn time_now_ms() -> u64;

    /// Seconds component of wall-clock time since the Unix epoch.
    fn time_since_epoch_secs() -> u32;

    /// Sub-second nanoseconds component of wall-clock time since the
    /// Unix epoch (i.e. the nanosecond remainder after the seconds are
    /// stripped; always in `0..1_000_000_000`).
    fn time_since_epoch_nanos() -> u32;
}

// ============================================================================
// Threading (multi-threaded platforms only)
// ============================================================================
//
// Handle types are opaque `*mut c_void` to match the shape zenoh-pico
// passes across the FFI boundary. The original draft had typed wrappers
// (`TaskHandle`, `MutexHandle`, ...) but every shipped platform used
// `*mut c_void` internally and the shim never materialised the typed
// forms — keeping them in the trait would have required a pointless
// cast at every impl site. (F4.1 / F4.5 decision, 2026-04-24.)
//
// Mutex / condvar / task method names keep their sub-namespace prefix
// (`mutex_*`, `condvar_*`, `task_*`) because the trait bundles three
// independent primitive families and unprefixed `init` / `drop` would
// be ambiguous *within* the trait itself.

/// Threading primitives: tasks, mutexes, and condition variables.
///
/// # Threading
///
/// Mutex / condvar operations must be safe under concurrent callers.
/// **Recursive mutex (`mutex_rec_*`) must support same-thread
/// re-entrancy** — zenoh-pico relies on this; a non-recursive mutex
/// backing `mutex_rec_*` deadlocks under load.
///
/// # ISR-safety
///
/// **None of these methods are ISR-safe** on hosted RTOSes
/// (FreeRTOS / NuttX / Zephyr / ThreadX) — the underlying primitives
/// panic or error when invoked from an ISR. Only `core::hint::spin_loop()`
/// in [`PlatformYield::yield_now`] is.
///
/// For single-threaded platforms (bare-metal), all operations should
/// be no-ops returning `0`, except [`task_init`](Self::task_init)
/// which should return `-1`.
pub trait PlatformThreading {
    // -- Tasks --

    /// Spawn a new task. `task` is opaque caller-provided storage;
    /// `attr` carries scheduling hints (priority, stack size) or
    /// `null` for defaults; `entry` is the task entry point; `arg`
    /// is forwarded to `entry`. Returns `0` on success, non-zero
    /// on failure.
    fn task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8;

    /// Block until `task` exits. Cleans up the task storage on
    /// success.
    fn task_join(task: *mut c_void) -> i8;
    /// Mark `task` as detached — its storage is reclaimed on exit
    /// without a join.
    fn task_detach(task: *mut c_void) -> i8;
    /// Request `task` to terminate at the next cancellation point.
    /// Cooperative.
    fn task_cancel(task: *mut c_void) -> i8;
    /// Terminate the calling task immediately. Does not return.
    fn task_exit();
    /// Free the task storage allocated by `task_init`.
    fn task_free(task: *mut *mut c_void);

    // -- Mutex --

    /// Initialise a non-recursive mutex in caller-provided storage.
    fn mutex_init(m: *mut c_void) -> i8;
    /// Tear down a non-recursive mutex.
    fn mutex_drop(m: *mut c_void) -> i8;
    /// Lock; block if held.
    fn mutex_lock(m: *mut c_void) -> i8;
    /// Try to lock; non-zero return immediately if held.
    fn mutex_try_lock(m: *mut c_void) -> i8;
    /// Unlock; only the owning thread may call this.
    fn mutex_unlock(m: *mut c_void) -> i8;

    // -- Recursive mutex --

    /// Initialise a *recursive* mutex (same-thread re-entrancy
    /// permitted). Required by zenoh-pico.
    fn mutex_rec_init(m: *mut c_void) -> i8;
    /// Tear down a recursive mutex.
    fn mutex_rec_drop(m: *mut c_void) -> i8;
    /// Lock; re-entry from the owning thread must succeed.
    fn mutex_rec_lock(m: *mut c_void) -> i8;
    /// Try to lock; same re-entry semantics as `mutex_rec_lock`.
    fn mutex_rec_try_lock(m: *mut c_void) -> i8;
    /// Unlock; releases when the lock count returns to zero.
    fn mutex_rec_unlock(m: *mut c_void) -> i8;

    // -- Condition variables --

    /// Initialise a condition variable in caller-provided storage.
    fn condvar_init(cv: *mut c_void) -> i8;
    /// Tear down a condition variable.
    fn condvar_drop(cv: *mut c_void) -> i8;
    /// Wake one waiter on the condition variable.
    fn condvar_signal(cv: *mut c_void) -> i8;
    /// Wake all waiters on the condition variable.
    fn condvar_signal_all(cv: *mut c_void) -> i8;
    /// Atomically release `m` and block on `cv`. The mutex is
    /// re-acquired before this function returns.
    fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8;

    /// Wait with absolute monotonic deadline (milliseconds since
    /// the [`PlatformClock::clock_ms`] epoch). Returns non-zero on
    /// timeout.
    fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8;
}

/// Network poll callback for bare-metal platforms using smoltcp.
///
/// Not required for platforms with OS-level networking (POSIX, Zephyr, NuttX).
///
/// **Dispatch model**: currently documentary only — bare-metal platforms
/// drive their `SmoltcpBridge::poll_network()` from timer ISRs and from
/// the `PlatformTcp` / `PlatformUdp` send/receive bodies directly.
/// Kept in the API surface for consistency with the other capability
/// traits; may become dispatch-active in a follow-up phase.
pub trait PlatformNetworkPoll {
    /// Poll the network stack to process pending I/O.
    fn network_poll();
}

// ============================================================================
// Networking — TCP
// ============================================================================

/// TCP networking.
///
/// Socket and endpoint parameters are opaque `*mut c_void` pointers to
/// platform-specific types (`_z_sys_net_socket_t`, `_z_sys_net_endpoint_t`).
/// The shim provides correctly-sized `#[repr(C)]` wrappers whose sizes are
/// auto-detected from C headers at build time (see Phase 80 design).
///
/// Read functions return `usize::MAX` on error. Send returns `usize::MAX` on error.
///
/// Method names are unprefixed — the trait already namespaces them. Shims
/// dispatch via `<ConcretePlatform as PlatformTcp>::open(...)` etc.
pub trait PlatformTcp {
    /// Resolve address + port strings into an endpoint handle.
    fn create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8;
    /// Free endpoint resources.
    fn free_endpoint(ep: *mut c_void);
    /// Open a TCP client connection. `endpoint` is by-value (opaque bytes on stack).
    fn open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8;
    /// Open a TCP listening socket.
    fn listen(sock: *mut c_void, endpoint: *const c_void) -> i8;
    /// Close a TCP socket.
    fn close(sock: *mut c_void);
    /// Read up to `len` bytes. Returns bytes read, or `usize::MAX` on error.
    fn read(sock: *const c_void, buf: *mut u8, len: usize) -> usize;
    /// Read exactly `len` bytes. Returns `len` on success, `usize::MAX` on error.
    fn read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize;
    /// Send `len` bytes. Returns bytes sent, or `usize::MAX` on error.
    fn send(sock: *const c_void, buf: *const u8, len: usize) -> usize;
}

// ============================================================================
// Networking — UDP unicast
// ============================================================================

/// UDP unicast networking.
pub trait PlatformUdp {
    fn create_endpoint(ep: *mut c_void, address: *const u8, port: *const u8) -> i8;
    fn free_endpoint(ep: *mut c_void);
    fn open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8;
    fn close(sock: *mut c_void);
    fn read(sock: *const c_void, buf: *mut u8, len: usize) -> usize;
    fn read_exact(sock: *const c_void, buf: *mut u8, len: usize) -> usize;
    fn send(sock: *const c_void, buf: *const u8, len: usize, endpoint: *const c_void) -> usize;
    /// Set the receive timeout on a UDP socket (milliseconds).
    /// 0 means block indefinitely (no timeout).
    fn set_recv_timeout(sock: *const c_void, timeout_ms: u32);

    /// Open a UDP socket in listen (server) mode, bound to the given
    /// endpoint. Returns 0 on success, negative on failure.
    ///
    /// Optional — the default returns `-1`, which the shim forwards to
    /// `_z_listen_udp_unicast` as "not implemented". Platforms that
    /// need UDP server sockets (e.g. for running an XRCE-DDS agent
    /// locally) should override this. Once Phase 84.F4 lands (the
    /// "platform traits become a real contract" refactor), the shim
    /// will dispatch through this trait method automatically.
    fn listen(_sock: *mut c_void, _endpoint: *const c_void, _timeout_ms: u32) -> i8 {
        -1
    }
}

// ============================================================================
// Networking — socket helpers
// ============================================================================

/// Socket helper operations called by zenoh-pico's transport layer.
///
/// Unprefixed method names: dispatch via
/// `<ConcretePlatform as PlatformSocketHelpers>::set_non_blocking(...)`.
/// Note that the `close` method here is the socket-layer close (shutdown +
/// close) used by zenoh-pico's generic helpers; `PlatformTcp::close` is the
/// TCP-specific close. Both exist because zenoh-pico's C surface has both.
pub trait PlatformSocketHelpers {
    /// Set socket to non-blocking mode.
    fn set_non_blocking(sock: *const c_void) -> i8;
    /// Accept a pending connection.
    fn accept(sock_in: *const c_void, sock_out: *mut c_void) -> i8;
    /// Close a socket (shutdown + close).
    fn close(sock: *mut c_void);
    /// Wait for socket events (multi-threaded platforms).
    fn wait_event(peers: *mut c_void, mutex: *mut c_void) -> i8;
}

// ============================================================================
// libc stubs (bare-metal only)
// ============================================================================

/// Standard C library functions needed by zenoh-pico on bare-metal targets.
///
/// Platforms with a C runtime (RTOS, POSIX) do NOT need to implement this.
///
/// # Dispatch model (Phase 84.F4.6)
///
/// This trait is **documentary only** — it is NOT dispatched through by
/// `zpico-platform-shim` or `xrce-platform-shim`. The C libraries resolve
/// these symbols (`strlen`, `memcpy`, `errno`, ...) at link time directly
/// from `#[unsafe(no_mangle)] extern "C" fn` definitions in
/// `nros-baremetal-common`, which bare-metal platform crates pull in
/// via the `libc-stubs` feature:
///
/// ```text
///   nros-baremetal-common = { ..., features = ["libc-stubs"] }
/// ```
///
/// The trait is retained in this API surface so that a future shim
/// refactor could route libc through typed Rust methods without
/// changing consumers. Today, implementing `PlatformLibc` on a platform
/// ZST would be pure documentation; the actual contract — "the linker
/// can resolve `strlen` etc." — is enforced at link time, not at
/// compile time. No platform crate implements this trait in the
/// current tree.
pub trait PlatformLibc {
    fn strlen(s: *const u8) -> usize;
    fn strcmp(s1: *const u8, s2: *const u8) -> c_int;
    fn strncmp(s1: *const u8, s2: *const u8, n: usize) -> c_int;
    fn strchr(s: *const u8, c: c_int) -> *mut u8;
    fn strncpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    fn memset(dest: *mut c_void, c: c_int, n: usize) -> *mut c_void;
    fn memcmp(s1: *const c_void, s2: *const c_void, n: usize) -> c_int;
    fn memchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void;
    fn strtoul(nptr: *const u8, endptr: *mut *mut u8, base: c_int) -> core::ffi::c_ulong;
    fn errno_ptr() -> *mut c_int;
}

// ============================================================================
// Networking — UDP multicast
// ============================================================================

/// UDP multicast networking (used for zenoh scouting on desktop platforms).
pub trait PlatformUdpMulticast {
    fn mcast_open(
        sock: *mut c_void,
        endpoint: *const c_void,
        lep: *mut c_void,
        timeout_ms: u32,
        iface: *const u8,
    ) -> i8;
    fn mcast_listen(
        sock: *mut c_void,
        endpoint: *const c_void,
        timeout_ms: u32,
        iface: *const u8,
        join: *const u8,
    ) -> i8;
    fn mcast_close(
        sockrecv: *mut c_void,
        socksend: *mut c_void,
        rep: *const c_void,
        lep: *const c_void,
    );
    fn mcast_read(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        lep: *const c_void,
        addr: *mut c_void,
    ) -> usize;
    fn mcast_read_exact(
        sock: *const c_void,
        buf: *mut u8,
        len: usize,
        lep: *const c_void,
        addr: *mut c_void,
    ) -> usize;
    fn mcast_send(
        sock: *const c_void,
        buf: *const u8,
        len: usize,
        endpoint: *const c_void,
    ) -> usize;
}

// ============================================================================
// Inter-VM / mailbox transport (NVIDIA IVC and similar)
// ============================================================================

/// Inter-processor mailbox transport, modelled after NVIDIA Tegra IVC.
///
/// IVC (Inter-VM Communication on Tegra; in practice CCPLEX↔SPE on AGX
/// Orin) is a header-prefixed lock-free SPSC ring in shared DRAM, paired
/// with a hardware doorbell for wake. One channel is one peer — there
/// is no discovery, naming, QoS, or fanout — which is why it's a *link*
/// transport (peer to TCP/UDP/Serial/RawEth inside zenoh-pico) rather
/// than a new RMW backend.
///
/// Channel handles are opaque `*mut c_void` to match the shape zenoh-pico
/// passes across its FFI boundary. The driver crate (`nvidia-ivc`)
/// translates between this opaque handle and either NVIDIA's FSP
/// `tegra_ivc_*` API (`fsp` feature) or a Unix-socket pair
/// (`unix-mock` feature, host dev + CI).
///
/// **Zero-copy contract** (Phase 11.3.A). NVIDIA's FSP IVC API is
/// fundamentally a "borrow ring slot, fill, commit / release"
/// pattern. Both backends mirror that here so consumers don't branch
/// on backend:
///
/// - [`Self::rx_get`] returns a pointer into the channel's RX slot
///   (and writes the frame length to `*len_out`); [`Self::rx_release`]
///   advances the producer-visible cursor. Returns null +
///   `*len_out = 0` if the ring is empty.
/// - [`Self::tx_get`] returns a writable pointer to the next free TX
///   slot (and writes the slot capacity to `*cap_out`);
///   [`Self::tx_commit`] makes the slot visible to the peer (and
///   rings the per-frame doorbell on FSP — see `notify` for batching).
///   [`Self::tx_abandon`] frees the slot without sending. Returns
///   null + `*cap_out = 0` if the ring is full.
///
/// Single outstanding RX slot and single outstanding TX slot per
/// channel — borrow, finish, repeat. Multi-frame batching is the
/// caller's job (loop `tx_get` / fill / `tx_commit`, then one
/// `notify`).
///
/// `frame_size` is the fixed per-channel frame size negotiated at
/// carveout setup (typical NVIDIA IVC: 64 bytes per frame, 16 frames
/// per channel). The link layer uses it to pick its reassembly buffer.
pub trait PlatformIvc {
    /// Resolve a channel ID into an opaque handle. Returns null on
    /// failure. The numeric ID matches the NVIDIA channel index
    /// (`channel 2 = aon_echo`).
    fn channel_get(id: u32) -> *mut c_void;

    /// Fixed frame size negotiated for this channel, in bytes.
    fn frame_size(ch: *mut c_void) -> u32;

    /// Borrow the next-available RX frame. Writes the frame length to
    /// `*len_out` and returns a pointer into the ring. Returns null +
    /// `*len_out = 0` if no frame is available.
    fn rx_get(ch: *mut c_void, len_out: *mut usize) -> *const u8;

    /// Release the most recently `rx_get`'d frame back to the
    /// producer. Pair 1:1 with `rx_get` calls that returned non-null.
    fn rx_release(ch: *mut c_void);

    /// Borrow the next free TX slot. Writes the slot capacity to
    /// `*cap_out` and returns a writable pointer. Returns null +
    /// `*cap_out = 0` if the ring is full.
    fn tx_get(ch: *mut c_void, cap_out: *mut usize) -> *mut u8;

    /// Commit `len` bytes from the most recently `tx_get`'d slot.
    /// Slot is then visible to the peer; per-frame doorbell may also
    /// fire on FSP. Pair 1:1 with `tx_get` calls that returned
    /// non-null.
    fn tx_commit(ch: *mut c_void, len: usize);

    /// Abandon the most recently `tx_get`'d slot without sending.
    /// Pair 1:1 with `tx_get` calls that returned non-null.
    fn tx_abandon(ch: *mut c_void);

    /// Ring the doorbell. On FSP this is redundant (commit/release
    /// already invoke `ch->notify_remote`); on unix-mock it's a no-op
    /// (SOCK_DGRAM wakes the peer naturally). Provided for symmetry
    /// so callers can batch-commit then notify once.
    fn notify(ch: *mut c_void);
}

// ============================================================================
// Serial (UART / PTY)
// ============================================================================

/// Serial (byte-stream) transport.
///
/// Used by XRCE-DDS's HDLC-framed serial transport and (once the
/// bare-metal migration lands) by zenoh-pico's serial link layer.
///
/// # Handle model
///
/// `open()` returns a platform-defined [`Handle`](PlatformSerial::Handle)
/// — an FD on POSIX, a port-table index on bare-metal, whatever the
/// impl wants. Every other method takes the handle back. This lets a
/// single platform impl service multiple concurrent devices (e.g.,
/// `zpico-serial`'s two-port table) without the trait constraining
/// the impl to a single active device.
///
/// Single-device platforms return the same handle forever and ignore
/// it internally; `INVALID` gives a well-defined "no live handle"
/// sentinel for shims that stash the current handle in a `static`.
///
/// # Path conventions
///
/// `path` in `open()` is platform-specific: a null-terminated UTF-8
/// device path on POSIX (e.g., `/dev/ttyUSB0` or a PTY), or a
/// board-defined port identifier on bare-metal (typically parsed by
/// the platform's internal handler). Callers pass the locator string
/// from their config unchanged; interpretation is the platform's job.
///
/// # I/O conventions
///
/// Read / write return `usize::MAX` on hard error. Read with
/// `timeout_ms == 0` should block for a platform-chosen default;
/// positive values are the poll/select deadline in milliseconds.
/// Returning `0` from `read()` indicates "no data within timeout" and
/// is **not** an error — both XRCE and zenoh-pico tolerate
/// timeout-zero reads.
pub trait PlatformSerial {
    /// Platform-specific handle type. POSIX returns the FD (`i32`);
    /// bare-metal returns a port-table index (`u8`). Must be `Copy`
    /// so shims can stash it in a `static`.
    type Handle: Copy;

    /// Sentinel handle meaning "not a live device". Shims initialise
    /// their cached handle to this and compare against it to detect
    /// "transport not yet opened" states.
    const INVALID: Self::Handle;

    /// Returns `true` if `h` is a live handle (not [`INVALID`](Self::INVALID)
    /// and points at a device that was opened and not yet closed).
    fn is_valid(h: Self::Handle) -> bool;

    /// Open the serial device identified by `path`. Returns a live
    /// handle on success, [`INVALID`](Self::INVALID) on failure.
    fn open(path: *const u8) -> Self::Handle;

    /// Close the given handle. No-op if already closed or invalid.
    fn close(h: Self::Handle);

    /// Configure baud rate (in bits per second). Returns 0 on success,
    /// -1 on error. Called after `open()`; implementations may choose
    /// to apply the baud rate during `open()` instead and make this a
    /// no-op.
    fn configure(h: Self::Handle, baudrate: u32) -> i8;

    /// Read up to `len` bytes into `buf`. Returns the number of bytes
    /// read, `0` on timeout, or `usize::MAX` on hard error.
    fn read(h: Self::Handle, buf: *mut u8, len: usize, timeout_ms: u32) -> usize;

    /// Write `len` bytes from `buf`. Returns bytes written, or
    /// `usize::MAX` on error.
    fn write(h: Self::Handle, buf: *const u8, len: usize) -> usize;
}
