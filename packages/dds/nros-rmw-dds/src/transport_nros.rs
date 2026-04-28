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

// Phase 97.4 debug — semihosting print macro for Cortex-M FreeRTOS /
// bare-metal bring-up. No-op when the feature is off so std builds
// don't pull in cortex-m-semihosting.
#[cfg(feature = "debug-cortex-m-semihosting")]
macro_rules! dbg_log {
    ($($arg:tt)*) => {
        cortex_m_semihosting::hprintln!("[nros-rmw-dds] {}", format_args!($($arg)*));
    };
}
#[cfg(not(feature = "debug-cortex-m-semihosting"))]
macro_rules! dbg_log {
    ($($arg:tt)*) => {{
        let _ = format_args!($($arg)*);
    }};
}

use alloc::boxed::Box;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::future::Future;
use core::pin::Pin;

use dust_dds::dcps::channels::mpsc::MpscSender;
use dust_dds::runtime::Spawner;
use dust_dds::transport::{
    interface::{RtpsTransportParticipant, TransportParticipantFactory, WriteMessage},
    types::{LOCATOR_KIND_UDP_V4, Locator},
};
use nros_platform::{PlatformUdp, PlatformUdpMulticast};

use crate::runtime::NrosPlatformRuntime;

// ---------------------------------------------------------------------------
// RTPS well-known port formulas (PSM RTPS-UDP §9.6.1.4)
// ---------------------------------------------------------------------------

/// PB — port base.
const PB: u32 = 7400;
/// DG — domain gain.
const DG: u32 = 250;
/// PG — participant gain.
const PG: u32 = 2;
/// d0 — multicast metatraffic offset.
const D0: u32 = 0;
/// d1 — unicast metatraffic offset.
const D1: u32 = 10;
/// d2 — multicast user-data offset (unused — nano-ros sends user data unicast).
#[allow(dead_code)]
const D2: u32 = 1;
/// d3 — unicast user-data offset.
const D3: u32 = 11;

/// Multicast metatraffic port (SPDP listen).
fn port_metatraffic_multicast(domain_id: u32) -> u16 {
    (PB + DG * domain_id + D0) as u16
}

/// Unicast metatraffic port (SEDP).
fn port_metatraffic_unicast(domain_id: u32, participant_id: u32) -> u16 {
    (PB + DG * domain_id + D1 + PG * participant_id) as u16
}

/// Unicast default-channel user-data port.
fn port_default_unicast(domain_id: u32, participant_id: u32) -> u16 {
    (PB + DG * domain_id + D3 + PG * participant_id) as u16
}

/// SPDP multicast group `239.255.0.1` as a `[u8; 16]` IPv6-mapped
/// representation matching dust-dds's `Locator::address` shape.
const SPDP_MULTICAST_ADDRESS: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 239, 255, 0, 1];

/// Local interface IPv4 advertised in SPDP unicast locators. Set at
/// build time via the `NROS_LOCAL_IPV4` env var (default `127.0.0.1`,
/// which keeps the host-loopback test path on native_sim working
/// unchanged). For Zephyr embedded targets, the
/// `nros_cargo_build` CMake helper passes `CONFIG_NET_CONFIG_MY_IPV4_ADDR`
/// through this env so each guest advertises its own iface IP.
/// Phase 92.5 — without this, peer SEDP/data sends go to localhost on
/// every guest and never cross.
pub(crate) const LOCAL_IPV4: [u8; 4] = {
    let s = env!("NROS_LOCAL_IPV4_BYTES");
    let bytes = s.as_bytes();
    // Tiny no-`alloc` parser: split by commas, parse each octet.
    let mut out = [0u8; 4];
    let mut i = 0;
    let mut acc: u32 = 0;
    let mut idx = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b',' {
            out[idx] = acc as u8;
            idx += 1;
            acc = 0;
        } else {
            acc = acc * 10 + (b - b'0') as u32;
        }
        i += 1;
    }
    out[idx] = acc as u8;
    out
};

