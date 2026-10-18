//! Issue 1940 — per-entity wake across the C ABI, in both directions.
//!
//! `rclc_polling_*_set_wake_callback`, the C++ twins and Rust's
//! `recv().await` all end in `register_waker` on an entity handle. Through
//! this crate that call used to reach the trait's empty default: the C ABI had
//! no per-entity wake slot, so `CffiSubscription` / `CffiService` /
//! `CffiClient` had nothing to forward to, and every caller got `OK` for a
//! callback nothing would ever fire. The slots are `subscription_set_wake`,
//! `service_set_wake` and `client_set_wake` in `nros/rmw_vtable.h`.
//!
//! This module is the ONE implementation of the two conversions those slots
//! need, shared by all three entity kinds:
//!
//! - [`EntityWake`] — Rust `Waker` -> C `(cb, ctx, token)`, used by the cffi
//!   handles. A waker built by [`nros_rmw::c_waker::make_waker`] (every C and
//!   C++ caller) is forwarded with no storage, because its data pointer is
//!   already a stable caller-owned `CWakeState`. Any other waker needs owned
//!   storage for a clone, which is a boxed [`WakerCell`] under `alloc`; without
//!   `alloc` the call reports `false` (unsupported) rather than pretending.
//! - [`AdapterWake`] — C `(cb, ctx, token)` -> Rust `Waker`, used by
//!   `RustBackendAdapter` to hand a C installer's callback to a Rust backend's
//!   own `register_waker`.

#[cfg(feature = "alloc")]
use core::task::{RawWaker, RawWakerVTable};
use core::{cell::Cell, ffi::c_void, task::Waker};

#[cfg(feature = "alloc")]
use portable_atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};

use crate::{NROS_RMW_RET_OK, NrosRmwRet};

/// The C callback type the per-entity slots install.
pub type EntityWakeFn = unsafe extern "C" fn(ctx: *mut c_void, token: u32);

/// `NROS_RMW_WAKE_TOKEN_UNKNOWN` as the `u32` the slots take (bindgen emits
/// the macro as `i64`).
pub const WAKE_TOKEN_UNKNOWN: u32 = crate::generated::NROS_RMW_WAKE_TOKEN_UNKNOWN as u32;

/// `cb` for a waker built by `c_waker::make_waker`: `ctx` is its
/// `CWakeState`.
unsafe extern "C" fn c_wake_state_cb(ctx: *mut c_void, _token: u32) {
    // SAFETY: installed only with a `CWakeState` pointer taken from such a
    // waker, which `make_waker`'s contract keeps alive and in place.
    unsafe { nros_rmw::c_waker::wake_state(ctx as *const nros_rmw::c_waker::CWakeState) };
}

// ============================================================================
// Rust Waker -> C triple (the cffi handles)
// ============================================================================

/// The cffi handle's side of a per-entity wake.
///
/// One per `CffiSubscription` / `CffiService` / `CffiClient`. It remembers the
/// `ctx` the backend currently holds so a `register_waker` per poll does not
/// re-enter the backend each time, and owns the boxed [`WakerCell`] (if any),
/// which must outlive the backend's last possible call — so it is released
/// only AFTER the entity's `destroy_*` slot has returned.
pub(crate) struct EntityWake {
    /// The `ctx` installed in the backend, `0` for none.
    installed: Cell<usize>,
    /// The boxed `WakerCell` (`alloc` builds only; always NULL otherwise).
    ///
    /// Present in EVERY build, typed as `c_void`, so this struct's size does
    /// not depend on a feature: `nros-c` sizes its opaque storage from a probe
    /// build whose feature set is not the image's, and a cfg'd field made the
    /// probe measure a smaller `CffiClient` than the image compiled.
    #[cfg_attr(not(feature = "alloc"), allow(dead_code))]
    cell: Cell<*mut c_void>,
}

impl EntityWake {
    pub(crate) const fn new() -> Self {
        Self {
            installed: Cell::new(0),
            cell: Cell::new(core::ptr::null_mut()),
        }
    }

    /// Forward `waker` through `slot` (the entity's `*_set_wake`, already
    /// bound to its view). Returns whether a wake will come.
    ///
    /// `slot` is called only when the `ctx` the backend should hold CHANGES,
    /// so a future re-registering on every poll costs one compare.
    pub(crate) fn register(
        &self,
        waker: &Waker,
        slot: impl FnOnce(EntityWakeFn, *mut c_void, u32) -> NrosRmwRet,
    ) -> bool {
        // A C caller's waker: forward its state, no storage, no allocator.
        if let Some(state) = nros_rmw::c_waker::state_of(waker) {
            return self.install(state as *mut c_void, c_wake_state_cb, slot);
        }
        self.register_owned(waker, slot)
    }

