//! `px4-rs-talker` — PX4 nano-ros example. Publishes a synthetic
//! [`SensorPing`] message at the WorkQueue's natural rate via the
//! direct typed `nros::uorb` API (lowest overhead, recommended for
//! high-rate sensor publishers).
//!
//! See [`book/src/getting-started/px4.md`](../../../../../book/src/getting-started/px4.md)
//! for the typeless `Node`-based variant.

#![no_std]
#![feature(type_alias_impl_trait)]

use core::ffi::{c_char, c_int};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use nros_rmw_uorb::publication;
use px4_log::{err, info, module, panic_handler};
use px4_msg_macros::px4_message;
use px4_workqueue::task;

module!("nros_talker");
panic_handler!();

#[px4_message("../SensorPing.msg")]
pub struct SensorPing;

#[task(wq = "lp_default")]
async fn pump() {
    let pub_ = match publication::<sensor_ping>("/fmu/out/sensor_ping", 0) {
        Ok(p) => p,
        Err(e) => {
            err!("publication failed: {:?}", e);
            return;
        }
    };
    info!("nros_talker started");

    let mut counter: u32 = 0;
    loop {
        counter = counter.wrapping_add(1);
        let sample = SensorPing {
            timestamp: counter as u64,
            seq: counter,
            value: counter as f32 * 0.1,
        };
        if pub_.publish(&sample).is_err() {
            err!("publish failed at counter {counter}");
        }
        // Yield so the WorkQueue can run other items.
        // A real talker would await a Timer to throttle.
        YieldOnce::new().await;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nros_talker_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
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
            err!("usage: nros_talker {{start|stop|status}}");
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

struct YieldOnce(bool);
impl YieldOnce {
    fn new() -> Self {
        Self(false)
    }
}
impl Future for YieldOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}
