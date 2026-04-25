//! Phase 71.2.b — non-blocking UDP transport on `nros-platform` traits.
//!
//! Provides `NrosUdpTransportFactory` — an implementation of
//! `dust_dds::transport::TransportParticipantFactory` that drives all
//! socket I/O through `<P as PlatformUdp>` and
//! `<P as PlatformUdpMulticast>` (where `P = ConcretePlatform`). No
//! background OS threads, no `socket2`, no `std::net::UdpSocket` —
//! every primitive lives in `nros-platform-api`, so the same module
//! covers POSIX, Zephyr, NuttX, FreeRTOS, ThreadX-Linux, and the
//! smoltcp-backed bare-metal boards.
//!
//! # Cooperative driving
//!
//! The factory spawns three `async` recv tasks (default unicast,
//! metatraffic unicast, metatraffic multicast) onto the
//! `NrosPlatformRuntime` spawner the consumer hands in.
//! `DdsSession::drive_io()` calls `runtime.drive()` once per
//! `Executor::spin_once()` — which polls each recv task once. Every
//! recv task does a non-blocking `<P as PlatformUdp>::read()` (the
//! caller pre-sets a 0-ms timeout via `set_recv_timeout`), forwards
//! anything received into the `MpscSender`, then yields. No thread
//! ever blocks.
//!
//! # Status
//!
//! | Piece                          | Status                |
//! |--------------------------------|-----------------------|
//! | Opaque socket / endpoint types | Working (over-sized)  |
//! | `WriteMessage` (unicast send)  | Working               |
//! | `TransportParticipantFactory`  | Skeleton              |
//! | Multicast SPDP join/recv       | Skeleton (TODO)       |
//! | Per-platform size validation   | Pending (Phase 71.2.b follow-up) |
//!
//! # Opaque buffer sizing
//!
//! `<P as PlatformUdp>::open` writes its platform-specific socket
//! state into a caller-allocated `*mut c_void`. The exact byte size
//! depends on the platform (POSIX fd: 4 bytes; smoltcp handle: 2;
//! ThreadX/NetX socket pointer: 4–8; …). `zpico-platform-shim`
//! probes the size via `cc::Build` against zenoh-pico's headers
//! during build. We deliberately don't replicate that probe here yet
//! — instead, the opaque buffers are over-sized to `[u8; 64]` aligned
//! to 8, which fits every shipped platform's `_z_sys_net_socket_t` /
//! `_z_sys_net_endpoint_t` (largest currently observed: 16 bytes on
//! POSIX with `addrinfo*` endpoint; smallest 2 bytes on smoltcp).
//! Wasting ~50 bytes per socket × 3 sockets per participant × handful
//! of participants is fine on every platform that has the heap to
//! run dust-dds in the first place. A follow-up commit can re-use
//! `zpico-platform-shim`'s size probe to make these exact.

#![cfg(feature = "alloc")]

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::future::Future;
use core::pin::Pin;

use dust_dds::transport::{
    interface::{RtpsTransportParticipant, TransportParticipantFactory, WriteMessage},
    types::{LOCATOR_KIND_UDP_V4, Locator},
};
use nros_platform::{PlatformUdp, PlatformUdpMulticast};

use crate::runtime::NrosPlatformRuntime;

// ---------------------------------------------------------------------------
// Opaque buffers
// ---------------------------------------------------------------------------

/// Caller-allocated storage for `<P as PlatformUdp>` socket state.
///
/// Sized to fit every shipped platform's `_z_sys_net_socket_t`. Aligned
/// to 8 bytes so any pointer-bearing platform layout (NetX, Zephyr,
/// FreeRTOS+lwIP) is naturally aligned.
#[repr(C, align(8))]
struct OpaqueSocket {
    bytes: [u8; 64],
}

impl OpaqueSocket {
    fn new() -> Self {
        Self { bytes: [0; 64] }
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.bytes.as_mut_ptr() as *mut c_void
    }

