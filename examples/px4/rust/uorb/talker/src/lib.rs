//! `px4-rs-talker` — PX4 nano-ros example. Publishes a synthetic
//! [`SensorPing`] at 20 Hz via the **Phase 99 zero-copy typed loan
//! API** (`nros_px4::uorb::Publisher<T>::publish`).
//!
//! ## Layered architecture
//!
//! - User code reaches `T: UorbTopic` only through `nros_px4::uorb`.
//! - The byte-shaped low-level surface (`EmbeddedRawPublisher`,
//!   `try_loan`, `commit`) lives in `nros::*` and is backend-agnostic.
//! - `nros-rmw-uorb` is internal plumbing — never imported by user code.
//!
//! ## Runtime model
//!
//! `#![no_std]` + libc `pthread_create` to spawn the executor on a
//! dedicated worker thread. `nros_talker_main` returns 0 immediately
//! so the PX4 shell stays responsive. The crate carries no
//! `#[global_allocator]` — every allocation hits the Executor's
//! static arena, never the heap. That's what lets two such modules
//! co-exist in the same `px4` binary without symbol collisions.

#![no_std]
#![feature(type_alias_impl_trait)]

use core::{
    ffi::{c_char, c_int, c_void},
    time::Duration,
};

use nros_node::{Executor, ExecutorConfig, TimerDuration};
use nros_px4::uorb;
use px4_log::{err, info, module, panic_handler};
use px4_msg_macros::px4_message;

module!("nros_talker");
panic_handler!();

#[px4_message("../msg/SensorPing.msg")]
pub struct sensor_ping;

// ---------------------------------------------------------------------------
// pthread shim
// ---------------------------------------------------------------------------
//
// Spawn a worker thread the no_std way — call libc directly. PX4 SITL
// links glibc; embedded NuttX provides POSIX threads too. Avoids
// pulling `std::thread::spawn` (which would bring libstd in and
// collide with the listener crate's libstd at link time).

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
pub extern "C" fn nros_talker_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    match parse_first_arg(argc, argv) {
        Some(b"start") => {
            let mut tid: pthread_t = 0;
            // SAFETY: pthread_create stores a thread id; worker_main
            // has the C ABI; we pass null arg.
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
            // SAFETY: detached threads are reaped automatically.
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
            err!("usage: nros_talker {{start|stop|status}}");
            1
        }
    }
}

fn run_executor() {
    let cfg = ExecutorConfig::new("").node_name("talker");
    let mut executor = match Executor::open(&cfg) {
        Ok(e) => e,
        Err(e) => {
            err!("Executor::open failed: {:?}", e);
            return;
        }
    };

    let publisher = {
        let mut node = match executor.create_node("talker") {
            Ok(n) => n,
            Err(e) => {
                err!("create_node failed: {:?}", e);
                return;
            }
        };
        match uorb::create_publisher::<sensor_ping>(&mut node, "/fmu/out/sensor_ping", 0) {
            Ok(p) => p,
            Err(e) => {
                err!("create_publisher failed: {:?}", e);
                return;
            }
        }
    };

    let mut counter: u32 = 0;
    if let Err(e) = executor.add_timer(TimerDuration::from_millis(50), move || {
        counter = counter.wrapping_add(1);
        let sample = SensorPing {
            timestamp: counter as u64,
            seq: counter,
            value: counter as f32 * 0.1,
        };
        if publisher.publish(&sample).is_err() {
            err!("publish failed at counter {counter}");
        }
    }) {
        err!("add_timer failed: {:?}", e);
        return;
    }

    info!("nros_talker started (zero-copy typed publisher)");

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
