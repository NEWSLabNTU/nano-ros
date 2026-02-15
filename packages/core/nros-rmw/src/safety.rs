//! E2E Safety Protocol
//!
//! CRC-32/ISO-HDLC integrity checking and sequence tracking for
//! safety-critical communication per AUTOSAR E2E / EN 50159.
//!
//! # Overview
//!
//! - **Publisher**: computes CRC-32 over CDR payload bytes, appends to attachment
//! - **Subscriber**: recomputes CRC, checks sequence continuity
//! - **Zero cost when disabled**: entire module is `#[cfg(feature = "safety-e2e")]`
//!
//! # Memory budget
//!
//! - CRC-32 lookup table: 1024 bytes (.rodata)
//! - `SafetyValidator`: 16 bytes per subscriber instance

/// CRC-32/ISO-HDLC lookup table (polynomial 0xEDB88320, reflected).
///
/// Standard Ethernet/AUTOSAR CRC. Generated at compile time.
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut crc = i;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i as usize] = crc;
        i += 1;
    }
    table
};

/// Compute CRC-32/ISO-HDLC over a byte slice.
///
/// This is the standard Ethernet CRC used by AUTOSAR E2E and EN 50159.
///
/// ```ignore
/// use nros_rmw::safety::crc32;
/// assert_eq!(crc32(b"123456789"), 0xCBF43926);
/// ```
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[index];
    }
    crc ^ 0xFFFF_FFFF
}

/// Result of E2E integrity validation for a received message.
///
/// Returned by `try_recv_validated()` on the subscriber side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntegrityStatus {
    /// Sequence gap: 0 = normal, >0 = messages lost, <0 should not occur
    /// in normal operation (would indicate reordering).
    pub gap: i64,
    /// True if this message's sequence number is less than the expected
    /// (duplicate or out-of-order delivery).
    pub duplicate: bool,
    /// CRC validation result:
    /// - `Some(true)` = CRC matched
    /// - `Some(false)` = CRC mismatch (data corrupted)
    /// - `None` = no CRC present (sender doesn't have safety-e2e)
    pub crc_valid: Option<bool>,
}

impl IntegrityStatus {
    /// Returns true if the message passed all integrity checks.
    ///
    /// Valid means: no sequence gap, not a duplicate, and CRC is
    /// either correct or absent (interop with non-safety publishers).
    pub fn is_valid(&self) -> bool {
        self.gap == 0 && !self.duplicate && self.crc_valid != Some(false)
    }
}

/// Subscriber-side sequence tracker for E2E safety validation.
///
/// Tracks the expected sequence number and detects gaps, duplicates,
/// and CRC mismatches.
///
/// Size: 16 bytes (i64 expected_seq + bool initialized + padding).
#[derive(Debug)]
pub struct SafetyValidator {
    /// Next expected sequence number. -1 means no message received yet.
    expected_seq: i64,
    /// Whether we've received at least one message.
    initialized: bool,
}

impl SafetyValidator {
    /// Create a new validator in the uninitialized state.
    pub const fn new() -> Self {
        Self {
            expected_seq: 0,
            initialized: false,
        }
    }

    /// Validate a received message's sequence number and CRC result.
    ///
    /// Call this for each received message with the message's sequence
    /// number (from the RMW attachment) and the CRC validation result.
    pub fn validate(&mut self, message_seq: i64, crc_valid: Option<bool>) -> IntegrityStatus {
        if !self.initialized {
            // First message — establish baseline
            self.initialized = true;
            self.expected_seq = message_seq + 1;
            return IntegrityStatus {
                gap: 0,
                duplicate: false,
                crc_valid,
            };
        }

        if message_seq == self.expected_seq {
            // Normal: expected sequence
            self.expected_seq = message_seq + 1;
            IntegrityStatus {
                gap: 0,
                duplicate: false,
                crc_valid,
            }
        } else if message_seq < self.expected_seq {
            // Duplicate or out-of-order
            IntegrityStatus {
                gap: 0,
                duplicate: true,
                crc_valid,
            }
        } else {
            // Gap: message_seq > expected_seq
            let gap = message_seq - self.expected_seq;
            self.expected_seq = message_seq + 1;
            IntegrityStatus {
                gap,
                duplicate: false,
                crc_valid,
            }
        }
    }

    /// Reset the validator to its initial state.
    pub fn reset(&mut self) {
        self.initialized = false;
        self.expected_seq = 0;
    }
}

