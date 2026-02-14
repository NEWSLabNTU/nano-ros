# Phase 35: Safety Hardening & E2E Protocol

**Status: In Progress**

**Prerequisites:** Phase 31 (Verus verification), Phase 32 (platform/transport split)

**Design docs:**
- `docs/design/e2e-safety-protocol-integration.md` — EN 50159 E2E protocol integration analysis
- `docs/research/safety-critical-platform-study.md` — Cross-domain safety platform study
- `docs/research/autoware-safety-island-architecture.md` — Autoware safety island architecture

## Goal

Close the gap between nano-ros's current embedded ROS 2 implementation and the safety requirements of AUTOSAR E2E / ISO 26262 / EN 50159 for deployment in safety-critical systems (Autoware safety island, railway interlocking, drone flight control).

This phase covers:
1. **E2E safety protocol** — CRC-32 + sequence tracking + integrity validation
2. **AUTOSAR gap closure** — MC/DC coverage, WCET baselines, tool qualification assessment
3. **Safety documentation** — structured gap analysis, safety case groundwork

## Context

Phase 31 established 67 unbounded Verus proofs and 82 Kani harnesses. Phase 32 refactored the platform/transport architecture for clean separation. The safety-critical platform study (cross-domain analysis of automotive, railway, and aviation) identified convergent safety patterns:

- **Black channel principle** (EN 50159): treat transport as untrusted, validate at endpoints
- **E2E protection** (AUTOSAR): CRC + counter + Data ID per message
- **Safety monitor / command gate** pattern: independent component validates commands before actuators
- **Physical isolation**: separate MCU for safety functions (already in nano-ros architecture)

The E2E safety protocol is the highest-priority deliverable because it's the foundation for all safety-critical communication.

## Completed Work

| Step | Description | Status |
|------|-------------|--------|
| 35.0a | Safety-critical platform study (automotive, railway, aviation) | Complete |
| 35.0b | E2E safety protocol integration analysis | Complete |
| 35.0c | AUTOSAR / ISO 26262 gap analysis | Complete |

### 35.0a: Safety-Critical Platform Study

**Commit:** `3b9792b` — `docs/research/safety-critical-platform-study.md` (985 lines)

Cross-domain analysis covering:
- **Automotive**: AUTOSAR Classic/Adaptive, lockstep MCUs (AURIX TC3xx, TMS570, S32K3), ASIL decomposition, E2E protection profiles (P01-P07)
- **Railway**: EN 50128/50129, vital computers (SIMIS W 2oo3, Smartlock 400, EBI Lock 950, MicroLok II, CLEARSY), coded processors, EN 50159 communication safety
- **Aviation/Drone**: DO-178C DAL levels, triple-triple dissimilar architectures (B777, A320, A380), certified RTOS (PikeOS SIL 4, VxWorks 653, INTEGRITY-178), ARINC 653 partitioning
- **Cross-domain convergence**: safety monitor/bag/RTA pattern, black channel, WCET analysis, formal methods adoption

Key finding: all three domains converge on the same safety patterns, and nano-ros already has most of the technical foundations (verification, isolation, deterministic execution). The main gaps are process/documentation and the E2E protocol layer.

### 35.0b: E2E Safety Protocol Integration Analysis

**Output:** `docs/design/e2e-safety-protocol-integration.md`

Detailed integration design covering:
- Existing RMW attachment format (33 bytes: seq + timestamp + GID)
- CRC-32 placement: append to zenoh attachment (37 bytes total), preserving ROS 2 interop
- Subscriber-side validation logic (sequence gap detection, CRC check, freshness)
- Feature flag design (`safety-e2e` through crate hierarchy)
- Memory impact: +1248 bytes total (1KB CRC table + 32 bytes buffers + 192 bytes validators)
- ROS 2 interoperability matrix (5 sender/receiver combinations, all backward-compatible)
- AUTOSAR E2E P01/P02 comparison (nano-ros E2E is stronger in CRC width and counter range)

### 35.0c: AUTOSAR / ISO 26262 Gap Analysis

**Output:** `docs/research/autosar-iso26262-gap-analysis.md` (this phase)

Comprehensive gap analysis mapping nano-ros capabilities to ISO 26262 ASIL A through D requirements.

## Steps

### 35.1: Implement CRC-32 module

Add `safety` module to `nros-rmw` with:
- CRC-32/ISO-HDLC implementation (1KB const lookup table, `no_std`)
- `IntegrityStatus` type (CRC result, sequence gap, duplicate flag)
- `SafetyValidator` subscriber-side state tracker
- Unit tests (known vectors, bit-flip detection, sequence tracking)

Feature flag: `safety-e2e = []` in Cargo.toml.

### 35.2: Publisher-side CRC computation

In `ShimPublisher::publish_raw()`, behind `#[cfg(feature = "safety-e2e")]`:
- Compute CRC-32 over CDR payload bytes
- Append 4-byte CRC to existing 33-byte RMW attachment (→ 37 bytes)
- No change to CDR payload format

### 35.3: Subscriber-side validation

In `ShimSubscriber`, behind `#[cfg(feature = "safety-e2e")]`:
- Increase `SubscriberBuffer.attachment` from 33 to 37 bytes
- Add `SafetyValidator` field to `ShimSubscriber`
- Add `try_recv_validated()` method returning `(len, IntegrityStatus)`
- Existing `try_recv_raw()` unchanged (backward-compatible)

### 35.4: Node-level API

In `nano-ros-node`, behind `#[cfg(feature = "safety-e2e")]`:
- Add `ShimNodeSubscription::try_recv_safe()` → `(M, IntegrityStatus)`
- Feature flag wiring: `nros → nros-node → nros-rmw`

### 35.5: MC/DC coverage infrastructure

- Add `cargo-llvm-cov` with MC/DC flags to CI
- Establish baseline coverage for safety-critical modules
- Document coverage targets per ASIL level

### 35.6: WCET baseline collection

- Collect DWT-based WCET measurements for key functions (CDR serialize/deserialize, CRC-32, subscription poll)
- Document baselines per platform (STM32F4, QEMU-ARM)
- Identify candidates for static WCET analysis (Platin / aiT)

### 35.7: Tool confidence level assessment

- Document TCL (Tool Confidence Level) assessment per ISO 26262 Part 8
- Assess rustc, Kani, Verus, cargo-nextest, Miri
- Identify tool qualification gaps and mitigation strategies

### 35.8: Verus proofs for safety module

- CRC-32 determinism proof
- Single-bit detection proof (for any data, any flip position, CRC changes)
- Sequence counter monotonicity proof
- Gap detection completeness proof
- `IntegrityStatus::is_valid()` correctness proof (no false positives)

## Dependencies

- Phase 34 (RMW abstraction) runs in parallel but is independent. E2E protocol is at the transport level, below the RMW abstraction boundary.
- Phase 31 (Verus verification) provides the proof infrastructure for step 35.8.

## Prior Fixup Work (from earlier phases)

Several fixup items were resolved during the research and analysis for this phase:

| Item | Description | Commit |
|------|-------------|--------|
| Ghost model validation | Phase 31.10 — 17 new structural and behavioral tests for ghost models | `729d569` |
| Linker script fix | Restore missing `mps2-an385.x` to `nano-ros-platform-qemu` after BSP deletion | `2d90ee9` |
| STM32F4 compile fix | Fix compile errors in stm32f4-{polling,rtic} after package renaming | `00381d1` |
| Ghost type infrastructure | Create `nros-ghost-types` shared crate for verification | `b88d7e4` |
| Ghost model strategy | Design doc for validating ghost models against production types | `08ecc21`, `ac5c292` |
