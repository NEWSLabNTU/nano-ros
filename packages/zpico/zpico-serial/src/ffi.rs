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

use core::sync::atomic::{AtomicBool, Ordering};

use crate::port::get_port;

// ============================================================================
// Read mode (blocking during handshake, non-blocking during spin_once)
// ============================================================================

/// When `true`, `_z_read_serial_internal` blocks up to 5s waiting for data.
/// When `false`, it returns immediately if no data is available.
///
/// Starts `true` for z_open's transport handshake (which expects blocking reads).
/// Set to `false` by `zpico_serial_set_nonblocking()` after z_open succeeds,
/// so `zpico_spin_once` doesn't block for 5s on every idle iteration.
static SERIAL_BLOCKING_MODE: AtomicBool = AtomicBool::new(true);

/// Switch serial reads to non-blocking mode.
///
/// Called from `zpico_open()` in zpico.c after `z_open()` succeeds.
/// After this, `_z_read_serial_internal` returns `SIZE_MAX` immediately
/// when no data is available, which is what `zpico_spin_once` expects.
#[unsafe(no_mangle)]
pub extern "C" fn zpico_serial_set_nonblocking() {
    SERIAL_BLOCKING_MODE.store(false, Ordering::Relaxed);
}

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

/// z-serial Init flag (Header byte).
const Z_FLAG_SERIAL_INIT: u8 = 0x01;
/// z-serial Ack flag (Header byte).
const Z_FLAG_SERIAL_ACK: u8 = 0x02;
/// z-serial Reset flag (Header byte).
const _Z_FLAG_SERIAL_RESET: u8 = 0x04;

/// Delay between connect retries (ms).
/// Must be long enough that zenohd's z-serial accept loop can consume
/// the Init frame before the next one is sent — otherwise duplicate Init
/// frames in the PTY buffer cause "Unexpected Init flag in message".
const SERIAL_CONNECT_THROTTLE_MS: u32 = 2000;
/// Maximum number of connect attempts before giving up.
const SERIAL_CONNECT_MAX_ATTEMPTS: u32 = 10; // 10 * 2s = 20s

/// Poll interval when waiting for the first byte in `_z_read_serial_internal` (ms).
const SERIAL_READ_POLL_INTERVAL_MS: u32 = 10;
/// Maximum number of polls before `_z_read_serial_internal` times out.
/// 500 * 10ms = 5s timeout for reading the first byte.
const SERIAL_READ_TIMEOUT_POLLS: u32 = 500;

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

    /// Sleep for the given number of milliseconds.
    fn z_sleep_ms(ms: u32);
}

// ============================================================================
// z-serial connect handshake (Rust implementation)
// ============================================================================

/// Perform the z-serial link-level Init handshake.
///
/// Sends an empty frame with `_Z_FLAG_SERIAL_INIT` (0x01) and waits for
/// a response with `INIT + ACK` flags (0x03). This handshake is required
/// by zenohd's z-serial crate before any transport messages can flow.
///
/// Uses stack-allocated buffers for the Init handshake to avoid exhausting
/// the bump allocator (which has no deallocation support).
///
/// IMPORTANT: Only one Init frame is sent per attempt. Sending duplicate
/// Init frames before the first is consumed causes z-serial to reset the
/// connection ("Unexpected Init flag in message") after the handshake.
fn connect_serial(sock: ZSysNetSocket) -> ZResult {
    let index = sock._handle as usize;

    // Drain any stale data in the RX buffer before starting
    drain_rx(sock);

    // Pre-serialize the Init frame once (stack-allocated).
    // The Init frame is tiny: header=0x01, empty payload → ~9 bytes COBS-encoded.
    let mut init_frame = [0u8; 32];
    let mut init_tmp = [0u8; 32];
    let empty_payload: u8 = 0;
    let init_frame_len = unsafe {
        _z_serial_msg_serialize(
            init_frame.as_mut_ptr(),
            init_frame.len(),
            &empty_payload as *const u8,
            0,
            Z_FLAG_SERIAL_INIT,
            init_tmp.as_mut_ptr(),
            init_tmp.len(),
        )
    };
    if init_frame_len == usize::MAX || init_frame_len > init_frame.len() {
        return Z_ERR_GENERIC;
    }

    for _ in 0..SERIAL_CONNECT_MAX_ATTEMPTS {
        // Send the pre-serialized Init frame directly via port.write()
        let port = match unsafe { get_port(index) } {
            Some(p) => p,
            None => return Z_ERR_GENERIC,
        };
        let written = port.write(&init_frame[..init_frame_len]);
        if written != init_frame_len {
            return Z_ERR_GENERIC;
        }

        // Wait up to ~2s for a response (200 polls × 10ms).
        // Only poll reads — do NOT send another Init until this one times out.
        let mut got_response = false;
        for _ in 0..200 {
            // Read one COBS frame from UART using stack buffers
            match read_handshake_frame(index) {
                HandshakeResult::NoData => {
                    // No data yet — wait and retry
                    unsafe { z_sleep_ms(10) };
                    continue;
                }
                HandshakeResult::InitAck => {
                    return Z_RES_OK;
                }
                HandshakeResult::Reset => {
                    got_response = true;
                    break; // re-send Init
                }
                HandshakeResult::Other => {
                    got_response = true;
                    continue; // ignore stale data
                }
            }
        }

        if !got_response {
            // No response at all — zenohd may not be ready yet
            unsafe { z_sleep_ms(SERIAL_CONNECT_THROTTLE_MS) };
        }
    }

    // Timed out
    Z_ERR_GENERIC
}

