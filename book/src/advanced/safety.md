# E2E Safety Protocol

> The `safety-e2e` feature described in this document has been implemented.
> It is available in `nros-rmw`, `nros-node`, and the top-level `nros` crate.

## Problem Statement

nano-ros treats zenoh as a trusted transport. Messages are serialized (CDR), transmitted over zenoh, and deserialized without any integrity verification. In safety-critical deployments (Autoware safety island), the transport channel must be treated as **untrusted** -- the EN 50159 "black channel" principle.

Without E2E protection:
- No CRC or checksum on message payloads (corruption undetected)
- No subscriber-side sequence tracking (message loss undetected)
- No duplicate detection (message repetition undetected)
- No freshness validation (stale messages accepted silently)
- No source authentication (masquerade undetected)

## Existing Infrastructure

The nano-ros transport layer carries metadata that partially addresses these concerns, but historically only on the publisher side.

### RMW Attachment (33 bytes)

Every published message includes a zenoh attachment with:

| Offset | Size | Field                       | Purpose                                |
|--------|------|-----------------------------|----------------------------------------|
| 0-7    | 8    | `sequence_number` (i64 LE)  | Monotonically increasing per publisher |
| 8-15   | 8    | `timestamp` (i64 LE, nanos) | Publication time                       |
| 16     | 1    | VLE length (always 16)      | GID size prefix                        |
| 17-32  | 16   | `rmw_gid`                   | Random per-publisher identifier        |

This exists for rmw_zenoh_cpp interoperability. Without `safety-e2e`, the subscriber parses it into `MessageInfo` but does not validate sequence continuity, freshness, or source identity.

### Message Data Flow

```
Publisher side:
  User msg -> CdrWriter::serialize() -> CDR payload (with 4-byte header)
  CDR payload -> ShimPublisher::publish_raw()
    -> Compute seq++, timestamp
    -> Serialize 33-byte RMW attachment
    -> zenoh publish(payload, attachment)

Subscriber side:
  zenoh callback -> Copy payload + attachment to static SubscriberBuffer
  User calls try_recv_raw() -> Copy from static buffer to user buffer
  User buffer -> CdrReader::deserialize() -> User msg
  (Attachment parsed into MessageInfo but NOT validated without safety-e2e)
```

### Key Observations

1. **Sequence numbers exist** but are only checked by subscribers when `safety-e2e` is enabled.
2. **Timestamps exist** but freshness validation requires a clock source (deferred).
3. **Publisher GID exists** but source authentication requires a registration mechanism.
4. **CRC is added by the `safety-e2e` feature.** It is computed over the CDR payload and appended to the attachment.
5. The attachment travels out-of-band from the CDR payload in zenoh. This is beneficial -- the CRC in the attachment provides a diverse check (different data paths).

## EN 50159 Threat Model

EN 50159 defines 7 threat classes for communication over untrusted channels:

| Threat           | EN 50159 Defense  | Integration Approach                                           |
|------------------|-------------------|----------------------------------------------------------------|
| **Corruption**   | CRC               | CRC-32 covering CDR payload, stored in extended attachment     |
| **Repetition**   | Sequence number   | Subscriber tracks expected sequence, flags duplicates          |
| **Deletion**     | Seq + timeout     | Subscriber detects sequence gaps                               |
| **Insertion**    | Seq + auth        | Sequence validation rejects unexpected messages                |
| **Resequencing** | Sequence number   | Subscriber validates monotonic sequence                        |
| **Delay**        | Timestamp/timeout | Subscriber compares message timestamp to current time          |
| **Masquerade**   | Authentication    | Subscriber validates expected source GID                       |

5 of 7 defenses only require subscriber-side validation of data that already exists in the attachment. Only CRC requires new publisher-side computation.

## CRC Architecture

The `safety-e2e` feature extends the zenoh attachment from 33 to 37 bytes:

```
Existing attachment (33 bytes):
  [seq:8][timestamp:8][vle:1][gid:16]

Extended attachment (37 bytes):
  [seq:8][timestamp:8][vle:1][gid:16][crc32:4]
```

The CRC covers the CDR payload bytes (not the attachment itself).

### Algorithm: CRC-32/ISO-HDLC

Standard Ethernet CRC (polynomial 0xEDB88320 reflected), used by both AUTOSAR E2E Profile 1/2 and EN 50159. The 1KB lookup table is `const`-generated at compile time. Deterministic execution time for WCET analysis.

### Interoperability

- rmw_zenoh_cpp reads exactly 33 bytes using VLE parsing -- extra bytes are ignored
- nano-ros without `safety-e2e` reads 33 bytes -- succeeds normally
- nano-ros with `safety-e2e` reads 37 bytes -- extracts CRC from bytes 33-36

