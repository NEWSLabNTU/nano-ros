//! POSIX serial (PTY) transport for XRCE-DDS.
//!
//! Provides serial custom transport callbacks using POSIX `termios` and PTY
//! devices. The XRCE-DDS C library handles HDLC framing automatically.

#![allow(static_mut_refs)]

use std::ffi::c_int;

// ============================================================================
// Global Transport State
// ============================================================================

/// Stack buffer size for PTY path string (including null terminator).
const PTY_PATH_BUF_SIZE: usize = 256;

/// Default timeout for serial transport reads (milliseconds).
const SERIAL_DEFAULT_TIMEOUT_MS: c_int = 1000;

static mut PTY_PATH: [u8; PTY_PATH_BUF_SIZE] = [0u8; PTY_PATH_BUF_SIZE];
static mut PTY_PATH_LEN: usize = 0;
static mut PTY_FD: c_int = -1;

/// Initialize a POSIX serial transport over a PTY device.
///
/// The PTY path should point to a pseudo-terminal device (e.g., created by
/// `socat`). The XRCE-DDS C library handles HDLC framing automatically
/// when `framing=true`.
///
/// Must be called before [`crate::XrceRmw::open()`].
///
/// # Safety
///
/// Must not be called concurrently. Only one transport may be active.
pub unsafe fn init_posix_serial_transport(pty_path: &str) {
    unsafe {
        let len = pty_path.len().min(PTY_PATH_BUF_SIZE - 1);
        PTY_PATH[..len].copy_from_slice(&pty_path.as_bytes()[..len]);
        PTY_PATH[len] = 0;
        PTY_PATH_LEN = len;

        crate::init_transport(
            Some(serial_transport_open),
            Some(serial_transport_close),
            Some(serial_transport_write),
            Some(serial_transport_read),
            true, // serial is byte-stream, needs HDLC framing
        );
    }
}

unsafe extern "C" fn serial_transport_open(_transport: *mut xrce_sys::uxrCustomTransport) -> bool {
    unsafe {
        let path_ptr = PTY_PATH.as_ptr() as *const libc::c_char;
        let fd = libc::open(path_ptr, libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK);
        if fd < 0 {
            let path_str = core::str::from_utf8(&PTY_PATH[..PTY_PATH_LEN]).unwrap_or("<invalid>");
            eprintln!("Failed to open PTY: {}", path_str);
            return false;
        }

        // Configure raw mode (no echo, no canonical processing)
        let mut tty: libc::termios = core::mem::zeroed();
        if libc::tcgetattr(fd, &mut tty) != 0 {
            eprintln!("tcgetattr failed");
            libc::close(fd);
            return false;
        }
        libc::cfmakeraw(&mut tty);
        // Set baud rate to 115200 (matches Agent default)
        libc::cfsetispeed(&mut tty, libc::B115200);
        libc::cfsetospeed(&mut tty, libc::B115200);
        // VMIN=0, VTIME=1 → non-blocking reads with 100ms timeout
        tty.c_cc[libc::VMIN] = 0;
        tty.c_cc[libc::VTIME] = 1;
        if libc::tcsetattr(fd, libc::TCSANOW, &tty) != 0 {
            eprintln!("tcsetattr failed");
            libc::close(fd);
            return false;
        }

        // Clear O_NONBLOCK after setup (reads will use VTIME timeout)
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);

        PTY_FD = fd;
        true
    }
}

unsafe extern "C" fn serial_transport_close(_transport: *mut xrce_sys::uxrCustomTransport) -> bool {
    unsafe {
        if PTY_FD >= 0 {
            libc::close(PTY_FD);
            PTY_FD = -1;
        }
        true
    }
}

unsafe extern "C" fn serial_transport_write(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *const u8,
    length: usize,
    error_code: *mut u8,
) -> usize {
    unsafe {
        if PTY_FD < 0 {
            *error_code = 1;
            return 0;
        }
        let ret = libc::write(PTY_FD, buffer as *const libc::c_void, length);
        if ret < 0 {
            *error_code = 1;
            0
        } else {
            ret as usize
        }
    }
}

unsafe extern "C" fn serial_transport_read(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *mut u8,
    length: usize,
    timeout: c_int,
    error_code: *mut u8,
) -> usize {
    unsafe {
        if PTY_FD < 0 {
            *error_code = 1;
            return 0;
        }

        // Use poll(2) for timeout-based reading
        let mut pfd = libc::pollfd {
            fd: PTY_FD,
            events: libc::POLLIN,
            revents: 0,
        };
        let timeout_ms = if timeout <= 0 { SERIAL_DEFAULT_TIMEOUT_MS } else { timeout };
        let poll_ret = libc::poll(&mut pfd, 1, timeout_ms);

        if poll_ret <= 0 {
            // Timeout or error — return 0 bytes (not an error for XRCE)
            return 0;
        }

        let ret = libc::read(PTY_FD, buffer as *mut libc::c_void, length);
        if ret < 0 {
            let errno = *libc::__errno_location();
            if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                0
            } else {
                *error_code = 1;
                0
            }
        } else {
            ret as usize
        }
    }
}
