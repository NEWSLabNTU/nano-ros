//! Application trace markers: one `(marker_id, arg)` event that the
//! APPLICATION places, emitted through the same installed sink the executor's
//! own callback events go to.
//!
//! The id space is shared with the executor's events, so the application gets
//! a reserved block of its own:
//!
//! | ids        | owner                                                  |
//! |------------|--------------------------------------------------------|
//! | 0          | reserved: what a zeroed id reads as; never emitted     |
//! | 1 ..= 15   | the APPLICATION ([`APP_MARKER_FIRST`]..=[`APP_MARKER_LAST`]) |
//! | 16 and up  | the executor (16-27 today; the block grows upward)     |
//!
//! An id outside the application block is REFUSED, never clamped: a clamped
//! id would land on another marker's meaning, and a capture with confidently
//! wrong labels is worse than one with a gap.
//!
//! The range check and the error type are ungated, so a caller needs no `cfg`
//! and an image built without callback tracing still validates its ids. The
//! emission itself only happens with the `trace-callbacks` feature and an
//! installed sink. With no sink installed, [`emit_app_marker`] costs the range
//! compare, one atomic load and one branch; without the feature, the range
//! compare alone.

// Issue 2641 (GitHub #4009). Why 1..=15: `callback_trace` started the
// executor's block at 16 "rather than 8 so those keep room to grow
// contiguously" -- ids 1..7 were already the safety island's hand-placed
// phase markers (`sys_trace_app_marker` called directly), and 8..15 is the
// room that sentence left them. Choosing that block means every capture
// already taken with ids 1..7 keeps its meaning, and no executor id (16-27,
// growing upward) can ever be reached by an application call.

/// First application marker id.
pub const APP_MARKER_FIRST: u32 = 1;
/// Last application marker id. Ids from 16 up belong to the executor.
pub const APP_MARKER_LAST: u32 = 15;

/// An application marker id outside
/// [`APP_MARKER_FIRST`]..=[`APP_MARKER_LAST`]. Carries the refused id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppMarkerIdOutOfRange(pub u32);

impl core::fmt::Display for AppMarkerIdOutOfRange {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "trace marker id {} is outside the application range {}..={}; ids from {} up \
             belong to the executor",
            self.0,
            APP_MARKER_FIRST,
            APP_MARKER_LAST,
            APP_MARKER_LAST + 1
        )
    }
}

/// Whether `id` is an application marker id.
#[inline]
pub const fn is_app_marker(id: u32) -> bool {
    id >= APP_MARKER_FIRST && id <= APP_MARKER_LAST
}

/// Emit one application event `(id, arg)` through the installed trace sink.
///
/// `id` must be in [`APP_MARKER_FIRST`]..=[`APP_MARKER_LAST`]; any other id is
/// refused with [`AppMarkerIdOutOfRange`] and nothing is emitted. With no sink
/// installed, or in a build without the `trace-callbacks` feature, an in-range
/// call emits nothing and returns `Ok`.
///
/// Safe from any thread and from inside a callback: it reads one atomic and
/// calls the sink, whose contract already covers being called from every
/// thread that dispatches.
#[inline]
pub fn emit_app_marker(id: u32, arg: u32) -> Result<(), AppMarkerIdOutOfRange> {
    if !is_app_marker(id) {
        return Err(AppMarkerIdOutOfRange(id));
    }
    #[cfg(all(any(has_rmw, test), feature = "trace-callbacks"))]
    super::callback_trace::app_marker(id, arg);
    #[cfg(not(all(any(has_rmw, test), feature = "trace-callbacks")))]
    let _ = arg;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_application_block_is_below_every_executor_id() {
        assert_eq!(APP_MARKER_FIRST, 1);
        assert!(!is_app_marker(0));
        assert!(is_app_marker(1) && is_app_marker(15));
        // 16-27 are the executor's (register .. take stamp), and the block
        // grows upward from there.
        for id in 16..=27 {
            assert!(!is_app_marker(id), "{id} is an executor id");
            assert_eq!(emit_app_marker(id, 0), Err(AppMarkerIdOutOfRange(id)));
        }
        assert_eq!(
            emit_app_marker(u32::MAX, 0),
            Err(AppMarkerIdOutOfRange(u32::MAX))
        );
    }
}
