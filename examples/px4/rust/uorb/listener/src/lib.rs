//! `px4-rs-listener` — PX4 nano-ros example. Subscribes to
//! [`SensorPing`] (published by the `px4-rs-talker` example) and logs
//! each message via `px4-log`.

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int};

use nros_rmw_uorb::subscription;
use px4_log::{err, info, module, panic_handler};
use px4_msg_macros::px4_message;
use px4_workqueue::task;

module!("nros_listener");
panic_handler!();

#[px4_message("../msg/SensorPing.msg")]
pub struct SensorPing;

#[task(wq = "lp_default")]
async fn pump() {
    let sub = match subscription::<sensor_ping>("/fmu/out/sensor_ping", 0) {
        Ok(s) => s,
        Err(e) => {
            err!("subscription failed: {:?}", e);
            return;
        }
    };
    info!("nros_listener started");

    loop {
        let msg: SensorPing = sub.recv().await;
        info!(
            "recv: ts={} seq={} value={}",
            msg.timestamp, msg.seq, msg.value as i32
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nros_listener_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    match parse_first_arg(argc, argv) {
        Some(b"start") => match pump::try_spawn() {
            Ok(token) => {
                token.forget();
                info!("started");
                0
            }
            Err(_) => {
                err!("already running");
                1
            }
        },
        Some(b"status") => {
            info!("running");
            0
        }
        Some(b"stop") => {
            info!("stop is not implemented in this example");
            0
        }
        _ => {
            err!("usage: nros_listener {{start|stop|status}}");
            1
        }
    }
}

fn parse_first_arg<'a>(argc: c_int, argv: *mut *mut c_char) -> Option<&'a [u8]> {
    if argc < 2 || argv.is_null() {
        return None;
    }
    // SAFETY: argv[1] is a NUL-terminated C string from PX4's shell.
    unsafe {
        let s = *argv.add(1);
        if s.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *s.add(len) != 0 {
            len += 1;
            if len > 64 {
                return None;
            }
        }
        Some(core::slice::from_raw_parts(s as *const u8, len))
    }
}