/// Result of reading a handshake frame.
enum HandshakeResult {
    /// No data available (non-blocking).
    NoData,
    /// Received Init + Ack (handshake success).
    InitAck,
    /// Received Reset flag (need to re-send Init).
    Reset,
    /// Received some other frame (ignore).
    Other,
}

/// Read and decode a single COBS-framed handshake response.
///
/// Uses stack-allocated buffers (no heap allocation). Only suitable for
/// small handshake frames (Init/Ack/Reset).
fn read_handshake_frame(port_index: usize) -> HandshakeResult {
    let port = match unsafe { get_port(port_index) } {
        Some(p) => p,
        None => return HandshakeResult::NoData,
    };

    // Read raw COBS bytes until 0x00 delimiter (stack buffer)
    let mut raw = [0u8; 32]; // Init/Ack frames are ~9 bytes
    let mut rb: usize = 0;

    loop {
        if rb >= raw.len() {
            break;
        }

        port.rx_fill();
        let mut byte = [0u8; 1];
        if port.rx_drain(&mut byte) == 0 {
            if rb == 0 {
                return HandshakeResult::NoData;
            }
            // Mid-frame: keep waiting for 0x00 delimiter
            continue;
        }

        raw[rb] = byte[0];
        rb += 1;

        if byte[0] == 0x00 {
            break;
        }
    }

    // Decode COBS frame using stack buffers
    let mut header: u8 = 0;
    let mut payload = [0u8; 16];
    let mut tmp = [0u8; 32];
    let ret = unsafe {
        _z_serial_msg_deserialize(
            raw.as_ptr(),
            rb,
            payload.as_mut_ptr(),
            payload.len(),
            &mut header as *mut u8,
            tmp.as_mut_ptr(),
            tmp.len(),
        )
    };

    if ret == usize::MAX {
        return HandshakeResult::Other;
    }

    if (header & Z_FLAG_SERIAL_ACK) != 0 && (header & Z_FLAG_SERIAL_INIT) != 0 {
        HandshakeResult::InitAck
    } else if (header & _Z_FLAG_SERIAL_RESET) != 0 {
        HandshakeResult::Reset
    } else {
        HandshakeResult::Other
    }
}

/// Drain any stale data from the UART RX buffer.
fn drain_rx(sock: ZSysNetSocket) {
    let index = sock._handle as usize;
    if let Some(port) = unsafe { get_port(index) } {
        port.rx_fill();
        let mut discard = [0u8; 64];
        while port.rx_drain(&mut discard) > 0 {
            port.rx_fill();
        }
    }
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

    // Perform the z-serial link-level Init handshake (I flag / I+A flag exchange).
    // This must happen before zenoh transport messages (InitSyn/InitAck) flow.
    // All other platforms (Zephyr, RPi Pico, ESP-IDF, ThreadX) do this in their
    // _z_open_serial_from_dev — see zenoh-pico/src/system/*/network.c.
    connect_serial(unsafe { *sock })
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

    // Perform z-serial link-level Init handshake
    connect_serial(unsafe { *sock })
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
/// Blocks up to `SERIAL_READ_TIMEOUT_MS` waiting for the first byte.
/// Once at least one byte has arrived, continues reading until the 0x00
/// delimiter completes the COBS frame.
///
/// Returns the number of payload bytes, or `SIZE_MAX` on error/timeout.
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

    // Read bytes until 0x00 delimiter.
    // In blocking mode (during z_open handshake): wait up to 5s for first byte.
    // In non-blocking mode (during spin_once): return immediately if no data.
    let blocking = SERIAL_BLOCKING_MODE.load(Ordering::Relaxed);
    let max_polls = if blocking {
        SERIAL_READ_TIMEOUT_POLLS
    } else {
        1 // Single poll attempt in non-blocking mode
    };
    let mut rb: usize = 0;
    let mut first_byte_polls: u32 = 0;
    loop {
        if rb >= Z_SERIAL_MAX_COBS_BUF_SIZE {
            break;
        }

        // Fill ring buffer from UART
        port.rx_fill();

        // Try to drain one byte
        let mut byte = [0u8; 1];
        if port.rx_drain(&mut byte) == 0 {
            if rb == 0 {
                // No data yet
                first_byte_polls += 1;
                if first_byte_polls >= max_polls {
                    unsafe { z_free(raw_buf) };
                    return usize::MAX;
                }
                unsafe { z_sleep_ms(SERIAL_READ_POLL_INTERVAL_MS) };
                continue;
            }
            // Mid-frame: we've started receiving a COBS frame, keep waiting
            // for the 0x00 delimiter to complete it
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
