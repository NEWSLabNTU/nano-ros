//! POSIX serial (PTY / UART) transport via `termios`.
//!
//! Implements [`PlatformSerial`](nros_platform_api::PlatformSerial) on
//! [`PosixPlatform`]. The handle is a `libc` file
//! descriptor — multiple concurrent devices are supported from a single
//! platform impl (open returns the FD; every other method takes it
//! back). Single-session consumers like `nros-rmw-xrce::platform_serial`
//! stash the FD in a local static between `init_platform_serial_transport`
//! and the transport callbacks.
//!
//! Migrated from `nros-rmw-xrce/src/posix_serial.rs` in Phase 80.14.

use core::ffi::c_int;

use crate::PosixPlatform;

/// Default read timeout if the caller passes `timeout_ms == 0`.
const SERIAL_DEFAULT_TIMEOUT_MS: c_int = 1000;

/// Maximum device-path length read from the user-supplied pointer
/// (including null terminator).
const PATH_BUF_SIZE: usize = 256;

#[inline]
fn speed_for(baudrate: u32) -> Option<libc::speed_t> {
    Some(match baudrate {
        9_600 => libc::B9600,
        19_200 => libc::B19200,
        38_400 => libc::B38400,
        57_600 => libc::B57600,
        115_200 => libc::B115200,
        230_400 => libc::B230400,
        460_800 => libc::B460800,
        921_600 => libc::B921600,
        _ => return None,
    })
}

/// Copy a null-terminated UTF-8 path from `src` into `dst`, writing a
/// terminator at `len` and returning the unfilled slice beyond it.
/// Returns `None` if the source isn't null-terminated within
/// `PATH_BUF_SIZE`.
fn copy_path(src: *const u8, dst: &mut [u8; PATH_BUF_SIZE]) -> Option<()> {
    let mut len = 0;
    unsafe {
        while len < PATH_BUF_SIZE - 1 {
            let b = *src.add(len);
            dst[len] = b;
            if b == 0 {
                return Some(());
            }
            len += 1;
        }
    }
    None
}

fn open_fd(path: *const u8, baudrate: libc::speed_t) -> c_int {
    let mut path_buf = [0u8; PATH_BUF_SIZE];
    if copy_path(path, &mut path_buf).is_none() {
        return -1;
    }

    unsafe {
        let fd = libc::open(
            path_buf.as_ptr() as *const libc::c_char,
            libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK,
        );
        if fd < 0 {
            return -1;
        }

        let mut tty: libc::termios = core::mem::zeroed();
        if libc::tcgetattr(fd, &mut tty) != 0 {
            libc::close(fd);
            return -1;
        }
        libc::cfmakeraw(&mut tty);
        libc::cfsetispeed(&mut tty, baudrate);
        libc::cfsetospeed(&mut tty, baudrate);
        // VMIN=0, VTIME=1 → short kernel-side read wait; the real
        // upper-bound timeout comes from `poll(2)` in `read()`.
        tty.c_cc[libc::VMIN] = 0;
        tty.c_cc[libc::VTIME] = 1;
        if libc::tcsetattr(fd, libc::TCSANOW, &tty) != 0 {
            libc::close(fd);
            return -1;
        }

        // Clear O_NONBLOCK; reads rely on VTIME + poll(2) instead.
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);

        fd
    }
}

impl nros_platform_api::PlatformSerial for PosixPlatform {
    type Handle = c_int;

    const INVALID: c_int = -1;

    fn is_valid(h: c_int) -> bool {
        h >= 0
    }

    fn open(path: *const u8) -> c_int {
        if path.is_null() {
            return -1;
        }
        open_fd(path, libc::B115200)
    }

    fn close(h: c_int) {
        if h >= 0 {
            unsafe {
                libc::close(h);
            }
        }
    }

    fn configure(h: c_int, baudrate: u32) -> i8 {
        if h < 0 {
            return -1;
        }
        let speed = match speed_for(baudrate) {
            Some(s) => s,
            None => return -1,
        };
        unsafe {
            let mut tty: libc::termios = core::mem::zeroed();
            if libc::tcgetattr(h, &mut tty) != 0 {
                return -1;
            }
            libc::cfsetispeed(&mut tty, speed);
            libc::cfsetospeed(&mut tty, speed);
            if libc::tcsetattr(h, libc::TCSANOW, &tty) != 0 {
                return -1;
            }
        }
        0
    }

    fn read(h: c_int, buf: *mut u8, len: usize, timeout_ms: u32) -> usize {
        if h < 0 {
            return usize::MAX;
        }

        let mut pfd = libc::pollfd {
            fd: h,
            events: libc::POLLIN,
            revents: 0,
        };
        let timeout_c = if timeout_ms == 0 {
            SERIAL_DEFAULT_TIMEOUT_MS
        } else {
            timeout_ms.min(i32::MAX as u32) as c_int
        };
        let poll_ret = unsafe { libc::poll(&mut pfd, 1, timeout_c) };

        if poll_ret <= 0 {
            // Timeout or signal — 0 bytes, not an error.
            return 0;
        }

        let ret = unsafe { libc::read(h, buf as *mut libc::c_void, len) };
        if ret < 0 {
            let errno = unsafe { *libc::__errno_location() };
            if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                0
            } else {
                usize::MAX
            }
        } else {
            ret as usize
        }
    }

    fn write(h: c_int, buf: *const u8, len: usize) -> usize {
        if h < 0 {
            return usize::MAX;
        }
        let ret = unsafe { libc::write(h, buf as *const libc::c_void, len) };
        if ret < 0 {
            usize::MAX
        } else {
            ret as usize
        }
    }
}
