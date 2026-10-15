//! phase-436 -- one backend wake slot, several executors.
//!
//! A backend session has ONE wake-callback slot (`Session::set_wake_callback`).
//! Before this module, the executor that installed into it was the only one an
//! arrival could wake. Every board's spawned tiers open over the boot
//! executor's session (`Executor::open_with_session_*`), and several C
//! executors can share one `rclc_support_t` session. So those executors' wake
//! objects were never signalled. With a park primitive installed they slept
//! out their whole bound before seeing data, and without one they fell back
//! to driving the transport for the full timeout.
//!
//! A [`WakeFanout`] is what the backend's slot holds instead: one callback
//! whose context is a fixed table of member wake contexts, each of which it
//! signals. The table is a `static` pool, so the pointer a backend holds can
//! never dangle, whichever executor leaves first.
//!
//! Lock-free, so the callback may run from a backend worker thread or an ISR:
//!
//! * the callback counts itself IN before it reads a slot, and OUT after;
//! * a member leaving clears its slot, then waits (yielding) until no callback
//!   is in flight. A callback that started after the clear cannot see the
//!   member; one that started before it is waited for. Both orders are
//!   `SeqCst`.
//!
//! An entry is keyed by the session's address. The owner of an OWNED session
//! claims an entry while its executor value may still move, so it re-keys the
//! entry when it hands the session's final address out
//! (`Executor::session_ptr`), which is what every tier is opened over.
//!
//! The cost is that every arrival wakes every member: the callback does not
//! say which executor the data is for. A member woken for another's data
//! drains its sessions non-blockingly and parks again.

// The executor's integration exists only with `alloc` + `rmw-cffi`; the
// table itself is also compiled for the unit tests below.
#![cfg_attr(not(all(feature = "alloc", feature = "rmw-cffi")), allow(dead_code))]

use core::ffi::c_void;
use portable_atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering::SeqCst};

/// Sessions that can be shared at once. Exhausting the pool is not an error:
/// the executor falls back to installing its own context directly, which is
/// the behaviour every executor had before this module.
pub(crate) const MAX_FANOUTS: usize = 4;
/// Executors per shared session: the boot executor plus its spawned tiers,
/// or the C executors over one support context. A member past this does not
/// join and is treated as having no async wake, so it never parks blind.
pub(crate) const MAX_FANOUT_MEMBERS: usize = 8;

/// What a member's context is signalled with (`nros_rmw_runtime_wake_cb` in a
/// real build; a test's own function in a unit test).
pub(crate) type WakeFn = unsafe extern "C" fn(*mut c_void);

pub(crate) struct WakeFanout {
    /// The session this entry serves; null = free.
    key: AtomicPtr<c_void>,
    /// Bumped on claim and on release, so a membership taken from an earlier
    /// tenancy of this entry can neither leave a later tenant's slot nor read
    /// it as live.
    generation: AtomicU32,
    /// The backend honours the callback the entry is installed as
    /// (`Session::supports_wake_callback` at install time).
    live: AtomicBool,
    inflight: AtomicU32,
    members: [AtomicPtr<c_void>; MAX_FANOUT_MEMBERS],
}

impl WakeFanout {
    const fn new() -> Self {
        Self {
            key: AtomicPtr::new(core::ptr::null_mut()),
            generation: AtomicU32::new(0),
            live: AtomicBool::new(false),
            inflight: AtomicU32::new(0),
            members: [const { AtomicPtr::new(core::ptr::null_mut()) }; MAX_FANOUT_MEMBERS],
        }
    }

    /// The pointer to hand a backend as the callback context.
    pub(crate) fn as_ctx(&'static self) -> *mut c_void {
        self as *const Self as *mut c_void
    }

    /// Signal every member with `wake`. What the backend's callback does.
    pub(crate) fn fire(&self, wake: WakeFn) {
        self.inflight.fetch_add(1, SeqCst);
        for slot in &self.members {
            let ctx = slot.load(SeqCst);
            if !ctx.is_null() {
                // SAFETY: a member's context stays valid until its slot is
                // cleared AND every callback that could have read it has
                // counted out, which `leave` waits for.
                unsafe { wake(ctx) };
            }
        }
        self.inflight.fetch_sub(1, SeqCst);
    }

    /// Join with `ctx`. `None` when every member slot is taken.
    pub(crate) fn join(&'static self, ctx: *mut c_void) -> Option<Membership> {
        let generation = self.generation.load(SeqCst);
        for (idx, slot) in self.members.iter().enumerate() {
            if slot
                .compare_exchange(core::ptr::null_mut(), ctx, SeqCst, SeqCst)
                .is_ok()
            {
                return Some(Membership {
                    fanout: self,
                    idx,
                    generation,
                });
            }
        }
        None
    }

