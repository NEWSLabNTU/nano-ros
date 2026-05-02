//! Phase 71 end-to-end demo — drives the cooperative no_std code path
//! on POSIX.
//!
//! `nros-rmw-dds`'s `Rmw::open()` on `platform-posix` uses dust-dds's
//! stock threaded transport (3 OS threads per participant). This test
//! takes the *other* path: instantiates `NrosPlatformRuntime<PosixPlatform>`
//! and `NrosUdpTransportFactory<PosixPlatform>` directly, then drives
//! a `DomainParticipantFactoryAsync` through `runtime.block_on(...)`.
//!
//! Same code path that `platform-zephyr` / `platform-freertos` /
//! `platform-nuttx` / `platform-threadx` use — just with `PosixPlatform`
//! standing in for `ConcretePlatform`.
//!
//! What this proves end-to-end on Linux:
//! * `NrosPlatformRuntime<PosixPlatform>` satisfies dust-dds's
//!   `DdsRuntime` bound (clock + timer + spawner all work).
//! * `NrosUdpTransportFactory::create_participant` opens RTPS sockets
//!   via `<PosixPlatform as PlatformUdp>::listen` (Phase 71.21) and
//!   spawns the recv loops onto the runtime spawner.
//! * `block_on` correctly drives both the caller's future and the
//!   spawned background tasks until the participant is created.

#![cfg(feature = "platform-posix")]

use core::time::Duration;
use std::sync::Arc;

use dust_dds::{
    dds_async::{
        domain_participant_factory::DomainParticipantFactoryAsync,
        domain_participant_listener::DomainParticipantListener,
    },
    infrastructure::{qos::QosKind, status::NO_STATUS},
};

use nros_platform_posix::PosixPlatform;
use nros_rmw_dds::{runtime::NrosPlatformRuntime, transport_nros::NrosUdpTransportFactory};

struct NoListener;
impl DomainParticipantListener for NoListener {}

#[test]
fn create_participant_via_cooperative_runtime() {
    // Use a high domain id to avoid colliding with any DDS daemons on
    // the dev box. The RTPS PSM ports for domain 200 land at 7400 +
    // 250·200 = 57400 + offsets — well above the registered range.
    let domain_id = 200i32;

    let runtime: NrosPlatformRuntime<PosixPlatform> = NrosPlatformRuntime::new();
    let runtime_arc = Arc::new(runtime.clone());
    let transport = NrosUdpTransportFactory::new(runtime_arc.clone());

    let factory = DomainParticipantFactoryAsync::new(
        runtime.clone(),
        /* app_id  */ [0u8; 4],
        /* host_id */ [0u8; 4],
        transport,
    );

    // Drive the async create_participant through our cooperative
    // block_on. If anything in the chain (transport bind, recv-task
    // spawn, factory mailbox) hangs, this test times out at the
    // outer `cargo test` budget.
    let participant = runtime
        .block_on(factory.create_participant(
            domain_id,
            QosKind::Default,
            None::<NoListener>,
            NO_STATUS,
        ))
        .expect(
            "create_participant should succeed via NrosPlatformRuntime + NrosUdpTransportFactory",
        );

    // Smoke check: the participant has the domain id we asked for.
    assert_eq!(participant.get_domain_id(), domain_id);

    // Give the runtime a few drives to flush any lingering background
    // work the factory enqueued (e.g. participant announce). Not
    // required for correctness — just verifies drive_io stays stable.
    for _ in 0..5 {
        runtime_arc.drive();
        std::thread::sleep(Duration::from_millis(10));
    }
}
