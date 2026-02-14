# E2E Safety Protocol Integration Analysis

> Date: 2026-02-14
> Prerequisite: `docs/research/safety-critical-platform-study.md`
> Status: Design analysis (not yet implemented)

## 1. Problem Statement

nano-ros treats zenoh as a trusted transport. Messages are serialized (CDR), transmitted over zenoh, and deserialized without any integrity verification. In safety-critical deployments (Autoware safety island), the transport channel must be treated as **untrusted** — the EN 50159 "black channel" principle.

Currently missing:
- No CRC or checksum on message payloads (corruption undetected)
- No subscriber-side sequence tracking (message loss undetected)
- No duplicate detection (message repetition undetected)
- No freshness validation (stale messages accepted silently)
- No source authentication (masquerade undetected)

## 2. Existing Infrastructure

The nano-ros transport layer already carries metadata that partially addresses these concerns, but only on the publisher side — the subscriber does not validate any of it.

### RMW Attachment (33 bytes, already implemented)

Every published message includes a zenoh attachment with:

| Offset | Size | Field                       | Purpose                                |
|--------|------|-----------------------------|----------------------------------------|
| 0-7    | 8    | `sequence_number` (i64 LE)  | Monotonically increasing per publisher |
| 8-15   | 8    | `timestamp` (i64 LE, nanos) | Publication time                       |
| 16     | 1    | VLE length (always 16)      | GID size prefix                        |
| 17-32  | 16   | `rmw_gid`                   | Random per-publisher identifier        |

This exists for rmw_zenoh_cpp interoperability. The subscriber parses it into `MessageInfo` but **does not validate** sequence continuity, freshness, or source identity.

### Message Data Flow

```
Publisher side:
  User msg → CdrWriter::serialize() → CDR payload (with 4-byte header)
  CDR payload → ShimPublisher::publish_raw()
    → Compute seq++, timestamp
    → Serialize 33-byte RMW attachment
    → zenoh publish(payload, attachment)

Subscriber side:
  zenoh callback → Copy payload + attachment to static SubscriberBuffer
  User calls try_recv_raw() → Copy from static buffer to user buffer
  User buffer → CdrReader::deserialize() → User msg
  (Attachment parsed into MessageInfo but NOT validated)
```

### Key Observations

1. **Sequence numbers exist** but are never checked by subscribers. Gap detection is trivial to add.
2. **Timestamps exist** but are never validated. Freshness checking requires a clock source.
3. **Publisher GID exists** but is never verified. Source authentication requires a registration mechanism.
4. **CRC is completely absent**. Must be added to detect payload corruption.
5. The attachment travels out-of-band from the CDR payload in zenoh. This is actually beneficial — the CRC in the attachment provides a diverse check (different data paths).

## 3. EN 50159 Threat Model Applied to nano-ros

EN 50159 defines 7 threat classes for communication over untrusted channels:

| Threat           | EN 50159 Defense  | nano-ros Current State        | Integration Approach                                           |
|------------------|-------------------|-------------------------------|----------------------------------------------------------------|
| **Corruption**   | CRC               | None                          | Add CRC-32 covering CDR payload, stored in extended attachment |
| **Repetition**   | Sequence number   | Seq exists, not checked       | Subscriber tracks expected sequence, flags duplicates          |
| **Deletion**     | Seq + timeout     | Seq exists, not checked       | Subscriber detects sequence gaps                               |
| **Insertion**    | Seq + auth        | Seq exists, not checked       | Sequence validation rejects unexpected messages                |
| **Resequencing** | Sequence number   | Seq exists, not checked       | Subscriber validates monotonic sequence                        |
| **Delay**        | Timestamp/timeout | Timestamp exists, not checked | Subscriber compares message timestamp to current time          |
| **Masquerade**   | Authentication    | GID exists, not checked       | Subscriber validates expected source GID                       |

**Key insight**: 5 of 7 defenses only require subscriber-side validation of data that already exists in the attachment. Only CRC requires new publisher-side computation.

## 4. Where CRC Fits in the Architecture

### Option A: Extend the zenoh attachment (recommended)

Append 4 bytes of CRC-32 after the existing 33-byte RMW attachment:

```
Existing attachment (33 bytes):
  [seq:8][timestamp:8][vle:1][gid:16]

Extended attachment (37 bytes):
  [seq:8][timestamp:8][vle:1][gid:16][crc32:4]
```

The CRC covers the CDR payload bytes (not the attachment itself).

**Interoperability**:
- rmw_zenoh_cpp reads exactly 33 bytes using VLE parsing → extra bytes ignored
- nano-ros without safety feature reads 33 bytes → `RmwAttachment::deserialize()` checks `buf.len() < RMW_ATTACHMENT_SIZE` and succeeds
- nano-ros with safety feature reads 37 bytes → extracts CRC from bytes 33-36

