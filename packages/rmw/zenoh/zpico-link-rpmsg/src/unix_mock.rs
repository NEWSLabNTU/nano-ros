//! A board RPMsg binding over Unix datagram sockets, for host tests.
//!
//! [`register`] names one end of a socket pair as an RPMsg endpoint; a
//! locator `rpmsg/<name>` then opens it. Each open endpoint gets a reader
//! thread that hands every datagram to the link the way a board's RPMsg
//! receive callback does, through [`crate::nros_rpmsg_deliver`] from another
//! context than the one draining it. A full ring makes the thread wait and
//! retry, as a board that holds its RPMsg buffer would.

use crate::{Backend, Endpoint};
use core::ffi::{c_char, c_int, c_void};
use std::{
    boxed::Box,
    os::unix::net::UnixDatagram,
    string::String,
    sync::{
        Arc, Condvar, Mutex, Once,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
    vec::Vec,
};

static NAMED: Mutex<Vec<(String, UnixDatagram)>> = Mutex::new(Vec::new());
static INSTALL: Once = Once::new();

static MOCK_BACKEND: Backend = Backend {
    open: Some(mock_open),
    close: Some(mock_close),
    send: Some(mock_send),
    wait: Some(mock_wait),
    ctx: core::ptr::null_mut(),
};

/// Name `sock` as the RPMsg endpoint `name`, and install this binding as the
/// board backend on first use. The socket should already be connected to the
/// other end. Panics if another backend is installed.
pub fn register(name: &str, sock: UnixDatagram) {
    INSTALL.call_once(|| {
        // SAFETY: a static table.
        let rc = unsafe { crate::nros_rpmsg_register_backend(&MOCK_BACKEND) };
        assert_eq!(
            rc, 0,
            "zpico-link-rpmsg unix-mock: another backend is installed"
        );
    });
    NAMED
        .lock()
        .expect("unix-mock registry poisoned")
        .push((String::from(name), sock));
}

struct MockEp {
    sock: UnixDatagram,
    stop: Arc<AtomicBool>,
    /// Deliveries not yet waited for, and the signal the reader thread
    /// gives after each: `wait` takes one, as a board's wait takes the
    /// semaphore its receive callback gives.
    arrived: Arc<(Mutex<u64>, Condvar)>,
}

struct SendPtr(*mut Endpoint);
// SAFETY: the endpoints are statics and `deliver` is the producer side of a
// single-producer ring; this thread is that one producer.
unsafe impl Send for SendPtr {}

unsafe extern "C" fn mock_open(
    _ctx: *mut c_void,
    name: *const c_char,
    name_len: usize,
    link_ep: *mut Endpoint,
) -> *mut c_void {
    // SAFETY: zenoh-pico passes the locator address and its length.
    let name = unsafe { core::slice::from_raw_parts(name.cast::<u8>(), name_len) };
    let named = NAMED.lock().expect("unix-mock registry poisoned");
    let Some((_, sock)) = named.iter().find(|(n, _)| n.as_bytes() == name) else {
        return core::ptr::null_mut();
    };
    let (Ok(tx), Ok(rx)) = (sock.try_clone(), sock.try_clone()) else {
        return core::ptr::null_mut();
    };
    let stop = Arc::new(AtomicBool::new(false));
    let reader_stop = Arc::clone(&stop);
    let arrived = Arc::new((Mutex::new(0u64), Condvar::new()));
    let reader_arrived = Arc::clone(&arrived);
    let link_ep = SendPtr(link_ep);
    rx.set_read_timeout(Some(Duration::from_millis(50))).ok();
    thread::spawn(move || {
        let link_ep = link_ep;
        let mut buf = [0u8; crate::FRAME_SIZE + 1];
        while !reader_stop.load(Ordering::Acquire) {
            let Ok(n) = rx.recv(&mut buf) else {
                continue;
            };
            // SAFETY: `link_ep` is a static endpoint, open until `close`
            // sets `stop`.
            while unsafe { crate::nros_rpmsg_deliver(link_ep.0, buf.as_ptr(), n) } != 0 {
                if n > crate::FRAME_SIZE || reader_stop.load(Ordering::Acquire) {
                    break;
                }
                thread::sleep(Duration::from_micros(200));
            }
            let (count, cv) = &*reader_arrived;
            *count.lock().expect("unix-mock wait poisoned") += 1;
            cv.notify_all();
        }
    });
    Box::into_raw(Box::new(MockEp { sock: tx, stop, arrived })).cast()
}

unsafe extern "C" fn mock_close(_ctx: *mut c_void, ep: *mut c_void) {
    // SAFETY: the box `mock_open` leaked.
    let ep = unsafe { Box::from_raw(ep.cast::<MockEp>()) };
    ep.stop.store(true, Ordering::Release);
}

unsafe extern "C" fn mock_send(
    _ctx: *mut c_void,
    ep: *mut c_void,
    data: *const u8,
    len: usize,
) -> c_int {
    // SAFETY: the box `mock_open` leaked, and `len` bytes the link wrote.
    let (ep, data) = unsafe {
        (
            &*ep.cast::<MockEp>(),
            core::slice::from_raw_parts(data, len),
        )
    };
    match ep.sock.send(data) {
        Ok(n) if n == len => 0,
        _ => -1,
    }
}

unsafe extern "C" fn mock_wait(_ctx: *mut c_void, ep: *mut c_void, timeout_ms: u32) -> c_int {
    // SAFETY: the box `mock_open` leaked.
    let ep = unsafe { &*ep.cast::<MockEp>() };
    // A counting semaphore, like the one a board's receive callback gives: a
    // delivery that lands before this call is still counted, so no wake is
    // lost between the link's empty check and here.
    let (count, cv) = &*ep.arrived;
    let pending = count.lock().expect("unix-mock wait poisoned");
    let (mut pending, _) = cv
        .wait_timeout_while(pending, Duration::from_millis(u64::from(timeout_ms)), |c| *c == 0)
        .expect("unix-mock wait poisoned");
    *pending = pending.saturating_sub(1);
    0
}
