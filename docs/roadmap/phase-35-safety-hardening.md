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

| Step  | Description                                                    | Status   |
|-------|----------------------------------------------------------------|----------|
| 35.0a | Safety-critical platform study (automotive, railway, aviation) | Complete |
| 35.0b | E2E safety protocol integration analysis                       | Complete |
| 35.0c | AUTOSAR / ISO 26262 gap analysis                               | Complete |

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

**Work items:**
- [x] Add `safety-e2e` feature flag to `nros-rmw/Cargo.toml`
- [x] Create `nros-rmw/src/safety.rs` module, gated by `#[cfg(feature = "safety-e2e")]`
- [x] Implement `crc32_iso_hdlc(data: &[u8]) -> u32` with 1KB `const` lookup table (polynomial `0xEDB88320` reflected)
- [x] Define `IntegrityStatus { gap: i64, duplicate: bool, crc_valid: Option<bool> }` with `is_valid()` method
- [x] Define `SafetyValidator { expected_seq: i64 }` (~24 bytes) with `validate(message_seq, crc_result) -> IntegrityStatus`
- [x] Sequence logic: first message sets baseline; `msg_seq == expected` → normal; `msg_seq < expected` → duplicate; `msg_seq > expected` → gap
- [x] Unit tests: CRC-32 known vectors (empty, `"123456789"` → `0xCBF43926`), single-bit flip detection, sequence gap/duplicate/normal tracking

**Passing criteria:**
- [x] `cargo test -p nros-rmw --features safety-e2e` passes all CRC and validator tests
- [x] `cargo test -p nros-rmw` (without feature) still compiles and passes — no regressions
- [x] `just quality` passes
- [x] Module compiles under `no_std` (verified by embedded clippy)

### 35.2: Publisher-side CRC computation

In `ShimPublisher::publish_raw()`, behind `#[cfg(feature = "safety-e2e")]`:
- Compute CRC-32 over CDR payload bytes
- Append 4-byte CRC to existing 33-byte RMW attachment (→ 37 bytes)
- No change to CDR payload format

**Work items:**
- [x] Add `safety-e2e` feature flag to `nros-rmw-zenoh/Cargo.toml`, forwarding to `nros-rmw/safety-e2e`
- [x] In `ShimPublisher::publish_raw()`: compute `crc32_iso_hdlc(payload)` when feature enabled
- [x] Append 4-byte CRC (little-endian) after the existing 33-byte attachment (total: 37 bytes)
- [x] Existing non-safety path unchanged (33 bytes, no CRC overhead)
- [ ] Unit test: publish with safety-e2e, verify attachment is 37 bytes and trailing 4 bytes match `crc32_iso_hdlc(payload)` *(moved to 35.4a)*

**Passing criteria:**
- [x] `cargo test -p nros-rmw-zenoh --features safety-e2e` passes
- [x] `cargo test -p nros-rmw-zenoh` (without feature) passes — no regressions
- [x] Attachment format: bytes 0–32 unchanged, bytes 33–36 = LE CRC-32 of CDR payload
- [x] `just quality` passes

### 35.3: Subscriber-side validation

In `ShimSubscriber`, behind `#[cfg(feature = "safety-e2e")]`:
- Increase `SubscriberBuffer.attachment` from 33 to 37 bytes
- Add `SafetyValidator` field to `ShimSubscriber`
- Add `try_recv_validated()` method returning `(len, IntegrityStatus)`
- Existing `try_recv_raw()` unchanged (backward-compatible)

**Work items:**
- [x] Conditionally increase `SubscriberBuffer.attachment` array from 33 to 37 bytes when `safety-e2e` enabled
- [x] Add `SafetyValidator` field to `ShimSubscriber` (behind feature gate)
- [x] Implement `try_recv_validated(&mut self, buf: &mut [u8]) -> Option<(usize, IntegrityStatus)>`
- [x] Extract sequence number from attachment bytes 0–7, CRC from bytes 33–36
- [x] Recompute CRC over received CDR payload, compare with attachment CRC
- [x] Feed sequence + CRC result into `SafetyValidator::validate()`
- [x] Existing `try_recv_raw()` signature and behavior unchanged
- [ ] Unit test: mock subscriber buffer with known payload + CRC, verify `IntegrityStatus` fields *(moved to 35.4a)*
- [ ] Unit test: tampered CRC → `crc_valid == Some(false)` *(moved to 35.4a)*
- [ ] Unit test: sequence gap and duplicate detection via successive calls *(moved to 35.4a)*

**Passing criteria:**
- [x] `cargo test -p nros-rmw-zenoh --features safety-e2e` passes all validation tests
- [x] `cargo test -p nros-rmw-zenoh` (without feature) passes — `try_recv_raw()` unchanged
- [x] Memory delta: +32 bytes for attachment buffers (8 subscribers × 4 bytes) + 192 bytes for validators (8 × 24 bytes)
- [x] `just quality` passes

### 35.4: Node-level API

