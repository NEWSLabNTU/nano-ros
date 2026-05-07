//! Phase 115.H — runtime-pluggable custom transport for dust-dds.
//!
//! Mirrors [`crate::transport_nros::NrosUdpTransportFactory`]'s shape
//! but routes every byte through the user-supplied
//! [`nros_rmw::NrosTransportOps`] vtable instead of UDP sockets.
//! Unblocks the same set of bridges Phase 115 already exposes for
//! XRCE (115.E) and zenoh-pico (115.B): USB-CDC, BLE GATT, RS-485
//! with framing, ring-buffer loopback, semihosting bridge — anything
//! the consumer can wire to four C function pointers.
//!
//! # Surface
//!
//! `NrosCustomTransportParticipantFactory::from_slot()` drains the
//! registered vtable out of `nros_rmw`'s slot, panics if the slot is
//! empty, and returns a factory ready to plug into
//! `DomainParticipantFactoryAsync::new` as the `T:
//! TransportParticipantFactory` parameter.
//!
//! # Wire model
//!
//! - **One byte pipe.** The vtable is point-to-point — no multicast,
//!   no per-locator routing. Every `WriteMessage::write_message`
//!   call ignores its `locator_list` and forwards the datagram body
//!   to the user's `cb_write`. Every reader-task iteration calls
//!   `cb_read` and pushes whatever bytes it returns into
//!   `data_channel_sender`.
//! - **Synthetic locator.** RTPS still wants a non-empty
//!   locator-list to route discovery messages. v1 fabricates a single
//!   `udp/127.0.0.1:7400` placeholder for `default_unicast_locator_list`
//!   and leaves the multicast lists empty. The actual delivery does
//!   not consult the placeholder.
//! - **No multicast SPDP.** Discovery via `239.255.0.1` is a UDP-only
//!   convention; the custom byte pipe has no equivalent. v1 leaves
//!   discovery convergence to the application — both peers must
//!   register matching vtables and use a higher-level rendezvous
//!   (static peer config, out-of-band handshake) to agree on
//!   identity. Tracked as the heavy-half of 115.H follow-up.
//!
//! # Threading
//!
//! Same contract as the rest of Phase 115:
//! - `cb_open` / `cb_close` run on the factory's create-participant
//!   path.
//! - `cb_write` runs from inside `WriteMessage::write_message`'s
//!   future — single-threaded with respect to itself; concurrent
//!   readers and writers may interleave at the vtable level.
//! - `cb_read` runs from the reader task spawned onto the runtime
//!   spawner, polled by `DdsSession::drive_io()` once per
//!   `Executor::spin_once()`.

#![cfg(feature = "alloc")]

extern crate alloc;

use crate::sync::Arc;
use alloc::{boxed::Box, vec::Vec};
use core::{future::Future, pin::Pin};

use dust_dds::{
    dcps::channels::mpsc::MpscSender,
    runtime::Spawner,
    transport::{
        interface::{RtpsTransportParticipant, TransportParticipantFactory, WriteMessage},
        types::{LOCATOR_KIND_UDP_V4, Locator},
    },
};
use nros_rmw::NrosTransportOps;

use crate::runtime::NrosPlatformRuntime;

/// Default RTPS fragment size — matches the UDP transport's default
/// (1344 bytes, leaving room under a 1500-byte MTU minus IPv4/UDP
/// headers and RTPS overhead). v1 keeps this constant; future
/// builders may parameterise it.
const DEFAULT_FRAGMENT_SIZE: usize = 1344;

/// `Send`-able wrapper over [`NrosTransportOps`].
///
/// Rust's auto-derive for `Send` walks the struct's fields, sees the
/// `*mut c_void` `user_data`, and refuses. The whole vtable is
/// declared `Send + Sync` by `nros_rmw` (the contract is "user keeps
/// `user_data` alive across all callbacks"), but that opt-in does
/// not propagate through async-block field extraction. This newtype
/// re-states the same opt-in at the dust-dds boundary so async
/// blocks that capture the ops by value compile.
#[derive(Copy, Clone)]
struct OpsHandle(NrosTransportOps);