impl Default for SafetyValidator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- CRC-32 tests ---

    #[test]
    fn crc32_empty() {
        assert_eq!(crc32(b""), 0x0000_0000);
    }

    #[test]
    fn crc32_check_value() {
        // The standard CRC-32 check value for ASCII "123456789"
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn crc32_single_byte() {
        // Known value for single 'A' (0x41)
        let crc = crc32(b"A");
        // Verify determinism
        assert_eq!(crc, crc32(b"A"));
        assert_ne!(crc, 0);
    }

    #[test]
    fn crc32_deterministic() {
        let data = b"hello world";
        assert_eq!(crc32(data), crc32(data));
    }

    #[test]
    fn crc32_different_data_different_crc() {
        assert_ne!(crc32(b"hello"), crc32(b"world"));
    }

    #[test]
    fn crc32_single_bit_flip_detected() {
        let original = [0x00u8; 64];
        let crc_original = crc32(&original);

        // Flip each bit position and verify CRC changes
        for byte_pos in 0..original.len() {
            for bit in 0..8u8 {
                let mut flipped = original;
                flipped[byte_pos] ^= 1 << bit;
                let crc_flipped = crc32(&flipped);
                assert_ne!(
                    crc_original, crc_flipped,
                    "CRC failed to detect bit flip at byte {} bit {}",
                    byte_pos, bit
                );
            }
        }
    }

    #[test]
    fn crc32_single_bit_flip_nonzero_data() {
        // Also test with non-zero data
        let original: [u8; 16] = [
            0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC,
            0xDE, 0xF0,
        ];
        let crc_original = crc32(&original);

        for byte_pos in 0..original.len() {
            for bit in 0..8u8 {
                let mut flipped = original;
                flipped[byte_pos] ^= 1 << bit;
                assert_ne!(
                    crc_original,
                    crc32(&flipped),
                    "CRC failed at byte {} bit {}",
                    byte_pos,
                    bit
                );
            }
        }
    }

    // --- IntegrityStatus tests ---

    #[test]
    fn integrity_status_valid() {
        let status = IntegrityStatus {
            gap: 0,
            duplicate: false,
            crc_valid: Some(true),
        };
        assert!(status.is_valid());
    }

    #[test]
    fn integrity_status_valid_no_crc() {
        // No CRC (interop with non-safety publisher) is still valid
        let status = IntegrityStatus {
            gap: 0,
            duplicate: false,
            crc_valid: None,
        };
        assert!(status.is_valid());
    }

    #[test]
    fn integrity_status_invalid_crc() {
        let status = IntegrityStatus {
            gap: 0,
            duplicate: false,
            crc_valid: Some(false),
        };
        assert!(!status.is_valid());
    }

    #[test]
    fn integrity_status_invalid_gap() {
        let status = IntegrityStatus {
            gap: 3,
            duplicate: false,
            crc_valid: Some(true),
        };
        assert!(!status.is_valid());
    }

    #[test]
    fn integrity_status_invalid_duplicate() {
        let status = IntegrityStatus {
            gap: 0,
            duplicate: true,
            crc_valid: Some(true),
        };
        assert!(!status.is_valid());
    }

    // --- SafetyValidator tests ---

    #[test]
    fn validator_first_message() {
        let mut v = SafetyValidator::new();
        let status = v.validate(0, Some(true));
        assert!(status.is_valid());
        assert_eq!(status.gap, 0);
        assert!(!status.duplicate);
    }

    #[test]
    fn validator_sequential_messages() {
        let mut v = SafetyValidator::new();
        for seq in 0..10 {
            let status = v.validate(seq, Some(true));
            assert!(status.is_valid(), "failed at seq {}", seq);
        }
    }

    #[test]
    fn validator_first_message_nonzero_seq() {
        // First message can start at any sequence number
        let mut v = SafetyValidator::new();
        let status = v.validate(100, Some(true));
        assert!(status.is_valid());

        let status = v.validate(101, Some(true));
        assert!(status.is_valid());
    }

    #[test]
    fn validator_gap_detection() {
        let mut v = SafetyValidator::new();
        v.validate(0, Some(true)); // seq 0 → expect 1
        v.validate(1, Some(true)); // seq 1 → expect 2

        // Skip seq 2,3,4 → gap of 3
        let status = v.validate(5, Some(true));
        assert_eq!(status.gap, 3);
        assert!(!status.duplicate);
        assert!(!status.is_valid()); // gap makes it invalid
    }

    #[test]
    fn validator_duplicate_detection() {
        let mut v = SafetyValidator::new();
        v.validate(0, Some(true)); // expect 1
        v.validate(1, Some(true)); // expect 2
        v.validate(2, Some(true)); // expect 3

        // Receive seq 1 again (duplicate)
        let status = v.validate(1, Some(true));
        assert!(status.duplicate);
        assert!(!status.is_valid());
    }

    #[test]
    fn validator_crc_failure() {
        let mut v = SafetyValidator::new();
        let status = v.validate(0, Some(false));
        assert!(!status.is_valid());
        assert_eq!(status.crc_valid, Some(false));
    }

    #[test]
    fn validator_no_crc_interop() {
        // Messages from non-safety publishers have no CRC
        let mut v = SafetyValidator::new();
        let status = v.validate(0, None);
        assert!(status.is_valid());
        assert_eq!(status.crc_valid, None);
    }

    #[test]
    fn validator_reset() {
        let mut v = SafetyValidator::new();
        v.validate(0, Some(true));
        v.validate(1, Some(true));

        v.reset();

        // After reset, seq 0 is treated as first message again
        let status = v.validate(0, Some(true));
        assert!(status.is_valid());
        assert_eq!(status.gap, 0);
    }

    #[test]
    fn validator_recovery_after_gap() {
        let mut v = SafetyValidator::new();
        v.validate(0, Some(true)); // expect 1

        // Gap: skip to 5
        let status = v.validate(5, Some(true));
        assert_eq!(status.gap, 4);

        // Next message is 6 → normal again
        let status = v.validate(6, Some(true));
        assert!(status.is_valid());
    }

    // --- Ghost model correspondence tests ---

    #[test]
    fn ghost_integrity_status_correspondence() {
        use nros_ghost_types::IntegrityStatusGhost;

        // Case 1: Valid with CRC ok
        let prod = IntegrityStatus {
            gap: 0,
            duplicate: false,
            crc_valid: Some(true),
        };
        let ghost = IntegrityStatusGhost {
            gap: prod.gap,
            duplicate: prod.duplicate,
            crc_known: prod.crc_valid.is_some(),
            crc_ok: prod.crc_valid == Some(true),
        };
        assert_eq!(ghost.gap, 0);
        assert!(!ghost.duplicate);
        assert!(ghost.crc_known);
        assert!(ghost.crc_ok);

        // Case 2: CRC mismatch
        let prod = IntegrityStatus {
            gap: 0,
            duplicate: false,
            crc_valid: Some(false),
        };
        let ghost = IntegrityStatusGhost {
            gap: prod.gap,
            duplicate: prod.duplicate,
            crc_known: prod.crc_valid.is_some(),
            crc_ok: prod.crc_valid == Some(true),
        };
        assert!(ghost.crc_known);
        assert!(!ghost.crc_ok);

        // Case 3: No CRC (interop)
        let prod = IntegrityStatus {
            gap: 0,
            duplicate: false,
            crc_valid: None,
        };
        let ghost = IntegrityStatusGhost {
            gap: prod.gap,
            duplicate: prod.duplicate,
            crc_known: prod.crc_valid.is_some(),
            crc_ok: prod.crc_valid == Some(true),
        };
        assert!(!ghost.crc_known);
        assert!(!ghost.crc_ok);

        // Verify is_valid correspondence for all cases
        // is_valid: gap == 0 && !duplicate && crc_valid != Some(false)
        // ghost:    gap == 0 && !duplicate && !(crc_known && !crc_ok)
        let cases: &[(i64, bool, Option<bool>)] = &[
            (0, false, Some(true)),  // valid
            (0, false, Some(false)), // invalid: CRC bad
            (0, false, None),        // valid: no CRC
            (3, false, Some(true)),  // invalid: gap
            (0, true, Some(true)),   // invalid: duplicate
        ];
        for &(gap, duplicate, crc_valid) in cases {
            let prod = IntegrityStatus {
                gap,
                duplicate,
                crc_valid,
            };
            let ghost = IntegrityStatusGhost {
                gap: prod.gap,
                duplicate: prod.duplicate,
                crc_known: prod.crc_valid.is_some(),
                crc_ok: prod.crc_valid == Some(true),
            };
            let prod_valid = prod.is_valid();
            let ghost_valid =
                ghost.gap == 0 && !ghost.duplicate && !(ghost.crc_known && !ghost.crc_ok);
            assert_eq!(
                prod_valid, ghost_valid,
                "Mismatch for gap={}, dup={}, crc={:?}: prod={}, ghost={}",
                gap, duplicate, crc_valid, prod_valid, ghost_valid
            );
        }
    }

    #[test]
    fn ghost_validator_correspondence() {
        use nros_ghost_types::SafetyValidatorGhost;

        // Initial state
        let v = SafetyValidator::new();
        let ghost = SafetyValidatorGhost {
            expected_seq: v.expected_seq,
            initialized: v.initialized,
        };
        assert_eq!(ghost.expected_seq, 0);
        assert!(!ghost.initialized);

        // After first message (seq 5)
        let mut v = SafetyValidator::new();
        let status = v.validate(5, Some(true));
        let ghost = SafetyValidatorGhost {
            expected_seq: v.expected_seq,
            initialized: v.initialized,
        };
        assert_eq!(ghost.expected_seq, 6);
        assert!(ghost.initialized);
        assert_eq!(status.gap, 0);

        // Normal message (seq 6)
        let status = v.validate(6, Some(true));
        let ghost = SafetyValidatorGhost {
            expected_seq: v.expected_seq,
            initialized: v.initialized,
        };
        assert_eq!(ghost.expected_seq, 7);
        assert!(status.gap == 0 && !status.duplicate);

        // Gap (seq 10, expected 7 → gap 3)
        let status = v.validate(10, Some(true));
        let ghost = SafetyValidatorGhost {
            expected_seq: v.expected_seq,
            initialized: v.initialized,
        };
        assert_eq!(ghost.expected_seq, 11);
        assert_eq!(status.gap, 3);

        // Duplicate (seq 5, expected 11)
        let status = v.validate(5, Some(true));
        let ghost_after_dup = SafetyValidatorGhost {
            expected_seq: v.expected_seq,
            initialized: v.initialized,
        };
        // expected_seq unchanged on duplicate
        assert_eq!(ghost_after_dup.expected_seq, ghost.expected_seq);
        assert!(status.duplicate);
    }
}
