// Copyright (c) 2026, NEWSLab NTU.
// SPDX-License-Identifier: Apache-2.0

//! phase-436 B2 — smoltcp as a deadline source for the executor's park.
//!
//! The executor's park is a `min` over declared deadlines (phase-436 §2). Many
//! sources say WHEN; one primitive waits. Until this module every wired port
//! had a primitive and no source: `set_park_primitive` had three callers and
//! `register_wake_source` had none outside tests.
//!
//! smoltcp already knows the answer and the bridge threw it away.
//! [`Interface::poll_delay`] returns how long the stack may be left alone —
//! the next TCP retransmit, delayed ACK, TIME-WAIT expiry, ARP or IGMP timer.
//! `SmoltcpBridge::poll` called `iface.poll(...)` and discarded the timing, so
//! the executor's only bound on a bare-metal image was the caller's budget.
//!
//! # What crosses the ABI
//!
//! [`NextDeadlineFn`][nextfn] is `unsafe extern "C" fn(ctx) -> u64`:
//! microseconds FROM NOW, with [`NOTHING_PENDING_US`] (`u64::MAX`) meaning
//! "this source does not shorten the park". That constant is the seam's
//! existing convention for the absent case, so `poll_delay`'s `Option<Duration>`
//! round-trips without inventing a sentinel:
//!
//! | `poll_delay` | contributed | meaning |
//! |---|---|---|
//! | `None` | `u64::MAX` | nothing scheduled — never a `0`, which would be a busy loop |
//! | `Some(d)`, `d > 0` | `d` in µs, corrected (below) | park at most this long |
//! | `Some(0)` | `0` | overdue — service before parking at all |
//!
//! `None` and `Some(0)` are the two ends that must not be confused: reading
//! "nothing scheduled" as "due now" spins the executor, and reading "due now"
//! as "never" sleeps through a retransmit.
//!
//! # The time base, and where it truncates
//!
//! phase-436 §1 is "one time base, nanoseconds, no truncation". smoltcp's
//! [`Instant`] is its own base, and the bridge builds it as
//! `Instant::from_millis(now_ns / 1_000_000)` — a FLOOR. A delay measured
//! against a floored `now` is too LONG by the sub-millisecond remainder the
//! floor threw away, by up to 999 µs. Overstating a deadline is the harmful
//! direction: the executor would park past the moment smoltcp asked for.
//!
//! So the remainder is subtracted back out, rounded UP (`div_ceil`) and
//! saturating at zero, which is the same "round against yourself" rule W2
//! applied to the park itself. The result is never later than the true
//! deadline, and at worst one microsecond early.
//!
//! [nextfn]: https://docs.rs/nros-node
//! [`Interface::poll_delay`]: smoltcp::iface::Interface::poll_delay
//! [`Instant`]: smoltcp::time::Instant

use core::{
    ffi::c_void,
    sync::atomic::{AtomicPtr, Ordering},
};

use smoltcp::{
    iface::{Interface, SocketSet},
    time::{Duration, Instant},
};

unsafe extern "C" {
    /// The canonical platform wall clock, nanoseconds. Same symbol
    /// `SmoltcpBridge::poll` times its `iface.poll` with, so the deadline this
    /// module reports and the poll it is a deadline for share one clock.
    fn nros_platform_time_now_ns() -> u64;
}

/// What a source contributes when it has nothing to ask for.
///
/// The seam's own spelling (`NextDeadlineFn`'s doc comment): `u64::MAX` means
/// "nothing pending", and is how an async source declines to shorten the park
/// while still breaking it by signalling.
pub const NOTHING_PENDING_US: u64 = u64::MAX;

/// The smoltcp deadline source: the `(Interface, SocketSet)` pair the
/// executor may ask "when do you next need servicing?".
///
/// Two pointers, not three — [`Interface::poll_delay`] needs no `Device`,
/// which is why this is not generic over one and a single non-generic
/// `extern "C"` entry point can serve every board.
pub struct SmoltcpDeadlineSource {
    iface: AtomicPtr<Interface>,
    sockets: AtomicPtr<SocketSet<'static>>,
}

