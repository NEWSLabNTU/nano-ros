//! Phase 173.6 — `embedded-nal` adapter over the smoltcp bridge.
//!
//! This is the "consume the ecosystem, don't reinvent" direction
//! (Phase 173.5 notes): rather than define a bespoke socket trait,
//! `nros-smoltcp` exposes its socket layer through the de-facto
//! [`embedded-nal`] `TcpClientStack` / `UdpClientStack` traits so any
//! conformant `embedded-nal` consumer (a Rust app, a future
//! `embedded-nal`-based RMW) can drive it.
//!
//! ## Why an adapter, not a rewrite
//!
//! The bridge's primary surface is the **C ABI** zenoh-pico calls
//! (`nros_platform_*` → [`crate::SmoltcpBridge`] static ops): global,
//! no-`self`, with the board owning the `Interface`/`Device` and the
//! bridge owning only the socket table (via `set_network_state`). The
//! `embedded-nal` traits take `&mut self` (the whole stack). Rather than
//! restructure that ownership (and the manual poll / idle-callback
//! model the C ABI relies on), [`SmoltcpNalStack`] is a zero-sized
//! handle to the existing global bridge: each trait method delegates to
//! the same `SmoltcpBridge::{tcp,udp}_*` op the C ABI uses. So both
//! surfaces share one socket table and one poll loop.
//!
//! ## Non-blocking semantics
//!
//! The bridge ops are already non-blocking (poll-driven), so they map
//! onto `nb`: a `0`-byte transfer on a live socket is
//! [`nb::Error::WouldBlock`]; a negative return is [`NalError`].
//!
//! [`embedded-nal`]: https://docs.rs/embedded-nal

use core::net::SocketAddr;

use embedded_nal::{TcpClientStack, TcpError, TcpErrorKind, UdpClientStack};

use crate::SmoltcpBridge;

/// Errors surfaced by the `embedded-nal` adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NalError {
    /// The bridge socket table is exhausted (`*_open` returned `< 0`).
    NoSocket,
    /// The remote address is not IPv4 (the bridge is `proto-ipv4`).
    NotIpv4,
    /// A bridge op reported an error (negative return).
    Io,
    /// The TCP connection closed while a transfer was pending.
    Closed,
}

impl TcpError for NalError {
    fn kind(&self) -> TcpErrorKind {
        match self {
            NalError::Closed => TcpErrorKind::PipeClosed,
            _ => TcpErrorKind::Other,
        }
    }
}

/// Zero-sized handle to the global [`SmoltcpBridge`]. Construct with
/// [`SmoltcpNalStack::new`]; all state lives in the bridge statics
/// (shared with the C-ABI path), so this carries nothing itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct SmoltcpNalStack;

impl SmoltcpNalStack {
    pub const fn new() -> Self {
        Self
    }
}

/// IPv4-only: extract `([u8; 4], port)` from a `SocketAddr`.
fn ipv4_parts(remote: &SocketAddr) -> Result<([u8; 4], u16), NalError> {
    match remote {
        SocketAddr::V4(v4) => Ok((v4.ip().octets(), v4.port())),
        SocketAddr::V6(_) => Err(NalError::NotIpv4),
    }
}

/// A TCP socket: the bridge handle + whether `connect` has fired the
/// SYN (so a re-poll under `WouldBlock` doesn't reconnect).
#[derive(Debug, Clone, Copy)]
pub struct NalTcpSocket {
    handle: i32,
    connecting: bool,
}

impl TcpClientStack for SmoltcpNalStack {
    type TcpSocket = NalTcpSocket;
    type Error = NalError;

    fn socket(&mut self) -> Result<Self::TcpSocket, Self::Error> {
        let handle = SmoltcpBridge::tcp_open();
        if handle < 0 {
            return Err(NalError::NoSocket);
        }
        Ok(NalTcpSocket {
            handle,
            connecting: false,
        })
    }