In `nros-node`, behind `#[cfg(feature = "safety-e2e")]`:
- Add `ShimNodeSubscription::try_recv_safe()` → `(M, IntegrityStatus)`
- Feature flag wiring: `nros → nros-node → nros-rmw`

**Work items:**
- [x] Add `safety-e2e` feature to `nros-node/Cargo.toml`, forwarding to `nros-rmw-zenoh/safety-e2e`
- [x] Add `safety-e2e` feature to `nros/Cargo.toml`, forwarding to `nros-node/safety-e2e`
- [x] Implement `ConnectedSubscription::try_recv_safe() -> Option<(M, IntegrityStatus)>` behind feature gate
- [x] Re-export `IntegrityStatus` and `SafetyValidator` from `nros-node` public API
**Passing criteria:**
- [x] `cargo check -p nros --features safety-e2e` compiles
- [x] `cargo test -p nros-node --features safety-e2e` passes
- [x] Feature disabled: no API surface change, no compile-time cost
- [x] `just quality` passes

### 35.4a: E2E Safety Integration Tests

Validate the full safety protocol stack end-to-end. Tests are split into two categories:
1. **In-process RMW-level tests** — exercise publisher CRC + subscriber validation using the shim API directly (no binaries to spawn)
2. **Binary-level E2E tests** — build talker/listener with `safety-e2e`, verify output patterns

**Test location:** `packages/testing/nros-tests/tests/safety_e2e.rs` (new test suite)

#### In-process RMW-level tests (requires `rmw` + `safety-e2e` features on nros-tests)

- [x] Add `safety-e2e` feature to `nros-tests/Cargo.toml` forwarding to `nros-rmw/safety-e2e` + `nros-rmw-zenoh/safety-e2e`
- [x] Add `[[test]] name = "safety_e2e"` entry
- [x] Test: **publish + validate roundtrip** — implemented as unit test in `shim.rs` via `validate_from_buffers()` helper (zenoh-pico single-session limitation prevents in-process pub/sub roundtrip)
- [x] Test: **sequential messages** — `test_safety_validate_sequential_messages` in `shim.rs`
- [x] Test: **attachment format** — `test_safety_attachment_format` in `shim.rs` verifies 37-byte layout and trailing CRC
- [x] Test: **CRC detects corruption** — `test_safety_validate_tampered_crc` and `test_safety_validate_tampered_payload` in `shim.rs`
- [x] Test: **backward compat** — `test_safety_validate_no_crc_interop` in `shim.rs` (33-byte attachment → `crc_valid == None`), plus binary-level `test_safety_talker_standard_listener`

#### Binary-level E2E tests (talker/listener with safety-e2e)

- [x] Add `safety-e2e` feature to `rs-talker/Cargo.toml` and `rs-listener/Cargo.toml` forwarding to `nros/safety-e2e`
- [x] In rs-listener: when `safety-e2e` enabled, call `try_recv_safe()` and print `IntegrityStatus` fields (e.g. `[SAFETY] seq_gap=0 dup=false crc=ok`)
- [x] Test: **full-stack E2E** — `test_safety_e2e_talker_listener` in `safety_e2e.rs`: zenohd + safety talker + safety listener, verifies ≥3 `crc=ok` messages with `seq_gap=0`
- [x] Test: **mixed mode** — `test_safety_talker_standard_listener` in `safety_e2e.rs`: safety talker + standard listener, verifies ≥2 messages received normally

#### Unit tests deferred from 35.2/35.3

- [x] Unit test (35.3): mock `SubscriberBuffer` with known payload + CRC attachment via `validate_from_buffers()` helper — `test_safety_validate_happy_path`
- [x] Unit test (35.3): tampered CRC in attachment → `crc_valid == Some(false)` — `test_safety_validate_tampered_crc`
- [x] Unit test (35.3): sequence gap (seq jumps from 1 to 5) and duplicate (same seq twice) — `test_safety_validate_sequence_gap`, `test_safety_validate_duplicate`

**Passing criteria:**
- [x] `just test-integration` passes with all safety-e2e tests (119 tests total)
- [x] In-process tests: 8 unit tests in `shim.rs` covering CRC roundtrip, sequence tracking, corruption detection, backward compat, attachment format
- [x] Binary-level tests: full-stack E2E passes with 3+ validated messages
- [x] Mixed-mode test proves no regression for non-safety subscribers
- [x] `just quality` passes

### 35.5: MC/DC coverage infrastructure

- Add `cargo-llvm-cov` with MC/DC flags to CI
- Establish baseline coverage for safety-critical modules
- Document coverage targets per ASIL level

**Work items:**
- [ ] Add `just coverage` recipe using `cargo llvm-cov` with `--mcdc` flag on nightly
- [ ] Target crates: `nros-rmw` (safety module), `nros-serdes` (CDR), `nros-core` (types)
- [ ] Generate HTML coverage report to `target/llvm-cov/html/`
- [ ] Record baseline branch + MC/DC coverage percentages for safety module
- [ ] Document ASIL coverage targets in `docs/research/autosar-iso26262-gap-analysis.md`: ASIL A (statement), ASIL B (branch), ASIL C/D (MC/DC)