**Trade-offs**:
- (+) CDR payload format unchanged — standard ROS 2 messages
- (+) CRC travels a different data path than payload (diverse integrity)
- (+) Backwards-compatible with existing publishers/subscribers
- (-) Requires increasing `SubscriberBuffer.attachment` from 33 to 37 bytes (trivial: +32 bytes across 8 buffers)

### Option B: CRC as footer after CDR payload

Append CRC after the CDR data in the payload itself.

**Trade-offs**:
- (+) Self-contained, CRC travels with the data it covers
- (-) Breaks ROS 2 interop: rmw_zenoh subscribers see extra bytes after CDR data
- (-) CDR deserializer may fail or produce garbage from the CRC bytes
- (-) Not backwards-compatible

### Option C: Separate safety topic

Publish safety metadata on a parallel topic (e.g., `/chatter/_e2e`).

**Trade-offs**:
- (+) No modification to existing messages at all
- (-) Timing correlation between data and safety metadata is complex
- (-) Doubles zenoh topic count
- (-) Additional bandwidth and latency

**Recommendation**: Option A. The zenoh attachment is the natural out-of-band channel for metadata, already used for sequence/timestamp/GID. Adding CRC there is consistent with the existing pattern.

## 5. CRC Algorithm Selection

| Algorithm            | Table Size | Speed                  | Detection                                                               | Standard Usage                      |
|----------------------|------------|------------------------|-------------------------------------------------------------------------|-------------------------------------|
| CRC-8                | 256 bytes  | Fast                   | Weak (8-bit, misses multi-bit errors)                                   | I2C, 1-Wire                         |
| CRC-16/CCITT         | 512 bytes  | Fast                   | Good for short messages (<4KB)                                          | HDLC, X.25                          |
| **CRC-32/ISO-HDLC**  | 1024 bytes | Fast                   | Excellent (detects all 1-3 bit errors, most burst errors up to 32 bits) | **Ethernet, AUTOSAR E2E, EN 50159** |
| CRC-32C (Castagnoli) | 1024 bytes | Fast (hw accel on x86) | Excellent (better burst detection than ISO)                             | iSCSI, ext4                         |

**Recommendation**: CRC-32/ISO-HDLC (polynomial 0xEDB88320 reflected). Standard Ethernet CRC, used by both AUTOSAR E2E Profile 1/2 and EN 50159. 1KB lookup table is acceptable for embedded (nano-ros already uses 8KB for subscriber buffers). Deterministic execution time for WCET analysis.

The lookup table can be `const`-generated at compile time, requiring no runtime initialization.

## 6. Subscriber-Side Validation Logic

### Sequence Tracking

```
State: expected_seq (i64), initialized to -1 (no message received)

On receive(message_seq):
  if expected_seq == -1:
    // First message — no gap detection possible
    expected_seq = message_seq + 1
    return IntegrityStatus { gap: 0, duplicate: false }

  if message_seq == expected_seq:
    // Normal: contiguous delivery
    expected_seq = message_seq + 1
    return IntegrityStatus { gap: 0, duplicate: false }

  if message_seq < expected_seq:
    // Duplicate or resequenced (out-of-order)
    return IntegrityStatus { gap: 0, duplicate: true }

  if message_seq > expected_seq:
    // Gap: (message_seq - expected_seq) messages lost
    gap = message_seq - expected_seq
    expected_seq = message_seq + 1
    return IntegrityStatus { gap: gap, duplicate: false }
```

### CRC Validation

```
On receive(payload, attachment, attachment_len):
  if attachment_len <= 33:
    // No CRC in attachment (legacy or ROS 2 publisher)
    return IntegrityStatus { crc_valid: None }

  expected_crc = u32_from_le(attachment[33..37])
  actual_crc = crc32(payload)

  if actual_crc == expected_crc:
    return IntegrityStatus { crc_valid: Some(true) }
  else:
    return IntegrityStatus { crc_valid: Some(false) }
```

### Freshness Validation

Requires a monotonic clock source, which is platform-dependent:
- POSIX: `clock_gettime(CLOCK_MONOTONIC)`
- Zephyr: `k_uptime_get()`
- Bare-metal: DWT cycle counter (already in nano-ros BSPs)
- RTIC: monotonic timer

This is the most complex part because `no_std` has no standard clock API. A trait-based clock abstraction would be needed:

```
trait MonotonicClock {
    fn now_ms(&self) -> u64;
}
```

**Recommendation**: Defer freshness validation to a follow-up. CRC + sequence tracking provides strong integrity without a clock dependency.

## 7. Integration Approach

### Feature flag: `safety-e2e`

A compile-time feature flag on `nano-ros-transport` that:
- Publisher: computes CRC-32 over CDR payload, appends to attachment
- Subscriber: validates CRC, tracks sequences, reports `IntegrityStatus`
- Zero cost when disabled (no code compiled)

