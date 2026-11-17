// RFC-0132 W9.c -- the CAN frame probe a Linux-process board's image runs.
//
// ONE body, `include!`d by each board's probe test (`nros-board-linux`'s and
// `nros-board-freertos-posix`'s), so both images send the same frames and
// `scripts/test/can-frame-socketcan.sh` checks one expectation against
// `candump`. It calls the C ABI through `nros_platform_cffi::can`, which the
// board's `socketcan` backend (`nros-board-linux/c/can_socketcan.c`) defines.
//
// Arguments: `<fd-iface> [<classic-iface>]`. The FD interface must have the
// CAN FD MTU (72); the classic one, when given, the classic MTU (16).

use nros_platform_cffi::{
    NROS_PLATFORM_CAN_EINVAL, NROS_PLATFORM_CAN_ENOFD, NROS_PLATFORM_CAN_ENOTSUP,
    NROS_PLATFORM_CAN_FLAG_BRS, NROS_PLATFORM_CAN_FLAG_EXT, NROS_PLATFORM_CAN_SEND_WAIT_TX,
    can::{Can, CanError, closed_storage, frame},
};

/// The classic frame: 0x123, 8 bytes.
pub const PROBE_CLASSIC_ID: u32 = 0x123;
pub const PROBE_CLASSIC_DATA: [u8; 8] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
/// The FD frame: 29-bit 0x1ABCDEF0, bit-rate switch, 64 bytes 0x00..0x3F.
pub const PROBE_FD_ID: u32 = 0x1abc_def0;

fn want(what: &str, got: Result<(), CanError>, code: i32) -> Result<(), String> {
    if got == Err(CanError(code as i8)) {
        Ok(())
    } else {
        Err(format!("{what}: expected code {code}, got {got:?}"))
    }
}

pub fn can_probe(args: &[String]) -> Result<(), String> {
    let iface = args.first().ok_or("usage: <fd-iface> [<classic-iface>]")?;
    let name = std::ffi::CString::new(iface.as_str()).map_err(|e| e.to_string())?;

    let mut storage = closed_storage();
    let mut can = Can::open(&mut storage, Some(&name), true)
        .map_err(|e| format!("open({iface}, want_fd) failed: {e:?}"))?;
    if !can.fd_active() {
        return Err(format!("{iface}: fd_active is false after open(want_fd)"));
    }

    let classic = frame(PROBE_CLASSIC_ID, 0, &PROBE_CLASSIC_DATA).expect("8 bytes");
    can.send(&classic, 100_000)
        .map_err(|e| format!("classic send failed: {e:?}"))?;
    let payload: Vec<u8> = (0u8..64).collect();
    let fd = frame(
        PROBE_FD_ID,
        (NROS_PLATFORM_CAN_FLAG_EXT | NROS_PLATFORM_CAN_FLAG_BRS) as u8,
        &payload,
    )
    .expect("64 bytes");
    can.send_fd(&fd, 100_000)
        .map_err(|e| format!("FD send failed: {e:?}"))?;
    println!(
        "CAN-PROBE: sent classic 0x{PROBE_CLASSIC_ID:03X} and FD 0x{PROBE_FD_ID:08X} on {iface}"
    );

    // Refusals that never reach the bus.
    want(
        "9-byte classic",
        can.send(&frame(0x1, 0, &[0; 9]).unwrap(), 0),
        NROS_PLATFORM_CAN_EINVAL,
    )?;
    want(
        "13-byte FD",
        can.send_fd(&frame(0x1, 0, &[0; 13]).unwrap(), 0),
        NROS_PLATFORM_CAN_EINVAL,
    )?;
    let waited = frame(0x1, NROS_PLATFORM_CAN_SEND_WAIT_TX as u8, &[0]).unwrap();
    want(
        "SEND_WAIT_TX",
        can.send(&waited, 0),
        NROS_PLATFORM_CAN_ENOTSUP,
    )?;
    drop(can);

    // A classic open of the FD interface: FD is not active, so an FD send is
    // refused with ENOFD (RFC-0132 revision of 2026-11-17) and never sent.
    let mut storage = closed_storage();
    let mut classic_open = Can::open(&mut storage, Some(&name), false)
        .map_err(|e| format!("open({iface}, classic) failed: {e:?}"))?;
    if classic_open.fd_active() {
        return Err(format!("{iface}: fd_active is true on a classic open"));
    }
    want(
        "FD send on a classic open",
        classic_open.send_fd(
            &frame(PROBE_FD_ID, NROS_PLATFORM_CAN_FLAG_EXT as u8, &[0; 8]).unwrap(),
            0,
        ),
        NROS_PLATFORM_CAN_ENOFD,
    )?;
    drop(classic_open);

    match args.get(1) {
        None => println!("CAN-PROBE: classic-interface check NOT RUN (no classic interface given)"),
        Some(classic_iface) => {
            let cname =
                std::ffi::CString::new(classic_iface.as_str()).map_err(|e| e.to_string())?;
            let mut storage = closed_storage();
            want(
                "open(classic, want_fd)",
                Can::open(&mut storage, Some(&cname), true).map(|_| ()),
                NROS_PLATFORM_CAN_ENOFD,
            )?;
            let can = Can::open(&mut storage, Some(&cname), false)
                .map_err(|e| format!("open({classic_iface}, classic) failed: {e:?}"))?;
            if can.fd_active() {
                return Err(format!(
                    "{classic_iface}: fd_active is true on a classic open"
                ));
            }
            println!("CAN-PROBE: {classic_iface}: want_fd refused (ENOFD), fd_active false");
        }
    }
    println!("CAN-PROBE: PASS");
    Ok(())
}
