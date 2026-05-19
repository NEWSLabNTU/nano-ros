//! Call-site formatting buffer for the `nros_*!` macros.
//!
//! Wraps a `heapless::String<N>` where `N` is picked at compile time
//! by the `buffer-size-<N>` Cargo feature family. Overflow truncates
//! and appends `…` rather than dropping the record.

use core::fmt::{self, Write};

/// Returns the configured capacity in bytes.
#[must_use]
pub const fn format_buffer_capacity() -> usize {
    if cfg!(feature = "buffer-size-1024") {
        1024
    } else if cfg!(feature = "buffer-size-512") {
        512
    } else if cfg!(feature = "buffer-size-128") {
        128
    } else {
        256
    }
}

const CAPACITY: usize = format_buffer_capacity();

/// Stack-resident UTF-8 buffer for one log record's formatted body.
///
/// `Write` impl truncates + appends a single `…` (3 bytes) on
/// overflow; subsequent writes drop silently. The truncated content
/// is still valid UTF-8 (`…` is 3 bytes; we shrink first to make
/// room rather than splitting a multi-byte sequence).
pub struct FormatBuffer {
    inner: heapless::String<CAPACITY>,
    truncated: bool,
}

impl FormatBuffer {
    /// Empty buffer at the configured capacity.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: heapless::String::new(),
            truncated: false,
        }
    }

    /// Borrow the current contents as `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    /// Whether the formatter saw an overflow on at least one write.
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

impl Default for FormatBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for FormatBuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.truncated {
            return Ok(());
        }
        match self.inner.push_str(s) {
            Ok(()) => Ok(()),
            Err(()) => {
                self.truncated = true;
                // Reserve 3 bytes for the ellipsis. heapless::String
                // exposes capacity / len; shrink without splitting a
                // multi-byte UTF-8 boundary.
                let target_len = CAPACITY.saturating_sub(3);
                let bytes = self.inner.as_bytes();
                let mut trunc_at = bytes.len().min(target_len);
                while trunc_at > 0 && (bytes[trunc_at - 1] & 0b1100_0000) == 0b1000_0000 {
                    trunc_at -= 1;
                }
                self.inner.truncate(trunc_at);
                let _ = self.inner.push('\u{2026}');
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;

    #[test]
    fn small_payload_fits() {
        let mut b = FormatBuffer::new();
        write!(b, "hello {}", 42).unwrap();
        assert_eq!(b.as_str(), "hello 42");
        assert!(!b.truncated());
    }

    #[test]
    fn overflow_truncates_and_appends_ellipsis() {
        let mut b = FormatBuffer::new();
        let pad = "x".repeat(CAPACITY * 2);
        write!(b, "{}", pad).unwrap();
        assert!(b.truncated());
        assert!(b.as_str().ends_with('\u{2026}'));
        assert!(b.as_str().len() <= CAPACITY);
    }

    #[test]
    fn capacity_matches_feature_default() {
        // Default feature set selects 256.
        assert_eq!(format_buffer_capacity(), 256);
    }
}
