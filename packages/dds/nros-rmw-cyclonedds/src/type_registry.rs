//! Bounded type registry.
//!
//! [`TypeRegistry`] memoises [`DescriptorBuilder::build`] results by
//! ROS type name. On a fresh `register::<M>()` / `get_or_build::<M>()`,
//! the registry calls the descriptor builder and inserts the resulting
//! pointer; subsequent calls return the cached pointer in O(1).
//!
//! The global [`global`] handle wraps a [`TypeRegistry`] in a
//! platform-selected mutex (see [`crate::sync`]) so it can be called
//! from any thread / ISR-safe context.
//!
//! # Sizing knob
//!
//! [`MAX_TYPES`] is set at compile time by `NROS_CYCLONEDDS_MAX_TYPES`
//! (default `32`). Overflow returns [`BuildError::RegistryFull`] from
//! [`TypeRegistry::get_or_build`] — no panic. Document the recovery
//! hint by surfacing the error variant to callers; bumping the env
//! knob + rebuilding restores capacity.
//!
//! # Concurrency
//!
//! All paths take the global mutex. The descriptor build itself runs
//! inside the lock — that's slightly pessimistic (two threads racing
//! to register different types serialise) but keeps the lock-free
//! cache path trivially correct and avoids the "double build" race.
//! For embedded targets the lock degrades to a [`critical_section`],
//! which masks interrupts; build work is O(fields), so the window is
//! short. The data path (pub/sub `take` / `write`) never touches
//! the registry — only entity creation does.

use heapless::FnvIndexMap;
use nros_serdes::schema::{Field, Message};

use crate::{
    bridge::nros_rmw_cyclonedds_register_descriptor,
    dynamic_type::{BuildError, DescriptorBuilder, DescriptorPtr},
    sync::{RegistryMutex, RegistryMutexExt},
};

/// Maximum number of distinct ROS message types the registry can
/// cache simultaneously. Compile-time knob:
/// `NROS_CYCLONEDDS_MAX_TYPES=<N>`. Default `32`.
///
/// Cost: `16 bytes × MAX_TYPES` plus the [`heapless::FnvIndexMap`]
/// overhead (rounded up to the next power of two — heapless'
/// constraint). Default 32 → 512 bytes static.
pub const MAX_TYPES: usize = parse_env_usize(option_env!("NROS_CYCLONEDDS_MAX_TYPES"), 32);

const _: () = assert!(MAX_TYPES >= 1, "MAX_TYPES must be at least 1");
const _: () = assert!(
    MAX_TYPES.is_power_of_two(),
    "MAX_TYPES must be a power of two (heapless::FnvIndexMap constraint)"
);

const fn parse_env_usize(s: Option<&str>, default: usize) -> usize {
    match s {
        None => default,
        Some(s) => {
            let bytes = s.as_bytes();
            if bytes.is_empty() {
                panic!("NROS_CYCLONEDDS_MAX_TYPES set but empty");
            }
            let mut acc: usize = 0;
            let mut i = 0;
            while i < bytes.len() {
                let b = bytes[i];
                if b < b'0' || b > b'9' {
                    panic!("non-digit in NROS_CYCLONEDDS_MAX_TYPES");
                }
                acc = acc * 10 + (b - b'0') as usize;
                i += 1;
            }
            acc
        }
    }
}

/// Wrapper around the raw `*const c_void` descriptor pointer.
///
/// `heapless::FnvIndexMap`'s value type must be `Sized`; the raw
/// pointer is, but isn't `Send`/`Sync` by default. The registry is
/// guarded by the global mutex so we can safely mark the wrapper.
#[derive(Copy, Clone, Debug)]
struct DescriptorSlot(DescriptorPtr);

unsafe impl Send for DescriptorSlot {}
unsafe impl Sync for DescriptorSlot {}

/// Bounded `type_name → descriptor` cache.
pub struct TypeRegistry {
    table: FnvIndexMap<&'static str, DescriptorSlot, MAX_TYPES>,
}

impl TypeRegistry {
    /// Empty registry — `const` so it can back a `static`.
    pub const fn new() -> Self {
        Self {
            table: FnvIndexMap::new(),
        }
    }

    /// O(1) cache lookup. Returns the cached descriptor pointer or
    /// `None` if not yet built. Does NOT trigger a build.
    pub fn get(&self, type_name: &str) -> Option<DescriptorPtr> {
        self.table.get(type_name).map(|s| s.0)
    }

