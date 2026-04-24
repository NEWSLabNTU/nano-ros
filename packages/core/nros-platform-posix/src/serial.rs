//! POSIX serial (PTY / UART) transport via `termios`.
//!
//! Implements [`PlatformSerial`](nros_platform_api::PlatformSerial) on
//! [`PosixPlatform`](crate::PosixPlatform). One active device per
//! process — matches the single-session shape both XRCE and zenoh-pico
//! assume for serial transports.
//!
//! Migrated from `nros-rmw-xrce/src/posix_serial.rs` in Phase 80.14.
//! Users of XRCE-over-serial on POSIX now wire their transport through
//! `nros-rmw-xrce::platform_serial::init_platform_serial_transport(path)`
//! instead of the old `init_posix_serial_transport(path)` — both paths
//! ultimately land here.

use core::ffi::c_int;

use crate::PosixPlatform;

/// Stack buffer size for PTY path (including null terminator).
const PTY_PATH_BUF_SIZE: usize = 256;

/// Default read timeout if the caller passes `timeout_ms == 0`.
const SERIAL_DEFAULT_TIMEOUT_MS: c_int = 1000;

/// Default baud rate if `configure()` isn't called.
const DEFAULT_BAUDRATE: libc::speed_t = libc::B115200;

// Single-instance state. Access is disciplined by XRCE's `ffi_guard` /
// zenoh-pico's single-threaded link driver — matches the discipline the
// legacy `nros-rmw-xrce::posix_serial` globals relied on.
static mut PTY_PATH: [u8; PTY_PATH_BUF_SIZE] = [0u8; PTY_PATH_BUF_SIZE];
static mut PTY_PATH_LEN: usize = 0;
static mut PTY_FD: c_int = -1;

#[allow(static_mut_refs)]
fn store_path(path_ptr: *const u8) -> usize {
    // Caller passes a null-terminated byte string. Copy into the PTY_PATH
    // buffer so `open_fd()` can later retrieve it.
    let mut len = 0;
    unsafe {
        while len < PTY_PATH_BUF_SIZE - 1 {
            let b = *path_ptr.add(len);
            PTY_PATH[len] = b;
            if b == 0 {
                PTY_PATH_LEN = len;
                return len;
            }
            len += 1;
        }
        // Path too long — null-terminate at the buffer limit.
        PTY_PATH[PTY_PATH_BUF_SIZE - 1] = 0;
        PTY_PATH_LEN = PTY_PATH_BUF_SIZE - 1;
    }
    len
}

#[allow(static_mut_refs)]
fn open_fd(baudrate: libc::speed_t) -> c_int {
    unsafe {
        let path_ptr = PTY_PATH.as_ptr() as *const libc::c_char;
        let fd = libc::open(path_ptr, libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK);
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
        // VMIN=0, VTIME=1 → non-blocking reads with 100ms timeout. The
        // actual upper-bound wait comes from poll(2) in `PlatformSerial::read`.
        tty.c_cc[libc::VMIN] = 0;
        tty.c_cc[libc::VTIME] = 1;
        if libc::tcsetattr(fd, libc::TCSANOW, &tty) != 0 {
            libc::close(fd);
            return -1;
        }

        // Clear O_NONBLOCK; reads rely on VTIME + poll(2) timeout instead.
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);

        fd
    }
}

impl nros_platform_api::PlatformSerial for PosixPlatform {
    fn open(path: *const u8) -> i8 {
        if path.is_null() {
            return -1;
        }
        store_path(path);
        let fd = open_fd(DEFAULT_BAUDRATE);
        if fd < 0 {
            return -1;
        }
        unsafe {
            PTY_FD = fd;
        }
        0
    }

    fn close() {
        unsafe {
            if PTY_FD >= 0 {
                libc::close(PTY_FD);
                PTY_FD = -1;
            }
        }
    }

    fn configure(baudrate: u32) -> i8 {
        let fd = unsafe { PTY_FD };
        if fd < 0 {
            return -1;
        }
        // Look up the nearest termios constant; extendable as needed.
        let speed: libc::speed_t = match baudrate {
            9_600 => libc::B9600,
            19_200 => libc::B19200,
            38_400 => libc::B38400,
            57_600 => libc::B57600,
            115_200 => libc::B115200,
            230_400 => libc::B230400,
            460_800 => libc::B460800,
            921_600 => libc::B921600,
            _ => return -1,
        };
        unsafe {
            let mut tty: libc::termios = core::mem::zeroed();
            if libc::tcgetattr(fd, &mut tty) != 0 {
                return -1;
            }
            libc::cfsetispeed(&mut tty, speed);
            libc::cfsetospeed(&mut tty, speed);
            if libc::tcsetattr(fd, libc::TCSANOW, &tty) != 0 {
                return -1;
            }
        }
        0
    }

    fn read(buf: *mut u8, len: usize, timeout_ms: u32) -> usize {
        let fd = unsafe { PTY_FD };
        if fd < 0 {
            return usize::MAX;
        }

        // poll(2) for timeout-based reading.
        let mut pfd = libc::pollfd {
            fd,
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

        let ret = unsafe { libc::read(fd, buf as *mut libc::c_void, len) };
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

    fn write(buf: *const u8, len: usize) -> usize {
        let fd = unsafe { PTY_FD };
        if fd < 0 {
            return usize::MAX;
        }
        let ret = unsafe { libc::write(fd, buf as *const libc::c_void, len) };
        if ret < 0 {
            usize::MAX
        } else {
            ret as usize
        }
    }
}
