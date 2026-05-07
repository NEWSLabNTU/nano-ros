//! Phase 115.G.4 — second-language smoke test for the canonical
//! transport-vtable C ABI.
//!
//! Drives a stub transport whose four callbacks + ops-struct
//! constructor are written in **plain C** (`tests/c_stubs/c_stub_transport.c`).
//! No Rust headers / cbindgen output / Rust types involved on the
//! C side — same shape a Zig / Python-ctypes / Lua-FFI consumer
//! would author.
//!
//! Verifies:
//!
//! 1. A vtable populated entirely from C (`abi_version`, fn ptrs,
//!    `user_data`) round-trips through `nros_rmw::set_custom_transport`
//!    + `peek_custom_transport` without layout drift.
//! 2. The Rust runtime accepts the C-side `abi_version` constant
//!    `1` (matches `NROS_TRANSPORT_OPS_ABI_VERSION_V1`).
//! 3. Calling each of the four registered fn pointers from Rust
//!    drives the C-side counters as expected.
//! 4. Mismatched `abi_version` (set from the C side via a separate
//!    constructor) is rejected with `TransportError::IncompatibleAbi`.
//!
//! Run via:
//! ```bash
//! cargo test -p nros-rmw-cffi --features c-stub-test --test c_stub_transport
//! ```

#![cfg(feature = "c-stub-test")]

use core::ffi::c_void;

use nros_rmw::{
    NROS_TRANSPORT_OPS_ABI_VERSION_V1, NrosTransportOps, TransportError, peek_custom_transport,
    set_custom_transport,
};

/// Mirror of `nros_c_stub_transport_ops_t` from
/// `tests/c_stubs/c_stub_transport.h`. Hand-written to PROVE that
/// a non-Rust consumer can hold the same `#[repr(C)]` layout. If
/// the runtime's `NrosTransportOps` ever drifts from this shape,
/// the round-trip test below will catch it.
#[repr(C)]
#[derive(Copy, Clone)]
struct CStubTransportOps {
    abi_version: u32,
    _reserved: u32,
    user_data: *mut c_void,
    open: unsafe extern "C" fn(*mut c_void, *const c_void) -> i32,
    close: unsafe extern "C" fn(*mut c_void),
    write: unsafe extern "C" fn(*mut c_void, *const u8, usize) -> i32,
    read: unsafe extern "C" fn(*mut c_void, *mut u8, usize, u32) -> i32,
}

unsafe extern "C" {
    fn nros_c_stub_make_ops(out: *mut CStubTransportOps);
    fn nros_c_stub_reset_counters();

    fn nros_c_stub_get_open_calls() -> u32;
    fn nros_c_stub_get_close_calls() -> u32;
    fn nros_c_stub_get_write_calls() -> u32;
    fn nros_c_stub_get_read_calls() -> u32;
}

// Touch the lib's anchor so the C static archive gets pulled into
// the integration-test binary's link line. Each test calls
// `force_link()` first; without it the C symbols fail to resolve.
#[inline(never)]
fn force_link() {
    let _ = nros_rmw_cffi::_phase_115_g4_anchor();
}

/// Static-assert the layout the C side and the Rust side agreed on
/// is byte-compatible with `nros_rmw::NrosTransportOps`. If either
/// side drifts, this fails to compile.
const _: () = {
    assert!(core::mem::size_of::<CStubTransportOps>() == core::mem::size_of::<NrosTransportOps>());
    assert!(
        core::mem::align_of::<CStubTransportOps>() == core::mem::align_of::<NrosTransportOps>()
    );
};

/// Round-trip a C-built ops struct through the Rust runtime. Drive
/// each callback and confirm the C-side counters bumped.
#[test]
fn c_built_ops_round_trips_and_drives() {
    force_link();
    unsafe { nros_c_stub_reset_counters() };

    // Have the C side construct the ops struct in plain C.
    let mut c_ops = core::mem::MaybeUninit::<CStubTransportOps>::zeroed();
    unsafe { nros_c_stub_make_ops(c_ops.as_mut_ptr()) };
    let c_ops = unsafe { c_ops.assume_init() };

    // C side wrote `abi_version = 1` — matches what Rust expects.
    assert_eq!(c_ops.abi_version, NROS_TRANSPORT_OPS_ABI_VERSION_V1);
    assert_eq!(c_ops.user_data as usize, 0xC0FFEE);

    // SAFETY: `CStubTransportOps` and `NrosTransportOps` have the
    // same `#[repr(C)]` layout (asserted at compile time above).
    let nros_ops: NrosTransportOps =
        unsafe { core::mem::transmute::<CStubTransportOps, NrosTransportOps>(c_ops) };

    unsafe { set_custom_transport(Some(nros_ops)).expect("set should accept v1") };

    // Slot now holds the C-built ops.
    let peeked = peek_custom_transport().expect("peek");
    assert_eq!(peeked.abi_version, NROS_TRANSPORT_OPS_ABI_VERSION_V1);
    assert_eq!(peeked.user_data as usize, 0xC0FFEE);

    // Drive each registered fn pointer. They run in C; counters bump.
    let r = unsafe { (peeked.open)(peeked.user_data, core::ptr::null()) };
    assert_eq!(r, 0);
    assert_eq!(unsafe { nros_c_stub_get_open_calls() }, 1);

    let payload = b"hello";
    let r = unsafe { (peeked.write)(peeked.user_data, payload.as_ptr(), payload.len()) };
    assert_eq!(r, 0);
    assert_eq!(unsafe { nros_c_stub_get_write_calls() }, 1);

    let mut buf = [0u8; 16];
    let r = unsafe { (peeked.read)(peeked.user_data, buf.as_mut_ptr(), buf.len(), 100) };
    assert_eq!(r, 0);
    assert_eq!(unsafe { nros_c_stub_get_read_calls() }, 1);

    unsafe { (peeked.close)(peeked.user_data) };
    assert_eq!(unsafe { nros_c_stub_get_close_calls() }, 1);

    // Drain to leave the slot clean for the next test.
    let _ = unsafe { set_custom_transport(None) };
}

/// Tampering the version on the C-built struct triggers
/// `TransportError::IncompatibleAbi` — same path C / C++ wrappers
/// surface as `NROS_RMW_RET_INCOMPATIBLE_ABI`.
#[test]
fn c_built_ops_with_bogus_abi_version_rejected() {
    force_link();
    let _ = unsafe { set_custom_transport(None) };

    let mut c_ops = core::mem::MaybeUninit::<CStubTransportOps>::zeroed();
    unsafe { nros_c_stub_make_ops(c_ops.as_mut_ptr()) };
    let mut c_ops = unsafe { c_ops.assume_init() };

    c_ops.abi_version = 0xBAD0_BAD0;
    let nros_ops: NrosTransportOps = unsafe { core::mem::transmute(c_ops) };

    let err = unsafe { set_custom_transport(Some(nros_ops)) };
    assert!(matches!(err, Err(TransportError::IncompatibleAbi)));
    assert!(peek_custom_transport().is_none());
}