The feature propagates through the crate hierarchy:
```
nano-ros (top-level) → nano-ros-node → nano-ros-transport
  safety-e2e              safety-e2e       safety-e2e
```

### Changes by layer

**`nano-ros-transport`** (core changes):
- New `safety` module: CRC-32 function, `IntegrityStatus` type, `SafetyValidator` state tracker
- `ShimPublisher::publish_raw()`: behind `#[cfg(feature = "safety-e2e")]`, compute CRC and extend attachment from 33 to 37 bytes
- `SubscriberBuffer`: increase attachment buffer from 33 to 37 bytes when feature enabled
- `ShimSubscriber`: add `SafetyValidator` field, add `try_recv_validated()` method

**`nano-ros-node`** (API surface):
- `ShimNodeSubscription`: add `try_recv_safe()` method returning `(M, IntegrityStatus)` when feature enabled

**No changes to**:
- CDR serialization (`nano-ros-serdes`) — payload format unchanged
- Core types (`nano-ros-core`) — no new traits needed
- Transport traits (`traits.rs`) — shim-specific, not trait-level
- Zenoh backend (`nano-ros-transport-zenoh`) — attachment handling already supports variable sizes
- Existing `try_recv()` API — remains available, unchanged behavior

### Memory impact

| Component                           | Without `safety-e2e` | With `safety-e2e`        |
|-------------------------------------|----------------------|--------------------------|
| CRC-32 lookup table                 | 0                    | +1024 bytes (.rodata)    |
| Subscriber attachment buffers (8x)  | 8 × 33 = 264 bytes   | 8 × 37 = 296 bytes (+32) |
| SafetyValidator per subscriber (8x) | 0                    | 8 × ~24 = 192 bytes      |
| **Total**                           | —                    | **+1248 bytes**          |

Negligible for any embedded target (even Cortex-M0 has 16+ KB SRAM).

## 8. ROS 2 Interoperability Matrix

| Sender               | Receiver             | CRC                              | Sequence    | Result               |
|----------------------|----------------------|----------------------------------|-------------|----------------------|
| nano-ros (safety)    | nano-ros (safety)    | Validated                        | Tracked     | Full E2E protection  |
| nano-ros (safety)    | nano-ros (no safety) | Ignored (extra attachment bytes) | Not tracked | Works, no protection |
| nano-ros (safety)    | ROS 2 (rmw_zenoh)    | Ignored (extra attachment bytes) | Not tracked | Works, no protection |
| nano-ros (no safety) | nano-ros (safety)    | `crc_valid: None`                | Tracked     | Partial (seq only)   |
| ROS 2 (rmw_zenoh)    | nano-ros (safety)    | `crc_valid: None`                | Tracked     | Partial (seq only)   |

No interoperability is broken. Safety degrades gracefully when one side doesn't support it.

## 9. AUTOSAR E2E Comparison

AUTOSAR E2E defines 7 profiles (P01-P07) with varying protection:

| Feature       | AUTOSAR E2E P01                 | AUTOSAR E2E P02 | nano-ros E2E (proposed)               |
|---------------|---------------------------------|-----------------|---------------------------------------|
| CRC           | CRC-8 (8-bit)                   | CRC-8 (8-bit)   | CRC-32 (32-bit, stronger)             |
| Counter       | 4-bit (0-14)                    | 4-bit (0-14)    | 64-bit (i64, no rollover in practice) |
| Data ID       | 16-bit                          | 16-bit          | 128-bit GID (stronger)                |
| Alive counter | Yes                             | Yes             | Yes (via sequence)                    |
| Timeout       | Configurable                    | Configurable    | Deferred (needs clock abstraction)    |
| State machine | E2E_SM (init → valid → invalid) | Same            | Simpler (valid/invalid per message)   |

nano-ros E2E is stronger than AUTOSAR P01/P02 in CRC and counter width, but currently lacks the state machine and timeout features.

## 10. Future Extensions

### Freshness validation
Requires a `MonotonicClock` trait and platform implementations. Would add `age_ms` to `IntegrityStatus`. The existing `timestamp` field in the RMW attachment already carries the publication time.

### Source authentication
The `rmw_gid` (16-byte random publisher ID) can be used for masquerade detection if the subscriber registers expected sources. This is a policy decision, not a protocol change.

### Watchdog supervision (AUTOSAR WdgM)
Separate from E2E protocol. Monitors execution flow:
- **Alive**: periodic heartbeat (already exists in Autoware MRM)
- **Deadline**: task completion within time bound
- **Logical**: execution follows expected control flow graph

### Safety bag invariants
Application-layer rules (acceleration bounds, steering rate limits) that can be formally verified with Verus. Independent of E2E protocol.

### Formal verification
CRC correctness, sequence tracking invariants, and `IntegrityStatus::is_valid()` are all candidates for Verus proofs.
