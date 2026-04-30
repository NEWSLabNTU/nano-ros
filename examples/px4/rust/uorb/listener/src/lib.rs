//! `px4-rs-listener` — PX4 nano-ros example. Subscribes to
//! [`SensorPing`] (published by `px4-rs-talker`) and logs each
//! delivered message via `px4-log`.
//!
//! Uses [`nros_px4::uorb::create_subscription_with_callback`] so the
//! user closure runs with `&SensorPing` directly — no manual byte
//! cast at the call site.
//!
//! Same `#![no_std]` + libc-pthread runtime pattern as the talker —
//! see that crate's docs for why the staticlib avoids `std`.

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int, c_void};
use core::time::Duration;

use nros_node::{Executor, ExecutorConfig};
use nros_px4::uorb;
use px4_log::{err, info, module, panic_handler};
use px4_msg_macros::px4_message;

module!("nros_listener");
panic_handler!();

#[px4_message("../msg/SensorPing.msg")]
pub struct sensor_ping;

#[allow(non_camel_case_types)]
type pthread_t = u64;

unsafe extern "C" {
    fn pthread_create(
        thread: *mut pthread_t,
        attr: *const c_void,
        start_routine: extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
    ) -> c_int;
    fn pthread_detach(thread: pthread_t) -> c_int;
}

extern "C" fn worker_main(_arg: *mut c_void) -> *mut c_void {
    run_executor();
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn nros_listener_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    match parse_first_arg(argc, argv) {
        Some(b"start") => {
            let mut tid: pthread_t = 0;
            let rc = unsafe {
                pthread_create(
                    &mut tid,
                    core::ptr::null(),
                    worker_main,
                    core::ptr::null_mut(),
                )
            };
            if rc != 0 {
                err!("pthread_create failed rc={rc}");
                return 1;
            }
            unsafe {
                pthread_detach(tid);
            }
            info!("started");
            0
        }
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

fn run_executor() {
    let cfg = ExecutorConfig::new("").node_name("listener");
    let mut executor = match Executor::open(&cfg) {
        Ok(e) => e,
        Err(e) => {
            err!("Executor::open failed: {:?}", e);
            return;
        }
    };

    if let Err(e) = uorb::create_subscription_with_callback::<sensor_ping, _>(
        &mut executor,
        "/fmu/out/sensor_ping",
        0,
        |msg: &SensorPing| {
            info!(
                "recv: ts={} seq={} value={}",
                msg.timestamp,
                msg.seq,
                msg.value as i32,
            );
        },
    ) {
        err!("create_subscription_with_callback failed: {:?}", e);
        return;
    }

    info!("nros_listener started (typed callback subscriber)");

    loop {
        let _ = executor.spin_once(Duration::from_millis(50));
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