**Passing criteria:**
- `just coverage` runs and produces HTML report without errors
- Safety module (`nros-rmw/src/safety.rs`) achieves ≥90% branch coverage
- Coverage targets documented per ASIL level

### 35.6: WCET baseline collection

- Collect DWT-based WCET measurements for key functions (CDR serialize/deserialize, CRC-32, subscription poll)
- Document baselines per platform (STM32F4, QEMU-ARM)
- Identify candidates for static WCET analysis (Platin / aiT)

**Work items:**
- [ ] Create WCET benchmark example in `examples/qemu/` using DWT cycle counter (Cortex-M3 CYCCNT)
- [ ] Measure: `crc32_iso_hdlc()` for 64/256/1024-byte payloads
- [ ] Measure: CDR serialize/deserialize for `std_msgs/Int32` and `ParameterValue`
- [ ] Measure: single `try_recv_validated()` call (poll + CRC + sequence check)
- [ ] Record min/max/mean cycles per function in a markdown table
- [ ] Document baselines in `docs/reference/wcet-baselines.md`
- [ ] Note candidates for static WCET tools (Platin / aiT / SWEET)

**Passing criteria:**
- WCET benchmark compiles and runs on QEMU MPS2-AN385 (`just test-qemu` or manual)
- All measured functions have documented cycle counts
- No function exceeds 100K cycles for typical payloads (sanity bound)

### 35.7: Tool confidence level assessment

- Document TCL (Tool Confidence Level) assessment per ISO 26262 Part 8
- Assess rustc, Kani, Verus, cargo-nextest, Miri
- Identify tool qualification gaps and mitigation strategies

**Work items:**
- [ ] Create `docs/research/tool-confidence-assessment.md`
- [ ] Classify each tool by ISO 26262 Part 8 Table 4 (TI1/TI2 × TD1/TD2/TD3 → TCL1/TCL2/TCL3)
- [ ] Tools to assess: `rustc`, `cargo-kani`, `verus`, `cargo-nextest`, `miri`, `cargo-llvm-cov`, `qemu-system-arm`
- [ ] For each tool: document version, use case, failure impact, existing qualification evidence
- [ ] Identify gaps: which tools need qualification kits, validation suites, or diversity arguments
- [ ] Propose mitigation strategies (diverse compilation, back-to-back testing, tool output validation)

**Passing criteria:**
- Assessment document exists with TCL classification for all 7 tools
- Each tool has clear gap/mitigation section
- Document reviewed for ISO 26262 Part 8 §11.4 compliance of methodology

### 35.8: Verus proofs for safety module

- CRC-32 determinism proof
- Single-bit detection proof (for any data, any flip position, CRC changes)
- Sequence counter monotonicity proof
- Gap detection completeness proof
- `IntegrityStatus::is_valid()` correctness proof (no false positives)

**Work items:**
- [ ] Add safety module ghost types to `nros-ghost-types`
- [ ] Proof: `crc32_iso_hdlc(data) == crc32_iso_hdlc(data)` (determinism — same input → same output)
- [ ] Proof: for all `data`, all `pos` in `0..data.len()*8`, flipping bit at `pos` changes CRC
- [ ] Proof: `SafetyValidator` increments `expected_seq` monotonically on normal messages
- [ ] Proof: gap detection is complete — if `msg_seq > expected_seq`, then `status.gap == msg_seq - expected_seq`
- [ ] Proof: `is_valid()` returns `true` iff `gap == 0 && !duplicate && crc_valid != Some(false)`
- [ ] All proofs pass `just verify-verus`

**Passing criteria:**
- `just verify-verus` passes with all 5 new proofs (72+ total proofs)
- No `assume` statements in proof bodies (all obligations discharged)
- Proofs cover the production `safety` module types (via ghost type specifications)

## Dependencies

- Phase 34 (RMW abstraction) runs in parallel but is independent. E2E protocol is at the transport level, below the RMW abstraction boundary.
- Phase 31 (Verus verification) provides the proof infrastructure for step 35.8.

## Prior Fixup Work (from earlier phases)

Several fixup items were resolved during the research and analysis for this phase:

| Item                      | Description                                                                      | Commit               |
|---------------------------|----------------------------------------------------------------------------------|----------------------|
| Ghost model validation    | Phase 31.10 — 17 new structural and behavioral tests for ghost models            | `729d569`            |
| Linker script fix         | Restore missing `mps2-an385.x` to `zpico-platform-mps2-an385` after BSP deletion | `2d90ee9`            |
| STM32F4 compile fix       | Fix compile errors in stm32f4-{polling,rtic} after package renaming              | `00381d1`            |
| Ghost type infrastructure | Create `nros-ghost-types` shared crate for verification                          | `b88d7e4`            |
| Ghost model strategy      | Design doc for validating ghost models against production types                  | `08ecc21`, `ac5c292` |