    /// Members currently joined.
    pub(crate) fn member_count(&self) -> usize {
        self.members
            .iter()
            .filter(|s| !s.load(SeqCst).is_null())
            .count()
    }

    /// Record whether the backend honours the installed callback.
    pub(crate) fn set_live(&self, live: bool) {
        self.live.store(live, SeqCst);
    }

    /// Serve `key` from now on: the owner's session reached its final
    /// address.
    pub(crate) fn rekey(&self, key: *mut c_void) {
        self.key.store(key, SeqCst);
    }

    /// Return the entry to the pool. The caller has already taken the
    /// callback off the backend, so no NEW callback can start; this waits out
    /// any still in flight before the entry can be reused.
    pub(crate) fn release(&self, yield_now: fn()) {
        self.live.store(false, SeqCst);
        self.generation.fetch_add(1, SeqCst);
        for slot in &self.members {
            slot.store(core::ptr::null_mut(), SeqCst);
        }
        self.quiesce(yield_now);
        self.key.store(core::ptr::null_mut(), SeqCst);
    }

    fn quiesce(&self, yield_now: fn()) {
        while self.inflight.load(SeqCst) != 0 {
            yield_now();
        }
    }
}

/// One executor's place in a [`WakeFanout`].
#[derive(Clone, Copy)]
pub(crate) struct Membership {
    fanout: &'static WakeFanout,
    idx: usize,
    generation: u32,
}

impl Membership {
    pub(crate) fn fanout(&self) -> &'static WakeFanout {
        self.fanout
    }

    /// Is the entry still the tenancy this membership joined?
    pub(crate) fn is_current(&self) -> bool {
        self.fanout.generation.load(SeqCst) == self.generation
    }

    /// Does an arrival on the shared session still reach this member? False
    /// once the entry was released (the session's owner tore its callback
    /// down) or the backend never honoured it.
    pub(crate) fn is_live(&self) -> bool {
        self.is_current() && self.fanout.live.load(SeqCst)
    }

    /// Leave. After this returns no callback holds this member's context.
    /// A membership from an earlier tenancy of the entry is a no-op: `release`
    /// already cleared it, and the slot may now be someone else's.
    pub(crate) fn leave(self, yield_now: fn()) {
        if !self.is_current() {
            return;
        }
        self.fanout.members[self.idx].store(core::ptr::null_mut(), SeqCst);
        self.fanout.quiesce(yield_now);
    }
}

static FANOUTS: [WakeFanout; MAX_FANOUTS] = [const { WakeFanout::new() }; MAX_FANOUTS];

/// The entry serving `key`, if any.
pub(crate) fn find(key: *mut c_void) -> Option<&'static WakeFanout> {
    if key.is_null() {
        return None;
    }
    FANOUTS.iter().find(|f| f.key.load(SeqCst) == key)
}

