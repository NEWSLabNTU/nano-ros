# AUTOSAR & ISO 26262 Gap Analysis for nano-ros

> Date: 2026-02-14
> Prerequisites: `docs/research/safety-critical-platform-study.md`, `docs/research/autoware-safety-island-architecture.md`
> Status: Research document (living)

## 1. Purpose

Map nano-ros's current capabilities against AUTOSAR and ISO 26262 requirements at each ASIL level (A through D). Identify specific gaps, prioritize them, and define actionable steps toward certification-readiness for the Autoware safety island use case.

This is not a certification assessment — it is an engineering gap analysis to guide development priorities.

## 2. Current nano-ros Safety Posture

### 2.1 Verification Infrastructure

| Tool | Scope | Count | Strength |
|------|-------|-------|----------|
| **Verus** | Scheduling, CDR, time, actions, params, E2E data path | 67 proofs | Unbounded (all inputs) |
| **Kani** | CDR, core types, params, C API | 82 harnesses | Bounded model checking |
| **Miri** | CDR, core types, params | 3 crates | UB detection (runtime) |
| **cargo-nextest** | Unit + integration | 435+ tests | Functional correctness |
| **Clippy** | All crates | Workspace-wide | Lint-level static analysis |

### 2.2 Architecture Strengths

- **Physical isolation**: Safety island runs on a separate MCU (STM32F4 / future ASIL-certified MCU). This is the strongest form of freedom from interference — no shared memory, no shared CPU.
- **`no_std` core**: All safety-critical code runs without heap allocation, without an OS scheduler (RTIC) or with a certifiable RTOS (Zephyr).
- **Deterministic communication**: CDR serialization is fixed-size, no dynamic allocation. zenoh-pico transport is bounded.
- **Formal verification**: Verus proofs cover scheduling guarantees, CDR roundtrip correctness, counter monotonicity, and E2E data path properties — exceeding what most embedded stacks provide.
- **Rust memory safety**: No data races, no use-after-free, no buffer overflows in safe Rust. `unsafe` is confined to FFI boundaries and hardware access.

### 2.3 Existing Safety-Relevant Features

| Feature | Status | Notes |
|---------|--------|-------|
| RMW attachment (seq, timestamp, GID) | Implemented | Publisher-side only; subscriber does not validate |
| DWT cycle counter infrastructure | Implemented | Hardware WCET measurement on Cortex-M |
| Ghost model validation | Implemented | 17 tests verify ghost models match production types |
| E2E data path proofs | Implemented | 10 Verus proofs covering publish → subscribe chain |
| Panic-freedom proofs | Implemented | Kani harnesses for CDR, core, params, C API |

## 3. ISO 26262 Requirements by ASIL Level

### 3.1 ASIL A — Low Risk

*Examples: rear wiper, interior lighting, non-safety telemetry*

| Requirement | ISO 26262 Reference | nano-ros Status | Gap |
|-------------|---------------------|-----------------|-----|
| Unit testing | Part 6, Table 9 | 435+ tests | **None** |
| Statement coverage | Part 6, Table 12 | Not measured | **Minor** — add `cargo-llvm-cov` |
| Requirements-based testing | Part 6, Table 9 | Phase docs trace features to tests | **Minor** — formalize traceability |
| Static analysis | Part 6, Table 7 | Clippy + Miri | **None** (Rust type system exceeds MISRA-C) |
| Development process documentation | Part 6, Clause 5 | Phase docs, CLAUDE.md | **Minor** — formalize into safety plan |

**Assessment: nano-ros meets or exceeds ASIL A requirements.** The only gaps are process documentation (formalizing what already exists).

### 3.2 ASIL B — Medium Risk

*Examples: headlights, windshield wipers, cruise control speed limiting*

| Requirement | ISO 26262 Reference | nano-ros Status | Gap |
|-------------|---------------------|-----------------|-----|
| Everything in ASIL A | — | See above | See above |
| Branch coverage | Part 6, Table 12 | Not measured | **Moderate** — add branch coverage tooling |
| Fault injection testing | Part 6, Table 15 | Not implemented | **Moderate** — test corrupted messages, dropped packets |
| Safety requirements specification | Part 6, Clause 6 | Informal (design docs) | **Moderate** — formalize safety requirements |
| Worst-case execution time | Part 6, Table 5 | DWT infra exists, no baselines | **Moderate** — collect and document WCET baselines |
| Software architectural design verification | Part 6, Table 3 | Design docs exist | **Minor** — add safety-specific architectural review |

