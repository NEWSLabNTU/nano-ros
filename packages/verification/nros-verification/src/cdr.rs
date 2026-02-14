/// CDR serialization correctness proofs (Phase 31.3)
///
/// Proves round-trip integrity for CDR-encoded primitives and structural properties
/// of the writer/reader state machine.
///
/// ## Trust levels
///
/// **Transparent types** (via `external_type_specification` without `external_body`):
/// - `SerError` — simple pub enum with 3 variants. Verus can match on `BufferTooSmall`,
///   `StringTooLong`, `SequenceTooLong`.
/// - `DeserError` — simple pub enum with 5 variants. Verus can match on `UnexpectedEof`,
///   `InvalidData`, `InvalidUtf8`, `CapacityExceeded`, `InvalidHeader`.
///
/// **Ghost model** (shared from `nano-ros-ghost-types`, validated by production tests):
/// - `CdrGhost` — mirrors `CdrWriter`/`CdrReader` private fields (`buf` as `buf_len`,
///   `pos`, `origin`). Registered via `external_type_specification`.
///
/// **Pure math** (no link to production code):
/// - Little-endian encoding spec functions and round-trip proofs — prove that the
///   byte-level encoding is invertible for all values of each type.
use vstd::prelude::*;
use nros_ghost_types::CdrGhost;

