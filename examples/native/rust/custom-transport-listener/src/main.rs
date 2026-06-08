//! Phase 115.F — Custom-transport loopback example (listener).
//!
//! Mirror of `custom-transport-talker`. Subscribes to `/chatter`
//! over a custom-transport-bridged TCP connection to the same
//! zenohd. See the talker for the design walkthrough.

use core::ffi::c_void;
use std::{
    io::{ErrorKind, Read, Write},
    net::{Shutdown, TcpStream},
    time::Duration,
};

use nros::prelude::*;
use nros_log::{Logger, nros_error, nros_info};
use std_msgs::msg::Int32;

// Phase 88.16.B — diagnostics route through `nros-log`.
static LOGGER: Logger = Logger::new("custom-transport-listener");

extern crate nros_platform_cffi as _;

/// The stream is stored directly (no `Mutex`): zenoh-pico drives the
/// `read` callback on a dedicated read-task thread while the main/tx
/// thread drives `write` concurrently. TCP is full-duplex, and
/// `std::net::TcpStream` implements `Read`/`Write` for `&TcpStream`, so
/// both directions operate on a shared `&self` without serializing.
/// A shared mutex held across the blocking `recv` would starve the tx
/// thread and deadlock session declaration (Phase 179.G).
struct TcpBridge {
    stream: TcpStream,
}

impl TcpBridge {
    fn new(target: &str) -> std::io::Result<Self> {
        let stream = TcpStream::connect(target)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_millis(50)))?;
        stream.set_write_timeout(Some(Duration::from_millis(1000)))?;
        Ok(Self { stream })
    }
}

unsafe extern "C" fn cb_open(_ud: *mut c_void, _params: *const c_void) -> i32 {
    0
}

unsafe extern "C" fn cb_close(ud: *mut c_void) {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let _ = bridge.stream.shutdown(Shutdown::Both);
}

// Stream-flow custom-link — zenoh-pico adds 2-byte LE length prefix.
unsafe extern "C" fn cb_write(ud: *mut c_void, buf: *const u8, len: usize) -> i32 {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let slice = unsafe { std::slice::from_raw_parts(buf, len) };
    // `Write` is implemented for `&TcpStream`, so this runs concurrently
    // with `cb_read` on the read-task thread (full-duplex, no lock).
    if (&bridge.stream).write_all(slice).is_err() {
        return -1;
    }
    let _ = (&bridge.stream).flush();
    0
}

unsafe extern "C" fn cb_read(ud: *mut c_void, buf: *mut u8, len: usize, timeout_ms: u32) -> i32 {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let slice = unsafe { std::slice::from_raw_parts_mut(buf, len) };
    let to = Duration::from_millis(timeout_ms.max(1) as u64);
    let _ = bridge.stream.set_read_timeout(Some(to));
    match (&bridge.stream).read(slice) {
        Ok(0) => -1,
        Ok(n) => n as i32,
        Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => 0,
        Err(_) => -1,
    }
}

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    let target =
        std::env::var("NROS_CUSTOM_TCP_TARGET").unwrap_or_else(|_| "127.0.0.1:7447".to_string());

    nros_info!(
        &LOGGER,
        "nros Custom-Transport Listener — bridging to TCP {target}"
    );

    let bridge = match TcpBridge::new(&target) {
        Ok(b) => Box::leak(Box::new(b)),
        Err(e) => {
            nros_error!(&LOGGER, "TCP connect to {target} failed: {e}");
            std::process::exit(1);
        }
    };

    let ops = nros_rmw::NrosTransportOps {
        abi_version: nros_rmw::NROS_TRANSPORT_OPS_ABI_VERSION_V1,
        _reserved: 0,
        user_data: bridge as *mut TcpBridge as *mut c_void,
        open: cb_open,
        close: cb_close,
        write: cb_write,
        read: cb_read,
    };
    unsafe { nros_rmw::set_custom_transport(Some(ops)).expect("abi v1 ok") };
    nros_info!(&LOGGER, "Custom transport vtable registered");

    // Phase 115.L.5-custom-transport — install zenoh-pico C-vtable
    // backend after staging the custom-transport slot (zenoh-pico
    // drains the slot during session_open).

    let config = ExecutorConfig::new("custom/loopback").node_name("listener");
    // Phase 227.3 (unified RMW) — no explicit register(); `nros`'s
    // `__FORCE_LINK_ZENOH` + the cffi walker self-register the backend.
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    let nid = executor
        .node_builder("listener")
        .build()
        .expect("Failed to build node");
    executor
        .node_mut(nid)
        .create_subscription::<Int32, _>("/chatter", |msg: &Int32| {
            nros_info!(&LOGGER, "Received: {}", msg.data);
        })
        .expect("Failed to add subscription");
    nros_info!(&LOGGER, "Subscriber created on /chatter");

    let max_secs: u64 = std::env::var("NROS_LISTENER_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let deadline = std::time::Instant::now() + Duration::from_secs(max_secs);
    while std::time::Instant::now() < deadline {
        let _ = executor.spin_once(Duration::from_millis(50));
    }

    nros_info!(&LOGGER, "Listener done");
}
