//! Issue 1940 — per-entity wake registration across the C vtable.
//!
//! Two halves, one on each side of the `*_set_on_new_*_callback` slots
//! (upstream's names and signature):
//!
//! - [`WakerSlot`] is the RUNTIME side. A `CffiSubscription` (or service, or
//!   client) keeps the `Waker` its caller registered here and hands the
//!   backend [`runtime_entity_wake`] plus a pointer to the slot as
//!   `user_data`. The ABI says a
//!   registration persists until replaced, so the slot wakes BY REFERENCE and
//!   keeps the waker: a C caller of `rclc_polling_subscription_set_wake_callback`
//!   registers once and expects every arrival to reach it.
//! - [`Hosted`] is the BACKEND side, for a Rust backend reached through
//!   [`crate::rust_adapter`]. Its handle lives inside a `Hosted<T>`, which also
//!   stores the C `(callback, user_data)` and gives the backend a `Waker` that
//!   calls it. A Rust backend's `register_waker` is allowed to CONSUME the waker on
//!   wake (`AtomicWaker::wake` does), so that waker re-arms itself before it
//!   calls `cb`.

#[cfg(feature = "alloc")]
use core::task::{RawWaker, RawWakerVTable};
use core::{cell::UnsafeCell, ffi::c_void, task::Waker};

use portable_atomic::{AtomicBool, Ordering};

#[cfg(feature = "alloc")]
/// Upstream `rmw_event_callback_t`, unwrapped from its `Option`.
pub(crate) type EntityWakeCb =
    unsafe extern "C" fn(user_data: *const c_void, number_of_events: usize);

/// A tiny spin lock around a `Copy`/`Clone` value.
///
/// Held only to copy the value in or out, never across a call, so contention
/// is a handful of instructions. A lock rather than atomics because the value
/// is several words that must change together: a callback paired with the
/// wrong context is a call into the wrong object.
struct Locked<V> {
    busy: AtomicBool,
    value: UnsafeCell<V>,
}

// SAFETY: every access to `value` happens with `busy` held.
unsafe impl<V: Send> Sync for Locked<V> {}
unsafe impl<V: Send> Send for Locked<V> {}

impl<V> Locked<V> {
    const fn new(value: V) -> Self {
        Self {
            busy: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut V) -> R) -> R {
        while self
            .busy
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        // SAFETY: `busy` is held, so no other reference to `value` exists.
        let out = f(unsafe { &mut *self.value.get() });
        self.busy.store(false, Ordering::Release);
        out
    }
}

/// The runtime's per-entity waker, woken by the backend through
/// [`runtime_entity_wake`].
pub(crate) struct WakerSlot {
    waker: Locked<Option<Waker>>,
}

impl WakerSlot {
    pub(crate) const fn new() -> Self {
        Self {
            waker: Locked::new(None),
        }
    }

    /// Replace the stored waker. Cheap when the same waker is registered
    /// again, which is what a future does on every `Pending` poll.
    pub(crate) fn register(&self, waker: &Waker) {
        self.waker.with(|slot| match slot {
            Some(old) if old.will_wake(waker) => {}
            _ => *slot = Some(waker.clone()),
        });
    }

    /// Wake the stored waker and keep it.
    pub(crate) fn wake(&self) {
        // Clone out under the lock and wake outside it: a C waker runs user
        // code, which may register again.
        if let Some(waker) = self.waker.with(|slot| slot.clone()) {
            waker.wake();
        }
    }

    /// The `user_data` to hand the backend alongside [`runtime_entity_wake`].
    pub(crate) fn as_user_data(&self) -> *const c_void {
        self as *const Self as *const c_void
    }
}

/// The callback every cffi entity registers through `*_set_on_new_*_callback`.
///
/// # Safety
///
/// `user_data` is a [`WakerSlot::as_user_data`] pointer whose slot is still
/// alive: the backend drops the registration at `destroy_*`, which runs before
/// the slot's owner is freed.
pub(crate) unsafe extern "C" fn runtime_entity_wake(
    user_data: *const c_void,
    _number_of_events: usize,
) {
    if let Some(slot) = unsafe { (user_data as *const WakerSlot).as_ref() } {
        slot.wake();
    }
}

#[cfg(feature = "alloc")]
/// A C registration: the callback and its `user_data`.
#[derive(Clone, Copy)]
struct Registration {
    cb: EntityWakeCb,
    user_data: *const c_void,
}

#[cfg(feature = "alloc")]
// SAFETY: `user_data` is an opaque pointer the C caller vouches for; the ABI
// says `callback(user_data, n)` may be called from a backend thread.
unsafe impl Send for Registration {}

#[cfg(feature = "alloc")]
/// A Rust backend's entity handle plus its C wake registration.
///
/// `repr(C)` with `inner` FIRST, so a pointer to a `Hosted<T>` is also a valid
/// pointer to its `T`: every adapter trampoline that reads `backend_data` as a
/// `*const T` keeps working unchanged. Only create and destroy know about the
/// wrapper.
#[repr(C)]
pub(crate) struct Hosted<T> {
    inner: T,
    registration: Locked<Option<Registration>>,
    /// The entity kind's `register_waker`, as a plain fn so one wrapper serves
    /// subscriptions, services and clients.
    rearm: fn(&T, &Waker) -> bool,
}

#[cfg(feature = "alloc")]
impl<T> Hosted<T> {
    pub(crate) const fn new(inner: T, rearm: fn(&T, &Waker) -> bool) -> Self {
        Self {
            inner,
            registration: Locked::new(None),
            rearm,
        }
    }

