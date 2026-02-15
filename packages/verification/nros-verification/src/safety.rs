/// E2E Safety Protocol proofs (Phase 35.8)
///
/// Proves properties of the CRC-32/ISO-HDLC integrity check and sequence
/// tracking in `nros-rmw/src/safety.rs`.
///
/// ## Proofs
///
/// 1. **`crc32_determinism`** — same input always produces same CRC
/// 2. **`crc32_single_bit_detection`** — flipping any single bit in 1-byte data changes CRC
/// 3. **`validator_sequence_monotonicity`** — normal message increments expected_seq
/// 4. **`gap_detection_completeness`** — gap equals msg_seq - expected_seq when msg_seq > expected_seq
/// 5. **`is_valid_correctness`** — is_valid iff gap==0, not duplicate, CRC not known-bad
///
/// ## Trust levels
///
/// **Ghost model** (shared from `nros-ghost-types`, validated by production tests):
/// - `IntegrityStatusGhost` — mirrors `IntegrityStatus` with decomposed Option<bool>
/// - `SafetyValidatorGhost` — mirrors `SafetyValidator` private fields
///
/// **Pure math** (no link to production code):
/// - CRC-32 spec functions model the table-based algorithm using unrolled polynomial division
use vstd::prelude::*;
use nros_ghost_types::{IntegrityStatusGhost, SafetyValidatorGhost};

