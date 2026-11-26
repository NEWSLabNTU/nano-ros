// Phase 579 W3a; RFC-0139 D3, D4 and R6.
//! A device as an executor event source: the `Device` entry kind's signal
//! state, and the completion handle a device port fires when a job is done.
//!
//! The executor owns the state. It lives in the arena entry the device was
//! registered into, so it has a fixed address for the life of the executor,
//! and a [`Completion`] is a plain copy of a pointer to it. A port never sees
//! the executor: it holds a `Completion`, and calls
//! [`signal`](Completion::signal) from a thread or
//! [`signal_from_isr`](Completion::signal_from_isr) from an interrupt handler.
//!
//! A completion sets the entry's flag, marks its slot ready through the
//! attributed wake path, and wakes the spin. The continuation then runs at the
//! priority of the sched context the entry is bound to, like any other ready
//! callback.

/// The state a completion writes: the entry's flag, its slot, and the wake it
/// signals. Embedded in the device entry, never moved.
pub(crate) struct DeviceSignal {
    pub(crate) flag: portable_atomic::AtomicBool,
    /// The slot whose ready bit the completion sets.
    #[cfg_attr(not(all(feature = "alloc", feature = "rmw-cffi")), allow(dead_code))]
    pub(crate) slot: usize,
    /// The executor's wake state. Held by `Arc`, as a guard condition's handle
    /// holds it, so a completion that fires while the executor is being torn
    /// down reaches live memory.
    #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
    pub(crate) wake_ctx: Option<portable_atomic_util::Arc<super::spin::WakeCtx>>,
}

impl DeviceSignal {
    pub(crate) fn new(slot: usize) -> Self {
        Self {
            flag: portable_atomic::AtomicBool::new(false),
            slot,
            #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
            wake_ctx: None,
        }
    }

    fn signal(&self) {
        self.flag.store(true, portable_atomic::Ordering::Release);
        #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
        if let Some(ctx) = self.wake_ctx.as_ref() {
            ctx.mark_ready(self.slot);
        }
    }

    fn signal_from_isr(&self) -> bool {
        self.flag.store(true, portable_atomic::Ordering::Release);
        #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
        if let Some(ctx) = self.wake_ctx.as_ref() {
            return ctx.mark_ready_from_isr(self.slot);
        }
        false
    }
}

/// The handle a device port signals when a job (or a source's frame) is done.
///
/// `Copy` and `Send`: a port stores it where its interrupt handler can reach
/// it, typically in a [`CompletionCell`]. It stays valid for the life of the
/// executor the device was registered with.
///
/// Only [`signal_from_isr`](Self::signal_from_isr) may be called from an
/// interrupt handler (or, on POSIX, a signal handler).
#[derive(Clone, Copy)]
pub struct Completion {
    signal: *const DeviceSignal,
}

// SAFETY: the pointee is the executor's arena entry, at a fixed address for
// the executor's life, and every operation through it is an atomic store or a
// lock-free wake. Nothing reads through it non-atomically.
unsafe impl Send for Completion {}
// SAFETY: as `Send`; a shared `&Completion` only allows the same operations.
unsafe impl Sync for Completion {}

impl Completion {
    pub(crate) fn new(signal: *const DeviceSignal) -> Self {
        Self { signal }
    }

    /// Signal the completion from a thread.
    ///
    /// The executor runs the device's continuation at its next spin, at the
    /// bound priority, and a parked spin is woken now.
    pub fn signal(self) {
        // SAFETY: see `unsafe impl Send`.
        unsafe { &*self.signal }.signal();
    }

    /// Signal the completion from an interrupt handler, or on POSIX from a
    /// signal handler. Bounded and non-blocking.
    ///
    /// Returns whether the executor's wake object took the signal, the same
    /// answer `GuardCondition::trigger_from_isr` gives. `false` means the
    /// completion is recorded but nothing was woken (a build with no wake, or a
    /// port with no interrupt-safe signal); the executor then sees it at its
    /// next deadline.
    pub fn signal_from_isr(self) -> bool {
        // SAFETY: see `unsafe impl Send`.
        unsafe { &*self.signal }.signal_from_isr()
    }

    /// Is the completion signalled and not yet taken by the executor?
    pub fn is_signalled(self) -> bool {
        // SAFETY: see `unsafe impl Send`.
        unsafe { &*self.signal }
            .flag
            .load(portable_atomic::Ordering::Acquire)
    }

    /// The opaque pointer the C completion handle carries.
    #[doc(hidden)]
    pub fn as_raw(self) -> *mut core::ffi::c_void {
        self.signal as *mut core::ffi::c_void
    }

    /// Rebuild a handle from [`as_raw`](Self::as_raw).
    ///
    /// # Safety
    /// `raw` must have come from `as_raw` on a completion whose executor is
    /// still alive.
    #[doc(hidden)]
    pub unsafe fn from_raw(raw: *mut core::ffi::c_void) -> Self {
        Self {
            signal: raw as *const DeviceSignal,
        }
    }
}

/// One [`Completion`], stored atomically, so the interrupt handler can reach
/// the handle a port's `submit` received without a lock.
pub struct CompletionCell {
    signal: portable_atomic::AtomicPtr<DeviceSignal>,
}

impl CompletionCell {
    /// An empty cell. A signal on an empty cell does nothing.
    pub const fn new() -> Self {
        Self {
            signal: portable_atomic::AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// Store the completion the next signal fires.
    pub fn set(&self, done: Completion) {
        self.signal.store(
            done.signal as *mut DeviceSignal,
            portable_atomic::Ordering::Release,
        );
    }

    /// The stored completion, if any.
    pub fn get(&self) -> Option<Completion> {
        let p = self.signal.load(portable_atomic::Ordering::Acquire);
        (!p.is_null()).then(|| Completion::new(p))
    }

    /// [`Completion::signal`] on the stored completion; `false` when empty.
    pub fn signal(&self) -> bool {
        self.get().map(Completion::signal).is_some()
    }

    /// [`Completion::signal_from_isr`] on the stored completion; `false` when
    /// empty.
    pub fn signal_from_isr(&self) -> bool {
        self.get().is_some_and(Completion::signal_from_isr)
    }
}

impl Default for CompletionCell {
    fn default() -> Self {
        Self::new()
    }
}

// The two C signal functions the device port header declares (RFC-0139 D3).
// Defined here, beside the state they write, wherever an RMW backend links.
// The handle's one pointer is a `Completion`'s raw pointer: the executor's
// arena entry, valid for the executor's life, so a port may keep it and signal
// it from an interrupt long after registration.

/// Signal a device completion from a thread. A null handle does nothing.
///
/// # Safety
/// `c` must be a handle the executor handed out (or zero), and its executor
/// must still be alive.
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_completion_signal(c: nros_rmw_cffi::nros_completion_t) {
    if !c._slot.is_null() {
        // SAFETY: the caller's contract above.
        unsafe { Completion::from_raw(c._slot) }.signal();
    }
}

/// Signal a device completion from an interrupt handler, or on POSIX from a
/// signal handler: the one call a port may make there. A null handle does
/// nothing. Whether a wake object took the signal is not returned, as the
/// header states; the Rust `Completion::signal_from_isr` reports it.
///
/// # Safety
/// As [`nros_completion_signal`].
#[cfg(feature = "rmw-cffi")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_completion_signal_from_isr(c: nros_rmw_cffi::nros_completion_t) {
    if !c._slot.is_null() {
        // SAFETY: the caller's contract above.
        let _ = unsafe { Completion::from_raw(c._slot) }.signal_from_isr();
    }
}