    /// Cache lookup + lazy build.
    ///
    /// If a descriptor for `M::TYPE_NAME` is already cached, returns
    /// it. Otherwise calls [`DescriptorBuilder::build::<M>`], inserts
    /// the result, and also calls
    /// [`nros_rmw_cyclonedds_register_descriptor`] so the existing
    /// C++ `descriptors.cpp` table is in sync (legacy
    /// `publisher.cpp`/`subscriber.cpp` paths resolve via that
    /// table).
    pub fn get_or_build<M: Message>(&mut self) -> Result<DescriptorPtr, BuildError> {
        if let Some(ptr) = self.get(M::TYPE_NAME) {
            return Ok(ptr);
        }
        let ptr = DescriptorBuilder::build::<M>()?;
        // Insert before calling the C++ side so a subsequent retry
        // (e.g. from a contending caller waiting on the mutex) finds
        // the new entry even if `register_descriptor` is a no-op.
        self.table
            .insert(M::TYPE_NAME, DescriptorSlot(ptr))
            .map_err(|_| BuildError::RegistryFull)?;
        // Mirror into the C++ registry. `register_descriptor` accepts
        // a NUL-terminated C string; the codegen template will emit
        // `c"…"` constants and `M::TYPE_NAME` will eventually be a
        // `&'static CStr`. Until that lands, we pass the same buffer
        // the builder validated. Bridge stub no-ops under test.
        //
        // SAFETY: the bridge does not capture the pointer beyond the
        // call; we hold the registry mutex throughout.
        unsafe {
            nros_rmw_cyclonedds_register_descriptor(
                // The codegen contract for K.7.6 is that TYPE_NAME is
                // NUL-terminated. For the test fixtures here it
                // already is (see dynamic_type.rs tests). Pass the
                // start of the byte slice — the bridge reads to NUL.
                M::TYPE_NAME.as_ptr() as *const core::ffi::c_char,
                ptr,
            );
        }
        Ok(ptr)
    }

    /// Phase 248 (C2) — raw-schema variant of [`Self::get_or_build`].
    ///
    /// Identical caching/build behaviour, but takes the flattened
    /// `(type_name, fields)` pair directly instead of a `M: Message`
    /// type parameter. This is what the generic descriptor-registration
    /// seam (`nros_rmw::register_type_descriptor`) forwards into, so the
    /// core executor no longer needs a monomorphised `register::<M>()`
    /// call into this crate.
    pub fn get_or_build_raw(
        &mut self,
        type_name: &'static str,
        fields: &'static [Field],
    ) -> Result<DescriptorPtr, BuildError> {
        if let Some(ptr) = self.get(type_name) {
            return Ok(ptr);
        }
        let ptr = DescriptorBuilder::build_raw(type_name, fields)?;
        self.table
            .insert(type_name, DescriptorSlot(ptr))
            .map_err(|_| BuildError::RegistryFull)?;
        // SAFETY: the bridge does not capture the pointer beyond the
        // call; we hold the registry mutex throughout. `type_name` is
        // NUL-terminated per the codegen contract.
        unsafe {
            nros_rmw_cyclonedds_register_descriptor(
                type_name.as_ptr() as *const core::ffi::c_char,
                ptr,
            );
        }
        Ok(ptr)
    }