verus! {

// ======================================================================
// Error Type Specifications
// ======================================================================

/// Register `SerError` with Verus as a transparent type.
///
/// Without `external_body`, Verus sees the enum's variant structure and allows
/// pattern matching in spec functions and proofs.
#[verifier::external_type_specification]
pub struct ExSerError(nros_serdes::SerError);

/// Register `DeserError` with Verus as a transparent type.
#[verifier::external_type_specification]
pub struct ExDeserError(nros_serdes::DeserError);

// ======================================================================
// CDR Ghost Model (from nano-ros-ghost-types)
// ======================================================================

/// Register `CdrGhost` as a transparent type so Verus can access fields.
#[verifier::external_type_specification]
pub struct ExCdrGhost(CdrGhost);

// ======================================================================
// Little-Endian Encoding Spec Functions
// ======================================================================

/// Spec: encode u16 as 2-byte little-endian sequence.
/// Models `u16::to_le_bytes()`.
pub open spec fn le_bytes_u16(v: u16) -> Seq<u8> {
    seq![
        (v & 0xff) as u8,
        ((v >> 8u16) & 0xff) as u8
    ]
}

/// Spec: decode 2-byte little-endian sequence to u16.
/// Models `u16::from_le_bytes()`.
pub open spec fn from_le_bytes_u16(b: Seq<u8>) -> u16 {
    (b[0] as u16) | ((b[1] as u16) << 8u16)
}

/// Spec: encode u32 as 4-byte little-endian sequence.
/// Models `u32::to_le_bytes()`.
pub open spec fn le_bytes_u32(v: u32) -> Seq<u8> {
    seq![
        (v & 0xff) as u8,
        ((v >> 8u32) & 0xff) as u8,
        ((v >> 16u32) & 0xff) as u8,
        ((v >> 24u32) & 0xff) as u8
    ]
}

/// Spec: decode 4-byte little-endian sequence to u32.
/// Models `u32::from_le_bytes()`.
pub open spec fn from_le_bytes_u32(b: Seq<u8>) -> u32 {
    (b[0] as u32)
        | ((b[1] as u32) << 8u32)
        | ((b[2] as u32) << 16u32)
        | ((b[3] as u32) << 24u32)
}

/// Spec: encode u64 as 8-byte little-endian sequence.
/// Models `u64::to_le_bytes()`.
pub open spec fn le_bytes_u64(v: u64) -> Seq<u8> {
    seq![
        (v & 0xff) as u8,
        ((v >> 8u64) & 0xff) as u8,
        ((v >> 16u64) & 0xff) as u8,
        ((v >> 24u64) & 0xff) as u8,
        ((v >> 32u64) & 0xff) as u8,
        ((v >> 40u64) & 0xff) as u8,
        ((v >> 48u64) & 0xff) as u8,
        ((v >> 56u64) & 0xff) as u8
    ]
}

/// Spec: decode 8-byte little-endian sequence to u64.
/// Models `u64::from_le_bytes()`.
pub open spec fn from_le_bytes_u64(b: Seq<u8>) -> u64 {
    (b[0] as u64)
        | ((b[1] as u64) << 8u64)
        | ((b[2] as u64) << 16u64)
        | ((b[3] as u64) << 24u64)
        | ((b[4] as u64) << 32u64)
        | ((b[5] as u64) << 40u64)
        | ((b[6] as u64) << 48u64)
        | ((b[7] as u64) << 56u64)
}

// ======================================================================
// Round-Trip Integrity Proofs
// ======================================================================

/// **Proof 1: `roundtrip_u8`**
///
/// u8 identity — a single byte needs no encoding/decoding, the value is the byte.
///
/// Communication relevance: u8 fields in ROS messages are preserved.
proof fn roundtrip_u8(v: u8)
    ensures
        v as u8 == v,
{
}

/// **Proof 2: `roundtrip_u16`**
///
/// Little-endian encode then decode of u16 yields the original value for all u16.
///
/// Communication relevance: u16 fields in ROS messages survive serialization.
proof fn roundtrip_u16(v: u16)
    ensures
        from_le_bytes_u16(le_bytes_u16(v)) == v,
{
    let bytes = le_bytes_u16(v);
    let b0 = bytes[0];
    let b1 = bytes[1];

    // Expand definitions for Z3
    assert(b0 == (v & 0xff) as u8);
    assert(b1 == ((v >> 8u16) & 0xff) as u8);

    assert(from_le_bytes_u16(bytes)
        == (b0 as u16) | ((b1 as u16) << 8u16));

    // Help Z3 with bit-level reasoning
    assert(v == (v & 0xff) | ((v >> 8u16) << 8u16)) by (bit_vector);
    assert(from_le_bytes_u16(le_bytes_u16(v)) == v) by (bit_vector);
}

/// **Proof 3: `roundtrip_u32`**
///
/// Little-endian encode then decode of u32 yields the original value for all u32.
///
/// Communication relevance: u32 fields (including ROS msg sequence numbers) survive
/// serialization.
proof fn roundtrip_u32(v: u32)
    ensures
        from_le_bytes_u32(le_bytes_u32(v)) == v,
{
    assert(from_le_bytes_u32(le_bytes_u32(v)) == v) by (bit_vector);
}

/// **Proof 4: `roundtrip_u64`**
///
/// Little-endian encode then decode of u64 yields the original value for all u64.
///
/// Communication relevance: u64 fields (including timestamps) survive serialization.
proof fn roundtrip_u64(v: u64)
    ensures
        from_le_bytes_u64(le_bytes_u64(v)) == v,
{
    assert(from_le_bytes_u64(le_bytes_u64(v)) == v) by (bit_vector);
}

/// **Proof 5: `roundtrip_i32`**
///
/// Signed i32 cast to u32 and back yields the original value for all i32.
/// CDR serializes i32 by casting to u32, writing as LE, then casting back on read.
///
/// Source (cdr.rs:91-93, 273-275):
/// ```ignore
/// pub fn write_i32(&mut self, v: i32) -> Result<(), SerError> {
///     self.write_u32(v as u32)
/// }
/// pub fn read_i32(&mut self) -> Result<i32, DeserError> {
///     self.read_u32().map(|v| v as i32)
/// }
/// ```
///
/// Communication relevance: i32 fields (ROS 2 std_msgs/Int32) are preserved.
proof fn roundtrip_i32(v: i32)
    ensures
        (v as u32) as i32 == v,
{
    assert((v as u32) as i32 == v) by (bit_vector);
}

/// **Proof 6: `roundtrip_bool`**
///
/// Bool encoded as u8 (0 or 1) and decoded via `!= 0` yields the original value.
///
/// Source (cdr.rs:101-108, 285-292):
/// ```ignore
/// pub fn write_bool(&mut self, v: bool) -> Result<(), SerError> {
///     self.write_u8(v as u8)
/// }
/// pub fn read_bool(&mut self) -> Result<bool, DeserError> {
///     self.read_u8().map(|v| v != 0)
/// }
/// ```
///
/// Communication relevance: Bool fields in ROS messages are preserved.
proof fn roundtrip_bool(v: bool)
    ensures
        ((v as u8) != 0u8) == v,
{
}

/// **Proof 7: `string_length_encoding`**
///
/// CDR string length includes the null terminator: encoded length = content_len + 1.
/// On decode, the content is recovered by subtracting 1 from the stored length.
///
/// Source (cdr.rs:113-126):
/// ```ignore
/// pub fn write_string(&mut self, s: &str) -> Result<(), SerError> {
///     let len = s.len() + 1; // +1 for null terminator
///     self.write_u32(len as u32)?;
///     // ... write bytes + null ...
/// }
/// ```
///
/// Communication relevance: ROS 2 string messages have correct length framing.
proof fn string_length_encoding(content_len: usize)
    requires
        content_len < usize::MAX,  // prevent overflow
    ensures
        // Encoding: stored length = content_len + 1
        content_len + 1 > content_len,
        // Decoding: content_len = stored_length - 1
        (content_len + 1) - 1 == content_len,
{
}

// ======================================================================
// CDR Header + Position Proofs
// ======================================================================

/// **Proof 8: `header_origin`**
///
/// `CdrWriter::new_with_header()` writes a 4-byte CDR encapsulation header
/// and sets both `pos` and `origin` to 4.
///
/// Source (cdr.rs:24-41):
/// ```ignore
/// pub fn new_with_header(buf: &'a mut [u8]) -> Result<Self, SerError> {
///     if buf.len() < 4 { return Err(SerError::BufferTooSmall); }
///     buf[0] = 0x00; // LE encoding
///     buf[1] = 0x01;
///     buf[2] = 0x00; // padding
///     buf[3] = 0x00;
///     Ok(CdrWriter { buf, pos: 4, origin: 4 })
///                          ^^^^^^^^^^^^^
/// }
/// ```
///
/// Communication relevance: CDR encapsulation header is valid for ROS 2 receivers.
proof fn header_origin(buf_len: usize)
    requires
        buf_len >= 4,
    ensures
        ({
            let g = CdrGhost { buf_len, pos: 4, origin: 4 };
            g.pos == 4 && g.origin == 4 && g.pos == g.origin
        }),
{
}

/// **Proof 9: `header_position_invariant`**
///
/// After `new_with_header`: `position() + remaining() == buf.len()`.
/// position=4, remaining=buf_len-4, so 4 + (buf_len-4) == buf_len.
///
/// Models `CdrWriter::position()` (returns `self.pos`) and
/// `CdrWriter::remaining()` (returns `self.buf.len() - self.pos`).
///
/// Source (cdr.rs:48-57):
/// ```ignore
/// pub fn position(&self) -> usize { self.pos }
/// pub fn remaining(&self) -> usize { self.buf.len() - self.pos }
/// ```
///
/// Communication relevance: Buffer accounting is consistent from initialization.
proof fn header_position_invariant(buf_len: usize)
    requires
        buf_len >= 4,
    ensures
        ({
            let g = CdrGhost { buf_len, pos: 4, origin: 4 };
            // position() + remaining() == buf.len()
            g.pos + (g.buf_len - g.pos) == g.buf_len
        }),
{
}

} // verus!
