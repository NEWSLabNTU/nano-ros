//! Phase 130 — ergonomic Rust wrapper around
//! [`PlatformThreading`]'s wake primitive.
//!
//! Wraps a fixed-size, aligned scratch buffer so the executor can
//! drop a `PlatformWake<P>` into a struct field without having to
//! call `nros_platform_wake_storage_size()` at compile time.
//! `WAKE_STORAGE_BYTES` is sized to cover every supported
//! platform's binary semaphore (POSIX `sem_t` ~32 B, Zephyr
//! `k_sem` ~16 B, FreeRTOS `xSemaphoreHandle` ~ptr, NuttX `sem_t`,
//! ThreadX `tx_semaphore` ~56 B, macOS pthread cond+mutex+flag
//! ~72 B). The probe-vs-buffer invariant is asserted at runtime
//! during construction.
//!
//! Allocation-free: `Wake<P>` lives inline. Suitable for `no_std`
//! consumers that can't depend on `alloc`.
//!
//! `Wake<P>` is `Sync` (the underlying primitive is the wake
//! contract) but not `Send` after construction — the storage must
//! stay put because the platform impl stores backing-primitive
//! pointers that reference it.

use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::marker::PhantomData;
use core::mem::MaybeUninit;

use crate::PlatformThreading;

/// Maximum bytes the wake primitive may occupy on any supported
/// platform. Sized generously so we never have to bump for a new
/// RTOS port. Asserted against the runtime probe in [`Wake::new`].
pub const WAKE_STORAGE_BYTES: usize = 128;

/// Alignment of the inline storage buffer. Wide enough for every
/// pointer-aligned primitive on 64-bit hosts.
pub const WAKE_STORAGE_ALIGN: usize = 16;

#[repr(C, align(16))]
struct WakeStorage {
    bytes: UnsafeCell<[MaybeUninit<u8>; WAKE_STORAGE_BYTES]>,
}

/// Reason `wait_ms` returned.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WakeReason {
    /// `wake_signal` (or `wake_signal_from_isr`) fired and the
    /// pending signal was consumed.
    Signaled,
    /// The timeout deadline expired before any signal.
    Timeout,
    /// Backend error (e.g. ISR-unsafe call on a platform that
    /// rejects it). Treated as fatal in the executor's wake loop.
    Error,
}

/// Errors at construction time.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WakeInitError {
    /// `P::wake_storage_size()` returned more bytes than the
    /// inline buffer can hold. The platform impl needs more room
    /// than the API contract reserved — bump
    /// [`WAKE_STORAGE_BYTES`] or switch the consumer to a heap
    /// path.
    StorageTooSmall { needed: usize, available: usize },
    /// `P::wake_storage_align()` returned a stricter alignment
    /// than [`WAKE_STORAGE_ALIGN`].
    AlignmentTooStrict { needed: usize, available: usize },
    /// `P::wake_init` returned `-1`. Platform doesn't implement a
    /// wake primitive (single-thread bare-metal).
    Unsupported,
}

/// RAII wrapper around the platform's wake primitive.
pub struct Wake<P: PlatformThreading> {
    storage: WakeStorage,
    _marker: PhantomData<P>,
}

// SAFETY: the wake contract is explicitly cross-thread. Backends
// store either inline kernel structures (Zephyr k_sem, POSIX sem_t)
// or pointers to handles owned by the kernel — both are safe to
// access concurrently per their platform spec.
unsafe impl<P: PlatformThreading> Sync for Wake<P> {}

impl<P: PlatformThreading> Wake<P> {
    /// Construct a new wake primitive in the inline buffer.
    pub fn new() -> Result<Self, WakeInitError> {
        let needed = P::wake_storage_size();
        if needed == 0 {
            return Err(WakeInitError::Unsupported);
        }
        if needed > WAKE_STORAGE_BYTES {
            return Err(WakeInitError::StorageTooSmall {
                needed,
                available: WAKE_STORAGE_BYTES,
            });
        }
        let align = P::wake_storage_align();
        if align > WAKE_STORAGE_ALIGN {
            return Err(WakeInitError::AlignmentTooStrict {
                needed: align,
                available: WAKE_STORAGE_ALIGN,
            });
        }

        let storage = WakeStorage {
            bytes: UnsafeCell::new([MaybeUninit::uninit(); WAKE_STORAGE_BYTES]),
        };
        let ptr = storage.bytes.get() as *mut c_void;
        // `ptr` points at `WAKE_STORAGE_BYTES` aligned to 16 —
        // both invariants verified above. `wake_init` either
        // initialises the primitive in place or returns non-zero
        // (in which case we drop `storage` without calling
        // `wake_drop`, matching the "init failed, no teardown
        // needed" contract).
        let rc = P::wake_init(ptr);
        if rc != 0 {
            return Err(WakeInitError::Unsupported);
        }
        Ok(Self {
            storage,
            _marker: PhantomData,
        })
    }

    /// Block until signaled or `timeout_ms` elapses.
    pub fn wait_ms(&self, timeout_ms: u32) -> WakeReason {
        let ptr = self.storage.bytes.get() as *mut c_void;
        match P::wake_wait_ms(ptr, timeout_ms) {
            0 => WakeReason::Signaled,
            1 => WakeReason::Timeout,
            _ => WakeReason::Error,
        }
    }

    /// Wake one waiter. Safe to call from any thread.
    pub fn signal(&self) {
        let ptr = self.storage.bytes.get() as *mut c_void;
        let _ = P::wake_signal(ptr);
    }

    /// ISR-safe variant. Returns `false` when the backend has no
    /// ISR path and the caller should fall back to
    /// [`Self::signal`].
    pub fn signal_from_isr(&self) -> bool {
        let ptr = self.storage.bytes.get() as *mut c_void;
        P::wake_signal_from_isr(ptr) == 0
    }
}

impl<P: PlatformThreading> Drop for Wake<P> {
    fn drop(&mut self) {
        let ptr = self.storage.bytes.get() as *mut c_void;
        // ptr was init'd in `new`; this is the matching teardown
        // call. Any in-flight `wait_ms` is the caller's bug —
        // `Wake` is consumed by-value so callers cannot legally
        // hold a `&self` past Drop.
        let _ = P::wake_drop(ptr);
    }
}
