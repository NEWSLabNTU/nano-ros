//! Shared parsing utilities for TCP and UDP operations.

/// Parse a C string as an IPv4 dotted-decimal address into 4 bytes.
///
/// Returns `Some([a, b, c, d])` on success, `None` on parse error.
///
/// # Safety
///
/// `s` must be a valid null-terminated C string.
pub unsafe fn parse_ip_address(s: *const u8) -> Option<[u8; 4]> {
    if s.is_null() {
        return None;
    }

    let mut octets = [0u8; 4];
    let mut octet_idx = 0usize;
    let mut value: u32 = 0;
    let mut has_digit = false;
    let mut p = s;

    loop {
        let ch = unsafe { *p };
        if ch == 0 {
            break;
        }

        if ch >= b'0' && ch <= b'9' {
            value = value * 10 + (ch - b'0') as u32;
            has_digit = true;
            if value > 255 {
                return None;
            }
        } else if ch == b'.' {
            if !has_digit || octet_idx >= 3 {
                return None;
            }
            octets[octet_idx] = value as u8;
            octet_idx += 1;
            value = 0;
            has_digit = false;
        } else {
            return None;
        }

        p = unsafe { p.add(1) };
    }

    if !has_digit || octet_idx != 3 {
        return None;
    }
    octets[3] = value as u8;

    Some(octets)
}

/// Parse a C string as a port number (0–65535).
///
/// # Safety
///
/// `s` must be a valid null-terminated C string.
pub unsafe fn parse_port(s: *const u8) -> Option<u16> {
    if s.is_null() {
        return None;
    }

    let mut value: u32 = 0;
    let mut has_digit = false;
    let mut p = s;

    loop {
        let ch = unsafe { *p };
        if ch == 0 {
            break;
        }

        if ch >= b'0' && ch <= b'9' {
            value = value * 10 + (ch - b'0') as u32;
            has_digit = true;
            if value > 65535 {
                return None;
            }
        } else {
            return None;
        }

        p = unsafe { p.add(1) };
    }

    if !has_digit {
        return None;
    }

    Some(value as u16)
}