    fn as_ptr(&self) -> *const c_void {
        self.bytes.as_ptr() as *const c_void
    }
}

/// Same shape as [`OpaqueSocket`] but for `_z_sys_net_endpoint_t`.
#[repr(C, align(8))]
struct OpaqueEndpoint {
    bytes: [u8; 64],
}

impl OpaqueEndpoint {
    fn new() -> Self {
        Self { bytes: [0; 64] }
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.bytes.as_mut_ptr() as *mut c_void
    }

    fn as_ptr(&self) -> *const c_void {
        self.bytes.as_ptr() as *const c_void
    }
}

// ---------------------------------------------------------------------------
// Locator → endpoint string conversion
// ---------------------------------------------------------------------------

/// Render the IPv4 portion of a UDP-v4 RTPS [`Locator`] as a
/// null-terminated `"a.b.c.d\0"` byte vector suitable for
/// `<P as PlatformUdp>::create_endpoint`.
///
/// Returns `None` for non-UDP-v4 kinds (multicast handling lives on
/// the dedicated multicast path).
fn locator_address_cstring(loc: &Locator) -> Option<Vec<u8>> {
    if loc.kind() != LOCATOR_KIND_UDP_V4 {
        return None;
    }
    let a = loc.address();
    Some(
        format!("{}.{}.{}.{}\0", a[12], a[13], a[14], a[15])
            .into_bytes(),
    )
}

fn port_cstring(port: u32) -> Vec<u8> {
    format!("{port}\0").into_bytes()
}

// ---------------------------------------------------------------------------
// Outbound message writer
// ---------------------------------------------------------------------------

/// Sends RTPS datagrams via `<P as PlatformUdp>::send`.
///
/// Owns its own send-side socket. Cloning duplicates the socket
/// reference — for now the writer is single-instance per participant
/// (the factory creates one and `Box`es it into the participant), so
/// `Clone` is unneeded and we don't implement it.
pub struct NrosMessageWriter<P> {
    /// Reusable outbound socket — dust-dds calls `write_message`
    /// from the spawner thread, never concurrently with itself
    /// (the underlying `MpscSender` serialises sends via the runtime).
    sock: spin::Mutex<OpaqueSocket>,
    _p: core::marker::PhantomData<fn() -> P>,
}

impl<P> NrosMessageWriter<P>
where
    P: PlatformUdp + 'static,
{
    fn new(sock: OpaqueSocket) -> Self {
        Self {
            sock: spin::Mutex::new(sock),
            _p: core::marker::PhantomData,
        }
    }
}

// SAFETY: `OpaqueSocket` is `[u8; 64]` plus alignment — the underlying
// platform implementation may stash a kernel fd or a smoltcp handle in
// it but never anything thread-unsafe at the Rust level. The
// `spin::Mutex` serialises any concurrent calls.
unsafe impl<P> Send for NrosMessageWriter<P> {}
unsafe impl<P> Sync for NrosMessageWriter<P> {}

impl<P> WriteMessage for NrosMessageWriter<P>
where
    P: PlatformUdp + 'static,
{
    fn write_message(
        &self,
        datagram: &[u8],
        locator_list: &[Locator],
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        // Resolve every locator in the list to a C-string endpoint and
        // dispatch the datagram synchronously. dust-dds's transport
        // contract returns a Future, but the actual send is non-blocking
        // here — we wrap an immediately-Ready future.
        for loc in locator_list {
            // Allocate a fresh endpoint for each destination —
            // create_endpoint on platforms that resolve hostnames may
            // touch state (DNS), so don't reuse.
            let Some(addr) = locator_address_cstring(loc) else {
                continue;
            };
            let port = port_cstring(loc.port());
            let mut ep = OpaqueEndpoint::new();
            let rc = <P as PlatformUdp>::create_endpoint(
                ep.as_mut_ptr(),
                addr.as_ptr(),
                port.as_ptr(),
            );
            if rc < 0 {
                continue;
            }
            let sock = self.sock.lock();
            let _ = <P as PlatformUdp>::send(
                sock.as_ptr(),
                datagram.as_ptr(),
                datagram.len(),
                ep.as_ptr(),
            );
            <P as PlatformUdp>::free_endpoint(ep.as_mut_ptr());
            drop(sock);
        }
        Box::pin(async {})
    }
}

