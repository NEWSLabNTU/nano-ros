//! RFC-0132 W9.a: classic and FD frames through the mock and back, over the
//! C ABI (`nros-platform-cffi`'s generated declarations and its `Can` wrapper),
//! and the frames every backend refuses.

use std::sync::Mutex;

use nros_can_mock as mock;
use nros_platform_cffi::{
    NROS_PLATFORM_CAN_EAGAIN, NROS_PLATFORM_CAN_EINVAL, NROS_PLATFORM_CAN_ENOFD,
    NROS_PLATFORM_CAN_ENOTOPEN, NROS_PLATFORM_CAN_FLAG_BRS, NROS_PLATFORM_CAN_FLAG_EXT,
    NROS_PLATFORM_CAN_FLAG_FD, NROS_PLATFORM_CAN_SEND_WAIT_TX,
    can::{Can, CanError, closed_storage, frame},
    nros_platform_can_open, nros_platform_can_send,
};

// One bus per process (the C ABI's `open(NULL)` names no instance).
static LOCK: Mutex<()> = Mutex::new(());

fn err(code: i32) -> Result<(), CanError> {
    Err(CanError(code as i8))
}

const FD: u8 = NROS_PLATFORM_CAN_FLAG_FD as u8;
const BRS: u8 = NROS_PLATFORM_CAN_FLAG_BRS as u8;
const EXT: u8 = NROS_PLATFORM_CAN_FLAG_EXT as u8;

#[test]
fn classic_and_fd_frames_round_trip() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    mock::reset();
    let mut storage = closed_storage();
    let mut can = Can::open(&mut storage, None, true).expect("open(NULL, want_fd)");
    assert!(can.fd_active());
    assert_eq!(mock::last_opened_device(), Some(None));

    let classic = frame(0x123, 0, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    can.send(&classic, 0).unwrap();
    let payload: Vec<u8> = (0..64).collect();
    let fd = frame(0x1abc_def0, EXT | BRS, &payload).unwrap();
    can.send_fd(&fd, 1000).unwrap();

    let sent = mock::take_sent();
    assert_eq!(sent.len(), 2);
    assert_eq!((sent[0].id, sent[0].flags, sent[0].len), (0x123, 0, 8));
    assert_eq!(&sent[0].data[..8], &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(
        (sent[1].id, sent[1].flags, sent[1].len),
        (0x1abc_def0, EXT | BRS | FD, 64)
    );
    assert_eq!(&sent[1].data[..], &payload[..]);

    // Read back what the bus replays, byte for byte.
    for f in &sent {
        mock::push_rx(*f);
    }
    for f in &sent {
        let got = can.recv(0).expect("recv");
        assert_eq!((got.id, got.flags, got.len), (f.id, f.flags, f.len));
        assert_eq!(&got.data[..got.len as usize], &f.data[..f.len as usize]);
    }
    assert_eq!(can.recv(0).map(|_| ()), err(NROS_PLATFORM_CAN_EAGAIN));

    // The mock records synchronously, so it reports completion.
    let waited = frame(0x10, NROS_PLATFORM_CAN_SEND_WAIT_TX as u8, &[9]).unwrap();
    can.send(&waited, 0).unwrap();
    assert_eq!(mock::take_sent().len(), 1);
}

#[test]
fn invalid_frames_are_refused() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    mock::reset();
    let mut storage = closed_storage();
    let mut can = Can::open(&mut storage, None, true).unwrap();

    // A 9-byte classic frame and a 13-byte FD frame: not valid sizes.
    assert_eq!(
        can.send(&frame(0x1, 0, &[0; 9]).unwrap(), 0),
        err(NROS_PLATFORM_CAN_EINVAL)
    );
    assert_eq!(
        can.send_fd(&frame(0x1, 0, &[0; 13]).unwrap(), 0),
        err(NROS_PLATFORM_CAN_EINVAL)
    );
    // Every valid FD size passes; every other length up to 64 is refused.
    for len in 0..=64usize {
        let ok = len <= 8 || [12, 16, 20, 24, 32, 48, 64].contains(&len);
        let rc = can.send_fd(&frame(0x1, 0, &vec![0; len]).unwrap(), 0);
        assert_eq!(rc.is_ok(), ok, "FD len {len}");
    }
    // Identifier out of range for its width, BRS without FD, an unknown flag.
    assert_eq!(
        can.send(&frame(0x800, 0, &[]).unwrap(), 0),
        err(NROS_PLATFORM_CAN_EINVAL)
    );
    assert_eq!(
        can.send(&frame(0x2000_0000, EXT, &[]).unwrap(), 0),
        err(NROS_PLATFORM_CAN_EINVAL)
    );
    assert_eq!(
        can.send(&frame(0x1, BRS, &[]).unwrap(), 0),
        err(NROS_PLATFORM_CAN_EINVAL)
    );
    assert_eq!(
        can.send(&frame(0x1, 0x40, &[]).unwrap(), 0),
        err(NROS_PLATFORM_CAN_EINVAL)
    );
    drop(can);
    // A NULL frame through the raw C call.
    let mut raw = closed_storage();
    assert_eq!(
        unsafe { nros_platform_can_open(&mut raw, core::ptr::null(), 0) },
        0
    );
    let rc = unsafe { nros_platform_can_send(&mut raw, core::ptr::null(), 0) };
    assert_eq!(rc as i32, NROS_PLATFORM_CAN_EINVAL);
    unsafe { nros_platform_cffi::nros_platform_can_close(&mut raw) };

    // An FD frame on a classic open.
    let mut storage = closed_storage();
    let mut classic = Can::open(&mut storage, None, false).unwrap();
    assert!(!classic.fd_active());
    assert_eq!(
        classic.send_fd(&frame(0x1, 0, &[0; 8]).unwrap(), 0),
        err(NROS_PLATFORM_CAN_ENOFD)
    );
    drop(classic);
    assert!(
        mock::take_sent()
            .iter()
            .all(|f| f.len <= 8 || f.flags & FD != 0)
    );
}

#[test]
fn want_fd_on_a_classic_device_fails_at_open() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    mock::reset();
    mock::set_fd_capable(false);
    let mut storage = closed_storage();
    assert_eq!(
        Can::open(&mut storage, None, true).map(|_| ()),
        err(NROS_PLATFORM_CAN_ENOFD)
    );
    let can = Can::open(&mut storage, Some(c"vcan7"), false).unwrap();
    assert!(!can.fd_active());
    assert_eq!(mock::last_opened_device(), Some(Some("vcan7".to_string())));
    drop(can);
    mock::reset();
}

#[test]
fn a_closed_handle_is_refused() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    mock::reset();
    let mut raw = closed_storage();
    let f = frame(0x1, 0, &[1]).unwrap();
    let rc = unsafe { nros_platform_can_send(&mut raw, &f, 0) };
    assert_eq!(rc as i32, NROS_PLATFORM_CAN_ENOTOPEN);
    assert_eq!(
        unsafe { nros_platform_cffi::nros_platform_can_fd_active(&raw) },
        0
    );
    unsafe { nros_platform_cffi::nros_platform_can_close(&mut raw) };
    assert!(mock::take_sent().is_empty());
}