    fn connect(
        &mut self,
        socket: &mut Self::TcpSocket,
        remote: SocketAddr,
    ) -> nb::Result<(), Self::Error> {
        let (ip, port) = ipv4_parts(&remote)?;
        if !socket.connecting {
            if SmoltcpBridge::tcp_connect(socket.handle, &ip, port) < 0 {
                return Err(nb::Error::Other(NalError::Io));
            }
            socket.connecting = true;
        }
        SmoltcpBridge::poll_network();
        if SmoltcpBridge::tcp_is_connected(socket.handle) {
            Ok(())
        } else {
            Err(nb::Error::WouldBlock)
        }
    }

    fn send(
        &mut self,
        socket: &mut Self::TcpSocket,
        buffer: &[u8],
    ) -> nb::Result<usize, Self::Error> {
        match SmoltcpBridge::tcp_send(socket.handle, buffer) {
            n if n > 0 => Ok(n as usize),
            0 => Err(nb::Error::WouldBlock),
            _ => Err(nb::Error::Other(NalError::Io)),
        }
    }

    fn receive(
        &mut self,
        socket: &mut Self::TcpSocket,
        buffer: &mut [u8],
    ) -> nb::Result<usize, Self::Error> {
        SmoltcpBridge::poll_network();
        match SmoltcpBridge::tcp_recv(socket.handle, buffer) {
            n if n > 0 => Ok(n as usize),
            0 if SmoltcpBridge::tcp_is_connected(socket.handle) => Err(nb::Error::WouldBlock),
            0 => Err(nb::Error::Other(NalError::Closed)),
            _ => Err(nb::Error::Other(NalError::Io)),
        }
    }

    fn close(&mut self, socket: Self::TcpSocket) -> Result<(), Self::Error> {
        SmoltcpBridge::tcp_close(socket.handle);
        Ok(())
    }
}

/// A UDP socket: the bridge handle + the connected remote (the bridge
/// `udp_recv` doesn't surface the sender, so `receive` reports this).
#[derive(Debug, Clone, Copy)]
pub struct NalUdpSocket {
    handle: i32,
    remote: Option<SocketAddr>,
}

impl UdpClientStack for SmoltcpNalStack {
    type UdpSocket = NalUdpSocket;
    type Error = NalError;

    fn socket(&mut self) -> Result<Self::UdpSocket, Self::Error> {
        let handle = SmoltcpBridge::udp_open();
        if handle < 0 {
            return Err(NalError::NoSocket);
        }
        Ok(NalUdpSocket {
            handle,
            remote: None,
        })
    }

    fn connect(
        &mut self,
        socket: &mut Self::UdpSocket,
        remote: SocketAddr,
    ) -> Result<(), Self::Error> {
        let (ip, port) = ipv4_parts(&remote)?;
        if SmoltcpBridge::udp_set_remote(socket.handle, &ip, port) < 0 {
            return Err(NalError::Io);
        }
        socket.remote = Some(remote);
        Ok(())
    }

    fn send(&mut self, socket: &mut Self::UdpSocket, buffer: &[u8]) -> nb::Result<(), Self::Error> {
        let remote = socket.remote.ok_or(nb::Error::Other(NalError::Io))?;
        let (ip, port) = ipv4_parts(&remote)?;
        match SmoltcpBridge::udp_send(socket.handle, buffer, &ip, port) {
            n if n > 0 => Ok(()),
            0 => Err(nb::Error::WouldBlock),
            _ => Err(nb::Error::Other(NalError::Io)),
        }
    }

    fn receive(
        &mut self,
        socket: &mut Self::UdpSocket,
        buffer: &mut [u8],
    ) -> nb::Result<(usize, SocketAddr), Self::Error> {
        SmoltcpBridge::poll_network();
        match SmoltcpBridge::udp_recv(socket.handle, buffer) {
            n if n > 0 => {
                let from = socket.remote.ok_or(nb::Error::Other(NalError::Io))?;
                Ok((n as usize, from))
            }
            0 => Err(nb::Error::WouldBlock),
            _ => Err(nb::Error::Other(NalError::Io)),
        }
    }

    fn close(&mut self, socket: Self::UdpSocket) -> Result<(), Self::Error> {
        SmoltcpBridge::udp_close(socket.handle);
        Ok(())
    }
}