**Assessment: nano-ros is close to ASIL B.** Key gaps are coverage measurement (branch coverage), WCET baselines, and formalized safety requirements. All are process/measurement tasks, not code changes.

### 3.3 ASIL C — High Risk

*Examples: cruise control actuation, electric power steering assist, active suspension*

| Requirement | ISO 26262 Reference | nano-ros Status | Gap |
|-------------|---------------------|-----------------|-----|
| Everything in ASIL B | — | See above | See above |
| MC/DC coverage (recommended) | Part 6, Table 12 | Not implemented | **Significant** — requires `cargo-llvm-cov` with MC/DC |
| Formal verification (highly recommended) | Part 6, Table 7 | Verus 67 + Kani 82 | **None** — exceeds ASIL C requirements |
| HARA (Hazard Analysis and Risk Assessment) | Part 3 | Not performed | **Significant** — requires domain-specific analysis |
| Safety case (recommended) | Part 10 | Not started | **Significant** — GSN/CAE structured argument |
| E2E communication protection | AUTOSAR E2E | Designed, not implemented | **Significant** — Phase 35.1-35.4 |
| Freedom from interference (FFI) | Part 6, Clause 7 | Physical isolation (separate MCU) | **None** — strongest form |
| Tool qualification (TCL 2) | Part 8 | Not assessed | **Moderate** — document tool confidence levels |

**Assessment: nano-ros has strong technical foundations for ASIL C but lacks process artifacts.** The formal verification exceeds requirements. The E2E protocol, HARA, safety case, and MC/DC coverage are the main gaps.

### 3.4 ASIL D — Highest Risk

*Examples: steering column lock, autonomous emergency braking, airbag deployment*

| Requirement | ISO 26262 Reference | nano-ros Status | Gap |
|-------------|---------------------|-----------------|-----|
| Everything in ASIL C | — | See above | See above |
| MC/DC coverage (required) | Part 6, Table 12 | Not implemented | **Critical** |
| Formal verification (recommended) | Part 6, Table 7 | Verus 67 + Kani 82 | **None** |
| Redundancy / diversity | Part 5, Clause 7 | Safety island = redundant path | **Partial** — need diverse implementation or monitoring |
| Back-to-back testing | Part 6, Table 10 | Not implemented | **Significant** — requires reference model |
| Certified RTOS | Part 6, Clause 7.4 | RTIC (compile-time) / Zephyr (in progress) | **Moderate** — Zephyr IEC 61508 upstream WIP |
| Certified compiler | Part 8 | rustc (unqualified) | **Critical** — no certified Rust compiler exists |
| Hardware safety mechanisms | Part 5 | STM32F4 (no lockstep) | **Critical** — need ASIL-certified MCU (AURIX, TMS570, S32K3) |
| Safety manager / watchdog | AUTOSAR WdgM pattern | Not implemented | **Significant** — need alive/deadline/logical supervision |
| Deterministic scheduling proof | Part 6, Table 5 | Verus scheduling proofs (18) | **Minor** — extend to actual task sets |

**Assessment: ASIL D requires certified hardware, certified toolchain, and extensive process artifacts that don't yet exist in the Rust embedded ecosystem.** nano-ros's verification depth is a strength, but the toolchain certification gap (rustc, LLVM) is an ecosystem-wide blocker, not specific to nano-ros.

## 4. AUTOSAR E2E Protocol Gap

### 4.1 Current State vs AUTOSAR E2E Profiles

| Feature | AUTOSAR E2E P01 | AUTOSAR E2E P02 | AUTOSAR E2E P04 | nano-ros (current) | nano-ros (proposed) |
|---------|-----------------|-----------------|-----------------|--------------------|--------------------|
| CRC | CRC-8 (SAE J1850) | CRC-8 (0x2F) | CRC-32 (Ethernet) | None | **CRC-32 (Ethernet)** |
| Counter | 4-bit (0-14) | 4-bit (0-14) | 16-bit | 64-bit (exists, unchecked) | **64-bit (validated)** |
| Data ID | 16-bit | 16-bit | 32-bit | 128-bit GID (exists, unchecked) | **128-bit GID (future)** |
| Timeout monitoring | Yes | Yes | Yes | Timestamp exists, unchecked | **Deferred** (needs clock trait) |
| State machine | INIT → VALID → INVALID | Same | Same | None | **Per-message valid/invalid** |
| Profile header | In-band (inside PDU) | In-band | In-band | **Out-of-band** (zenoh attachment) | Same |

