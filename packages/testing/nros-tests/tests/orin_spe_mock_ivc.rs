//! Phase 100.8 — Orin SPE mock-IVC E2E test.
//!
//! Exercises the IVC link-transport wire format documented in
//! `docs/roadmap/phase-100-04-link-ivc-design.md` §5 against the
//! `nvidia-ivc` `unix-mock` backend. No NVIDIA SDK is required — the
//! mock implements one IVC channel on top of an `AF_UNIX SOCK_DGRAM`
//! pair, so this test runs on any POSIX host.
//!
//! What this test pins down (and what the CCPLEX-side bridge daemon
//! in `autoware_sentinel/src/ivc-bridge/` must match byte-for-byte):
//!
//! * 64-byte fixed frame size (NVIDIA IVC default for `aon_echo`).
//! * 4-byte little-endian header per frame: `u16 total_len` + `u16 offset`.
//! * Payload up to `frame_size - 4 = 60` bytes per frame.
//! * SPSC FIFO, no reordering — `offset` must equal the running byte
//!   count on the receive side.
//! * `total_len = 0, offset = 0` is a reserved keep-alive ping the
//!   receiver drops silently.
//!
//! The Rust framing helpers in this file mirror the C
//! `__z_ivc_send_batch` / `__z_ivc_recv_batch` state machines in
//! `packages/zpico/zpico-sys/zenoh-pico/src/link/unicast/ivc.c`. If
//! you change the wire spec, both sides + the bridge daemon move
//! together.
//!
//! Run with:
//!   `cargo nextest run -p nros-tests --test orin_spe_mock_ivc`
//! or via the platform recipe:
//!   `just orin_spe test`

#![cfg(unix)]

use nvidia_ivc::{Channel, IvcError, unix_mock};

const FRAME_SIZE: usize = 64;
const HEADER_SIZE: usize = 4;
const PAYLOAD_MAX: usize = FRAME_SIZE - HEADER_SIZE;

// Channel IDs picked above the loopback test's range so cargo-test's
// per-test parallelism doesn't collide. Each test uses a distinct pair.
mod ids {
    pub const SINGLE_TX: u32 = 210;
    pub const SINGLE_RX: u32 = 211;
    pub const MULTI_TX: u32 = 212;
    pub const MULTI_RX: u32 = 213;
    pub const PING_TX: u32 = 214;
    pub const PING_RX: u32 = 215;
    pub const BAD_TX: u32 = 216;
    pub const BAD_RX: u32 = 217;
}

// =============================================================================
// Framing helpers (Rust mirror of `ivc.c`'s send/recv batch state machine).
// =============================================================================