/// Build an `Ipv4` locator from an explicit `[a, b, c, d]` address +
/// port, using the IPv6-mapped layout that RTPS / dust-dds expects.
fn ipv4_locator(addr: [u8; 4], port: u32) -> Locator {
    let mut full = [0u8; 16];
    full[12..16].copy_from_slice(&addr);
    Locator::new(LOCATOR_KIND_UDP_V4, port, full)
}

// ---------------------------------------------------------------------------
// Opaque buffers
// ---------------------------------------------------------------------------

/// Caller-allocated storage for `<P as PlatformUdp>` socket state.
///
/// Phase 71.22: sized exactly from the active platform's
/// `core::mem::size_of::<Socket>()` (re-exported via
/// `nros_platform::NET_SOCKET_SIZE`). Bare-metal / cffi platforms
/// without a typed socket struct fall back to the 64-byte legacy
/// shape via `nros-platform`'s `fallback_net_sizes` module.
#[repr(C, align(8))]
struct OpaqueSocket {
    bytes: [u8; nros_platform::NET_SOCKET_SIZE],
}

impl OpaqueSocket {
    fn new() -> Self {
        Self {
            bytes: [0; nros_platform::NET_SOCKET_SIZE],
        }
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
    bytes: [u8; nros_platform::NET_ENDPOINT_SIZE],
}

impl OpaqueEndpoint {
    fn new() -> Self {
        Self {
            bytes: [0; nros_platform::NET_ENDPOINT_SIZE],
        }
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
    Some(format!("{}.{}.{}.{}\0", a[12], a[13], a[14], a[15]).into_bytes())
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
    /// Reusable outbound socket for unicast destinations.
    sock: spin::Mutex<OpaqueSocket>,
    /// Optional outbound socket for IPv4 multicast destinations.
    /// On Zephyr, the IP stack only forwards multicast TX through a
    /// socket that has joined the destination group via
    /// `IP_ADD_MEMBERSHIP`. A separate unbound socket silently drops
    /// outbound mcast (Phase 92.5 diagnosis). We share the
    /// metatraffic-multicast recv-loop's socket fd here — it has
    /// IGMP-joined `239.255.0.1` and can both recv *and* send.
    mcast_sock: Option<spin::Mutex<OpaqueSocket>>,
    _p: core::marker::PhantomData<fn() -> P>,
}

impl<P> NrosMessageWriter<P>
where
    P: PlatformUdp + 'static,
{
    fn new(sock: OpaqueSocket, mcast_sock: Option<OpaqueSocket>) -> Self {
        Self {
            sock: spin::Mutex::new(sock),
            mcast_sock: mcast_sock.map(spin::Mutex::new),
            _p: core::marker::PhantomData,
        }
    }
}

/// Returns `true` if the locator targets an IPv4 multicast group
/// (`224.0.0.0/4`). Used by the writer to choose between the unicast
/// and the IGMP-joined send sockets.
fn is_ipv4_multicast(loc: &Locator) -> bool {
    let addr = loc.address();
    // RTPS IPv6-mapped layout: octets 12..16 hold the IPv4 address.
    let v4_first = addr[12];
    (224..=239).contains(&v4_first)
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
        dbg_log!(
            "write_message ENTER: locators={} datagram_len={}",
            locator_list.len(),
            datagram.len()
        );
        // Resolve every locator in the list to a C-string endpoint and
        // dispatch the datagram synchronously. dust-dds's transport
        // contract returns a Future, but the actual send is non-blocking
        // here — we wrap an immediately-Ready future.
        for loc in locator_list {
            dbg_log!(
                "write_message: dst_pre {}.{}.{}.{}:{}",
                loc.address()[12], loc.address()[13], loc.address()[14], loc.address()[15],
                loc.port()
            );
            // Allocate a fresh endpoint for each destination —
            // create_endpoint on platforms that resolve hostnames may
            // touch state (DNS), so don't reuse.
            let Some(addr) = locator_address_cstring(loc) else {
                continue;
            };
            let port = port_cstring(loc.port());
            let mut ep = OpaqueEndpoint::new();
            let rc =
                <P as PlatformUdp>::create_endpoint(ep.as_mut_ptr(), addr.as_ptr(), port.as_ptr());
            dbg_log!("write_message: create_endpoint rc={}", rc as i32);
            if rc < 0 {
                continue;
            }
            // Pick the right socket based on destination class.
            // Multicast destinations *must* go through the
            // IGMP-joined socket on Zephyr; unicast goes through the
            // dedicated send socket on every platform.
            let mcast = is_ipv4_multicast(loc);
            let active_sock = if mcast {
                self.mcast_sock.as_ref().unwrap_or(&self.sock)
            } else {
                &self.sock
            };
            let sock = active_sock.lock();
            let n = <P as PlatformUdp>::send(
                sock.as_ptr(),
                datagram.as_ptr(),
                datagram.len(),
                ep.as_ptr(),
            );
            dbg_log!(
                "write_message: dst={}.{}.{}.{}:{} mcast={} datagram_len={} sent={}",
                loc.address()[12], loc.address()[13], loc.address()[14], loc.address()[15],
                loc.port(), mcast as u32, datagram.len(), n as i32
            );
            <P as PlatformUdp>::free_endpoint(ep.as_mut_ptr());
            drop(sock);
        }
        Box::pin(async {})
    }
}

// ---------------------------------------------------------------------------
// Yield-once helper
// ---------------------------------------------------------------------------

/// Future that returns `Pending` on first poll (with `wake_by_ref()` to
/// re-schedule immediately) and `Ready` on subsequent polls. Used by
/// the recv loops below to give the cooperative spawner one tick of
/// headroom between each non-blocking `read()` attempt.
struct YieldOnce(bool);

impl YieldOnce {
    fn new() -> Self {
        Self(false)
    }
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut core::task::Context<'_>) -> core::task::Poll<()> {
        if self.0 {
            core::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    }
}

// ---------------------------------------------------------------------------
// Recv loops — spawned onto the runtime spawner, polled by drive_io
// ---------------------------------------------------------------------------

/// Maximum RTPS datagram payload — leaves headroom under typical 65 KiB
/// IP datagram limits. Matches `MAX_DATAGRAM_SIZE` in the stock
/// dust-dds UDP transport.
const RECV_BUF_SIZE: usize = 65507;

async fn unicast_recv_loop<P>(sock: OpaqueSocket, sender: MpscSender<Arc<[u8]>>)
where
    P: PlatformUdp + 'static,
{
    // Make every read non-blocking so the loop yields cooperatively.
    <P as PlatformUdp>::set_recv_timeout(sock.as_ptr(), 0);
    let mut buf = alloc::vec![0u8; RECV_BUF_SIZE];
    loop {
        let n = <P as PlatformUdp>::read(sock.as_ptr(), buf.as_mut_ptr(), buf.len());
        if n != usize::MAX && n > 0 {
            if sender.send(Arc::from(&buf[..n])).await.is_err() {
                break;
            }
        }
        YieldOnce::new().await;
    }
}

async fn multicast_recv_loop<P>(
    sock: OpaqueSocket,
    local_ep: OpaqueEndpoint,
    sender: MpscSender<Arc<[u8]>>,
) where
    P: PlatformUdpMulticast + 'static,
{
    // PlatformUdpMulticast doesn't expose `set_recv_timeout` — most
    // implementations make `mcast_read` non-blocking when the listen
    // side was opened with `timeout_ms = 0`.
    let mut buf = alloc::vec![0u8; RECV_BUF_SIZE];
    let mut sender_addr = OpaqueEndpoint::new();
    loop {
        let n = <P as PlatformUdpMulticast>::mcast_read(
            sock.as_ptr(),
            buf.as_mut_ptr(),
            buf.len(),
            local_ep.as_ptr(),
            sender_addr.as_mut_ptr(),
        );
        if n != usize::MAX && n > 0 {
            dbg_log!("multicast_recv_loop: got n={} bytes", n);
            if sender.send(Arc::from(&buf[..n])).await.is_err() {
                break;
            }
        }
        YieldOnce::new().await;
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
/// `create_participant` does the full RTPS bind sequence:
/// * default unicast socket bound to `port_default_unicast` (RTPS
///   PSM 9.6.1.4 formula),
/// * metatraffic unicast socket bound to `port_metatraffic_unicast`,
/// * metatraffic multicast socket joining `239.255.0.1` on
///   `port_metatraffic_multicast`.
///
/// Each of the three sockets gets a recv loop spawned onto the
/// runtime's `Spawner`, draining datagrams into the
/// `data_channel_sender` whenever `Executor::spin_once()` calls
/// `runtime.drive()`.
///
/// `participant_id` is configurable on the builder (default 0). For
/// multi-participant nodes, allocate fresh ids at each call site so
/// the unicast ports don't collide.
pub struct NrosUdpTransportFactory<P> {
    runtime: Arc<NrosPlatformRuntime<P>>,
    fragment_size: usize,
    participant_id: u32,
}

impl<P> NrosUdpTransportFactory<P> {
    pub fn new(runtime: Arc<NrosPlatformRuntime<P>>) -> Self {
        Self {
            runtime,
            fragment_size: 1344,
            participant_id: 0,
        }
    }

    pub fn with_fragment_size(mut self, size: usize) -> Self {
        self.fragment_size = size;
        self
    }

    pub fn with_participant_id(mut self, id: u32) -> Self {
        self.participant_id = id;
        self
    }
}

/// Bind a unicast UDP socket to `0.0.0.0:port` for inbound RTPS
/// traffic. Calls `<P as PlatformUdp>::listen` (Phase 71.20), which
/// is the trait method specifically for the bind-then-recv flow that
/// `udp_open` doesn't cover (open is connect-style for zenoh-pico's
/// outbound queries; listen does an explicit `bind(2)`).
///
/// Returns `None` on bind failure.
fn bind_unicast<P: PlatformUdp + 'static>(port: u16) -> Option<OpaqueSocket> {
    dbg_log!("bind_unicast({}) ENTER", port);
    let addr = b"0.0.0.0\0".as_ptr();
    let port_str = port_cstring(port as u32);
    let mut ep = OpaqueEndpoint::new();
    if <P as PlatformUdp>::create_endpoint(ep.as_mut_ptr(), addr, port_str.as_ptr()) < 0 {
        dbg_log!("bind_unicast({}): create_endpoint FAIL", port);
        return None;
    }
    dbg_log!("bind_unicast({}): create_endpoint OK", port);
    let mut sock = OpaqueSocket::new();
    // Pass `timeout_ms = 0` — recv is set non-blocking by the recv
    // loop's `set_recv_timeout(0)` call before any read happens, so
    // the bind-time timeout is irrelevant.
    let rc = <P as PlatformUdp>::listen(sock.as_mut_ptr(), ep.as_ptr(), 0);
    dbg_log!("bind_unicast({}): listen rc={}", port, rc as i32);
    <P as PlatformUdp>::free_endpoint(ep.as_mut_ptr());
    if rc < 0 { None } else { Some(sock) }
}

/// Open + join the SPDP multicast group on `239.255.0.1:port`.
/// Returns the (recv socket, local endpoint) pair, or `None` on
/// failure.
fn bind_multicast<P: PlatformUdpMulticast + PlatformUdp + 'static>(
    port: u16,
) -> Option<(OpaqueSocket, OpaqueEndpoint)> {
    dbg_log!("bind_multicast({}) ENTER", port);
    let local_addr = b"0.0.0.0\0".as_ptr();
    let port_str = port_cstring(port as u32);
    let mut local_ep = OpaqueEndpoint::new();
    if <P as PlatformUdp>::create_endpoint(local_ep.as_mut_ptr(), local_addr, port_str.as_ptr()) < 0
    {
        dbg_log!("bind_multicast({}): create_endpoint FAIL", port);
        return None;
    }
    dbg_log!("bind_multicast({}): create_endpoint OK", port);
    let join = b"239.255.0.1\0".as_ptr();
    let mut sock = OpaqueSocket::new();
    dbg_log!("bind_multicast({}): mcast_listen pre-call", port);
    let rc = <P as PlatformUdpMulticast>::mcast_listen(
        sock.as_mut_ptr(),
        local_ep.as_ptr(),
        0,
        core::ptr::null(),
        join,
    );
    dbg_log!("bind_multicast({}): mcast_listen rc={}", port, rc as i32);
    if rc < 0 {
        <P as PlatformUdp>::free_endpoint(local_ep.as_mut_ptr());
        return None;
    }
    Some((sock, local_ep))
}

impl<P> TransportParticipantFactory for NrosUdpTransportFactory<P>
where
    P: PlatformUdp + PlatformUdpMulticast + Send + Sync + 'static,
{
    fn create_participant(
        &self,
        domain_id: i32,
        data_channel_sender: MpscSender<Arc<[u8]>>,
    ) -> impl Future<Output = RtpsTransportParticipant> + Send {
        let runtime = self.runtime.clone();
        let fragment_size = self.fragment_size;
        let start_pid = self.participant_id;
        async move {
            dbg_log!("create_participant: ENTER domain={}", domain_id);
            let domain = domain_id as u32;

            // ---- Auto-increment participant_id until both unicast
            // ports are free. RTPS PSM 9.6.1.4 reserves 120 ids per
            // domain; cap at 32 to keep the search bounded. Multiple
            // participants on the same host (talker + listener test,
            // or two participants in one process) need different ids
            // so their unicast bind(2) calls don't `EADDRINUSE`.
            let mut participant_id = start_pid;
            let mut default_uc_port;
            let mut default_uc_sock;
            let mut metatraffic_uc_port;
            let mut metatraffic_uc_sock;
            loop {
                default_uc_port = port_default_unicast(domain, participant_id);
                metatraffic_uc_port = port_metatraffic_unicast(domain, participant_id);
                default_uc_sock = bind_unicast::<P>(default_uc_port);
                if default_uc_sock.is_some() {
                    metatraffic_uc_sock = bind_unicast::<P>(metatraffic_uc_port);
                    if metatraffic_uc_sock.is_some() {
                        break;
                    }
                    // metatraffic port collided — release default and
                    // retry with the next id.
                    default_uc_sock = None;
                }
                if participant_id >= start_pid + 32 {
                    metatraffic_uc_sock = None;
                    break;
                }
                participant_id += 1;
            }
            let default_unicast_locator_list = if default_uc_sock.is_some() {
                alloc::vec![ipv4_locator(LOCAL_IPV4, default_uc_port as u32)]
            } else {
                Vec::new()
            };
            let metatraffic_unicast_locator_list = if metatraffic_uc_sock.is_some() {
                alloc::vec![ipv4_locator(LOCAL_IPV4, metatraffic_uc_port as u32)]
            } else {
                Vec::new()
            };

            dbg_log!(
                "create_participant: unicast binds done, pid={}",
                participant_id
            );

            // ---- Metatraffic multicast (SPDP) -----------------------
            let metatraffic_mc_port = port_metatraffic_multicast(domain);
            let metatraffic_mc_pair = bind_multicast::<P>(metatraffic_mc_port);
            dbg_log!(
                "create_participant: mcast pair = {}",
                if metatraffic_mc_pair.is_some() { "Some" } else { "None" }
            );
            let metatraffic_multicast_locator_list = if metatraffic_mc_pair.is_some() {
                alloc::vec![ipv4_locator([239, 255, 0, 1], metatraffic_mc_port as u32)]
            } else {
                Vec::new()
            };
            let _ = SPDP_MULTICAST_ADDRESS;

            // ---- Spawn recv loops -----------------------------------
            let spawner = runtime.spawner_handle();
            if let Some(sock) = default_uc_sock {
                spawner.spawn(unicast_recv_loop::<P>(sock, data_channel_sender.clone()));
            }
            if let Some(sock) = metatraffic_uc_sock {
                spawner.spawn(unicast_recv_loop::<P>(sock, data_channel_sender.clone()));
            }
            // Capture a clone of the mcast socket bytes (the kernel fd
            // value, opaque to us) before handing the original off to
            // the recv loop. `OpaqueSocket` is `[u8; 64]` so this is a
            // raw byte copy; both halves end up referring to the same
            // underlying kernel fd. UDP semantics make concurrent
            // recvfrom + sendto safe on the same fd.
            let mcast_send_sock_for_writer: Option<OpaqueSocket> =
                metatraffic_mc_pair.as_ref().map(|(s, _)| {
                    let mut copy = OpaqueSocket::new();
                    copy.bytes.copy_from_slice(&s.bytes);
                    copy
                });
            if let Some((sock, lep)) = metatraffic_mc_pair {
                spawner.spawn(multicast_recv_loop::<P>(
                    sock,
                    lep,
                    data_channel_sender.clone(),
                ));
            }

            // ---- Outbound writer ------------------------------------
            // The send socket is unbound — every `write_message` call
            // does sendto() with the per-call destination locator.
            //
            // Zephyr's `getaddrinfo` rejects port "0" with EAI_NONAME
            // (Phase 92.5 diagnosis: `port < 1 || port > 65535` ->
            // EAI_NONAME), so we use port 1 as a placeholder. The
            // socket is never bind(2)-ed; the placeholder endpoint
            // exists only to satisfy `udp_open`'s `ai_family`-derived
            // `socket(2)` call.
            let mut send_sock = OpaqueSocket::new();
            let mut send_ep = OpaqueEndpoint::new();
            let any_addr = b"0.0.0.0\0".as_ptr();
            let placeholder_port = port_cstring(1);
            if <P as PlatformUdp>::create_endpoint(
                send_ep.as_mut_ptr(),
                any_addr,
                placeholder_port.as_ptr(),
            ) >= 0
            {
                let _ = <P as PlatformUdp>::open(send_sock.as_mut_ptr(), send_ep.as_ptr(), 0);
                <P as PlatformUdp>::free_endpoint(send_ep.as_mut_ptr());
            }
            let writer = NrosMessageWriter::<P>::new(send_sock, mcast_send_sock_for_writer);
            dbg_log!("create_participant: send sock + writer ready");

            dbg_log!("create_participant: RETURN");
            RtpsTransportParticipant {
                message_writer: Box::new(writer),
                default_unicast_locator_list,
                metatraffic_unicast_locator_list,
                metatraffic_multicast_locator_list,
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
    fn rtps_port_formulas_match_spec() {
        // RTPS PSM 9.6.1.4: PB=7400, DG=250, PG=2, d0=0, d1=10, d3=11.
        // domain 0, participant 0:
        //   metatraffic multicast = 7400
        //   metatraffic unicast   = 7410
        //   default unicast       = 7411
        assert_eq!(port_metatraffic_multicast(0), 7400);
        assert_eq!(port_metatraffic_unicast(0, 0), 7410);
        assert_eq!(port_default_unicast(0, 0), 7411);
        // domain 1, participant 2:
        //   mc = 7400 + 250 + 0      = 7650
        //   uc = 7400 + 250 + 10 + 4 = 7664
        //   du = 7400 + 250 + 11 + 4 = 7665
        assert_eq!(port_metatraffic_multicast(1), 7650);
        assert_eq!(port_metatraffic_unicast(1, 2), 7664);
        assert_eq!(port_default_unicast(1, 2), 7665);
    }

    #[test]
    fn ipv4_locator_layout_matches_dust_dds() {
        let loc = ipv4_locator([192, 168, 1, 7], 7411);
        assert_eq!(loc.kind(), LOCATOR_KIND_UDP_V4);
        assert_eq!(loc.port(), 7411);
        let a = loc.address();
        assert_eq!(&a[0..12], &[0u8; 12]);
        assert_eq!(&a[12..16], &[192, 168, 1, 7]);
    }

    #[test]
    fn bind_unicast_then_send_then_recv_roundtrip_posix() {
        // Phase 71.24 host-side validation. Picks a high port to avoid
        // colliding with any other RTPS-on-loopback test that might be
        // running concurrently.
        let port: u16 = 47411;
        let bound = bind_unicast::<ConcretePlatform>(port);
        assert!(
            bound.is_some(),
            "bind_unicast should succeed on POSIX loopback"
        );
        let mut bound = bound.unwrap();

        // Outbound socket — uses udp_open (connect-style; no bind).
        let target = b"127.0.0.1\0";
        let port_str = port_cstring(port as u32);
        let mut ep = OpaqueEndpoint::new();
        assert_eq!(
            <ConcretePlatform as PlatformUdp>::create_endpoint(
                ep.as_mut_ptr(),
                target.as_ptr(),
                port_str.as_ptr(),
            ),
            0
        );
        let mut send_sock = OpaqueSocket::new();
        assert_eq!(
            <ConcretePlatform as PlatformUdp>::open(send_sock.as_mut_ptr(), ep.as_ptr(), 0),
            0
        );

        let payload = b"phase-71-bind-roundtrip";
        let n = <ConcretePlatform as PlatformUdp>::send(
            send_sock.as_ptr(),
            payload.as_ptr(),
            payload.len(),
            ep.as_ptr(),
        );
        assert_eq!(n, payload.len(), "send should report full datagram");

        // Set the bound socket's recv timeout high enough that the
        // packet arrives even on a loaded host — the recv loop in
        // production would set it to 0 and yield repeatedly.
        <ConcretePlatform as PlatformUdp>::set_recv_timeout(bound.as_ptr(), 500);
        let mut buf = [0u8; 64];
        let got =
            <ConcretePlatform as PlatformUdp>::read(bound.as_ptr(), buf.as_mut_ptr(), buf.len());
        assert!(
            got >= payload.len(),
            "recv should return at least {} bytes, got {}",
            payload.len(),
            got
        );
        assert_eq!(&buf[..payload.len()], payload);

        <ConcretePlatform as PlatformUdp>::close(send_sock.as_mut_ptr());
        <ConcretePlatform as PlatformUdp>::close(bound.as_mut_ptr());
        <ConcretePlatform as PlatformUdp>::free_endpoint(ep.as_mut_ptr());
    }

    // Note: a multicast bind/send/recv roundtrip test was attempted
    // here but POSIX's `mcast_listen` requires a real interface name
    // (it walks `getifaddrs` looking for an exact match), and
    // `bind_multicast` passes NULL because the bare-metal / RTOS
    // backends don't use named interfaces. The proper validation for
    // SPDP join is the per-platform QEMU E2E test (Phase 71.27),
    // not a host-side unit test.

    #[test]
    fn factory_default_fragment_size_is_1344() {
        let rt = Arc::new(NrosPlatformRuntime::<ConcretePlatform>::new());
        let f = NrosUdpTransportFactory::<ConcretePlatform>::new(rt);
        assert_eq!(f.fragment_size, 1344);
        let f = f.with_fragment_size(8192);
        assert_eq!(f.fragment_size, 8192);
    }
}