verus! {

// ======================================================================
// Ghost Type Registrations
// ======================================================================

/// Register `IntegrityStatusGhost` as a transparent type.
#[verifier::external_type_specification]
pub struct ExIntegrityStatusGhost(IntegrityStatusGhost);

/// Register `SafetyValidatorGhost` as a transparent type.
#[verifier::external_type_specification]
pub struct ExSafetyValidatorGhost(SafetyValidatorGhost);

// ======================================================================
// CRC-32 Spec Functions
// ======================================================================

/// Single step of CRC-32 table entry generation (one bit of polynomial division).
///
/// Models the inner loop body of CRC32_TABLE generation (safety.rs:27-31):
/// ```ignore
/// if crc & 1 != 0 {
///     crc = (crc >> 1) ^ 0xEDB8_8320;
/// } else {
///     crc >>= 1;
/// }
/// ```
pub open spec fn crc32_bit_step(crc: u32) -> u32 {
    if crc & 1u32 != 0u32 {
        (crc >> 1u32) ^ 0xEDB8_8320u32
    } else {
        crc >> 1u32
    }
}

/// Compute a CRC-32 table entry by unrolling 8 bit steps.
///
/// Models the CRC32_TABLE generation (safety.rs:23-37):
/// start with `i`, apply `crc32_bit_step` 8 times.
pub open spec fn crc32_table_entry(i: u32) -> u32 {
    let s0 = crc32_bit_step(i);
    let s1 = crc32_bit_step(s0);
    let s2 = crc32_bit_step(s1);
    let s3 = crc32_bit_step(s2);
    let s4 = crc32_bit_step(s3);
    let s5 = crc32_bit_step(s4);
    let s6 = crc32_bit_step(s5);
    crc32_bit_step(s6)
}

/// Single-byte CRC step using table lookup.
///
/// Models the loop body (safety.rs:51-53):
/// ```ignore
/// let index = ((crc ^ byte as u32) & 0xFF) as usize;
/// crc = (crc >> 8) ^ CRC32_TABLE[index];
/// ```
pub open spec fn crc32_step(crc: u32, byte: u8) -> u32 {
    let index = (crc ^ (byte as u32)) & 0xFFu32;
    (crc >> 8u32) ^ crc32_table_entry(index)
}

/// Recursive CRC-32 over a byte sequence.
///
/// Process bytes left-to-right, accumulating into `acc`.
pub open spec fn crc32_inner(data: Seq<u8>, acc: u32) -> u32
    decreases data.len(),
{
    if data.len() == 0 {
        acc
    } else {
        crc32_inner(data.drop_first(), crc32_step(acc, data[0]))
    }
}

/// CRC-32/ISO-HDLC spec: init with 0xFFFFFFFF, finalize with XOR 0xFFFFFFFF.
///
/// Models the complete `crc32()` function (safety.rs:48-55).
pub open spec fn crc32_spec(data: Seq<u8>) -> u32 {
    crc32_inner(data, 0xFFFF_FFFFu32) ^ 0xFFFF_FFFFu32
}

/// Flip one bit in a byte sequence.
///
/// `pos` is the bit index: byte `pos / 8`, bit `pos % 8`.
pub open spec fn flip_bit(data: Seq<u8>, pos: int) -> Seq<u8>
    recommends
        0 <= pos < data.len() * 8,
{
    let byte_idx = pos / 8;
    let bit_idx = pos % 8;
    data.update(byte_idx, data[byte_idx] ^ (1u8 << (bit_idx as u8)))
}

// ======================================================================
// Validator Spec Functions
// ======================================================================

/// Spec: `SafetyValidator::validate()` behavior.
///
/// Models the validate method (safety.rs:112-149).
pub open spec fn validate_spec(
    v: SafetyValidatorGhost,
    msg_seq: i64,
    crc_known: bool,
    crc_ok: bool,
) -> (SafetyValidatorGhost, IntegrityStatusGhost) {
    if !v.initialized {
        // First message — establish baseline
        (
            SafetyValidatorGhost { expected_seq: (msg_seq + 1) as i64, initialized: true },
            IntegrityStatusGhost { gap: 0i64, duplicate: false, crc_known, crc_ok },
        )
    } else if msg_seq == v.expected_seq {
        // Normal: expected sequence
        (
            SafetyValidatorGhost { expected_seq: (msg_seq + 1) as i64, initialized: true },
            IntegrityStatusGhost { gap: 0i64, duplicate: false, crc_known, crc_ok },
        )
    } else if msg_seq < v.expected_seq {
        // Duplicate or out-of-order — expected_seq unchanged
        (
            v,
            IntegrityStatusGhost { gap: 0i64, duplicate: true, crc_known, crc_ok },
        )
    } else {
        // Gap: msg_seq > expected_seq
        let gap = (msg_seq - v.expected_seq) as i64;
        (
            SafetyValidatorGhost { expected_seq: (msg_seq + 1) as i64, initialized: true },
            IntegrityStatusGhost { gap, duplicate: false, crc_known, crc_ok },
        )
    }
}

/// Spec: `IntegrityStatus::is_valid()` behavior.
///
/// Models (safety.rs:80-82):
/// ```ignore
/// self.gap == 0 && !self.duplicate && self.crc_valid != Some(false)
/// ```
///
/// Ghost decomposition of `crc_valid != Some(false)`:
/// - `Some(false)` means `crc_known && !crc_ok`
/// - `!= Some(false)` means `!(crc_known && !crc_ok)`
pub open spec fn integrity_is_valid(s: IntegrityStatusGhost) -> bool {
    s.gap == 0 && !s.duplicate && !(s.crc_known && !s.crc_ok)
}

// ======================================================================
// CRC-32 Proofs
// ======================================================================

/// **Proof 1: `crc32_determinism`**
///
/// CRC-32 is a pure function: the same input always produces the same output.
/// This is trivially true for spec functions (no side effects), but stating it
/// explicitly documents the requirement from AUTOSAR E2E Profile specifications.
///
/// Safety relevance: CRC-based error detection requires determinism —
/// sender and receiver must compute the same CRC for the same payload.
proof fn crc32_determinism(data: Seq<u8>)
    ensures
        crc32_spec(data) == crc32_spec(data),
{
    // Trivially true — spec functions are pure
}

/// **Proof 2: `crc32_single_bit_detection`**
///
/// For any 1-byte input and any bit position 0..8, flipping that single bit
/// changes the CRC-32 output. This proves the fundamental error detection
/// property of CRC-32 for the smallest non-trivial input.
///
/// The proof is bounded to 1-byte data where Z3 can fully evaluate the
/// unrolled CRC-32 table entry computation. Z3's bitvector theory handles
/// the 8 polynomial division steps + XOR operations on u32 automatically.
///
/// Safety relevance: EN 50159 requires detection of single-bit errors in
/// safety-critical communication. CRC-32 provides this for all message sizes,
/// and we prove it constructively for 1-byte data.
proof fn crc32_single_bit_detection(data: Seq<u8>, pos: int)
    requires
        data.len() == 1,
        0 <= pos < 8,
    ensures
        crc32_spec(data) != crc32_spec(flip_bit(data, pos)),
{
    // Allow Verus to unfold the recursive crc32_inner (depth 2: non-empty + empty)
    reveal_with_fuel(crc32_inner, 2);

    let b: u8 = data[0];
    let pos_u8: u8 = pos as u8;

    // Construct the flipped byte directly
    let mask: u8 = 1u8 << pos_u8;
    let fb: u8 = (b ^ mask) as u8;

    // Show that flip_bit produces a sequence with fb at index 0
    // flip_bit spec: data.update(pos/8, data[pos/8] ^ (1u8 << (pos%8 as u8)))
    // Since 0 <= pos < 8: pos/8 == 0 and pos%8 == pos
    assert(0 <= pos < 8);
    assert(pos / 8 == 0 && pos % 8 == pos);
    let flipped = flip_bit(data, pos);
    assert(flipped =~= data.update(0, fb));

    // The XOR mask is nonzero for pos in [0, 8)
    assert(mask != 0u8) by (bit_vector)
        requires 0u8 <= pos_u8 < 8u8, mask == 1u8 << pos_u8;

    // Nonzero XOR changes the byte
    assert(fb != b) by (bit_vector)
        requires mask != 0u8, fb == b ^ mask;

    let init = 0xFFFF_FFFFu32;

    // Unfold crc32_spec through crc32_inner for both sequences
    let orig_inner = crc32_inner(data, init);
    let flip_inner = crc32_inner(flipped, init);
    assert(orig_inner == crc32_step(init, b));
    assert(flip_inner == crc32_step(init, fb));

    // Connect to crc32_spec
    assert(crc32_spec(data) == orig_inner ^ 0xFFFF_FFFFu32);
    assert(crc32_spec(flip_bit(data, pos)) == flip_inner ^ 0xFFFF_FFFFu32);

    // Z3 bitvector reasoning: different bytes produce different CRC steps
    assert(crc32_step(init, b) != crc32_step(init, fb)) by (bit_vector)
        requires fb != b, init == 0xFFFF_FFFFu32;

    // Different CRC steps → different XOR results → different crc32_spec
    assert(orig_inner != flip_inner);
    assert(orig_inner ^ 0xFFFF_FFFFu32 != flip_inner ^ 0xFFFF_FFFFu32) by (bit_vector)
        requires orig_inner != flip_inner;
}

// ======================================================================
// Validator Proofs
// ======================================================================

/// **Proof 3: `validator_sequence_monotonicity`**
///
/// On a normal message (msg_seq == expected_seq), the new expected_seq is
/// strictly greater than the old expected_seq.
///
/// Models the increment at safety.rs:126: `self.expected_seq = message_seq + 1`.
///
/// Safety relevance: Sequence numbers advance monotonically, ensuring that
/// gap and duplicate detection remain meaningful across messages.
proof fn validator_sequence_monotonicity(v: SafetyValidatorGhost, msg_seq: i64)
    requires
        v.initialized,
        msg_seq == v.expected_seq,
        // No overflow (i64::MAX messages haven't been sent)
        msg_seq < i64::MAX,
    ensures
        ({
            let (new_v, _status) = validate_spec(v, msg_seq, true, true);
            new_v.expected_seq > v.expected_seq
        }),
{
    // msg_seq == v.expected_seq, so new expected_seq = msg_seq + 1 = v.expected_seq + 1
}

/// **Proof 4: `gap_detection_completeness`**
///
/// When msg_seq > expected_seq, the returned gap equals the exact number of
/// missing messages (msg_seq - expected_seq), and is strictly positive.
///
/// Models the gap calculation at safety.rs:141:
/// `let gap = message_seq - self.expected_seq;`
///
/// Safety relevance: Gap detection is complete — every missing message is
/// accounted for. No silent drops go undetected.
proof fn gap_detection_completeness(v: SafetyValidatorGhost, msg_seq: i64)
    requires
        v.initialized,
        msg_seq > v.expected_seq,
        // Gap fits in i64 (no wraparound in as-cast)
        msg_seq - v.expected_seq <= i64::MAX as int,
    ensures
        ({
            let (_new_v, status) = validate_spec(v, msg_seq, true, true);
            &&& status.gap == msg_seq - v.expected_seq
            &&& status.gap > 0
            &&& !status.duplicate
        }),
{
    // The gap is (msg_seq - v.expected_seq) as i64.
    // Since 0 < msg_seq - v.expected_seq <= i64::MAX, the as-cast is identity.
}

/// **Proof 5: `is_valid_correctness`**
///
/// `integrity_is_valid` returns true if and only if:
/// - gap == 0 (no missing messages)
/// - duplicate == false (not a redelivery)
/// - CRC is not known-bad (either correct or absent)
///
/// This is the boolean decomposition of `crc_valid != Some(false)`:
/// - `Some(false)` maps to `crc_known && !crc_ok`
/// - Therefore `!= Some(false)` maps to `!(crc_known && !crc_ok)`
///
/// Safety relevance: The is_valid() check is the safety gate — messages that
/// pass this check are safe to deliver to the application. No false positives
/// (corrupt/missing messages accepted) and no unnecessary false negatives
/// (valid messages rejected).
proof fn is_valid_correctness(s: IntegrityStatusGhost)
    ensures
        // Forward: all conditions met implies valid
        (s.gap == 0 && !s.duplicate && !(s.crc_known && !s.crc_ok))
            ==> integrity_is_valid(s),
        // Reverse: valid implies all conditions met
        integrity_is_valid(s)
            ==> (s.gap == 0 && !s.duplicate && !(s.crc_known && !s.crc_ok)),
        // Specific cases: known-good CRC is valid
        (s.gap == 0 && !s.duplicate && s.crc_known && s.crc_ok)
            ==> integrity_is_valid(s),
        // No CRC (interop) is valid
        (s.gap == 0 && !s.duplicate && !s.crc_known)
            ==> integrity_is_valid(s),
        // Known-bad CRC is invalid
        (s.crc_known && !s.crc_ok)
            ==> !integrity_is_valid(s),
        // Gap is invalid
        (s.gap != 0)
            ==> !integrity_is_valid(s),
        // Duplicate is invalid
        (s.duplicate)
            ==> !integrity_is_valid(s),
{
    // Boolean tautology — Z3 propositional solver
}

// ======================================================================
// Helper Proofs
// ======================================================================

/// **Helper: `validate_first_message_valid`**
///
/// The first message to an uninitialized validator always produces gap==0,
/// duplicate==false (i.e., the message is valid if CRC is ok or absent).
///
/// Models the first-message branch at safety.rs:113-122.
proof fn validate_first_message_valid(v: SafetyValidatorGhost, msg_seq: i64)
    requires
        !v.initialized,
        msg_seq < i64::MAX,
    ensures
        ({
            let (new_v, status) = validate_spec(v, msg_seq, true, true);
            &&& new_v.initialized
            &&& new_v.expected_seq == msg_seq + 1
            &&& status.gap == 0
            &&& !status.duplicate
            &&& integrity_is_valid(status)
        }),
{
}

/// **Helper: `validate_duplicate_no_state_change`**
///
/// When a duplicate message arrives (msg_seq < expected_seq), the validator's
/// expected_seq is unchanged — duplicates don't advance the sequence.
///
/// Models the duplicate branch at safety.rs:132-138.
proof fn validate_duplicate_no_state_change(v: SafetyValidatorGhost, msg_seq: i64)
    requires
        v.initialized,
        msg_seq < v.expected_seq,
    ensures
        ({
            let (new_v, status) = validate_spec(v, msg_seq, true, true);
            &&& new_v.expected_seq == v.expected_seq
            &&& status.duplicate
            &&& !integrity_is_valid(status)
        }),
{
}

/// **Helper: `gap_advances_past_missing`**
///
/// After a gap, the validator advances expected_seq past the gap to
/// msg_seq + 1, so subsequent sequential messages are accepted normally.
///
/// Models the gap branch at safety.rs:139-148.
proof fn gap_advances_past_missing(v: SafetyValidatorGhost, msg_seq: i64)
    requires
        v.initialized,
        msg_seq > v.expected_seq,
        msg_seq < i64::MAX,
    ensures
        ({
            let (new_v, _status) = validate_spec(v, msg_seq, true, true);
            new_v.expected_seq == msg_seq + 1
        }),
{
}

} // verus!
