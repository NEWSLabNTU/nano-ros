//! RFC-0132 / phase-560 W9.e -- the cargo road's CAN device delivery probe.
//!
//! `register()` opens the image's CAN device (`open(NULL)`) and prints which
//! device the board handed over and what `open` returned; on success it sends
//! one classic frame (0x263, 8 bytes) that `scripts/test/can-image-device.sh`
//! reads back with `candump`. It prints and returns: the entry keeps running,
//! and the script stops it once the line is out.

// The `socketcan` backend and the device store live in the board's archive.
extern crate nros_board_linux as _;

use nros::{Callback, CallbackCtx, Component, ExecutableNode, NodeContext, NodeResult};
use nros_platform_cffi::can::{Can, closed_storage, frame};

unsafe extern "C" {
    // The board's store of the image's device (RFC-0132 D4), which the
    // backend's `open(NULL)` reads. Board-internal; the probe reads it only to
    // report the device it opened.
    fn nros_board_can_image_device() -> *const core::ffi::c_char;
}

/// The probe's classic frame.
pub const PROBE_ID: u32 = 0x263;
pub const PROBE_DATA: [u8; 8] = [0x26, 0x33, 0x0e, 0x00, 0xca, 0xfe, 0x01, 0x02];

pub struct CanImageProbe;

fn probe() {
    // SAFETY: the store returns NULL or a NUL-terminated string it owns.
    let dev = unsafe {
        let p = nros_board_can_image_device();
        if p.is_null() {
            "-".to_string()
        } else {
            core::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    };
    let mut storage = closed_storage();
    match Can::open(&mut storage, None, false) {
        Ok(mut can) => {
            println!("CAN-IMAGE: device={dev} open(NULL)=0");
            let f = frame(PROBE_ID, 0, &PROBE_DATA).expect("8 bytes");
            match can.send(&f, 100_000) {
                Ok(()) => println!("CAN-IMAGE: sent 0x{PROBE_ID:03X} on {dev}"),
                Err(e) => println!("CAN-IMAGE: send failed: {}", e.0),
            }
        }
        Err(e) => println!("CAN-IMAGE: device={dev} open(NULL)={}", e.0),
    }
}

impl Component for CanImageProbe {
    const NAME: &'static str = "can_image_probe";
    const ENTITY_BOUNDS: nros::EntityBounds = nros::EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let _node = ctx.create_node(nros::NodeOptions::new("can_image_probe"))?;
        probe();
        Ok(())
    }
}

impl ExecutableNode for CanImageProbe {
    type State = ();
    const HAS_TICK: bool = false;

    fn init() -> Self::State {}

    fn on_callback(_state: &mut Self::State, _callback: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {}
}

nros::node!(CanImageProbe);