/// Claim a free entry for `key`. `None` when the pool is exhausted.
pub(crate) fn claim(key: *mut c_void) -> Option<&'static WakeFanout> {
    if key.is_null() {
        return None;
    }
    for f in &FANOUTS {
        if f.key
            .compare_exchange(core::ptr::null_mut(), key, SeqCst, SeqCst)
            .is_ok()
        {
            f.generation.fetch_add(1, SeqCst);
            f.live.store(false, SeqCst);
            return Some(f);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{boxed::Box, sync::atomic::AtomicUsize, vec::Vec};

    // The pool is process-global and `cargo test` runs tests in parallel, so
    // every test holds `POOL`, keys its entries by its own static's address,
    // and releases what it claims.

    fn spin() {
        std::thread::yield_now();
    }

    /// The pool is shared by every test in the process; hold this while
    /// claiming from it.
    static POOL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[repr(C)]
    struct Probe {
        hits: AtomicUsize,
    }

    unsafe extern "C" fn count(ctx: *mut c_void) {
        let p = unsafe { &*(ctx as *const Probe) };
        p.hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn ctx(p: &Probe) -> *mut c_void {
        p as *const Probe as *mut c_void
    }

    #[test]
    fn one_arrival_wakes_every_member() {
        let _pool = POOL.lock().unwrap();
        static KEY: u8 = 0;
        let key = &KEY as *const u8 as *mut c_void;
        let f = claim(key).expect("a free entry");
        f.set_live(true);
        let (a, b, c) = (
            Probe {
                hits: AtomicUsize::new(0),
            },
            Probe {
                hits: AtomicUsize::new(0),
            },
            Probe {
                hits: AtomicUsize::new(0),
            },
        );
        let ma = f.join(ctx(&a)).unwrap();
        let mb = f.join(ctx(&b)).unwrap();
        let mc = f.join(ctx(&c)).unwrap();
        assert!(find(key).is_some_and(|g| core::ptr::eq(g, f)));
        f.fire(count);
        for p in [&a, &b, &c] {
            assert_eq!(p.hits.load(std::sync::atomic::Ordering::SeqCst), 1);
        }
        assert!(ma.is_live() && mb.is_live() && mc.is_live());

        // A member that left is not woken; the others still are.
        mb.leave(spin);
        f.fire(count);
        assert_eq!(a.hits.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(b.hits.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(c.hits.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(f.member_count(), 2);
        f.release(spin);
        assert!(find(key).is_none());
    }

    /// The owner tears its callback down while tiers are still joined: they
    /// read as not live (so the executor stops parking on a wake nothing will
    /// signal), and their later `leave` cannot touch the entry's next tenant.
    #[test]
    fn release_makes_every_membership_dead_and_its_leave_a_no_op() {
        let _pool = POOL.lock().unwrap();
        static KEY: u8 = 0;
        let f = claim(&KEY as *const u8 as *mut c_void).unwrap();
        f.set_live(true);
        let p = Probe {
            hits: AtomicUsize::new(0),
        };
        let tier = f.join(ctx(&p)).unwrap();
        assert!(tier.is_live());
        f.release(spin);
        assert!(!tier.is_live(), "a released entry is not a live wake");

        // The next tenant of the same entry.
        static KEY2: u8 = 0;
        let g = claim(&KEY2 as *const u8 as *mut c_void).unwrap();
        assert!(
            core::ptr::eq(f, g),
            "the pool hands out the first free entry"
        );
        let q = Probe {
            hits: AtomicUsize::new(0),
        };
        let fresh = g.join(ctx(&q)).unwrap();
        tier.leave(spin);
        assert_eq!(
            g.member_count(),
            1,
            "a stale membership cleared the new tenant's slot"
        );
        fresh.leave(spin);
        g.release(spin);
    }

    /// An entry whose backend does not honour the callback is never live, so a
    /// member of it falls back to the receive-blocking wait.
    #[test]
    fn a_backend_that_ignores_the_callback_is_not_live() {
        let _pool = POOL.lock().unwrap();
        static KEY: u8 = 0;
        let f = claim(&KEY as *const u8 as *mut c_void).unwrap();
        let p = Probe {
            hits: AtomicUsize::new(0),
        };
        let m = f.join(ctx(&p)).unwrap();
        assert!(!m.is_live());
        m.leave(spin);
        f.release(spin);
    }

    #[test]
    fn a_full_table_refuses_the_next_member() {
        let _pool = POOL.lock().unwrap();
        static KEY: u8 = 0;
        let f = claim(&KEY as *const u8 as *mut c_void).unwrap();
        let probes: Vec<Probe> = (0..=MAX_FANOUT_MEMBERS)
            .map(|_| Probe {
                hits: AtomicUsize::new(0),
            })
            .collect();
        let joined: Vec<_> = probes[..MAX_FANOUT_MEMBERS]
            .iter()
            .map(|p| f.join(ctx(p)).expect("room"))
            .collect();
        assert!(f.join(ctx(&probes[MAX_FANOUT_MEMBERS])).is_none());
        for m in joined {
            m.leave(spin);
        }
        f.release(spin);
    }

    /// `leave` must not return while a callback that read the member's context
    /// is still running: the context is freed right after. A callback is held
    /// open inside the member's wake; `leave` on another thread has to wait.
    #[test]
    fn leave_waits_out_a_callback_in_flight() {
        let _pool = POOL.lock().unwrap();
        use std::sync::atomic::{AtomicBool as StdBool, Ordering};
        static KEY: u8 = 0;
        static ENTERED: StdBool = StdBool::new(false);
        static RELEASE: StdBool = StdBool::new(false);
        unsafe extern "C" fn slow(_ctx: *mut c_void) {
            ENTERED.store(true, Ordering::SeqCst);
            while !RELEASE.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
        }
        let f = claim(&KEY as *const u8 as *mut c_void).unwrap();
        f.set_live(true);
        let p = Box::leak(Box::new(Probe {
            hits: AtomicUsize::new(0),
        }));
        let m = f.join(ctx(p)).unwrap();
        let firer = std::thread::spawn(move || f.fire(slow));
        while !ENTERED.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        let left = std::sync::Arc::new(StdBool::new(false));
        let left2 = left.clone();
        let leaver = std::thread::spawn(move || {
            m.leave(spin);
            left2.store(true, Ordering::SeqCst);
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !left.load(Ordering::SeqCst),
            "leave returned while a callback holding the context was running"
        );
        RELEASE.store(true, Ordering::SeqCst);
        firer.join().unwrap();
        leaver.join().unwrap();
        assert!(left.load(Ordering::SeqCst));
        f.release(spin);
    }
}