// SAFETY: `NrosTransportOps` itself carries `unsafe impl Send +
// Sync`. The user-data pointer's thread-safety is the registration
// contract's responsibility.
unsafe impl Send for OpsHandle {}
unsafe impl Sync for OpsHandle {}

/// Reader-task buffer size. RTPS-over-custom-byte-pipe has no
/// natural MTU; pick the same 65 KiB ceiling the UDP transport uses
/// so big fragmented samples don't truncate at the transport layer.
const RECV_BUF_SIZE: usize = 65507;

/// Build the synthetic placeholder locator that v1 advertises as the
/// participant's default unicast endpoint. Routing logic above the
/// transport never inspects the address; the locator exists only so
/// dust-dds's discovery state machine has something to announce.
fn placeholder_locator() -> Locator {
    let mut addr = [0u8; 16];
    // IPv6-mapped 127.0.0.1 — last four octets carry the IPv4 bytes.
    addr[12..16].copy_from_slice(&[127, 0, 0, 1]);
    Locator::new(LOCATOR_KIND_UDP_V4, 7400, addr)
}

// ---------------------------------------------------------------------------
// Outbound writer
// ---------------------------------------------------------------------------

/// `WriteMessage` impl that funnels every datagram through
/// `cb_write`.
pub struct CustomMessageWriter {
    ops: NrosTransportOps,
}

impl CustomMessageWriter {
    fn new(ops: NrosTransportOps) -> Self {
        Self { ops }
    }
}

// SAFETY: `NrosTransportOps` already carries `unsafe impl Send +
// Sync` (see `nros_rmw::custom_transport`). The wrapping struct
// adds no thread-unsafe state.
unsafe impl Send for CustomMessageWriter {}
unsafe impl Sync for CustomMessageWriter {}

impl WriteMessage for CustomMessageWriter {
    fn write_message(
        &self,
        datagram: &[u8],
        _locator_list: &[Locator],
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        // Locator list is ignored — the byte pipe is point-to-point.
        // dust-dds calls write_message synchronously from its sender
        // task; wrap the immediate `cb_write` in a Ready future to
        // satisfy the trait's async signature without yielding.
        let handle = OpsHandle(self.ops);
        let payload = datagram.to_vec();
        Box::pin(async move {
            // SAFETY: registration contract guarantees `ops.user_data`
            // outlives the participant; `cb_write` receives a
            // valid `(buf, len)` slice descriptor.
            let ops = handle.0;
            let _ = unsafe { (ops.write)(ops.user_data, payload.as_ptr(), payload.len()) };
        })
    }
}

// ---------------------------------------------------------------------------
// Inbound reader task
// ---------------------------------------------------------------------------

/// One-shot yield future, matching the cooperative-yield pattern in
/// `transport_nros.rs`. After a non-blocking `cb_read` returns 0
/// bytes, the task yields once before retrying so the runtime can
/// poll other tasks.
struct YieldOnce(bool);

impl YieldOnce {
    fn new() -> Self {
        Self(false)
    }
}

