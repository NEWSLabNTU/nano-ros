//! Issue 2745 -- the hosted ThreadX Linux port's timer interrupt wakes a
//! parked waiter through the interrupt-safe wake signal.
//!
//! The hosted twin of `isr-wake-threadx-riscv64`. This port's only interrupt is
//! its timer thread: a host thread that brackets each tick with
//! `_tx_thread_context_save` / `_tx_thread_context_restore` and runs
//! `_tx_timer_interrupt` in between, which ThreadX treats as interrupt context.
//! The image links with `--wrap=_tx_timer_interrupt` (its `build.rs`), so
//! [`__wrap__tx_timer_interrupt`] runs the port's own tick and then this
//! image's hook, still inside that bracket. Every [`TICKS_PER_FIRE`] ticks the
//! hook stamps the clock and calls `nros_platform_wake_signal_from_isr`, the
//! call `GuardCondition::trigger_from_isr` reaches.
//!
//! The app thread parks on the same wake object with a 1 s budget and nothing
//! else to wake it, so only the interrupt can end a park early. It opens no
//! session: an executor needs one, and this port reaches a router only over
//! the veth bridge, so the image drives the wake object the executor parks
//! on directly.
//!
//! Each wake prints `ISR_WAKE <latency_us>`; after [`SAMPLES`] the image prints
//! `ISR_WAKE_NOT_IN_ISR <n>` (hook runs outside interrupt context, which must
//! be none) and `ISR_WAKE_DONE <fires> <wakes> <refused>`, then exits.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use nros_board_threadx_linux::ThreadxLinux;
use nros_log::{Logger, Severity, log_info, register_logger};

static LOGGER: Logger = Logger::new("isr_wake");

/// ThreadX ticks at 100 Hz on this board, so the interrupt fires every 50 ms:
/// a host-scheduled wake has tails of several ms under load, and the test
/// holds each wake to arriving before the next interrupt (issue 2745).
const TICKS_PER_FIRE: u32 = 5;
/// How many wakes the run collects.
const SAMPLES: u32 = 50;
/// Parks that end on their deadline before the run gives up.
const MAX_TIMEOUTS: u32 = 5;

/// Storage for the platform wake object (a `TX_SEMAPHORE` on this port),
/// over-sized and over-aligned; `main` checks the port's own size and
/// alignment against it.
#[repr(C, align(16))]
struct WakeStorage([u8; 256]);
static mut WAKE: WakeStorage = WakeStorage([0; 256]);

/// The hook acts only once the wake object exists and the run has started.
static ARMED: AtomicBool = AtomicBool::new(false);
/// Ticks seen since arming.
static TICKS: AtomicU32 = AtomicU32::new(0);
/// Interrupts that signalled the wake.
static FIRES: AtomicU32 = AtomicU32::new(0);
/// Interrupts whose signal the port refused.
static REFUSED: AtomicU32 = AtomicU32::new(0);
/// Hook runs that were not in interrupt context.
static NOT_IN_ISR: AtomicU32 = AtomicU32::new(0);
/// The clock when the last signal fired, in ns.
static FIRED_AT: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" {
    fn __real__tx_timer_interrupt();
    /// The port's interrupt nesting count: non-zero inside the context
    /// save/restore bracket. A `ULONG`, 32 bits on this 64-bit port.
    static _tx_thread_system_state: u32;
    fn nros_platform_wake_storage_size() -> usize;
    fn nros_platform_wake_storage_align() -> usize;
    fn nros_platform_wake_init(w: *mut core::ffi::c_void) -> i8;
    fn nros_platform_wake_wait_ms(w: *mut core::ffi::c_void, timeout_ms: u32) -> i8;
    fn nros_platform_wake_signal_from_isr(w: *mut core::ffi::c_void) -> i8;
    fn clock_gettime(clock: i32, ts: *mut Timespec) -> i32;
    fn nros_platform_panic(msg: *const u8, len: usize) -> !;
}

/// glibc's `struct timespec` on a 64-bit host.
#[repr(C)]
struct Timespec {
    sec: i64,
    nsec: i64,
}

/// The host's monotonic clock in ns. The port's tick clock counts in 10 ms,
/// too coarse to time a wake; this one is a lock-free vDSO read, safe from the
/// timer thread.
fn now_ns() -> u64 {
    const CLOCK_MONOTONIC: i32 = 1;
    let mut ts = Timespec { sec: 0, nsec: 0 };
    // SAFETY: a valid out-pointer; CLOCK_MONOTONIC always exists on Linux.
    unsafe { clock_gettime(CLOCK_MONOTONIC, &mut ts) };
    ts.sec as u64 * 1_000_000_000 + ts.nsec as u64
}