impl SmoltcpDeadlineSource {
    /// An unarmed source. Usable in `static` context.
    pub const fn new() -> Self {
        Self {
            iface: AtomicPtr::new(core::ptr::null_mut()),
            sockets: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// Point the source at a live interface and socket set.
    ///
    /// # Safety
    /// Both pointers must stay valid until [`disarm`][Self::disarm], and no
    /// other code may hold a `&mut` to either while a query is in flight.
    /// [`NetworkState::set`][crate::NetworkState::set] carries exactly this
    /// obligation and is the one caller, so arming the poll callback and
    /// arming the deadline source are a single act rather than two things to
    /// remember.
    pub unsafe fn arm(&self, iface: *mut Interface, sockets: *mut SocketSet<'static>) {
        self.iface.store(iface, Ordering::Release);
        self.sockets.store(sockets, Ordering::Release);
    }

    /// Stop answering. Subsequent queries report [`NOTHING_PENDING_US`].
    pub fn disarm(&self) {
        self.iface.store(core::ptr::null_mut(), Ordering::Release);
        self.sockets.store(core::ptr::null_mut(), Ordering::Release);
    }

    /// Whether this source can currently answer.
    pub fn is_armed(&self) -> bool {
        !self.iface.load(Ordering::Acquire).is_null()
            && !self.sockets.load(Ordering::Acquire).is_null()
    }

    /// The query, with the clock supplied — the arithmetic half, so a test can
    /// pin the time base instead of reading the platform clock.
    ///
    /// # Safety
    /// The armed pointers must still satisfy [`arm`][Self::arm]'s contract.
    pub unsafe fn next_deadline_us_at(&self, now_ns: u64) -> u64 {
        let iface = self.iface.load(Ordering::Acquire);
        let sockets = self.sockets.load(Ordering::Acquire);
        if iface.is_null() || sockets.is_null() {
            // Not armed: inert, not "due now". A board that never brings a
            // network up must keep the park it had.
            return NOTHING_PENDING_US;
        }
        let timestamp = Instant::from_millis((now_ns / 1_000_000) as i64);
        // SAFETY: `arm`'s contract — the pointers are live and unaliased for
        // the duration of this call. `poll_delay` takes `&mut Interface`
        // because it stamps `inner.now`; that is the same field
        // `SmoltcpBridge::poll` writes, and both run on the executor's thread.
        let delay = unsafe { (*iface).poll_delay(timestamp, &*sockets) };
        deadline_us_from(delay, now_ns)
    }
}

impl Default for SmoltcpDeadlineSource {
    fn default() -> Self {
        Self::new()
    }
}

// `AtomicPtr<T>` is unconditionally `Sync`; the pointees are reached only
// under `arm`'s unsafe contract, exactly as `NetworkState` does it.
unsafe impl Sync for SmoltcpDeadlineSource {}

/// Convert smoltcp's answer into the seam's `u64` microseconds-from-now,
/// undoing the millisecond floor the smoltcp time base imposed on `now`.
///
/// Split out from the pointer handling because this is where the truncation
/// lives, and the truncation is the part worth a test.
pub(crate) fn deadline_us_from(delay: Option<Duration>, now_ns: u64) -> u64 {
    let Some(delay) = delay else {
        return NOTHING_PENDING_US;
    };
    // `Instant::from_millis(now_ns / 1_000_000)` floored the clock, so
    // `delay` is measured from a moment up to 999 µs in the past and
    // overstates what is left. Give that back, rounding UP so the corrected
    // deadline is never later than the real one.
    let floored_away_us = (now_ns % 1_000_000).div_ceil(1_000);
    delay.total_micros().saturating_sub(floored_away_us)
}

/// The process-wide smoltcp deadline source.
///
/// A singleton because the bridge already is one — `SOCKET_TABLE`, the
/// staging buffers and the poll callback are all module statics, so an image
/// has exactly one smoltcp stack. Being a `'static` is also what makes the
/// raw `ctx` honest: the obvious hazard with this seam is a context pointer
/// outliving what it points at, and a `static` cannot.
pub static SMOLTCP_DEADLINE_SOURCE: SmoltcpDeadlineSource = SmoltcpDeadlineSource::new();

/// The `ctx` to register [`next_deadline_us`] with.
pub fn deadline_source_ctx() -> *mut c_void {
    &SMOLTCP_DEADLINE_SOURCE as *const SmoltcpDeadlineSource as *mut c_void
}

/// `NextDeadlineFn` for the smoltcp stack: microseconds until smoltcp next
/// needs servicing, or [`NOTHING_PENDING_US`].
///
/// Register it with
/// `executor.register_wake_source(nros_smoltcp::next_deadline_us, nros_smoltcp::deadline_source_ctx())`.
/// The signature is deliberately spelt as the raw C-ABI type rather than
/// importing `nros_node::NextDeadlineFn`: this is a driver crate and the core
/// is above it, so the fn-pointer type is the whole interface.
///
/// # Safety
/// `ctx` must be [`deadline_source_ctx`]'s value (or null, which reports
/// nothing pending), and the armed pointers must still be live.
pub unsafe extern "C" fn next_deadline_us(ctx: *mut c_void) -> u64 {
    if ctx.is_null() {
        return NOTHING_PENDING_US;
    }
    // SAFETY: by this function's contract `ctx` is a `&'static
    // SmoltcpDeadlineSource`.
    let source = unsafe { &*(ctx as *const SmoltcpDeadlineSource) };
    // SAFETY: the extern clock is the platform ABI's, resolved for every
    // image that links this crate.
    let now_ns = unsafe { nros_platform_time_now_ns() };
    // SAFETY: `arm`'s contract, upheld by `NetworkState::set`.
    unsafe { source.next_deadline_us_at(now_ns) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The busy-loop control. `poll_delay` returning `None` means "nothing
    /// scheduled"; reporting that as `0` would make the executor's bound zero,
    /// which skips the park and drains the transport non-blockingly — a spin,
    /// forever, on an idle stack.
    #[test]
    fn nothing_scheduled_is_max_not_zero() {
        assert_eq!(deadline_us_from(None, 1_234_567), NOTHING_PENDING_US);
    }

    /// And the other end: `Some(0)` is "overdue", which must NOT become
    /// `u64::MAX`. Sleeping out a full budget with a retransmit already due is
    /// the failure this source exists to remove.
    #[test]
    fn an_overdue_stack_is_zero_not_max() {
        assert_eq!(deadline_us_from(Some(Duration::ZERO), 5_000_000), 0);
    }

    /// The truncation control (phase-436 §1, issue 1193's class). smoltcp is
    /// asked at a FLOORED millisecond, so its answer is too long by the
    /// remainder; the correction hands that back and never overstates.
    #[test]
    fn the_millisecond_floor_is_given_back_not_kept() {
        // 400 µs into the millisecond: a 5 ms answer is really 4.6 ms away.
        let now_ns = 7_000_000 + 400_000;
        assert_eq!(
            deadline_us_from(Some(Duration::from_millis(5)), now_ns),
            4_600
        );

        // Exactly on a millisecond: nothing was thrown away, nothing to give
        // back.
        assert_eq!(
            deadline_us_from(Some(Duration::from_millis(5)), 7_000_000),
            5_000
        );

        // One nanosecond in still costs a whole microsecond: the correction
        // rounds UP, against itself, so the reported deadline is early rather
        // than late.
        assert_eq!(
            deadline_us_from(Some(Duration::from_millis(5)), 7_000_001),
            4_999
        );
    }

    /// A correction larger than the delay saturates at zero — "service me
    /// now" — rather than wrapping to `u64::MAX`, which would read as
    /// "nothing pending" and sleep straight through it.
    #[test]
    fn a_correction_bigger_than_the_delay_saturates_at_zero() {
        assert_eq!(
            deadline_us_from(Some(Duration::from_micros(100)), 999_000),
            0
        );
    }

    /// An unarmed source is INERT, which is what lets a board register it
    /// before (or without) network bring-up.
    #[test]
    fn an_unarmed_source_reports_nothing_pending() {
        let source = SmoltcpDeadlineSource::new();
        assert!(!source.is_armed());
        assert_eq!(
            unsafe { source.next_deadline_us_at(1_000_000) },
            NOTHING_PENDING_US
        );
    }

    /// A null `ctx` is the same inert answer rather than a dereference.
    #[test]
    fn a_null_ctx_reports_nothing_pending() {
        assert_eq!(
            unsafe { next_deadline_us(core::ptr::null_mut()) },
            NOTHING_PENDING_US
        );
    }

    /// End to end against a REAL `Interface`: a TCP socket that has sent a SYN
    /// to a peer that will never answer owes a retransmit, and smoltcp knows
    /// when. Before this module that number existed and nothing could read it.
    #[test]
    fn a_live_interface_reports_its_retransmit_deadline() {
        use smoltcp::{
            iface::{Config, SocketSet},
            phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
            socket::tcp,
            wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr},
        };

        /// A device that swallows every frame and never receives one, so the
        /// SYN goes out and no SYN-ACK comes back.
        struct NullDevice;
        struct NullTx;
        impl TxToken for NullTx {
            fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
                let mut buf = [0u8; 1536];
                f(&mut buf[..len])
            }
        }
        impl Device for NullDevice {
            type RxToken<'a> = NullTx;
            type TxToken<'a> = NullTx;
            fn receive(&mut self, _: Instant) -> Option<(NullTx, NullTx)> {
                None
            }
            fn transmit(&mut self, _: Instant) -> Option<NullTx> {
                Some(NullTx)
            }
            fn capabilities(&self) -> DeviceCapabilities {
                let mut caps = DeviceCapabilities::default();
                caps.medium = Medium::Ethernet;
                caps.max_transmission_unit = 1500;
                caps
            }
        }
        impl RxToken for NullTx {
            fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
                f(&[])
            }
        }

        let mut device = NullDevice;
        let mut config = Config::new(HardwareAddress::Ethernet(EthernetAddress([
            2, 0, 0, 0, 0, 1,
        ])));
        config.random_seed = 0x5eed;
        let start = Instant::from_millis(0);
        let mut iface = Interface::new(config, &mut device, start);
        iface.update_ip_addrs(|addrs| {
            addrs
                .push(IpCidr::new(IpAddress::v4(192, 0, 2, 1), 24))
                .unwrap();
        });

        let mut rx = [0u8; 256];
        let mut tx = [0u8; 256];
        let socket = tcp::Socket::new(
            tcp::SocketBuffer::new(&mut rx[..]),
            tcp::SocketBuffer::new(&mut tx[..]),
        );
        let mut storage: [smoltcp::iface::SocketStorage; 1] = Default::default();
        let mut sockets = SocketSet::new(&mut storage[..]);
        let handle = sockets.add(socket);
        sockets
            .get_mut::<tcp::Socket>(handle)
            .connect(iface.context(), (IpAddress::v4(192, 0, 2, 2), 7447), 49152)
            .expect("connect must be accepted");

        // Flush the SYN so the socket owes a retransmit rather than an
        // immediate send.
        iface.poll(start, &mut device, &mut sockets);

        let source = SmoltcpDeadlineSource::new();
        // SAFETY: both live for the rest of this test, and nothing else holds
        // a reference while the query runs.
        unsafe {
            source.arm(
                &mut iface as *mut Interface,
                &mut sockets as *mut SocketSet<'_> as *mut SocketSet<'static>,
            )
        };
        assert!(source.is_armed());

        let due_us = unsafe { source.next_deadline_us_at(0) };
        assert_ne!(
            due_us, NOTHING_PENDING_US,
            "a socket awaiting a SYN retransmit has a deadline, and the \
             source must report it"
        );
        // MEASURED: smoltcp's initial RTO is 1 s, and that is what reaches the
        // seam. The number is pinned rather than bounded so that a change in
        // units — the classic way a deadline source goes wrong — fails here.
        assert_eq!(due_us, 1_000_000);

        // And the floor correction is live on the real path, not only in
        // `deadline_us_from`: half a millisecond into the same millisecond,
        // smoltcp still answers 1 s from a FLOORED `now`, so the true
        // remaining time is 500 µs less.
        assert_eq!(unsafe { source.next_deadline_us_at(500_000) }, 999_500);

        source.disarm();
        assert_eq!(
            unsafe { source.next_deadline_us_at(0) },
            NOTHING_PENDING_US,
            "a disarmed source stops shortening the park"
        );
    }
}