**Key insight**: nano-ros's proposed E2E protection is stronger than AUTOSAR P01/P02 (CRC-32 vs CRC-8, 64-bit counter vs 4-bit) and comparable to P04. The out-of-band placement in the zenoh attachment provides diverse integrity (different data paths for payload and CRC) which is arguably better than in-band profiles.

### 4.2 EN 50159 Threat Coverage

| Threat | Defense | nano-ros Current | After Phase 35.1-35.4 |
|--------|---------|-------------------|----------------------|
| **Corruption** | CRC | Not covered | **CRC-32 in attachment** |
| **Repetition** | Sequence check | Seq exists, unchecked | **Duplicate detection** |
| **Deletion** | Seq + timeout | Seq exists, unchecked | **Gap detection** |
| **Insertion** | Seq + auth | Seq exists, unchecked | **Sequence validation** |
| **Resequencing** | Sequence number | Seq exists, unchecked | **Monotonic check** |
| **Delay** | Timestamp/timeout | Timestamp exists, unchecked | **Deferred** (needs clock abstraction) |
| **Masquerade** | Authentication | GID exists, unchecked | **Deferred** (needs registration) |

After Phase 35.1-35.4: 5 of 7 threats covered. Remaining 2 (delay, masquerade) require platform-dependent clock and policy-dependent source registration, respectively.

## 5. Tool Qualification Assessment

ISO 26262 Part 8 classifies tools by their potential to introduce or fail to detect errors:

| Tool | TCL (Tool Confidence Level) | Purpose | Qualification Path |
|------|----------------------------|---------|-------------------|
| **rustc + LLVM** | TCL 3 (can introduce errors) | Compilation | No certified Rust compiler exists. Mitigation: extensive testing at binary level, comparison with `gcc-rs` (future) |
| **Kani (CBMC backend)** | TCL 2 (can fail to detect) | Bounded verification | Open-source, well-studied. Could achieve TCL 2 with usage documentation |
| **Verus (Z3 backend)** | TCL 2 | Unbounded verification | Z3 is formally grounded. Verus framework newer, less toolchain maturity |
| **Miri** | TCL 2 | UB detection | Official Rust tool, well-maintained |
| **cargo-nextest** | TCL 2 | Test execution | Wrapper around `libtest`; low risk of masking failures |
| **cargo-llvm-cov** | TCL 2 | Coverage measurement | Uses LLVM instrumentation; well-studied approach |
| **Clippy** | TCL 1 (cannot introduce errors) | Static analysis | Advisory only; no code transformation |

**Ecosystem gap**: The biggest toolchain qualification challenge is the Rust compiler itself. Unlike GCC (qualified via Qualification Support Kits from AdaCore, etc.) and IAR/Green Hills (pre-qualified), rustc has no qualification kit. The Ferrocene project (AdaCore + Ferrous Systems) provides a qualified Rust toolchain but at significant cost.

**Mitigation strategies**:
1. **Ferrocene**: Use the Ferrocene-qualified Rust toolchain for production builds
2. **Diverse compilation**: Compare rustc and gcc-rs output (when mature)
3. **Binary-level testing**: Test the compiled binary, not just source-level properties
4. **Object code verification**: Use Kani/CBMC at the GOTO-program level (post-compilation)

## 6. MC/DC Coverage Gap

ISO 26262 requires Modified Condition/Decision Coverage (MC/DC) for ASIL D unit tests. MC/DC means: every condition in every decision independently affects the decision outcome.

### Current state

- No coverage measurement at any level (statement, branch, or MC/DC)
- Rust's `cargo-llvm-cov` supports statement and branch coverage
- LLVM 18+ supports MC/DC instrumentation (`-Cinstrument-coverage=mcdc`)

### Gap closure path

