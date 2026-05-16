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

// Phase 128.B.3 — `nros-rmw-xrce-cffi` now depends on
// `nros-rmw-cffi` (for the `RMW_INIT_ENTRIES` distributed slice),
// so the real `nros_rmw_cffi_register_named` symbol is linked into
// the test binary by default. The old hand-written stub became a
// duplicate-symbol link error and is removed.
