//! Phase 115.H — dust-DDS custom-transport smoke test.
//!
//! Mirrors the 115.B / 115.E pattern: register stub callbacks via
//! `nros_rmw::set_custom_transport`, drain the slot into a
//! `NrosCustomTransportParticipantFactory`, drive a participant
//! creation, observe the four counters move.
//!
//! v1 stops short of a full RTPS handshake — there is no multicast
//! SPDP over a custom byte pipe, and dust-dds's discovery state
//! machine has no static-peer mode yet. This test validates the
//! transport plumbing only:
//!   1. factory drains the slot exactly once (`open` ↑ 1)
//!   2. `WriteMessage::write_message` reaches `cb_write` (write ↑ ≥1)
//!   3. spawned reader loop reaches `cb_read` (read ↑ ≥1) once the
//!      runtime drives a few times
//!
//! The full discovery-over-byte-pipe story is the heavy half of
//! 115.H follow-up.
//!
//! Run via:
//! ```bash
//! cargo test -p nros-rmw-dds --features platform-posix \
//!     --test custom_transport
//! ```

#![cfg(feature = "platform-posix")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering},
};
use std::time::Duration;

use dust_dds::dcps::channels::mpsc::mpsc_channel;
use dust_dds::transport::interface::TransportParticipantFactory;
use nros_platform_posix::PosixPlatform;
use nros_rmw::{NROS_TRANSPORT_OPS_ABI_VERSION_V1, NrosTransportOps, set_custom_transport};
use dust_dds::sync::Arc;
use nros_rmw_dds::{
    runtime::NrosPlatformRuntime,
    transport_custom::NrosCustomTransportParticipantFactory,
};

static OPEN_CALLS: AtomicU32 = AtomicU32::new(0);
static CLOSE_CALLS: AtomicU32 = AtomicU32::new(0);
static WRITE_CALLS: AtomicU32 = AtomicU32::new(0);
static READ_CALLS: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn stub_open(_ud: *mut c_void, _params: *const c_void) -> i32 {
    OPEN_CALLS.fetch_add(1, Ordering::Relaxed);
    0
}
unsafe extern "C" fn stub_close(_ud: *mut c_void) {
    CLOSE_CALLS.fetch_add(1, Ordering::Relaxed);
}
unsafe extern "C" fn stub_write(_ud: *mut c_void, _buf: *const u8, _len: usize) -> i32 {
    WRITE_CALLS.fetch_add(1, Ordering::Relaxed);
    0
}
unsafe extern "C" fn stub_read(_ud: *mut c_void, _buf: *mut u8, _len: usize, _to: u32) -> i32 {
    READ_CALLS.fetch_add(1, Ordering::Relaxed);
    0
}

fn make_ops() -> NrosTransportOps {
    NrosTransportOps {
        abi_version: NROS_TRANSPORT_OPS_ABI_VERSION_V1,
        _reserved: 0,
        user_data: 0xDEAD_BEEF_usize as *mut c_void,
        open: stub_open,
        close: stub_close,
        write: stub_write,
        read: stub_read,
    }
}

#[test]
fn factory_drains_slot_and_drives_callbacks() {
    OPEN_CALLS.store(0, Ordering::Relaxed);
    CLOSE_CALLS.store(0, Ordering::Relaxed);
    WRITE_CALLS.store(0, Ordering::Relaxed);
    READ_CALLS.store(0, Ordering::Relaxed);

    // Step 1 — register the vtable through the public slot API.
    unsafe { set_custom_transport(Some(make_ops())).expect("set v1 ok") };

    // Step 2 — runtime + factory.
    let runtime: NrosPlatformRuntime<PosixPlatform> = NrosPlatformRuntime::new();
    let runtime_arc = Arc::new(runtime.clone());
    let factory = NrosCustomTransportParticipantFactory::from_slot(runtime_arc.clone())
        .expect("slot drained");

    // Step 3 — create the participant via `block_on` so the factory's
    // `create_participant` future runs. This is what
    // `DomainParticipantFactoryAsync` would do internally for us in
    // a real `DdsRmw::open` call. We bypass that path because v1
    // skips RTPS discovery — a full participant would block on
    // multicast SPDP that has no analogue here.
    let (sender, _receiver) = mpsc_channel();
    let participant = runtime.block_on(factory.create_participant(0, sender));

    // open() should have fired exactly once.
    assert_eq!(
        OPEN_CALLS.load(Ordering::Relaxed),
        1,
        "open should have been called once on participant creation"
    );

    // Step 4 — exercise the writer. dust-dds invokes
    // `write_message` from its sender task; we call it directly to
    // confirm the byte pipe trips `cb_write`.
    let writer = participant.message_writer;
    runtime.block_on(writer.write_message(&[0xAA, 0xBB], &[]));
    assert!(
        WRITE_CALLS.load(Ordering::Relaxed) >= 1,
        "write_message should have triggered cb_write"
    );

    // Step 5 — drive the runtime several times so the spawned recv
    // task gets a chance to call cb_read. Each `runtime.drive()`
    // polls the spawner queue once; the recv task's
    // `YieldOnce`-after-zero-bytes pattern means each drive
    // produces one cb_read.
    for _ in 0..8 {
        runtime_arc.drive();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        READ_CALLS.load(Ordering::Relaxed) >= 1,
        "recv task should have called cb_read at least once after drive iterations"
    );

    // Cleanup — clear the slot so other tests in the same process
    // start fresh.
    unsafe { set_custom_transport(None).expect("clear ok") };
}
