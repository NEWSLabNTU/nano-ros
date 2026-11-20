// RFC-0132 D4 (phase-560 W9.e).

//! The image's CAN frame device, from the
//! `can` row of its `NROS_IMAGE_NET` text to the backend's `open(NULL)`.
//!
//! The device is named once per board stack (`[image.<id>.can] device` on a
//! host board, a `[canN]` table on a `"board"` board) and the CLI writes it into
//! the image's network text. A board decodes that row at boot with
//! [`boot_device`] and stores the device with `nros_board_can_set_image_device`
//! (`c/nros_board_can_image.c`, which `can_socketcan::compile_socketcan_backend`
//! compiles for the Linux-process boards); the CAN backend reads it back when
//! the application opens `NULL`. The store is apart from every backend, so
//! the board's reference pulls in the store and no backend.

use crate::image_net::{self, CanDriver};

/// The device a board hands to its derived backend: the `can` row's device
/// when the row selects no other backend. `None` when the text carries no
/// `can` row, names no device, or selects the mock (whose `open` ignores the
/// device).
///
/// # Errors
/// A text the codec cannot decode (a malformed row, two `can` rows).
pub fn device(text: &str) -> Result<Option<&str>, image_net::DecodeError> {
    Ok(match image_net::can(text)? {
        Some(row) if row.driver == CanDriver::Derived => row.device,
        _ => None,
    })
}

/// The device a board hands over at boot, from `DeployOverlay::net` (the
/// image's `NROS_IMAGE_NET` text, baked verbatim). The board then stores it
/// with `nros_board_can_set_image_device` (the call is the board's: this
/// crate forbids `unsafe`).
///
/// # Panics
/// When the text does not decode, the same way
/// [`image_net::boot_interface`] refuses a network it cannot apply: the CLI
/// wrote the text, so it is a build defect, never a runtime condition.
#[must_use]
pub fn boot_device(net: Option<&'static str>) -> Option<&'static str> {
    match device(net?) {
        Ok(d) => d,
        Err(e) => panic!("NROS_IMAGE_NET: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::device;

    #[test]
    fn a_device_row_hands_its_device() {
        assert_eq!(device("vcan7,can,-,-,-"), Ok(Some("vcan7")));
    }

    #[test]
    fn the_device_rides_beside_an_interface() {
        let text = "eth0,ethernet,qemu,10.0.2.50,24,10.0.2.2,02:00:00:4f:1c:a2,slirp|can1,can,-,500000,2000000";
        assert_eq!(device(text), Ok(Some("can1")));
    }

    #[test]
    fn no_can_row_hands_nothing() {
        assert_eq!(device(""), Ok(None));
        assert_eq!(
            device("eth0,ethernet,qemu,10.0.2.50,24,10.0.2.2,02:00:00:4f:1c:a2,slirp"),
            Ok(None)
        );
    }

    #[test]
    fn the_mock_takes_no_device() {
        assert_eq!(device("-,can,mock,-,-"), Ok(None));
    }

    #[test]
    fn two_can_rows_are_refused() {
        assert!(device("vcan0,can,-,-,-|vcan1,can,-,-,-").is_err());
    }
}