fn wake_ptr() -> *mut core::ffi::c_void {
    // SAFETY: only the address is taken; the port owns the bytes after init.
    unsafe { core::ptr::addr_of_mut!(WAKE).cast() }
}

/// The port's timer interrupt, then this image's tick hook, both inside the
/// timer thread's context save/restore.
#[unsafe(no_mangle)]
extern "C" fn __wrap__tx_timer_interrupt() {
    // SAFETY: the port's own tick, called exactly where the port calls it.
    unsafe { __real__tx_timer_interrupt() };
    if !ARMED.load(Ordering::Acquire) {
        return;
    }
    if TICKS.fetch_add(1, Ordering::Relaxed) % TICKS_PER_FIRE != TICKS_PER_FIRE - 1 {
        return;
    }
    // SAFETY: a read of the port's nesting count, written under its mutex.
    if unsafe { core::ptr::read_volatile(core::ptr::addr_of!(_tx_thread_system_state)) } == 0 {
        NOT_IN_ISR.fetch_add(1, Ordering::Relaxed);
    }
    FIRES.fetch_add(1, Ordering::Relaxed);
    FIRED_AT.store(now_ns(), Ordering::Release);
    // SAFETY: the wake object was initialised before the hook was armed.
    if unsafe { nros_platform_wake_signal_from_isr(wake_ptr()) } != 0 {
        REFUSED.fetch_add(1, Ordering::Relaxed);
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write as _;
    struct Buf {
        bytes: [u8; 192],
        used: usize,
    }
    impl core::fmt::Write for Buf {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let room = self.bytes.len() - self.used;
            let n = s.len().min(room);
            self.bytes[self.used..self.used + n].copy_from_slice(&s.as_bytes()[..n]);
            self.used += n;
            Ok(())
        }
    }
    let mut buf = Buf {
        bytes: [0u8; 192],
        used: 0,
    };
    let _ = write!(buf, "{info}");
    // SAFETY: `buf.bytes[..used]` is initialised and outlives the diverging call.
    unsafe { nros_platform_panic(buf.bytes.as_ptr(), buf.used) }
}

#[unsafe(no_mangle)]
extern "C" fn main(_argc: i32, _argv: *const *const core::ffi::c_char) -> i32 {
    let _ = ThreadxLinux::run_bare(|| {
        register_logger(&LOGGER);
        LOGGER.set_level(Severity::Info);
        // SAFETY: plain queries of the port's layout.
        let (size, align) =
            unsafe { (nros_platform_wake_storage_size(), nros_platform_wake_storage_align()) };
        assert!(
            size <= core::mem::size_of::<WakeStorage>()
                && align <= core::mem::align_of::<WakeStorage>(),
            "the wake object needs {size} bytes at {align}"
        );
        // SAFETY: the storage is static, sized and aligned above.
        assert_eq!(unsafe { nros_platform_wake_init(wake_ptr()) }, 0, "wake init");
        log_info!(&LOGGER, "ISR_WAKE_READY period_us={}", TICKS_PER_FIRE * 10_000);
        ARMED.store(true, Ordering::Release);

        let mut wakes = 0u32;
        let mut timeouts = 0u32;
        while wakes < SAMPLES && timeouts < MAX_TIMEOUTS {
            // SAFETY: the initialised wake object; this thread is a ThreadX thread.
            match unsafe { nros_platform_wake_wait_ms(wake_ptr(), 1000) } {
                0 => {
                    let late = now_ns() as i64 - FIRED_AT.load(Ordering::Acquire) as i64;
                    wakes += 1;
                    log_info!(&LOGGER, "ISR_WAKE {}", late / 1000);
                }
                1 => timeouts += 1,
                rc => panic!("wake wait failed: {rc}"),
            }
        }
        ARMED.store(false, Ordering::Release);
        log_info!(&LOGGER, "ISR_WAKE_NOT_IN_ISR {}", NOT_IN_ISR.load(Ordering::Relaxed));
        log_info!(
            &LOGGER,
            "ISR_WAKE_DONE {} {} {}",
            FIRES.load(Ordering::Relaxed),
            wakes,
            REFUSED.load(Ordering::Relaxed)
        );
        nros_log::flush();
        Ok::<(), &'static str>(())
    });
    0
}
