//! Zenoh-pico serial platform symbols implemented in Rust
//!
//! These FFI functions are called by zenoh-pico's serial link layer when
//! `Z_FEATURE_LINK_SERIAL=1`. They delegate to the registered [`SerialPort`]
//! via the static port table.
//!
//! The COBS framing and CRC32 are handled by zenoh-pico's
//! `_z_serial_msg_serialize` / `_z_serial_msg_deserialize` (in
//! `protocol/codec/serial.c`). We only provide raw byte I/O and
//! port management.

use crate::port::get_port;

// ============================================================================
// C types matching bare-metal/platform.h
// ============================================================================

/// Socket handle passed between zenoh-pico and platform layer.
///
/// Must match the layout of `_z_sys_net_socket_t` in
/// `c/platform/bare-metal/platform.h`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct ZSysNetSocket {
    _handle: i8,
    _connected: bool,
    // Note: no _tls_sock field — serial doesn't use TLS.
    // The link-tls feature in zpico-sys adds this field, but serial FFI
    // never touches it. The C struct size is consistent because serial.c
    // is compiled separately from tls.c and only accesses _handle/_connected.
}

/// zenoh-pico result type
type ZResult = i8;
const Z_RES_OK: ZResult = 0;
const Z_ERR_GENERIC: ZResult = -1;

// ============================================================================
// Serial protocol constants (from zenoh-pico headers)
// ============================================================================

/// Maximum COBS-encoded wire frame size.
const Z_SERIAL_MAX_COBS_BUF_SIZE: usize = 1516;

/// Maximum frame size (MTU + header + length + CRC32).
const Z_SERIAL_MFS_SIZE: usize = 1510;

// Imported from zenoh-pico's protocol/codec/serial.h via zpico-sys linkage
unsafe extern "C" {
    fn _z_serial_msg_serialize(
        dest: *mut u8,
        dest_len: usize,
        src: *const u8,
        src_len: usize,
        header: u8,
        tmp_buf: *mut u8,
        tmp_buf_len: usize,
    ) -> usize;

    fn _z_serial_msg_deserialize(
        src: *const u8,
        src_len: usize,
        dst: *mut u8,
        dst_len: usize,
        header: *mut u8,
        tmp_buf: *mut u8,
        tmp_buf_len: usize,
    ) -> usize;

    fn z_malloc(size: usize) -> *mut u8;
    fn z_free(ptr: *mut u8);
}

// ============================================================================
// Port open/close
// ============================================================================

/// Open a serial port by device name (e.g., "UART_0").
///
/// Parses the trailing digit as a port index. The board crate must have
/// already registered a [`SerialPort`] at that index via [`register_port`].
#[unsafe(no_mangle)]
pub extern "C" fn _z_open_serial_from_dev(
    sock: *mut ZSysNetSocket,
    dev: *const u8,
    _baudrate: u32,
) -> ZResult {
    if sock.is_null() || dev.is_null() {
        return Z_ERR_GENERIC;
    }

    let index = match parse_port_index(dev) {
        Some(i) => i,
        None => return Z_ERR_GENERIC,
    };

    let port = unsafe { get_port(index) };
    if port.is_none() {
        return Z_ERR_GENERIC;
    }

    unsafe {
        (*sock)._handle = index as i8;
        (*sock)._connected = true;
    }

    // zenoh-pico calls _z_connect_serial() after this to do Init/Ack handshake
    Z_RES_OK
}

/// Open a serial port by TX/RX pin numbers.
///
/// For bare-metal, pins are not directly meaningful — we use a simple
/// mapping: port index = txpin (ignoring rxpin). Board crates should
/// register ports at index 0 and 1.
#[unsafe(no_mangle)]
pub extern "C" fn _z_open_serial_from_pins(
    sock: *mut ZSysNetSocket,
    txpin: u32,
    _rxpin: u32,
    _baudrate: u32,
) -> ZResult {
    if sock.is_null() {
        return Z_ERR_GENERIC;
    }

    let index = txpin as usize;
    let port = unsafe { get_port(index) };
    if port.is_none() {
        return Z_ERR_GENERIC;
    }

    unsafe {
        (*sock)._handle = index as i8;
        (*sock)._connected = true;
    }

    Z_RES_OK
}

/// Listen on a serial port by device name (server mode).
///
/// For client-mode operation (which is the typical embedded use case),
/// this is equivalent to open.
#[unsafe(no_mangle)]
pub extern "C" fn _z_listen_serial_from_dev(
    sock: *mut ZSysNetSocket,
    dev: *const u8,
    baudrate: u32,
) -> ZResult {
    _z_open_serial_from_dev(sock, dev, baudrate)
}

/// Listen on a serial port by pin numbers (server mode).
#[unsafe(no_mangle)]
pub extern "C" fn _z_listen_serial_from_pins(
    sock: *mut ZSysNetSocket,
    txpin: u32,
    rxpin: u32,
    baudrate: u32,
) -> ZResult {
    _z_open_serial_from_pins(sock, txpin, rxpin, baudrate)
}