1. **Immediate**: Add `cargo-llvm-cov` for statement + branch coverage on safety-critical crates (`nano-ros-serdes`, `nano-ros-core`, `nano-ros-transport/safety`)
2. **Near-term**: Enable MC/DC coverage when Rust stabilizes the LLVM MC/DC pass (tracking: rust-lang/rust#124032)
3. **Target**: 100% MC/DC on safety module (`safety.rs`), ≥90% branch coverage on CDR serialization

### MC/DC-relevant code

The safety module has clear MC/DC targets:

```
IntegrityStatus::is_valid():
  crc_valid != Some(false) && !is_duplicate && sequence_gap == 0
  → 3 conditions, 4 MC/DC test cases needed

SafetyValidator::validate():
  Multiple branches for attachment length, sequence comparison, CRC match
  → ~8 conditions across the function
```

## 7. WCET Analysis Gap

### Current state

- DWT (Data Watchpoint and Trace) cycle counter infrastructure exists on Cortex-M platforms
- No systematic WCET baselines have been collected
- No static WCET analysis tool integrated

### Gap closure path

| Step | Description | Tooling |
|------|-------------|---------|
| 1. Baseline collection | Measure key functions on STM32F4 (hardware) and QEMU-ARM | DWT cycle counter |
| 2. Documentation | Record WCET per function, per platform, per optimization level | Structured table |
| 3. Static analysis | Integrate a static WCET tool for Cortex-M | Platin (open-source) or OTAWA |
| 4. Certification-grade | Commercial WCET tool with formal soundness guarantee | AbsInt aiT or RapiTime |

### Key functions for WCET measurement

| Function | Expected Bound | Criticality |
|----------|---------------|-------------|
| `crc32(payload)` | O(n), n = payload size | High — called on every message |
| `CdrWriter::serialize()` | O(n), n = message fields | High — called on every publish |
| `CdrReader::deserialize()` | O(n), n = message fields | High — called on every receive |
| `SafetyValidator::validate()` | O(1) + CRC cost | High — called on every receive |
| `ShimPublisher::publish_raw()` | Fixed + serialize + CRC | High — publishing path |
| `ShimSubscriber::try_recv_raw()` | Fixed + memcpy | High — receiving path |
| `executor.spin_once()` | Depends on callback count | Medium — main loop |

### Challenge: Rust WCET analysis

Static WCET tools (aiT, Platin, OTAWA) operate on compiled binaries, not source code. They are language-agnostic but require:
- Control flow graph extraction from ELF binary
- Loop bound annotations (Rust monomorphization can generate complex CFGs)
- Cache/pipeline modeling for the target MCU

Rust-specific complications:
- Monomorphization generates duplicated function bodies (one per type instantiation)
- Panic paths add unexpected control flow (mitigated by Kani panic-freedom proofs)
- Iterator chains can generate complex inlined code (mitigated by using explicit loops in safety-critical paths)

## 8. Safety Case Gap

ISO 26262 Part 10 recommends (ASIL C) or requires (ASIL D) a structured safety case — a documented argument that the system is acceptably safe.

### Current state

No safety case exists. Safety arguments are implicit in design documents and verification results.

### Gap closure path

1. **GSN (Goal Structuring Notation)** argument structure:
   ```
   G1: nano-ros safety island provides ASIL D emergency stop
   ├── G1.1: Communication integrity is maintained (EN 50159)
   │   ├── E1: CRC-32 covers payload corruption
   │   ├── E2: Sequence tracking detects loss/repetition
   │   └── E3: Verus proofs verify protocol correctness
   ├── G1.2: Execution meets timing requirements
   │   ├── E4: WCET measurements on target hardware
   │   └── E5: Verus scheduling proofs
   ├── G1.3: Software is free from systematic faults
   │   ├── E6: 82 Kani panic-freedom harnesses
   │   ├── E7: 67 Verus unbounded proofs
   │   ├── E8: Miri UB detection
   │   └── E9: MC/DC coverage results
   └── G1.4: Freedom from interference
       ├── E10: Physical isolation (separate MCU)
       └── E11: no_std, no heap, no shared memory
   ```

2. **Tool**: Use an open-source GSN tool (e.g., `gsn2x`, Assurance Case Editor) or a commercial tool (NOR-STA, Adelard ASCE)

3. **Timing**: Safety case development should follow E2E protocol implementation and WCET baseline collection, as these provide the evidence.

## 9. Priority-Ordered Roadmap

Based on the gap analysis, the recommended priority order:

### Tier 1: Foundation (Phase 35.1–35.4)

| Priority | Gap | Effort | Impact |
|----------|-----|--------|--------|
| **P1** | E2E safety protocol (CRC + sequence) | 1-2 weeks | Enables AUTOSAR E2E compliance, covers 5/7 EN 50159 threats |
| **P2** | WCET baseline collection | 3-5 days | Establishes timing evidence for safety case |
| **P3** | Statement/branch coverage | 2-3 days | Baseline for MC/DC path, immediate visibility |

### Tier 2: Process (Phase 35.5–35.7)

| Priority | Gap | Effort | Impact |
|----------|-----|--------|--------|
| **P4** | Tool confidence level assessment | 1 week | Documents tool risk per ISO 26262 Part 8 |
| **P5** | Verus proofs for safety module | 1 week | Formal correctness of CRC + sequence logic |
| **P6** | Fault injection testing | 1-2 weeks | Validates E2E protocol against corrupted/dropped messages |

### Tier 3: Certification (future phases)

| Priority | Gap | Effort | Impact |
|----------|-----|--------|--------|
| **P7** | MC/DC coverage | Depends on LLVM/Rust stabilization | Required for ASIL D |
| **P8** | Safety case (GSN) | 2-4 weeks | Required for any certification claim |
| **P9** | HARA for safety island | 2-4 weeks | Required for ASIL assignment |
| **P10** | Certified toolchain (Ferrocene) | Commercial engagement | Required for ASIL C/D production |
| **P11** | ASIL-certified MCU migration | Hardware selection + BSP port | Required for ASIL D production |
| **P12** | Watchdog supervision (WdgM) | 1-2 weeks code + design | Required for AUTOSAR compliance |
| **P13** | Freshness/delay validation | 1 week + clock trait | Covers 6th EN 50159 threat |
| **P14** | Source authentication | 1 week + policy design | Covers 7th EN 50159 threat |

### Dependencies

```
P1 (E2E protocol) ──→ P5 (Verus proofs) ──→ P8 (Safety case)
P1 (E2E protocol) ──→ P6 (Fault injection)
P2 (WCET baselines) ──→ P8 (Safety case)
P3 (Coverage) ──→ P7 (MC/DC) ──→ P8 (Safety case)
P4 (TCL assessment) ──→ P10 (Certified toolchain)
P9 (HARA) ──→ P8 (Safety case)
```

## 10. ASIL Readiness Summary

| Level | Technical Readiness | Process Readiness | Overall |
|-------|--------------------|--------------------|---------|
| **ASIL A** | Exceeds requirements | Minor gaps (traceability, safety plan) | **Ready with minor documentation** |
| **ASIL B** | Strong (needs coverage + WCET baselines) | Moderate gaps (formal safety requirements) | **Achievable in ~1 month** |
| **ASIL C** | Strong (needs E2E protocol + MC/DC recommended) | Significant gaps (HARA, safety case, TCL) | **Achievable in ~3-6 months** |
| **ASIL D** | Partial (needs certified MCU + toolchain) | Major gaps (full safety case, certified tools) | **Depends on ecosystem maturity** |

nano-ros's technical position is unusually strong for an open-source embedded project — the combination of 67 unbounded proofs, 82 bounded proofs, UB detection, and `no_std` architecture is competitive with commercial safety stacks. The remaining gaps are primarily process artifacts (HARA, safety case, traceability) and ecosystem dependencies (certified Rust compiler, certified MCU).

## 11. Recommended Hardware Platform

### Selection Criteria

The target MCU must satisfy:
1. **ASIL D capable** — lockstep cores, safety-certified silicon
2. **ARM Cortex-M** — same architecture as current nano-ros STM32F4 port (`thumbv7em` target)
3. **Rust-compatible** — stable Rust target triple, standard `cargo build`
4. **Accessible** — dev boards available, free toolchain, reasonable cost
5. **Zephyr support** — upstream board definition for near-term RTOS path

### Platform Comparison

| Criterion | NXP S32K344 | TI TMS570LS12x | Infineon AURIX TC375 | Renesas RH850 | NXP S32K144 |
|-----------|-------------|----------------|---------------------|---------------|-------------|
| **Core** | Cortex-M7 160 MHz | Cortex-R4F 180 MHz | TriCore 300 MHz | V850E3 400 MHz | Cortex-M4F 112 MHz |
| **Safety** | **ASIL D** | **ASIL D / SIL 3** | **ASIL D** | **ASIL D** | ASIL B |
| **Lockstep** | Yes (configurable) | Yes (always-on) | Yes (1 of 3 cores) | Yes | No |
| **Rust target** | `thumbv7em` (stable) | `armebv7r` (nightly, BE) | None (proprietary) | None | `thumbv7em` (stable) |
| **PAC on crates.io** | No (SVD available) | No | GitHub only | No | **Yes** (`s32k144-pac`) |
| **Dev board cost** | ~$179 (MR-CANHUBK344) | ~$20-60 (LaunchPad) | ~$169 (ShieldBuddy) | ~$480 | ~$114 (EVB) |
| **Free toolchain** | Yes (GCC ARM) | Yes (GCC ARM) | No (HighTec commercial) | Limited | Yes (GCC ARM) |
| **Zephyr upstream** | **Yes** (MR-CANHUBK3) | No | No | No | **Yes** (S32K148EVB) |
| **Distance from STM32F4** | Close (M7 vs M4) | Moderate (big-endian) | Incompatible (TriCore) | Incompatible (V850) | **Nearest** (same M4F) |

### Recommendation: NXP S32K3 (S32K344)

**Primary target for ASIL D production.** The S32K344 is the most practical path from nano-ros's current STM32F4 platform to a safety-certified automotive MCU:

- **Same Rust target**: `thumbv7em-none-eabihf` — all nano-ros core code compiles unmodified
- **ASIL D lockstep**: Dual Cortex-M7 with hardware comparison; FCCU (Fault Collection and Control Unit) fires on mismatch
- **Automotive I/O**: 6x CAN-FD + 100BASE-T1 Ethernet on MR-CANHUBK344 board — directly relevant to Autoware vehicle bus
- **Zephyr upstream**: MR-CANHUBK3 board has full Zephyr support, matching nano-ros's Zephyr BSP path
- **NXP AUTOSAR MCAL**: Production-grade peripheral drivers available (C, for reference)

**What's needed for S32K3 support:**
1. Generate PAC from NXP SVD files using `svd2rust` (NXP provides CMSIS-Pack SVDs)
2. Create `nano-ros-platform-s32k3` BSP crate (same pattern as `nano-ros-platform-stm32f4`)
3. Write minimal HAL for GPIO, UART, Ethernet (or use Zephyr for peripheral access)

### Stepping Stone: NXP S32K144

**For ecosystem validation before committing to S32K3:**

- **Identical architecture**: Cortex-M4F — same as STM32F4, same Rust target
- **PAC exists**: `s32k144-pac` on crates.io (SVD-generated, community-maintained)
- **ASIL B**: No lockstep, but adequate for body electronics validation
- **Lower cost**: ~$114 dev board
- **Same NXP peripheral model**: FlexCAN, LPSPI, LPUART — validates NXP driver patterns before S32K3

### Platforms to Avoid

| Platform | Reason |
|----------|--------|
| **TI TMS570** | Big-endian (Cortex-R4F in BE32 mode). Rust BE targets are Tier 3 (nightly only). nano-ros CDR serialization is little-endian — byte swapping would be needed throughout. Practical deal-breaker. |
| **Renesas RH850** | Proprietary V850 ISA with no LLVM backend. No Rust target triple exists. Dead end for Rust. |
| **Infineon AURIX** | TriCore ISA requires HighTec's proprietary Rust compiler (commercial license). Cannot use standard `cargo build`. Interesting long-term if LLVM gains TriCore support, but impractical for open-source work today. |

### Migration Path

```
STM32F4 (current, QM, development)
  → S32K144 (validate NXP ecosystem, ASIL B, PAC exists)
    → S32K344 (production target, ASIL D, lockstep)
      → S32K344 + Ferrocene (certified Rust compiler, full ASIL D stack)
```

The S32K family maintains the ARM Cortex-M ecosystem throughout — same Rust target, same Zephyr support, same JTAG/SWD debugging. The Ferrocene-qualified Rust toolchain (supporting Cortex-M7) could eventually provide the certified compilation step for ISO 26262 Part 8 tool qualification.

## References

- ISO 26262:2018 Parts 1-12 — Road vehicles, Functional safety
- AUTOSAR E2E Protocol Specification R22-11 — End-to-end communication protection
- EN 50159:2010 — Railway applications, Safety-related communication
- IEC 61508:2010 — Functional safety of E/E/PE safety-related systems
- DO-178C — Software Considerations in Airborne Systems and Equipment Certification
- Ferrocene Language Specification — https://spec.ferrocene.dev/
- rust-lang/rust#124032 — MC/DC instrumentation tracking issue
- NXP S32K3 Product Page — https://www.nxp.com/products/processors-and-microcontrollers/s32-automotive-platform/s32k-auto-general-purpose-mcus
- NXP MR-CANHUBK344 — https://www.nxp.com/design/design-center/development-boards-and-designs/automotive-development-platforms/s32k-mcu-platforms/s32k344-evaluation-board-for-mobile-robotics-with-100baset1-and-six-can-fd:MR-CANHUBK344