    /// Diagnostic — number of cached types. Useful for tests.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// `true` iff no types are cached.
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// Test-only — clear the cache. Lives behind `#[cfg(test)]` or
    /// the `bridge-stub` feature so production code can't accidentally
    /// invalidate descriptor pointers Cyclone is still using. The
    /// `bridge-stub` arm is for downstream crates' test builds (notably
    /// `nros-node`'s K.7.6.b smoke).
    #[cfg(any(test, feature = "bridge-stub"))]
    pub fn clear_for_test(&mut self) {
        self.table.clear();
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-global registry, mutex-guarded.
static GLOBAL: RegistryMutex<TypeRegistry> = RegistryMutex::new(TypeRegistry::new());

/// Borrow the global type registry.
///
/// The returned reference is `'static`; lock it via
/// [`RegistryMutexExt::with`] to get exclusive access. The mutex flavour
/// is platform-selected — see [`crate::sync`].
pub fn global() -> &'static RegistryMutex<TypeRegistry> {
    &GLOBAL
}

/// Convenience wrapper: locks the global registry, calls
/// [`TypeRegistry::get_or_build::<M>`], and returns the descriptor
/// pointer (or [`BuildError`]).
///
/// This is the K.7.6 entry point: pub/sub creation paths call
/// `register::<M>()` to ensure the Cyclone descriptor exists before
/// `dds_create_topic`.
pub fn register<M: Message>() -> Result<DescriptorPtr, BuildError> {
    GLOBAL.with(|r| r.get_or_build::<M>())
}

/// Phase 248 (C2) — raw-schema entry point used by the generic
/// descriptor-registration seam.
///
/// Locks the global registry and calls
/// [`TypeRegistry::get_or_build_raw`]. This is the function the
/// installed [`nros_rmw::TypeDescriptorRegistrar`] wraps, letting the
/// platform/RMW-agnostic core (`nros-node`) register a type's Cyclone
/// descriptor without a direct `register::<M>()` monomorphisation into
/// this crate.
pub fn register_raw(
    type_name: &'static str,
    fields: &'static [Field],
) -> Result<DescriptorPtr, BuildError> {
    GLOBAL.with(|r| r.get_or_build_raw(type_name, fields))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::test_stub::BUILD_COUNTER;
    use core::sync::atomic::Ordering;
    use nros_serdes::schema::{Field, FieldType};

    struct A;
    impl Message for A {
        const TYPE_NAME: &'static str = "test_msgs/msg/A\0";
        const FIELDS: &'static [Field] = &[Field {
            name: "x\0",
            ty: FieldType::Int32,
            offset: 0,
        }];
    }
    struct B;
    impl Message for B {
        const TYPE_NAME: &'static str = "test_msgs/msg/B\0";
        const FIELDS: &'static [Field] = &[Field {
            name: "y\0",
            ty: FieldType::Uint64,
            offset: 0,
        }];
    }
    struct C;
    impl Message for C {
        const TYPE_NAME: &'static str = "test_msgs/msg/C\0";
        const FIELDS: &'static [Field] = &[Field {
            name: "z\0",
            ty: FieldType::Float32,
            offset: 0,
        }];
    }

    #[test]
    fn const_max_types_default_is_32() {
        // Default sizing — keeps the docs honest.
        assert_eq!(MAX_TYPES, 32);
    }

    #[test]
    fn get_or_build_caches_subsequent_lookups() {
        let mut r = TypeRegistry::new();
        BUILD_COUNTER.store(0, Ordering::SeqCst);

        let p1 = r.get_or_build::<A>().expect("first build");
        let p2 = r.get_or_build::<A>().expect("second lookup");
        let p3 = r.get_or_build::<A>().expect("third lookup");
        assert_eq!(p1, p2);
        assert_eq!(p2, p3);
        // Only the first call hits the bridge stub; the rest are
        // cache hits.
        assert_eq!(BUILD_COUNTER.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn distinct_types_get_distinct_entries() {
        let mut r = TypeRegistry::new();
        let pa = r.get_or_build::<A>().unwrap();
        let pb = r.get_or_build::<B>().unwrap();
        let pc = r.get_or_build::<C>().unwrap();
        assert_eq!(r.len(), 3);
        // Cache lookup by name finds the same pointers.
        assert_eq!(r.get(A::TYPE_NAME), Some(pa));
        assert_eq!(r.get(B::TYPE_NAME), Some(pb));
        assert_eq!(r.get(C::TYPE_NAME), Some(pc));
    }

    #[test]
    fn registry_full_returns_error_not_panic() {
        // Fill the registry past capacity by synthesising distinct
        // type names. We can't impl Message on N anonymous types in
        // a loop, so call build_raw + insert manually via the
        // shared registry path.
        let mut r = TypeRegistry::new();
        // Bypass `get_or_build` (needs a `Message` impl per type) —
        // exercise the overflow via direct table insert.
        for name in STATIC_NAMES.iter().take(MAX_TYPES) {
            // Distinct &'static keys from a fixed bank. heapless::
            // FnvIndexMap dedupes by key so we need genuinely
            // distinct entries.
            let dummy = core::ptr::null::<core::ffi::c_void>();
            r.table
                .insert(*name, DescriptorSlot(dummy))
                .expect("fits within MAX_TYPES");
        }
        assert_eq!(r.len(), MAX_TYPES);
        // Now the (MAX_TYPES+1)th distinct type overflows via
        // get_or_build → RegistryFull.
        let err = r.get_or_build::<A>().expect_err("must overflow");
        assert_eq!(err, BuildError::RegistryFull);
    }

    // 32 distinct &'static strs — one per slot at default MAX_TYPES.
    // If the const knob changes the test still compiles because we
    // index up to MAX_TYPES, which is bounded by this array's length.
    const STATIC_NAMES: [&str; 32] = [
        "n00\0", "n01\0", "n02\0", "n03\0", "n04\0", "n05\0", "n06\0", "n07\0", "n08\0", "n09\0",
        "n10\0", "n11\0", "n12\0", "n13\0", "n14\0", "n15\0", "n16\0", "n17\0", "n18\0", "n19\0",
        "n20\0", "n21\0", "n22\0", "n23\0", "n24\0", "n25\0", "n26\0", "n27\0", "n28\0", "n29\0",
        "n30\0", "n31\0",
    ];

    #[test]
    fn global_registry_lock_works() {
        // Smoke: global() is callable from a hosted unit test and
        // `with` hands out exclusive access. We don't share state
        // across tests deliberately (BUILD_COUNTER is shared,
        // tests run in parallel under cargo test by default — so we
        // only assert the call succeeds, not the count).
        let _ = global().with(|r| r.is_empty() || !r.is_empty());
    }
}
