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
    sync::Mutex,
    time::Duration,
};

use log::{error, info};
use nros::prelude::*;
use std_msgs::msg::Int32;

// ============================================================================
// Custom-transport bridge to TCP
// ============================================================================

/// Per-bridge state passed as `user_data` to every callback.
struct TcpBridge {
    stream: Mutex<Option<TcpStream>>,
}

impl TcpBridge {
    fn new(target: &str) -> std::io::Result<Self> {
        let stream = TcpStream::connect(target)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_millis(50)))?;
        stream.set_write_timeout(Some(Duration::from_millis(1000)))?;
        Ok(Self {
            stream: Mutex::new(Some(stream)),
        })
    }
}

unsafe extern "C" fn cb_open(_ud: *mut c_void, _params: *const c_void) -> i32 {
    // The TcpStream was already connected at TcpBridge::new; nothing
    // to do here. Return success.
    0
}

unsafe extern "C" fn cb_close(ud: *mut c_void) {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    if let Ok(mut guard) = bridge.stream.lock() {
        if let Some(s) = guard.take() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }
}

// zenoh-pico's `Z_LINK_TYPE_CUSTOM` is declared stream-flow, so the
// 2-byte LE length prefix is added by zenoh-pico itself. The bridge
// just shovels raw bytes between the vtable and the TCP socket.
unsafe extern "C" fn cb_write(ud: *mut c_void, buf: *const u8, len: usize) -> i32 {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let slice = unsafe { std::slice::from_raw_parts(buf, len) };
    let mut guard = match bridge.stream.lock() {
        Ok(g) => g,
        Err(_) => return -1,
    };
    let Some(stream) = guard.as_mut() else {
        return -1;
    };
    if stream.write_all(slice).is_err() {
        return -1;
    }
    let _ = stream.flush();
    0
}

unsafe extern "C" fn cb_read(ud: *mut c_void, buf: *mut u8, len: usize, timeout_ms: u32) -> i32 {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let slice = unsafe { std::slice::from_raw_parts_mut(buf, len) };
    let mut guard = match bridge.stream.lock() {
        Ok(g) => g,
        Err(_) => return -1,
    };
    let Some(stream) = guard.as_mut() else {
        return -1;
    };
    let to = Duration::from_millis(timeout_ms.max(1) as u64);
    let _ = stream.set_read_timeout(Some(to));
    match stream.read(slice) {
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
    env_logger::init();

    let target = std::env::var("NROS_CUSTOM_TCP_TARGET").unwrap_or_else(|_| {
        info!("NROS_CUSTOM_TCP_TARGET not set; defaulting to 127.0.0.1:7447");
        "127.0.0.1:7447".to_string()
    });

    info!("nros Custom-Transport Talker — bridging to TCP {target}");

    // Connect TcpStream + leak Box so the user_data outlives the
    // session (custom-transport contract).
    let bridge = match TcpBridge::new(&target) {
        Ok(b) => Box::leak(Box::new(b)),
        Err(e) => {
            error!("TCP connect to {target} failed: {e}");
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
    info!("Custom transport vtable registered");

    // Open zenoh session via the custom-link locator. Address is
    // opaque to v1; just needs to be non-empty.
    let config = ExecutorConfig::new("custom/loopback").node_name("talker");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    let mut node = executor
        .create_node("talker")
        .expect("Failed to create node");

    let publisher = node
        .create_publisher::<Int32>("/chatter")
        .expect("Failed to create publisher");
    info!("Publisher created on /chatter");

    let max_msgs: i32 = std::env::var("NROS_TALKER_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    for i in 0..max_msgs {
        let msg = Int32 { data: i };
        if let Err(e) = publisher.publish(&msg) {
            error!("Publish failed: {e:?}");
        } else {
            info!("Published: {i}");
        }
        std::thread::sleep(Duration::from_millis(100));
        // Drive session I/O so writes flush.
        let _ = executor.spin_once(Duration::from_millis(10));
    }

    info!("Talker done — published {max_msgs} messages");
}
