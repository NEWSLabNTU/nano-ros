// RFC-0132 -- the CAN frame driver.

//! The Rust face of `<nros/platform_can.h>`: a CAN frame driver
//! handle over the five generated `nros_platform_can_*` declarations.
//!
//! The definitions come from whichever backend the image links (Zephyr's CAN
//! API, SocketCAN, a board package's driver, or `nros-can-mock`); this module
//! only calls them. `send` and `send_fd` are one C call: `send_fd` sets
//! `NROS_PLATFORM_CAN_FLAG_FD` on a copy of the frame.

use core::ffi::CStr;

use crate::generated::{
    NROS_PLATFORM_CAN_FD_MAX_LEN, NROS_PLATFORM_CAN_FLAG_FD, NROS_PLATFORM_CAN_OPAQUE_U64S,
    nros_platform_can_close, nros_platform_can_fd_active, nros_platform_can_frame_t,
    nros_platform_can_open, nros_platform_can_recv, nros_platform_can_send, nros_platform_can_t,
};

/// One CAN or CAN FD frame, as the header lays it out.
pub type Frame = nros_platform_can_frame_t;

/// A negative `NROS_PLATFORM_CAN_E*` code from the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanError(pub i8);

fn check(rc: i8) -> Result<(), CanError> {
    if rc == 0 { Ok(()) } else { Err(CanError(rc)) }
}

/// Handle storage in the closed state, for a `static` or a stack slot.
pub const fn closed_storage() -> nros_platform_can_t {
    nros_platform_can_t {
        opaque: [0; NROS_PLATFORM_CAN_OPAQUE_U64S as usize],
    }
}

/// A frame with identifier `id`, `flags` (`NROS_PLATFORM_CAN_FLAG_*`) and
/// payload `data`. `None` when `data` is longer than an FD payload; whether
/// the length is valid for the frame's kind is the backend's check at send.
pub fn frame(id: u32, flags: u8, data: &[u8]) -> Option<Frame> {
    if data.len() > NROS_PLATFORM_CAN_FD_MAX_LEN as usize {
        return None;
    }
    let mut f = Frame {
        id,
        flags,
        len: data.len() as u8,
        data: [0; 64],
    };
    f.data[..data.len()].copy_from_slice(data);
    Some(f)
}

/// An open CAN device. It borrows its storage, so the storage cannot move
/// while the handle is open (a backend may keep self-referencing kernel
/// objects in it); dropping the handle closes it.
pub struct Can<'s> {
    raw: &'s mut nros_platform_can_t,
}

impl<'s> Can<'s> {
    /// Open `device` (`None`: the image's device) in `storage`. With
    /// `want_fd`, a device without CAN FD fails here
    /// (`NROS_PLATFORM_CAN_ENOFD`), never at the first FD send.
    pub fn open(
        storage: &'s mut nros_platform_can_t,
        device: Option<&CStr>,
        want_fd: bool,
    ) -> Result<Self, CanError> {
        let dev = device.map_or(core::ptr::null(), CStr::as_ptr);
        // SAFETY: `storage` is a live, exclusively borrowed handle; `dev` is
        // NULL or a NUL-terminated string that outlives the call.
        check(unsafe { nros_platform_can_open(storage, dev, u8::from(want_fd)) })?;
        Ok(Self { raw: storage })
    }

    /// Send `frame` as given, waiting at most `timeout_us` (`0`: do not wait).
    pub fn send(&mut self, frame: &Frame, timeout_us: u32) -> Result<(), CanError> {
        // SAFETY: an open handle and a frame that outlives the call.
        check(unsafe { nros_platform_can_send(self.raw, frame, timeout_us) })
    }

    /// Send `frame` as a CAN FD frame (`NROS_PLATFORM_CAN_FLAG_FD` set).
    pub fn send_fd(&mut self, frame: &Frame, timeout_us: u32) -> Result<(), CanError> {
        let mut f = *frame;
        f.flags |= NROS_PLATFORM_CAN_FLAG_FD as u8;
        self.send(&f, timeout_us)
    }

    /// Whether this handle carries CAN FD (opened with `want_fd` on an FD device).
    pub fn fd_active(&self) -> bool {
        // SAFETY: an open handle, read only.
        unsafe { nros_platform_can_fd_active(self.raw) != 0 }
    }

    /// Receive one frame, waiting at most `timeout_us` (`0`: poll).
    pub fn recv(&mut self, timeout_us: u32) -> Result<Frame, CanError> {
        let mut f = Frame {
            id: 0,
            flags: 0,
            len: 0,
            data: [0; 64],
        };
        // SAFETY: an open handle and a frame slot that outlives the call.
        check(unsafe { nros_platform_can_recv(self.raw, &mut f, timeout_us) })?;
        Ok(f)
    }
}

impl Drop for Can<'_> {
    fn drop(&mut self) {
        // SAFETY: the handle this value opened; close is idempotent.
        unsafe { nros_platform_can_close(self.raw) }
    }
}
