//! Phase 115.K.2.5.1.0 smoke test.
//!
//! Confirms the shim crate's `register()` reaches the C-side
//! `nros_rmw_xrce_register` symbol, which in turn calls
//! `nros_rmw_cffi_register_named` on the static vtable. We can't test the
//! latter call's effect from here without dragging in a fake cffi
//! runtime, so the test settles for "register() either returns Ok
//! or a defined RegisterError" — i.e. the linker resolved the
//! symbol. UnresolvedSymbol would manifest as a link error well
//! before this test runs.

#[test]
fn register_resolves_and_returns() {
    // The runtime is not installed in this test crate, so
    // `nros_rmw_cffi_register_named` is unresolved at link time. We
    // satisfy it with a stub right here.
    let r = nros_rmw_xrce_cffi::register();
    // Stub returns 0 on success; treat any return as proof the
    // chain linked. Surface unexpected non-zero so future ABI
    // version bumps don't slip through silently.
    if let Err(e) = r {
        panic!("register returned unexpected error: {:?}", e);
    }
}

/// Stub the runtime side of the canonical RMW vtable register so
/// the linker can resolve the chain.
///
/// SAFETY: this is a test fixture; it intentionally aliases the
/// real `nros_rmw_cffi_register_named` symbol that ships in the
/// `nros-rmw-cffi` Rust crate. The shim crate is `no_std` and does
/// not depend on `nros-rmw-cffi` (consumers wire the runtime
/// separately), so no real conflict at link time.
#[unsafe(no_mangle)]
extern "C" fn nros_rmw_cffi_register_named(
    _name: *const core::ffi::c_char,
    _vtable: *const core::ffi::c_void,
) -> i32 {
    0
}
