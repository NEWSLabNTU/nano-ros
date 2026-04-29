//! Phase 100.0 acceptance test — open two `unix-mock` channels paired
//! by `register_pair`, exchange frames in both directions, and confirm
//! the multi-frame "reassembly" pattern (one 200-byte payload split
//! across four 64-byte writes) round-trips with frame boundaries
//! preserved.
//!
//! Single test function (not split across `#[test]`s) so the registry
//! reset stays serialised and we don't fight cargo-test's per-test
//! parallelism.
//!
//! Build with: `cargo test -p nvidia-ivc --features unix-mock`.

#![cfg(feature = "unix-mock")]

use nvidia_ivc::{Channel, IvcError, unix_mock};

#[test]
fn loopback_round_trip_and_fragmentation() {
    unix_mock::reset_for_tests();
    unix_mock::register_pair(/*spe*/ 100, /*ccplex*/ 101);

    let spe = Channel::open(100).expect("channel 100");
    let ccplex = Channel::open(101).expect("channel 101");

    assert_eq!(spe.frame_size(), 64, "frame size matches NVIDIA IVC default");
    assert_eq!(ccplex.frame_size(), 64);

    // Empty queue → WouldBlock, never silent.
    assert!(matches!(spe.read(&mut [0u8; 64]), Err(IvcError::WouldBlock)));

    // Single-frame round trip, SPE → CCPLEX.
    let n = spe.write(b"ping").expect("write ping");
    assert_eq!(n, 4);
    let mut rx = [0u8; 64];
    let m = ccplex.read(&mut rx).expect("read ping");
    assert_eq!(m, 4);
    assert_eq!(&rx[..m], b"ping");

    // Reverse direction.
    let n = ccplex.write(b"pong!").expect("write pong");
    assert_eq!(n, 5);
    let m = spe.read(&mut rx).expect("read pong");
    assert_eq!(m, 5);
    assert_eq!(&rx[..m], b"pong!");

    // Fragmentation: a 200-byte logical payload split across four
    // 64-byte frames, exactly as the link layer will frame zenoh
    // messages on real hardware. Datagram boundaries must be
    // preserved — each `read` returns one full frame.
    let payload: Vec<u8> = (0..200).map(|i| (i & 0xff) as u8).collect();
    for chunk in payload.chunks(64) {
        let n = spe.write(chunk).expect("write fragment");
        assert_eq!(n, chunk.len());
    }
    let mut reassembled = Vec::with_capacity(200);
    let mut frame_count = 0;
    while reassembled.len() < payload.len() {
        let m = ccplex.read(&mut rx).expect("read fragment");
        assert!(m > 0 && m <= 64, "frame size in IVC bounds: {m}");
        reassembled.extend_from_slice(&rx[..m]);
        frame_count += 1;
    }
    assert_eq!(frame_count, 4, "200 bytes ≈ ⌈200/64⌉ = 4 frames");
    assert_eq!(reassembled, payload, "byte-perfect reassembly");

    // Queue drained again → WouldBlock.
    assert!(matches!(ccplex.read(&mut rx), Err(IvcError::WouldBlock)));

    unix_mock::reset_for_tests();
}
