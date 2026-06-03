//! Phase 122.3.c.6.e — C-ABI-compatible `Waker` bridge.
//!
//! Rust's [`core::task::Waker`] is the trait-level event-driven
//! primitive but it isn't a C-friendly handle (the `RawWaker` data
//! pointer + vtable layout isn't exposed across the ABI boundary).
//! Backends that wake via C function pointers can register a
//! [`CWakeState`] here, and the [`make_waker`] helper builds a
//! `Waker` that calls back into the C function on wake.
//!
//! Used by the L1 polling-mode C / C++ FFI to register
//! event-driven callbacks for subscription / service-server /
//! service-client / action server-channel and client-channel
//! events. See `nros-c/src/action/server.rs::nros_action_server_set_*_wake_callback`
//! for usage.
//!
//! # Safety
//!
//! [`CWakeState`] must remain at a stable address for the entire
//! lifetime of any Waker (or clone of) built from it. Typical
//! pattern: store it inline in the C handle's caller-provided
//! `_opaque` storage so it lives as long as the entity itself.

use core::{
    ffi::c_void,
    task::{RawWaker, RawWakerVTable, Waker},
};

/// C function-pointer signature for wake callbacks. Backends call
/// this when the underlying entity has data / a reply / a request
/// pending.
pub type CWakeFn = unsafe extern "C" fn(*mut c_void);

/// Stable-address storage for a C wake callback. The pointer to
/// this struct is what the [`Waker`] holds in its
/// [`RawWaker::data`] slot, so it MUST NOT move after a Waker has
/// been built from it.
#[repr(C)]
pub struct CWakeState {
    /// Function called when the Waker is woken. `None` disables.
    pub fn_ptr: Option<CWakeFn>,
    /// Opaque pointer passed to `fn_ptr` on wake.
    pub ctx: *mut c_void,
}

impl CWakeState {
    /// Empty / disabled state.
    pub const fn empty() -> Self {
        Self {
            fn_ptr: None,
            ctx: core::ptr::null_mut(),
        }
    }

    /// Update the callback. Existing Wakers built from `self` pick
    /// up the new value on their next wake — no re-registration
    /// needed (the Waker's data pointer is unchanged).
    pub fn set(&mut self, fn_ptr: Option<CWakeFn>, ctx: *mut c_void) {
        self.fn_ptr = fn_ptr;
        self.ctx = ctx;
    }
}

/// SAFETY: `CWakeState` holds a function pointer (`Send`/`Sync`) +
/// an opaque user `ctx` pointer (no auto-impl). For the wake path
/// to be sound across threads the C caller must ensure the
/// underlying object behind `ctx` is reachable from whatever thread
/// the backend dispatches the wake callback on. By marking the
/// state as Send+Sync we let backends (e.g. `AtomicWaker`) stash
/// the Waker in shared state; the C contract is documented at the
/// FFI surface ("the C `ctx` must outlive every wake-callback
/// invocation and be safe to read from any thread the runtime
/// dispatches wakes on").
unsafe impl Send for CWakeState {}
unsafe impl Sync for CWakeState {}

/// Build a [`Waker`] that calls `state.fn_ptr(state.ctx)` on wake.
///
/// # Safety
/// * `state` must point to a valid `CWakeState`.
/// * `state` must remain at the same address for as long as the
///   returned Waker (and any clones a backend stashes) lives.
/// * If a backend dispatches wakes from another thread, `state`'s
///   `ctx` must be safe to read from that thread (see [`CWakeState`]
///   `Send` / `Sync` discussion).
pub unsafe fn make_waker(state: *const CWakeState) -> Waker {
    unsafe { Waker::from_raw(RawWaker::new(state as *const (), &VTABLE)) }
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop_fn);

/// # Safety
///
/// Invoked by the `RawWakerVTable` clone slot. `data` is the pointer
/// passed to [`make_waker`] (i.e. a `*const CWakeState`); the runtime
/// upholds [`make_waker`]'s contract (stable address, valid for the
/// Waker's lifetime), so cloning is a trivial pointer copy.
unsafe fn clone(data: *const ()) -> RawWaker {
    RawWaker::new(data, &VTABLE)
}

/// # Safety
///
/// Invoked by the `RawWakerVTable` wake slot. `data` must be the
/// `*const CWakeState` originally supplied to [`make_waker`] and must
/// still point at a valid, stable-address `CWakeState`. Delegates to
/// [`wake_by_ref`].
unsafe fn wake(data: *const ()) {
    unsafe { wake_by_ref(data) };
}

/// # Safety
///
/// Invoked by the `RawWakerVTable` wake-by-ref slot. `data` must be a
/// live `*const CWakeState` as established by [`make_waker`].
/// Dereferences the state and, if a callback is set, calls
/// `fn_ptr(ctx)` — so `ctx` must satisfy the FFI contract documented
/// on [`CWakeState`] (outlive every wake invocation, be safe to read
/// from the dispatching thread).
unsafe fn wake_by_ref(data: *const ()) {
    let state = unsafe { &*(data as *const CWakeState) };
    if let Some(f) = state.fn_ptr {
        unsafe { f(state.ctx) };
    }
}

/// # Safety
///
/// Invoked by the `RawWakerVTable` drop slot. The `CWakeState` is
/// owned by the C caller, so this intentionally does nothing —
/// callers do not need to uphold any invariant beyond passing the
/// original `data` pointer.
unsafe fn drop_fn(_data: *const ()) {
    // `CWakeState` is owned by the caller; we never free.
}