// ---------------------------------------------------------------------------
// Factory skeleton
// ---------------------------------------------------------------------------

/// dust-dds transport factory backed by `nros-platform-api` traits.
///
/// Generic over `P` so test fixtures can pin a specific platform ZST;
/// the canonical instantiation is
/// `NrosUdpTransportFactory<nros_platform::ConcretePlatform>`.
///
/// **Status**: skeleton — `create_participant` allocates a default
/// unicast socket and a `NrosMessageWriter` but does not yet open
/// metatraffic sockets, join the SPDP multicast group, or spawn recv
/// tasks. Subsequent commits will fill those in. Sufficient to compile
/// and demonstrate the wire-up pattern; not yet capable of receiving
/// data.
pub struct NrosUdpTransportFactory<P> {
    runtime: Arc<NrosPlatformRuntime<P>>,
    fragment_size: usize,
}

impl<P> NrosUdpTransportFactory<P> {
    pub fn new(runtime: Arc<NrosPlatformRuntime<P>>) -> Self {
        Self {
            runtime,
            fragment_size: 1344,
        }
    }

    pub fn with_fragment_size(mut self, size: usize) -> Self {
        self.fragment_size = size;
        self
    }
}

impl<P> TransportParticipantFactory for NrosUdpTransportFactory<P>
where
    P: PlatformUdp + PlatformUdpMulticast + Send + 'static,
{
    fn create_participant(
        &self,
        domain_id: i32,
        _data_channel_sender: dust_dds::dcps::channels::mpsc::MpscSender<Arc<[u8]>>,
    ) -> impl Future<Output = RtpsTransportParticipant> + Send {
        let _ = self.runtime.clone();
        let fragment_size = self.fragment_size;
        async move {
            // Default unicast socket — bound by the platform to an
            // ephemeral port. Phase 71.2.b follow-up will discover the
            // bound port via the platform's `getsockname` analogue and
            // populate `default_unicast_locator_list` accordingly.
            let mut send_sock = OpaqueSocket::new();
            let dummy_ep = OpaqueEndpoint::new();
            let _ =
                <P as PlatformUdp>::open(send_sock.as_mut_ptr(), dummy_ep.as_ptr(), 0);
            let writer = NrosMessageWriter::<P>::new(send_sock);

            // SPDP / SEDP locator lists are placeholders for now.
            // Filling them in (and joining the SPDP multicast group on
            // 239.255.0.1:7400+250*domain_id) is Phase 71.2.b follow-up.
            let _ = domain_id;

            RtpsTransportParticipant {
                message_writer: Box::new(writer),
                default_unicast_locator_list: Vec::new(),
                metatraffic_unicast_locator_list: Vec::new(),
                metatraffic_multicast_locator_list: Vec::new(),
                default_multicast_locator_list: Vec::new(),
                fragment_size,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "platform-posix"))]
mod tests {
    use super::*;
    use nros_platform::ConcretePlatform;

    #[test]
    fn locator_to_cstring_roundtrip() {
        let loc = Locator::new(
            LOCATOR_KIND_UDP_V4,
            7400,
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 127, 0, 0, 1],
        );
        let s = locator_address_cstring(&loc).unwrap();
        assert_eq!(&s, b"127.0.0.1\0");
        assert_eq!(&port_cstring(7400), b"7400\0");
    }

    #[test]
    fn factory_default_fragment_size_is_1344() {
        let rt = Arc::new(NrosPlatformRuntime::<ConcretePlatform>::new());
        let f = NrosUdpTransportFactory::<ConcretePlatform>::new(rt);
        assert_eq!(f.fragment_size, 1344);
        let f = f.with_fragment_size(8192);
        assert_eq!(f.fragment_size, 8192);
    }
}