/// Close a serial port.
#[unsafe(no_mangle)]
pub extern "C" fn _z_close_serial(sock: *mut ZSysNetSocket) {
    if !sock.is_null() {
        unsafe {
            (*sock)._connected = false;
            (*sock)._handle = -1;
        }
    }
}

// ============================================================================
// Frame-level I/O (COBS encode/decode + CRC)
// ============================================================================

/// Read a complete serial frame (COBS-decoded, CRC-checked).
///
/// Reads raw bytes from UART until the 0x00 delimiter, then calls
/// `_z_serial_msg_deserialize` to decode COBS and verify CRC32.
///
/// Returns the number of payload bytes, or `SIZE_MAX` on error.
#[unsafe(no_mangle)]
pub extern "C" fn _z_read_serial_internal(
    sock: ZSysNetSocket,
    header: *mut u8,
    ptr: *mut u8,
    len: usize,
) -> usize {
    let index = sock._handle as usize;
    let port = match unsafe { get_port(index) } {
        Some(p) => p,
        None => return usize::MAX,
    };

    // Allocate COBS buffer from zenoh-pico's allocator
    let raw_buf = unsafe { z_malloc(Z_SERIAL_MAX_COBS_BUF_SIZE) };
    if raw_buf.is_null() {
        return usize::MAX;
    }

    // Read bytes until 0x00 delimiter
    let mut rb: usize = 0;
    loop {
        if rb >= Z_SERIAL_MAX_COBS_BUF_SIZE {
            break;
        }

        // Fill ring buffer from UART
        port.rx_fill();

        // Try to drain one byte
        let mut byte = [0u8; 1];
        if port.rx_drain(&mut byte) == 0 {
            // No data yet — yield briefly and retry
            // On bare-metal this is a busy-wait, acceptable for serial speeds
            continue;
        }

        unsafe { *raw_buf.add(rb) = byte[0] };
        rb += 1;

        if byte[0] == 0x00 {
            break;
        }
    }

    // Allocate temporary buffer for COBS decode
    let tmp_buf = unsafe { z_malloc(Z_SERIAL_MFS_SIZE) };
    if tmp_buf.is_null() {
        unsafe { z_free(raw_buf) };
        return usize::MAX;
    }

    let ret = unsafe {
        _z_serial_msg_deserialize(raw_buf, rb, ptr, len, header, tmp_buf, Z_SERIAL_MFS_SIZE)
    };

    unsafe {
        z_free(raw_buf);
        z_free(tmp_buf);
    }

    ret
}

/// Send a complete serial frame (COBS-encoded with CRC).
///
/// Calls `_z_serial_msg_serialize` to encode payload with COBS and CRC32,
/// then writes the raw bytes to the UART.
///
/// Returns the number of payload bytes sent, or `SIZE_MAX` on error.
#[unsafe(no_mangle)]
pub extern "C" fn _z_send_serial_internal(
    sock: ZSysNetSocket,
    header: u8,
    ptr: *const u8,
    len: usize,
) -> usize {
    let index = sock._handle as usize;
    let port = match unsafe { get_port(index) } {
        Some(p) => p,
        None => return usize::MAX,
    };

    // Allocate buffers from zenoh-pico's allocator
    let tmp_buf = unsafe { z_malloc(Z_SERIAL_MFS_SIZE) };
    let raw_buf = unsafe { z_malloc(Z_SERIAL_MAX_COBS_BUF_SIZE) };
    if tmp_buf.is_null() || raw_buf.is_null() {
        if !tmp_buf.is_null() {
            unsafe { z_free(tmp_buf) };
        }
        if !raw_buf.is_null() {
            unsafe { z_free(raw_buf) };
        }
        return usize::MAX;
    }

    let wire_len = unsafe {
        _z_serial_msg_serialize(
            raw_buf,
            Z_SERIAL_MAX_COBS_BUF_SIZE,
            ptr,
            len,
            header,
            tmp_buf,
            Z_SERIAL_MFS_SIZE,
        )
    };

    if wire_len == usize::MAX {
        unsafe {
            z_free(raw_buf);
            z_free(tmp_buf);
        }
        return usize::MAX;
    }

    // Write all bytes to UART
    let data = unsafe { core::slice::from_raw_parts(raw_buf, wire_len) };
    let written = port.write(data);

    unsafe {
        z_free(raw_buf);
        z_free(tmp_buf);
    }

    if written == wire_len {
        len // Return payload length (not wire length), matching Zephyr convention
    } else {
        usize::MAX
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Parse a device name like "UART_0" or "0" to a port index.
fn parse_port_index(dev: *const u8) -> Option<usize> {
    // Walk the C string to find the last digit
    let mut ptr = dev;
    let mut last_digit: Option<u8> = None;

    unsafe {
        while *ptr != 0 {
            let c = *ptr;
            if c >= b'0' && c <= b'9' {
                last_digit = Some(c - b'0');
            }
            ptr = ptr.add(1);
        }
    }

    last_digit.map(|d| d as usize)
}