impl Future for YieldOnce {
    type Output = ();
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<()> {
        if self.0 {
            core::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    }
}

async fn custom_recv_loop(handle: OpsHandle, sender: MpscSender<Arc<[u8]>>) {
    let ops = handle.0;
    let mut buf = alloc::vec![0u8; RECV_BUF_SIZE];
    loop {
        // SAFETY: `ops.user_data` stays valid for the lifetime of
        // the participant per registration contract; `buf` is a
        // valid mutable slice. `timeout_ms = 0` requests a
        // non-blocking poll.
        let n = unsafe { (ops.read)(ops.user_data, buf.as_mut_ptr(), buf.len(), 0) };
        if n > 0 {
            let bytes = Arc::from(&buf[..n as usize]);
            if sender.send(bytes).await.is_err() {
                break;
            }
        } else if n < 0 {
            // Negative ⇒ user signalled fatal error (peer-close or
            // medium failure). Stop driving the channel; dust-dds's
            // upper layer will surface the dropped sender as a
            // participant-down condition.
            break;
        }
        YieldOnce::new().await;
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// dust-dds transport factory backed by an
/// [`nros_rmw::NrosTransportOps`] vtable.
///
/// Construct via [`Self::from_slot`] (drains the slot registered
/// through `nros_rmw::set_custom_transport`) or
/// [`Self::with_ops`] (test fixtures that bypass the slot).
///
/// `P` is the platform parameter, kept symmetric with
/// `NrosUdpTransportFactory<P>` so call sites can swap factories
/// without touching the surrounding type machinery. The custom
/// factory does not actually invoke any `nros-platform` traits —
/// the byte pipe replaces the platform-dependent UDP socket layer
/// entirely — but the spawner-bearing runtime still needs the same
/// `P` parameter to compile.
pub struct NrosCustomTransportParticipantFactory<P> {
    ops: NrosTransportOps,
    runtime: Arc<NrosPlatformRuntime<P>>,
    fragment_size: usize,
}

impl<P> NrosCustomTransportParticipantFactory<P> {
    /// Construct a factory by draining the currently-registered
    /// transport vtable from [`nros_rmw`]'s slot. Returns `None` if
    /// no vtable has been registered (caller forgot to call
    /// `nros_rmw::set_custom_transport(...)` before opening the
    /// participant).
    pub fn from_slot(runtime: Arc<NrosPlatformRuntime<P>>) -> Option<Self> {
        nros_rmw::take_custom_transport().map(|ops| Self::with_ops(runtime, ops))
    }

    /// Construct a factory directly from a known vtable. Used by
    /// integration tests that bypass the global slot.
    pub fn with_ops(runtime: Arc<NrosPlatformRuntime<P>>, ops: NrosTransportOps) -> Self {
        Self {
            ops,
            runtime,
            fragment_size: DEFAULT_FRAGMENT_SIZE,
        }
    }

    /// Override the RTPS fragment size advertised by participants
    /// this factory creates. Default is 1344 bytes.
    pub fn with_fragment_size(mut self, size: usize) -> Self {
        self.fragment_size = size;
        self
    }
}

impl<P> TransportParticipantFactory for NrosCustomTransportParticipantFactory<P>
where
    P: Send + Sync + 'static,
{
    fn create_participant(
        &self,
        _domain_id: i32,
        data_channel_sender: MpscSender<Arc<[u8]>>,
    ) -> impl Future<Output = RtpsTransportParticipant> + Send {
        let handle = OpsHandle(self.ops);
        let runtime = self.runtime.clone();
        let fragment_size = self.fragment_size;
        async move {
            let ops = handle.0;
            // 1. Bring the medium up. v1 passes NULL params — future
            // minor versions can thread structured params through
            // here. Non-zero return = open failure; we cannot recover
            // from there inside an async fn returning `RtpsTransportParticipant`,
            // so we proceed without surfacing the error and let
            // discovery time out instead. (Documented in the phase
            // doc as a known v1 limitation.)
            // SAFETY: ops vtable is valid for the lifetime of the
            // participant per registration contract.
            let _ = unsafe { (ops.open)(ops.user_data, core::ptr::null()) };

            // 2. Spawn the single reader task onto the runtime
            // spawner. `data_channel_sender` is cloned only into the
            // task; the participant itself does not retain a sender
            // copy.
            let spawner = runtime.spawner_handle();
            spawner.spawn(custom_recv_loop(handle, data_channel_sender));

            // 3. Hand back the participant with the byte-pipe writer
            // and a single placeholder locator. The metatraffic +
            // default multicast lists stay empty — no multicast over
            // a custom byte pipe.
            RtpsTransportParticipant {
                message_writer: Box::new(CustomMessageWriter::new(ops)),
                default_unicast_locator_list: alloc::vec![placeholder_locator()],
                metatraffic_unicast_locator_list: alloc::vec![placeholder_locator()],
                metatraffic_multicast_locator_list: Vec::new(),
                default_multicast_locator_list: Vec::new(),
                fragment_size,
            }
        }
    }
}