    fn install(
        &self,
        ctx: *mut c_void,
        cb: EntityWakeFn,
        slot: impl FnOnce(EntityWakeFn, *mut c_void, u32) -> NrosRmwRet,
    ) -> bool {
        if self.installed.get() == ctx as usize {
            return true;
        }
        if slot(cb, ctx, WAKE_TOKEN_UNKNOWN) == NROS_RMW_RET_OK {
            self.installed.set(ctx as usize);
            true
        } else {
            false
        }
    }

    #[cfg(feature = "alloc")]
    fn register_owned(
        &self,
        waker: &Waker,
        slot: impl FnOnce(EntityWakeFn, *mut c_void, u32) -> NrosRmwRet,
    ) -> bool {
        let mut cell = self.cell.get() as *mut WakerCell;
        if cell.is_null() {
            // `Box::try_new` is unstable; `nros_rmw::fallible::try_box` is the
            // one spelling (issue 1551), so an exhausted heap answers "no wake"
            // instead of reaching the OOM handler.
            let Ok(boxed) = nros_rmw::fallible::try_box(WakerCell::new()) else {
                return false;
            };
            cell = alloc::boxed::Box::into_raw(boxed);
            self.cell.set(cell as *mut c_void);
        }
        // SAFETY: `cell` is owned by `self` and freed only by `release`.
        unsafe { (*cell).register(waker) };
        self.install(cell as *mut c_void, waker_cell_cb, slot)
    }

    #[cfg(not(feature = "alloc"))]
    fn register_owned(
        &self,
        _waker: &Waker,
        _slot: impl FnOnce(EntityWakeFn, *mut c_void, u32) -> NrosRmwRet,
    ) -> bool {
        // An arbitrary waker needs owned storage for its clone, and there is no
        // allocator to put it in. Say so: the caller falls back to polling,
        // which is what it got silently before issue 1940.
        false
    }

    /// Free the owned cell. Call only after the entity's `destroy_*` slot
    /// returned OK — the slot contract says the backend never invokes the
    /// callback after that. On a failed destroy the cell is LEAKED on purpose:
    /// the backend may still hold it, and a leak is the safe direction.
    pub(crate) fn release(&self, destroyed: bool) {
        #[cfg(feature = "alloc")]
        {
            let cell = self.cell.replace(core::ptr::null_mut()) as *mut WakerCell;
            if destroyed && !cell.is_null() {
                // SAFETY: from `Box::into_raw` above, and no longer reachable
                // from the backend.
                drop(unsafe { alloc::boxed::Box::from_raw(cell) });
            }
        }
        let _ = destroyed;
        self.installed.set(0);
    }
}

/// Storage for one Rust waker the backend's callback wakes.
///
/// The `atomic-waker` protocol (register on the polling thread, wake from any
/// other), written here rather than taken as a dependency so this crate's
/// lockfiles stay where they are.
#[cfg(feature = "alloc")]
pub(crate) struct WakerCell {
    state: AtomicUsize,
    waker: core::cell::UnsafeCell<Option<Waker>>,
}

#[cfg(feature = "alloc")]
const WAITING: usize = 0;
#[cfg(feature = "alloc")]
const REGISTERING: usize = 1;
#[cfg(feature = "alloc")]
const WAKING: usize = 2;

#[cfg(feature = "alloc")]
impl WakerCell {
    const fn new() -> Self {
        Self {
            state: AtomicUsize::new(WAITING),
            waker: core::cell::UnsafeCell::new(None),
        }
    }

