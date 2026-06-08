//! Phase 115.F — Custom-transport loopback example (talker).
//!
//! Demonstrates a fully-runtime-pluggable transport: the user
//! supplies four C function pointers that bridge raw zenoh wire
//! bytes to the medium of their choice. This example bridges to a
//! real zenohd over TCP, but the same vtable shape covers
//! USB-CDC, BLE GATT, RS-485 with framing, ring-buffer loopback,
//! semihosting bridge, and so on. See
//! `book/src/porting/custom-transport.md` for the full design.
//!
//! # Wire layout for this example
//!
//! ```text
//! talker (this binary)            zenohd (separate process)
//! ──────────────────              ─────────────
//! Publisher<Int32>                tcp/127.0.0.1:N
//!     │
//!     ▼
//! zenoh-pico session
//!     │ wire bytes via custom://
//!     ▼
//! NrosTransportOps callbacks ──tcp──▶ zenohd
//! ```
//!
//! The other end of the loop (a subscriber) runs the same shape
//! through the matching `custom-transport-listener` example.
//!
//! # Usage
//!
//! ```bash
//! # Start zenohd:
//! zenohd --listen tcp/127.0.0.1:7447 --no-multicast-scouting
//!
//! # Run talker (bridges to that zenohd):
//! NROS_CUSTOM_TCP_TARGET=127.0.0.1:7447 cargo run -p native-rs-custom-transport-talker
//! ```

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
static LOGGER: Logger = Logger::new("custom-transport-talker");

extern crate nros_platform_cffi as _;

// ============================================================================
// Custom-transport bridge to TCP
// ============================================================================

/// Per-bridge state passed as `user_data` to every callback.
///
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
    // The TcpStream was already connected at TcpBridge::new; nothing
    // to do here. Return success.
    0
}

unsafe extern "C" fn cb_close(ud: *mut c_void) {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let _ = bridge.stream.shutdown(Shutdown::Both);
}

// zenoh-pico's `Z_LINK_TYPE_CUSTOM` is declared stream-flow, so the
// 2-byte LE length prefix is added by zenoh-pico itself. The bridge
// just shovels raw bytes between the vtable and the TCP socket.
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
        Ok(0) => -1, // peer closed
        Ok(n) => n as i32,
        Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => 0,
        Err(_) => -1,
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    nros_log::register_logger(&LOGGER);
    nros_log::init(nros_log::sinks::default());

    let target = std::env::var("NROS_CUSTOM_TCP_TARGET").unwrap_or_else(|_| {
        nros_info!(
            &LOGGER,
            "NROS_CUSTOM_TCP_TARGET not set; defaulting to 127.0.0.1:7447"
        );
        "127.0.0.1:7447".to_string()
    });

    nros_info!(
        &LOGGER,
        "nros Custom-Transport Talker — bridging to TCP {target}"
    );

    // Connect TcpStream + leak Box so the user_data outlives the
    // session (custom-transport contract).
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

    // SAFETY: bridge is Box::leak'd, lives until process exit.
    unsafe {
        nros_rmw::set_custom_transport(Some(ops)).expect("abi_version v1 ok");
    }
    nros_info!(&LOGGER, "Custom transport vtable registered");

    // Phase 115.L.5-custom-transport — install zenoh-pico C-vtable
    // backend before Executor::open. Order matters: the custom-
    // transport slot set above is drained by zenoh-pico during
    // session open, so the cffi register must happen first so the
    // runtime knows which backend's open() to dispatch to.

    // Open zenoh session via the custom-link locator. Address is
    // opaque to v1; just needs to be non-empty.
    let config = ExecutorConfig::new("custom/loopback").node_name("talker");
    // Phase 227.3 (unified RMW) — no explicit register(). `nros`'s
    // `__FORCE_LINK_ZENOH` keeps the backend's self-register section in the
    // link graph; the cffi walker fires it inside `Executor::open`.
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("talker")
        .expect("Failed to create node");

    let publisher = node
        .create_publisher::<Int32>("/chatter")
        .expect("Failed to create publisher");
    nros_info!(&LOGGER, "Publisher created on /chatter");

    let max_msgs: i32 = std::env::var("NROS_TALKER_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    for i in 0..max_msgs {
        let msg = Int32 { data: i };
        if let Err(e) = publisher.publish(&msg) {
            nros_error!(&LOGGER, "Publish failed: {e:?}");
        } else {
            nros_info!(&LOGGER, "Published: {i}");
        }
        std::thread::sleep(Duration::from_millis(100));
        // Drive session I/O so writes flush.
        let _ = executor.spin_once(Duration::from_millis(10));
    }

    nros_info!(&LOGGER, "Talker done — published {max_msgs} messages");
}
