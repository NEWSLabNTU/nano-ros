//! Serial port trait and static port table

/// Maximum number of serial ports that can be registered.
pub const MAX_SERIAL_PORTS: usize = 2;

/// Size of the per-port RX ring buffer.
///
/// Must be larger than `_Z_SERIAL_MAX_COBS_BUF_SIZE` (1516) to hold a
/// complete COBS-encoded frame while it's being assembled byte-by-byte.
const RX_BUF_SIZE: usize = 2048;

/// Trait for UART peripherals. Board crates implement this for their
/// specific hardware.
pub trait SerialPort {
    /// Write bytes to the UART TX FIFO.
    ///
    /// Returns the number of bytes actually written. Implementations
    /// should poll/busy-wait until all bytes are transmitted (serial
    /// is slow enough that blocking is acceptable).
    fn write(&mut self, data: &[u8]) -> usize;

    /// Read available bytes from the UART RX FIFO (non-blocking).
    ///
    /// Returns the number of bytes read into `buf`. Returns 0 if
    /// no data is available.
    fn read(&mut self, buf: &mut [u8]) -> usize;
}

/// Internal state for a registered serial port.
pub(crate) struct PortState {
    /// The registered serial port (None if slot is empty).
    port: Option<&'static mut dyn SerialPort>,
    /// RX ring buffer for assembling incoming bytes.
    rx_buf: [u8; RX_BUF_SIZE],
    /// Read position in the ring buffer.
    rx_head: usize,
    /// Write position in the ring buffer.
    rx_tail: usize,
}

impl PortState {
    const fn new() -> Self {
        Self {
            port: None,
            rx_buf: [0u8; RX_BUF_SIZE],
            rx_head: 0,
            rx_tail: 0,
        }
    }

    /// Drain bytes from the ring buffer into `dst`.
    /// Returns the number of bytes copied.
    pub(crate) fn rx_drain(&mut self, dst: &mut [u8]) -> usize {
        let mut n = 0;
        while n < dst.len() && self.rx_head != self.rx_tail {
            dst[n] = self.rx_buf[self.rx_head];
            self.rx_head = (self.rx_head + 1) % RX_BUF_SIZE;
            n += 1;
        }
        n
    }

    /// Push bytes from the UART RX FIFO into the ring buffer.
    /// Returns the number of bytes pushed.
    pub(crate) fn rx_fill(&mut self) -> usize {
        let port = match self.port.as_mut() {
            Some(p) => p,
            None => return 0,
        };

        let mut total = 0;
        // Read in chunks to reduce call overhead
        let mut tmp = [0u8; 64];
        loop {
            let n = port.read(&mut tmp);
            if n == 0 {
                break;
            }
            for i in 0..n {
                let next_tail = (self.rx_tail + 1) % RX_BUF_SIZE;
                if next_tail == self.rx_head {
                    // Ring buffer full — drop remaining bytes
                    return total;
                }
                self.rx_buf[self.rx_tail] = tmp[i];
                self.rx_tail = next_tail;
            }
            total += n;
        }
        total
    }

    /// Write bytes to the port. Returns bytes written.
    pub(crate) fn write(&mut self, data: &[u8]) -> usize {
        match self.port.as_mut() {
            Some(p) => p.write(data),
            None => 0,
        }
    }

    /// Check if a port is registered.
    pub(crate) fn is_registered(&self) -> bool {
        self.port.is_some()
    }
}

/// Static port table.
static mut PORTS: [PortState; MAX_SERIAL_PORTS] = [
    PortState::new(),
    PortState::new(),
];

/// Register a serial port at the given index.
///
/// The board crate calls this during `init_hardware()` after creating the
/// UART driver in static storage.
///
/// # Safety
///
/// - `index` must be less than [`MAX_SERIAL_PORTS`]
/// - The `port` reference must have `'static` lifetime (e.g., from a `static mut`)
/// - Must only be called once per index
///
/// # Panics
///
/// Panics if `index >= MAX_SERIAL_PORTS`.
#[allow(static_mut_refs)]
pub unsafe fn register_port(index: usize, port: &'static mut dyn SerialPort) {
    assert!(index < MAX_SERIAL_PORTS, "serial port index out of range");
    unsafe {
        PORTS[index].port = Some(port);
        PORTS[index].rx_head = 0;
        PORTS[index].rx_tail = 0;
    }
}

/// Get a mutable reference to a port state by index.
///
/// # Safety
///
/// Must not be called concurrently for the same index.
#[allow(static_mut_refs)]
pub(crate) unsafe fn get_port(index: usize) -> Option<&'static mut PortState> {
    if index >= MAX_SERIAL_PORTS {
        return None;
    }
    let port = unsafe { &mut PORTS[index] };
    if port.is_registered() {
        Some(port)
    } else {
        None
    }
}