    fn register(&self, waker: &Waker) {
        match self
            .state
            .compare_exchange(WAITING, REGISTERING, Ordering::Acquire, Ordering::Acquire)
            .unwrap_or_else(|s| s)
        {
            WAITING => {
                // SAFETY: REGISTERING is held, so nothing else touches `waker`.
                let old = unsafe {
                    let slot = &mut *self.waker.get();
                    match slot {
                        Some(w) if w.will_wake(waker) => None,
                        _ => slot.replace(waker.clone()),
                    }
                };
                if self
                    .state
                    .compare_exchange(REGISTERING, WAITING, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
                {
                    // A wake arrived while registering: deliver it now.
                    // SAFETY: still the only writer until the swap below.
                    let w = unsafe { (*self.waker.get()).take() };
                    self.state.swap(WAITING, Ordering::AcqRel);
                    drop(old);
                    if let Some(w) = w {
                        w.wake();
                    }
                } else {
                    drop(old);
                }
            }
            // A wake is in flight: answer it with the new waker.
            WAKING => waker.wake_by_ref(),
            // Concurrent registration is a caller bug; ignore it.
            _ => {}
        }
    }

    fn wake(&self) {
        if self.state.fetch_or(WAKING, Ordering::AcqRel) == WAITING {
            // SAFETY: WAKING is held, so nothing else touches `waker`.
            let w = unsafe { (*self.waker.get()).take() };
            self.state.fetch_and(!WAKING, Ordering::Release);
            if let Some(w) = w {
                w.wake();
            }
        }
    }
}

// SAFETY: the state machine above serialises every access to `waker`.
#[cfg(feature = "alloc")]
unsafe impl Sync for WakerCell {}
#[cfg(feature = "alloc")]
unsafe impl Send for WakerCell {}

#[cfg(feature = "alloc")]
unsafe extern "C" fn waker_cell_cb(ctx: *mut c_void, _token: u32) {
    // SAFETY: installed only with a live `WakerCell` that `EntityWake` frees
    // after the backend can no longer call this.
    unsafe { (*(ctx as *const WakerCell)).wake() };
}

// ============================================================================
// C triple -> Rust Waker (RustBackendAdapter)
// ============================================================================

/// A C installer's `(cb, ctx, token)`, at a stable address beside the Rust
/// backend's entity handle, presented to that backend as a `Waker`.
///
/// The `Waker` it builds points at `self` and has no-op clone and drop, so a
/// backend may keep any number of clones; each wake reads the CURRENT triple.
/// The triple is written under a sequence counter, so a wake never sees one
/// installer's `cb` with another's `ctx`. A wake that races a re-install may
/// be delivered to either installer, or dropped; the slot contract (one
/// installer per entity, a clear before teardown) makes that window empty in
/// practice.
#[cfg(feature = "alloc")]
pub(crate) struct AdapterWake {
    seq: AtomicU32,
    cb: AtomicPtr<()>,
    ctx: AtomicPtr<c_void>,
    token: AtomicU32,
}

#[cfg(feature = "alloc")]
impl AdapterWake {
    pub(crate) const fn new() -> Self {
        Self {
            seq: AtomicU32::new(0),
            cb: AtomicPtr::new(core::ptr::null_mut()),
            ctx: AtomicPtr::new(core::ptr::null_mut()),
            token: AtomicU32::new(0),
        }
    }

    /// Install (or, with `cb == None`, clear) the triple. Single writer: the
    /// slot trampoline, on the thread that owns the entity.
    pub(crate) fn set(&self, cb: Option<EntityWakeFn>, ctx: *mut c_void, token: u32) {
        self.seq.fetch_add(1, Ordering::AcqRel);
        self.cb.store(
            cb.map_or(core::ptr::null_mut(), |f| f as *mut ()),
            Ordering::Release,
        );
        self.ctx.store(ctx, Ordering::Release);
        self.token.store(token, Ordering::Release);
        self.seq.fetch_add(1, Ordering::AcqRel);
    }

    /// Whether a callback is installed.
    pub(crate) fn is_armed(&self) -> bool {
        !self.cb.load(Ordering::Acquire).is_null()
    }

    /// A `Waker` that calls the installed callback.
    pub(crate) fn waker(&self) -> Waker {
        // SAFETY: `self` lives beside the entity handle for its whole life, and
        // the vtable never frees anything.
        unsafe {
            Waker::from_raw(RawWaker::new(
                self as *const Self as *const (),
                &ADAPTER_VTABLE,
            ))
        }
    }

    /// Call the installed callback, if any.
    pub(crate) fn fire(&self) {
        // Bounded: a writer preempted mid-update by this reader must not spin it
        // forever. Giving up drops one wake during a re-install, which the type
        // doc already allows.
        for _ in 0..4 {
            let s1 = self.seq.load(Ordering::Acquire);
            if s1 & 1 != 0 {
                continue;
            }
            let cb = self.cb.load(Ordering::Acquire);
            let ctx = self.ctx.load(Ordering::Acquire);
            let token = self.token.load(Ordering::Acquire);
            if self.seq.load(Ordering::Acquire) != s1 {
                continue;
            }
            if !cb.is_null() {
                // SAFETY: stored from an `EntityWakeFn` in `set`.
                let f: EntityWakeFn = unsafe { core::mem::transmute(cb) };
                // SAFETY: the installer keeps `ctx` valid while installed.
                unsafe { f(ctx, token) };
            }
            return;
        }
    }
}

#[cfg(feature = "alloc")]
static ADAPTER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(adapter_clone, adapter_wake, adapter_wake, adapter_drop);

#[cfg(feature = "alloc")]
unsafe fn adapter_clone(data: *const ()) -> RawWaker {
    RawWaker::new(data, &ADAPTER_VTABLE)
}

#[cfg(feature = "alloc")]
unsafe fn adapter_wake(data: *const ()) {
    // SAFETY: `data` is the `AdapterWake` `waker()` was called on.
    unsafe { (*(data as *const AdapterWake)).fire() };
}

#[cfg(feature = "alloc")]
unsafe fn adapter_drop(_data: *const ()) {}