    const VTABLE: RawWakerVTable = RawWakerVTable::new(
        Self::waker_clone,
        Self::waker_wake,
        Self::waker_wake,
        Self::waker_drop,
    );

    /// A waker that calls this entity's C registration. It BORROWS `self`, so
    /// cloning and dropping are free; it is valid while `self` is, and `self`
    /// is boxed or parked in a static slot until `destroy_*`.
    fn waker(&self) -> Waker {
        let raw = RawWaker::new(self as *const Self as *const (), &Self::VTABLE);
        // SAFETY: the vtable's functions honour the RawWaker contract for a
        // borrowed pointer: clone copies it, drop does nothing.
        unsafe { Waker::from_raw(raw) }
    }

    unsafe fn waker_clone(data: *const ()) -> RawWaker {
        RawWaker::new(data, &Self::VTABLE)
    }

    unsafe fn waker_drop(_data: *const ()) {}

    unsafe fn waker_wake(data: *const ()) {
        // SAFETY: `data` came from `waker()`, so it points at a live
        // `Hosted<T>`.
        let this = unsafe { &*(data as *const Self) };
        let Some(reg) = this.registration.with(|r| *r) else {
            return;
        };
        // Re-arm BEFORE calling out: the backend may have consumed the waker
        // to call us, and an arrival during `cb` must still wake.
        //
        // This reaches `inner` through `&T` from the backend's thread, which is
        // what `register_waker(&self)` is for: the backends that implement it
        // keep the waker in an `AtomicWaker`.
        (this.rearm)(&this.inner, &this.waker());
        // SAFETY: the C caller's contract for `set_on_new_*_callback`. One
        // call per wake, so `1` — the lower bound the header allows.
        unsafe { (reg.cb)(reg.user_data, 1) };
    }

    /// The body of the adapter's three `*_set_on_new_*_callback` trampolines.
    ///
    /// # Safety
    ///
    /// `data` is NULL or a `backend_data` the adapter minted for a `Hosted<T>`.
    pub(crate) unsafe fn set_wake(
        data: *mut c_void,
        cb: Option<EntityWakeCb>,
        user_data: *const c_void,
    ) -> crate::NrosRmwRet {
        let Some(this) = (unsafe { (data as *const Self).as_ref() }) else {
            return crate::NROS_RMW_RET_INVALID_ARGUMENT;
        };
        match cb {
            None => {
                this.registration.with(|r| *r = None);
                // Replace whatever the backend holds, so it stops pointing at
                // a registration that is gone. The return is irrelevant here.
                (this.rearm)(&this.inner, Waker::noop());
                crate::NROS_RMW_RET_OK
            }
            Some(cb) => {
                this.registration
                    .with(|r| *r = Some(Registration { cb, user_data }));
                if (this.rearm)(&this.inner, &this.waker()) {
                    crate::NROS_RMW_RET_OK
                } else {
                    this.registration.with(|r| *r = None);
                    crate::NROS_RMW_RET_UNSUPPORTED
                }
            }
        }
    }
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use core::sync::atomic::AtomicU32;

    use super::*;

    /// A backend entity whose `register_waker` CONSUMES the waker on wake,
    /// the way `AtomicWaker` does.
    struct OneShot {
        waker: Locked<Option<Waker>>,
    }

    impl OneShot {
        fn arrive(&self) {
            if let Some(w) = self.waker.with(|w| w.take()) {
                w.wake();
            }
        }
    }

    fn rearm(t: &OneShot, w: &Waker) -> bool {
        t.waker.with(|slot| *slot = Some(w.clone()));
        true
    }

    static FIRED: AtomicU32 = AtomicU32::new(0);
    static LAST_USER_DATA: AtomicU32 = AtomicU32::new(0);

    unsafe extern "C" fn count(user_data: *const c_void, n: usize) {
        assert!(n >= 1);
        FIRED.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        LAST_USER_DATA.store(
            user_data as usize as u32,
            core::sync::atomic::Ordering::SeqCst,
        );
    }

    #[test]
    fn a_registration_survives_a_backend_that_consumes_its_waker() {
        let hosted = Hosted::new(
            OneShot {
                waker: Locked::new(None),
            },
            rearm,
        );
        let data = &hosted as *const _ as *mut c_void;
        let ret = unsafe { Hosted::<OneShot>::set_wake(data, Some(count), 7 as *const c_void) };
        assert_eq!(ret, crate::NROS_RMW_RET_OK);
        let before = FIRED.load(core::sync::atomic::Ordering::SeqCst);
        hosted.inner.arrive();
        hosted.inner.arrive();
        hosted.inner.arrive();
        assert_eq!(FIRED.load(core::sync::atomic::Ordering::SeqCst) - before, 3);
        assert_eq!(LAST_USER_DATA.load(core::sync::atomic::Ordering::SeqCst), 7);

        // Cleared: the next arrival calls nobody.
        let ret = unsafe { Hosted::<OneShot>::set_wake(data, None, core::ptr::null()) };
        assert_eq!(ret, crate::NROS_RMW_RET_OK);
        let before = FIRED.load(core::sync::atomic::Ordering::SeqCst);
        hosted.inner.arrive();
        assert_eq!(FIRED.load(core::sync::atomic::Ordering::SeqCst), before);
    }

    #[test]
    fn a_backend_that_cannot_wake_reports_unsupported() {
        let hosted = Hosted::new((), |_: &(), _: &Waker| false);
        let data = &hosted as *const _ as *mut c_void;
        let ret = unsafe { Hosted::<()>::set_wake(data, Some(count), core::ptr::null()) };
        assert_eq!(ret, crate::NROS_RMW_RET_UNSUPPORTED);
    }
}
