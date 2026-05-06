//! Phase 11.3.A acceptance — open two `unix-mock` channels paired by
//! `register_pair`, exchange frames in both directions through the
//! **zero-copy** API (`read_frame` / `write_frame` / `commit` / `ack`),
//! and confirm the multi-frame fragmentation pattern (one 200-byte
//! payload split across four 64-byte frames) round-trips with frame
//! boundaries preserved.
//!
//! Single test function (not split across `#[test]`s) so the registry
//! reset stays serialised and we don't fight cargo-test's per-test
//! parallelism.
//!
//! Build with: `cargo test -p nvidia-ivc --features unix-mock`.

#![cfg(feature = "unix-mock")]

use nvidia_ivc::{Channel, unix_mock};

#[test]
fn loopback_round_trip_and_fragmentation() {
    unix_mock::reset_for_tests();
    unix_mock::register_pair(/*spe*/ 100, /*ccplex*/ 101);

    let spe = Channel::open(100).expect("channel 100");
    let ccplex = Channel::open(101).expect("channel 101");

    assert_eq!(
        spe.frame_size(),
        64,
        "frame size matches NVIDIA IVC default"
    );
    assert_eq!(ccplex.frame_size(), 64);

    // Empty queue → read_frame returns None.
    assert!(spe.read_frame().is_none(), "no frame on fresh queue");

    // Single-frame round trip, SPE → CCPLEX, via zero-copy commit.
    {
        let mut tx = spe.write_frame().expect("tx slot");
        tx.as_mut_slice()[..4].copy_from_slice(b"ping");
        tx.commit(4);
    }
    {
        let rx = ccplex.read_frame().expect("rx ping");
        assert_eq!(rx.as_slice(), b"ping");
        rx.ack();
    }

    // Reverse direction.
    {
        let mut tx = ccplex.write_frame().expect("tx slot");
        tx.as_mut_slice()[..5].copy_from_slice(b"pong!");
        tx.commit(5);
    }
    {
        let rx = spe.read_frame().expect("rx pong");
        assert_eq!(rx.as_slice(), b"pong!");
        // Drop releases — equivalent to .ack().
    }

    // Fragmentation: a 200-byte logical payload split across four
    // 64-byte frames, exactly as the link layer will frame zenoh
    // messages on real hardware. Datagram boundaries must be
    // preserved — each `read_frame` returns one full frame.
    let payload: Vec<u8> = (0..200).map(|i| (i & 0xff) as u8).collect();
    for chunk in payload.chunks(64) {
        let mut tx = spe.write_frame().expect("tx slot");
        tx.as_mut_slice()[..chunk.len()].copy_from_slice(chunk);
        tx.commit(chunk.len());
    }
    let mut reassembled = Vec::with_capacity(200);
    let mut frame_count = 0;
    while reassembled.len() < payload.len() {
        let rx = ccplex.read_frame().expect("read fragment");
        let bytes = rx.as_slice();
        assert!(
            !bytes.is_empty() && bytes.len() <= 64,
            "frame size in IVC bounds: {}",
            bytes.len()
        );
        reassembled.extend_from_slice(bytes);
        frame_count += 1;
        // implicit ack via Drop
    }
    assert_eq!(frame_count, 4, "200 bytes ≈ ⌈200/64⌉ = 4 frames");
    assert_eq!(reassembled, payload, "byte-perfect reassembly");

    // Queue drained → read_frame None again.
    assert!(ccplex.read_frame().is_none(), "queue drained");

    // Abandon test: write_frame without commit should leave the slot
    // free and not produce a frame on the peer.
    {
        let mut tx = spe.write_frame().expect("tx slot");
        tx.as_mut_slice()[..3].copy_from_slice(b"xyz");
        // Drop without commit
    }
    assert!(
        ccplex.read_frame().is_none(),
        "abandoned slot must not deliver"
    );

    unix_mock::reset_for_tests();
}
