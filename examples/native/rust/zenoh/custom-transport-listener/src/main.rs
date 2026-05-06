//! Phase 115.F — Custom-transport loopback example (listener).
//!
//! Mirror of `custom-transport-talker`. Subscribes to `/chatter`
//! over a custom-transport-bridged TCP connection to the same
//! zenohd. See the talker for the design walkthrough.

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
    0
}

unsafe extern "C" fn cb_close(ud: *mut c_void) {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    if let Ok(mut g) = bridge.stream.lock() {
        if let Some(s) = g.take() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }
}

// Stream-flow custom-link — zenoh-pico adds 2-byte LE length prefix.
unsafe extern "C" fn cb_write(ud: *mut c_void, buf: *const u8, len: usize) -> i32 {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let slice = unsafe { std::slice::from_raw_parts(buf, len) };
    let mut g = match bridge.stream.lock() {
        Ok(g) => g,
        Err(_) => return -1,
    };
    let Some(s) = g.as_mut() else { return -1 };
    if s.write_all(slice).is_err() {
        return -1;
    }
    let _ = s.flush();
    0
}

unsafe extern "C" fn cb_read(ud: *mut c_void, buf: *mut u8, len: usize, timeout_ms: u32) -> i32 {
    let bridge = unsafe { &*(ud as *const TcpBridge) };
    let slice = unsafe { std::slice::from_raw_parts_mut(buf, len) };
    let mut g = match bridge.stream.lock() {
        Ok(g) => g,
        Err(_) => return -1,
    };
    let Some(s) = g.as_mut() else { return -1 };
    let to = Duration::from_millis(timeout_ms.max(1) as u64);
    let _ = s.set_read_timeout(Some(to));
    match s.read(slice) {
        Ok(0) => -1,
        Ok(n) => n as i32,
        Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => 0,
        Err(_) => -1,
    }
}

fn main() {
    env_logger::init();

    let target = std::env::var("NROS_CUSTOM_TCP_TARGET")
        .unwrap_or_else(|_| "127.0.0.1:7447".to_string());

    info!("nros Custom-Transport Listener — bridging to TCP {target}");

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
    unsafe { nros_rmw::set_custom_transport(Some(ops)).expect("abi v1 ok") };
    info!("Custom transport vtable registered");

    let config = ExecutorConfig::new("custom/loopback").node_name("listener");
    let mut executor: Executor = Executor::open(&config).expect("Failed to open session");

    executor
        .add_subscription::<Int32, _>("/chatter", |msg: &Int32| {
            info!("Received: {}", msg.data);
        })
        .expect("Failed to add subscription");
    info!("Subscriber created on /chatter");

    let max_secs: u64 = std::env::var("NROS_LISTENER_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let deadline = std::time::Instant::now() + Duration::from_secs(max_secs);
    while std::time::Instant::now() < deadline {
        let _ = executor.spin_once(Duration::from_millis(50));
    }

    info!("Listener done");
}