/// Send `payload` framed over `tx`, splitting across `frame_size`-bound
/// frames with a `u16 total_len + u16 offset` LE header. Mirrors
/// `__z_ivc_send_batch`.
fn send_framed(tx: &Channel, payload: &[u8]) -> Result<(), IvcError> {
    assert!(payload.len() <= u16::MAX as usize, "payload fits in u16 total_len");
    let total = payload.len() as u16;
    let mut off: u16 = 0;
    while (off as usize) < payload.len() {
        let chunk = (payload.len() - off as usize).min(PAYLOAD_MAX);
        let mut frame = [0u8; FRAME_SIZE];
        frame[0..2].copy_from_slice(&total.to_le_bytes());
        frame[2..4].copy_from_slice(&off.to_le_bytes());
        frame[HEADER_SIZE..HEADER_SIZE + chunk]
            .copy_from_slice(&payload[off as usize..off as usize + chunk]);
        let n = tx.write(&frame[..HEADER_SIZE + chunk])?;
        assert_eq!(n, HEADER_SIZE + chunk, "unix-mock writes are atomic");
        off = off.checked_add(chunk as u16).expect("offset stays in u16");
    }
    tx.notify();
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum RecvErr {
    Wire,
    Closed,
}

/// Read framed bytes from `rx` until one full batch is reassembled.
/// Mirrors `__z_ivc_recv_batch`. Drops `total=0, offset=0` keep-alives.
fn recv_framed(rx: &Channel) -> Result<Vec<u8>, RecvErr> {
    let mut expected_total: u16 = 0;
    let mut acc: Vec<u8> = Vec::new();
    let mut frame = [0u8; FRAME_SIZE];
    loop {
        let n = match rx.read(&mut frame) {
            Ok(n) => n,
            Err(IvcError::WouldBlock) => {
                std::thread::sleep(std::time::Duration::from_millis(1));
                continue;
            }
            Err(_) => return Err(RecvErr::Closed),
        };
        if n < HEADER_SIZE {
            return Err(RecvErr::Wire);
        }
        let total = u16::from_le_bytes([frame[0], frame[1]]);
        let off = u16::from_le_bytes([frame[2], frame[3]]);
        let payload_len = n - HEADER_SIZE;

        // Reserved keep-alive ping (§5.2). Drop and keep looping.
        if total == 0 && off == 0 {
            continue;
        }

        if expected_total == 0 {
            expected_total = total;
            acc.clear();
        } else if total != expected_total {
            return Err(RecvErr::Wire);
        }

        // SPSC FIFO ⇒ no reordering. `offset` should equal accumulated len.
        if (off as usize) != acc.len() || off as usize + payload_len > expected_total as usize {
            return Err(RecvErr::Wire);
        }
        acc.extend_from_slice(&frame[HEADER_SIZE..HEADER_SIZE + payload_len]);
        if acc.len() == expected_total as usize {
            return Ok(acc);
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[test]
fn single_frame_message_round_trips() {
    unix_mock::register_pair(ids::SINGLE_TX, ids::SINGLE_RX);
    let tx = Channel::open(ids::SINGLE_TX).expect("tx channel");
    let rx = Channel::open(ids::SINGLE_RX).expect("rx channel");

    assert_eq!(tx.frame_size(), FRAME_SIZE, "wire spec § frame size");

    let payload: &[u8] = b"hello sentinel";
    send_framed(&tx, payload).expect("send");
    let got = recv_framed(&rx).expect("recv");
    assert_eq!(got, payload);
}

#[test]
fn multi_frame_zenoh_batch_reassembles() {
    unix_mock::register_pair(ids::MULTI_TX, ids::MULTI_RX);
    let tx = Channel::open(ids::MULTI_TX).expect("tx channel");
    let rx = Channel::open(ids::MULTI_RX).expect("rx channel");

    // 200 bytes — straddles four 60-byte payload frames + a partial.
    // ⌈200 / 60⌉ = 4 frames (60 + 60 + 60 + 20).
    let payload: Vec<u8> = (0..200u16).map(|i| (i & 0xff) as u8).collect();

    send_framed(&tx, &payload).expect("send");
    let got = recv_framed(&rx).expect("recv");
    assert_eq!(got, payload, "byte-perfect reassembly across 4 frames");

    // After reassembly completes the next read must block again.
    let mut buf = [0u8; FRAME_SIZE];
    assert!(matches!(rx.read(&mut buf), Err(IvcError::WouldBlock)));
}

#[test]
fn keepalive_ping_is_dropped_silently() {
    unix_mock::register_pair(ids::PING_TX, ids::PING_RX);
    let tx = Channel::open(ids::PING_TX).expect("tx channel");
    let rx = Channel::open(ids::PING_RX).expect("rx channel");

    // Write a reserved keep-alive (total=0, offset=0) followed by a real
    // message; the recv state machine must drop the ping and surface the
    // payload only.
    let ping = [0u8; HEADER_SIZE];
    let n = tx.write(&ping).expect("write ping");
    assert_eq!(n, HEADER_SIZE);
    tx.notify();
    send_framed(&tx, b"after-ping").expect("send real payload");

    let got = recv_framed(&rx).expect("recv");
    assert_eq!(got, b"after-ping", "ping dropped, payload survives");
}

#[test]
fn wire_violation_yields_protocol_error() {
    unix_mock::register_pair(ids::BAD_TX, ids::BAD_RX);
    let tx = Channel::open(ids::BAD_TX).expect("tx channel");
    let rx = Channel::open(ids::BAD_RX).expect("rx channel");

    // Send a frame with `offset != 0` for a brand-new batch — the
    // receive state machine must reject this. Wire format violation
    // (corrupt or hostile peer).
    let mut frame = [0u8; HEADER_SIZE + 8];
    frame[0..2].copy_from_slice(&20u16.to_le_bytes()); // total_len = 20
    frame[2..4].copy_from_slice(&8u16.to_le_bytes()); // offset = 8 (must be 0!)
    let n = tx.write(&frame).expect("write violating frame");
    assert_eq!(n, frame.len());
    tx.notify();

    let got = recv_framed(&rx);
    assert_eq!(got, Err(RecvErr::Wire));
}
