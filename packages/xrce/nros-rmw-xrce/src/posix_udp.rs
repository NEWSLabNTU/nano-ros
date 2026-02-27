//! POSIX UDP transport for XRCE-DDS native integration tests.
//!
//! Provides UDP custom transport callbacks using POSIX socket syscalls via `libc`.

#![allow(static_mut_refs)]

use core::ffi::c_int;

// ============================================================================
// Global Transport State
// ============================================================================

/// Stack buffer size for agent address string (including null terminator).
const AGENT_ADDR_BUF_SIZE: usize = 64;

static mut AGENT_ADDR: [u8; AGENT_ADDR_BUF_SIZE] = [0u8; AGENT_ADDR_BUF_SIZE];
static mut AGENT_ADDR_LEN: usize = 0;
static mut UDP_FD: c_int = -1;

/// Store the XRCE Agent address and initialize the transport callbacks.
///
/// Must be called before [`crate::XrceRmw::open()`].
///
/// # Safety
///
/// Must not be called concurrently. Only one transport may be active.
pub unsafe fn init_posix_udp_transport(agent_addr: &str) {
    unsafe {
        let len = agent_addr.len().min(AGENT_ADDR_BUF_SIZE - 1);
        AGENT_ADDR[..len].copy_from_slice(&agent_addr.as_bytes()[..len]);
        AGENT_ADDR[len] = 0;
        AGENT_ADDR_LEN = len;

        crate::init_transport(
            Some(transport_open),
            Some(transport_close),
            Some(transport_write),
            Some(transport_read),
            false, // UDP is packet-oriented, no framing needed
        );
    }
}

/// Parse "ip:port" into a `sockaddr_in`. Returns `None` on invalid input.
fn parse_addr(addr: &[u8]) -> Option<libc::sockaddr_in> {
    // Find the last ':' separator
    let colon_pos = addr.iter().rposition(|&b| b == b':')?;
    let ip_part = &addr[..colon_pos];
    let port_part = &addr[colon_pos + 1..];

    // Parse port
    let mut port: u16 = 0;
    for &b in port_part {
        if b < b'0' || b > b'9' {
            return None;
        }
        port = port.checked_mul(10)?.checked_add((b - b'0') as u16)?;
    }

    // Parse IPv4 octets (a.b.c.d)
    let mut octets = [0u8; 4];
    let mut octet_idx = 0;
    let mut current: u16 = 0;
    let mut digit_count = 0;

    for &b in ip_part {
        if b == b'.' {
            if digit_count == 0 || octet_idx >= 3 || current > 255 {
                return None;
            }
            octets[octet_idx] = current as u8;
            octet_idx += 1;
            current = 0;
            digit_count = 0;
        } else if b >= b'0' && b <= b'9' {
            current = current * 10 + (b - b'0') as u16;
            digit_count += 1;
        } else {
            return None;
        }
    }
    // Last octet
    if digit_count == 0 || octet_idx != 3 || current > 255 {
        return None;
    }
    octets[octet_idx] = current as u8;

    Some(libc::sockaddr_in {
        sin_family: libc::AF_INET as libc::sa_family_t,
        sin_port: port.to_be(),
        sin_addr: libc::in_addr {
            s_addr: u32::from_ne_bytes(octets),
        },
        sin_zero: [0; 8],
    })
}

unsafe extern "C" fn transport_open(_transport: *mut xrce_sys::uxrCustomTransport) -> bool {
    unsafe {
        let addr_bytes = &AGENT_ADDR[..AGENT_ADDR_LEN];
        let sockaddr = match parse_addr(addr_bytes) {
            Some(sa) => sa,
            None => return false,
        };

        let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
        if fd < 0 {
            return false;
        }

        // Bind to any local address
        let bind_addr = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: 0,
            sin_addr: libc::in_addr { s_addr: 0 }, // INADDR_ANY
            sin_zero: [0; 8],
        };
        if libc::bind(
            fd,
            &bind_addr as *const libc::sockaddr_in as *const libc::sockaddr,
            core::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        ) < 0
        {
            libc::close(fd);
            return false;
        }

        // Connect to agent (enables send/recv instead of sendto/recvfrom)
        if libc::connect(
            fd,
            &sockaddr as *const libc::sockaddr_in as *const libc::sockaddr,
            core::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        ) < 0
        {
            libc::close(fd);
            return false;
        }

        UDP_FD = fd;
        true
    }
}

unsafe extern "C" fn transport_close(_transport: *mut xrce_sys::uxrCustomTransport) -> bool {
    unsafe {
        if UDP_FD >= 0 {
            libc::close(UDP_FD);
            UDP_FD = -1;
        }
        true
    }
}

unsafe extern "C" fn transport_write(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *const u8,
    length: usize,
    error_code: *mut u8,
) -> usize {
    unsafe {
        if UDP_FD < 0 {
            *error_code = 1;
            return 0;
        }
        let ret = libc::send(UDP_FD, buffer as *const libc::c_void, length, 0);
        if ret < 0 {
            *error_code = 1;
            0
        } else {
            ret as usize
        }
    }
}

unsafe extern "C" fn transport_read(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *mut u8,
    length: usize,
    timeout: c_int,
    error_code: *mut u8,
) -> usize {
    unsafe {
        if UDP_FD < 0 {
            *error_code = 1;
            return 0;
        }

        // Set receive timeout via SO_RCVTIMEO.
        // timeout > 0: use as milliseconds; timeout == 0: 1ms minimum; timeout < 0: block forever.
        // timeval {0, 0} means "no timeout" (block forever) per POSIX.
        let timeout_ms = if timeout > 0 {
            timeout
        } else if timeout == 0 {
            1
        } else {
            0
        };
        let tv = libc::timeval {
            tv_sec: (timeout_ms / 1000) as libc::time_t,
            tv_usec: ((timeout_ms % 1000) * 1000) as libc::suseconds_t,
        };
        libc::setsockopt(
            UDP_FD,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const libc::timeval as *const libc::c_void,
            core::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );

        let ret = libc::recv(UDP_FD, buffer as *mut libc::c_void, length, 0);
        if ret < 0 {
            let errno = *libc::__errno_location();
            if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK || errno == libc::ETIMEDOUT {
                0 // timeout — return 0 bytes, not an error
            } else {
                *error_code = 1;
                0
            }
        } else {
            ret as usize
        }
    }
}