## Subscriber-Side Validation

### Sequence Tracking

The subscriber maintains an `expected_seq` counter (initialized to -1). On each received message:

- **First message**: initializes `expected_seq` to `message_seq + 1`
- **Contiguous**: `message_seq == expected_seq` -- normal delivery
- **Duplicate**: `message_seq < expected_seq` -- flagged in `IntegrityStatus`
- **Gap**: `message_seq > expected_seq` -- gap count reported in `IntegrityStatus`

### CRC Validation

If the attachment is longer than 33 bytes, the subscriber extracts the CRC-32 from bytes 33-36 and compares it against a locally computed CRC of the received payload. If the attachment is 33 bytes (legacy or ROS 2 publisher), `crc_valid` is `None`.

## Feature Integration

The `safety-e2e` feature flag propagates through the crate hierarchy:

```
nros (top-level) -> nros-node -> nros-rmw
  safety-e2e        safety-e2e   safety-e2e
```

### Changes by layer

**`nros-rmw`** (core changes):
- `safety` module: CRC-32 function, `IntegrityStatus` type, `SafetyValidator` state tracker
- `ShimPublisher::publish_raw()`: computes CRC and extends attachment to 37 bytes
- `SubscriberBuffer`: attachment buffer sized to 37 bytes
- `ShimSubscriber`: `SafetyValidator` field, `try_recv_validated()` method

**`nros-node`** (API surface):
- `ShimNodeSubscription`: `try_recv_safe()` returning `(M, IntegrityStatus)`

**Unchanged**:
- CDR serialization (`nros-serdes`) -- payload format unchanged
- Core types (`nros-core`) -- no new traits needed
- Zenoh backend (`nros-rmw-zenoh`) -- attachment handling supports variable sizes
- Existing `try_recv()` API -- unchanged behavior

### Memory Impact

| Component                           | Without `safety-e2e` | With `safety-e2e`        |
|-------------------------------------|----------------------|--------------------------|
| CRC-32 lookup table                 | 0                    | +1024 bytes (.rodata)    |
| Subscriber attachment buffers (8x)  | 8 x 33 = 264 bytes   | 8 x 37 = 296 bytes (+32) |
| SafetyValidator per subscriber (8x) | 0                    | 8 x ~24 = 192 bytes      |
| **Total**                           | --                    | **+1248 bytes**          |

## ROS 2 Interoperability

| Sender               | Receiver             | CRC       | Sequence    | Result               |
|----------------------|----------------------|-----------|-------------|----------------------|
| nano-ros (safety)    | nano-ros (safety)    | Validated | Tracked     | Full E2E protection  |
| nano-ros (safety)    | nano-ros (no safety) | Ignored   | Not tracked | Works, no protection |
| nano-ros (safety)    | ROS 2 (rmw_zenoh)    | Ignored   | Not tracked | Works, no protection |
| nano-ros (no safety) | nano-ros (safety)    | None      | Tracked     | Partial (seq only)   |
| ROS 2 (rmw_zenoh)    | nano-ros (safety)    | None      | Tracked     | Partial (seq only)   |

No interoperability is broken. Safety degrades gracefully when one side doesn't support it.

## AUTOSAR E2E Comparison

| Feature       | AUTOSAR E2E P01      | AUTOSAR E2E P02 | nano-ros E2E                          |
|---------------|----------------------|-----------------|---------------------------------------|
| CRC           | CRC-8 (8-bit)        | CRC-8 (8-bit)   | CRC-32 (32-bit, stronger)             |
| Counter       | 4-bit (0-14)         | 4-bit (0-14)    | 64-bit (i64, no rollover in practice) |
| Data ID       | 16-bit               | 16-bit          | 128-bit GID (stronger)                |
| Alive counter | Yes                  | Yes             | Yes (via sequence)                    |
| Timeout       | Configurable         | Configurable    | Deferred (needs clock abstraction)    |
| State machine | E2E_SM               | Same            | Simpler (valid/invalid per message)   |

nano-ros E2E is stronger than AUTOSAR P01/P02 in CRC and counter width, but currently lacks the state machine and timeout features.

## Future Extensions

- **Freshness validation**: Requires a `MonotonicClock` trait and platform implementations. The existing `timestamp` field already carries the publication time.
- **Source authentication**: The `rmw_gid` (16-byte random publisher ID) can be used for masquerade detection if the subscriber registers expected sources.
- **Watchdog supervision**: Separate from E2E protocol -- monitors alive heartbeats, deadline completion, and logical execution flow.
- **Formal verification**: CRC correctness, sequence tracking invariants, and `IntegrityStatus::is_valid()` are candidates for Verus proofs.
