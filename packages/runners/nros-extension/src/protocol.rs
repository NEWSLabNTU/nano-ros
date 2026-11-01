//! Protocol versions (RFC-0119 D6).
//!
//! `protocol` is versioned from the first release. A runner states the
//! versions it speaks in its `describe` reply, and nros refuses a runner it
//! shares no version with, naming both sides. The crate's own version follows
//! this list: while the crate is 0.x, its minor number moves exactly when
//! [`SUPPORTED`] changes (README "Versioning").

use std::fmt;

/// The newest protocol version this crate speaks, and the one it writes.
pub const PROTOCOL: u32 = 1;

/// Every protocol version this crate can read, oldest first.
pub const SUPPORTED: &[u32] = &[1];

/// The two sides of a protocol negotiation share no version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolMismatch {
    /// What the caller (nros) speaks.
    pub ours: Vec<u32>,
    /// What the other side (the runner or command) states.
    pub theirs: Vec<u32>,
}

impl fmt::Display for ProtocolMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "protocol mismatch: nros speaks {}, the package speaks {} — \
             upgrade whichever side is older",
            list(&self.ours),
            list(&self.theirs)
        )
    }
}

impl std::error::Error for ProtocolMismatch {}

fn list(v: &[u32]) -> String {
    if v.is_empty() {
        return "no version".to_string();
    }
    v.iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The newest version both sides speak.
///
/// # Errors
///
/// [`ProtocolMismatch`] when the two lists share no version.
pub fn negotiate(ours: &[u32], theirs: &[u32]) -> Result<u32, ProtocolMismatch> {
    ours.iter()
        .filter(|v| theirs.contains(v))
        .max()
        .copied()
        .ok_or_else(|| ProtocolMismatch {
            ours: ours.to_vec(),
            theirs: theirs.to_vec(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_newest_shared_version() {
        assert_eq!(negotiate(&[1, 2, 3], &[2, 3, 4]), Ok(3));
        assert_eq!(negotiate(SUPPORTED, &[PROTOCOL]), Ok(PROTOCOL));
    }

    #[test]
    fn a_mismatch_names_both_sides() {
        let e = negotiate(&[1], &[2]).unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("nros speaks 1"), "{msg}");
        assert!(msg.contains("package speaks 2"), "{msg}");
        assert!(negotiate(&[1], &[]).is_err());
    }
}
