//! Platform-selected mutex for the global type registry.
//!
//! * Hosted (`target_os = "linux" | "macos" | "windows" | "freebsd" |
//!   "netbsd" | "openbsd" | "dragonfly"`): [`spin::Mutex`]. Multi-thread,
//!   no ISR concerns; the registry is touched on every first-use
//!   `create_publisher` / `create_subscription`, so spin contention is
//!   negligible (one cache-miss per topic per process).
//! * Bare-metal (`target_os = "none"`): [`critical_section::Mutex`]
//!   wrapped in a thin newtype that mirrors the `spin::Mutex` lock
//!   surface. ISR-safe.
//! * Other (e.g. RTOS targets compiled with their own `target_os`):
//!   falls back to `spin::Mutex` — the platform's mutex layer already
//!   adapts via `nros-platform-critical-section`.
//!
//! The newtype-uniform [`RegistryMutex`] alias keeps the call-site in
//! [`crate::type_registry`] free of `#[cfg]`s.

#[cfg(target_os = "none")]
pub use cs::RegistryMutex;
#[cfg(not(target_os = "none"))]
pub use hosted::RegistryMutex;

#[cfg(not(target_os = "none"))]
mod hosted {
    /// Spin lock — fine on hosted multi-thread; the registry is
    /// touched once per topic per process.
    pub type RegistryMutex<T> = spin::Mutex<T>;
}

#[cfg(target_os = "none")]
mod cs {
    use core::cell::RefCell;

    /// Thin wrapper that adapts [`critical_section::Mutex`] to the
    /// `lock() -> Guard` shape the registry uses on hosted targets.
    ///
    /// `critical_section` masks interrupts for the duration of the
    /// closure body, which is fine here — descriptor-build work is
    /// O(fields) and runs on entity creation (not in the data path).
    pub struct RegistryMutex<T> {
        inner: critical_section::Mutex<RefCell<T>>,
    }

    impl<T> RegistryMutex<T> {
        pub const fn new(value: T) -> Self {
            Self {
                inner: critical_section::Mutex::new(RefCell::new(value)),
            }
        }

        /// Internal accessor used by the [`super::RegistryMutexExt`]
        /// blanket impl on this newtype. Kept `pub(super)` so callers
        /// always go through the trait — that keeps the
        /// `.with(|r| …)` call site shape identical between hosted
        /// and bare-metal arms.
        pub(super) fn with_inner<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
            critical_section::with(|cs| {
                let mut borrow = self.inner.borrow_ref_mut(cs);
                f(&mut *borrow)
            })
        }
    }
}

/// Adapter wrapper that gives [`spin::Mutex`] the same `.with()`
/// shape as the bare-metal newtype. Lets `type_registry` use one
/// call style under both `#[cfg]` arms.
pub trait RegistryMutexExt<T> {
    fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;
}

#[cfg(not(target_os = "none"))]
impl<T> RegistryMutexExt<T> for spin::Mutex<T> {
    fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut guard = self.lock();
        f(&mut *guard)
    }
}

#[cfg(target_os = "none")]
impl<T> RegistryMutexExt<T> for cs::RegistryMutex<T> {
    fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        cs::RegistryMutex::with_inner(self, f)
    }
}
