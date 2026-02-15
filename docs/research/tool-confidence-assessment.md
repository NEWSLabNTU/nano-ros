# Tool Confidence Level Assessment

> Date: 2026-02-15
> ISO 26262 Reference: Part 8 — Supporting processes, Clause 11 (Qualification of software tools)
> Prerequisites: `docs/research/autosar-iso26262-gap-analysis.md`
> Status: Initial assessment (living document)

## 1. Purpose

Classify each software tool used in the nano-ros development and verification workflow by its Tool Confidence Level (TCL) per ISO 26262:2018 Part 8, Table 4. Identify qualification gaps and propose mitigation strategies.

This assessment covers the 7 tools specified in the Phase 35.7 work items. It follows the methodology in ISO 26262 Part 8 §11.4.

## 2. ISO 26262 Part 8 Classification Framework

### 2.1 Tool Impact (TI)

Tool Impact classifies the potential consequence of a tool malfunction:

| Level   | Definition                                                              | Example                                     |
|---------|-------------------------------------------------------------------------|---------------------------------------------|
| **TI1** | Tool cannot introduce or fail to detect errors in a safety-related item | Documentation generators, text editors      |
| **TI2** | Tool can introduce or fail to detect errors in a safety-related item    | Compilers, test runners, verification tools |

### 2.2 Tool Error Detection (TD)

Tool Error Detection classifies the confidence that a tool malfunction will be detected:

| Level   | Definition                                              | Example                                                           |
|---------|---------------------------------------------------------|-------------------------------------------------------------------|
| **TD1** | High confidence: tool error will be detected            | Back-to-back test comparison, diverse tools producing same output |
| **TD2** | Medium confidence: some measures detect tool errors     | Code review of output, partial redundancy                         |
| **TD3** | Low confidence: no specific measures detect tool errors | Single tool with no output validation                             |

### 2.3 TCL Determination (Part 8, Table 4)

|           | **TD1** | **TD2** | **TD3** |
|-----------|---------|---------|---------|
| **TI1**   | TCL 1   | TCL 1   | TCL 1   |
| **TI2**   | TCL 1   | TCL 2   | TCL 3   |

- **TCL 1**: No qualification required. Normal usage is sufficient.
- **TCL 2**: Qualification required. Increased confidence from usage (§11.4.6), development process (§11.4.7), or validation (§11.4.8).
- **TCL 3**: Qualification strongly required. Most stringent evidence needed.

## 3. Tool Assessments

### 3.1 `rustc` (Rust Compiler)

| Attribute                | Value                                                                                                                                                                 |
|--------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Version**              | 1.93.0 (254b59607 2026-01-19)                                                                                                                                         |
| **LLVM backend**         | LLVM 19.x                                                                                                                                                             |
| **Use case**             | Compiles all Rust source code to machine code. Sole compiler for production firmware.                                                                                 |
| **Failure mode**         | Miscompilation: generates incorrect machine code from correct source. Could introduce any type of runtime error (incorrect logic, memory corruption, timing changes). |
| **Failure impact**       | **Critical** — compiler errors propagate silently to the binary. A miscompilation in the safety module could defeat E2E protection without any source-level evidence. |
| **Tool Impact**          | **TI2** — can introduce errors into the safety-related item                                                                                                           |
| **Tool Error Detection** | **TD3** — no systematic detection of miscompilation in current workflow                                                                                               |
| **TCL**                  | **TCL 3**                                                                                                                                                             |

**Existing qualification evidence:**
- Rust's extensive test suite (>20,000 tests in `rust-lang/rust` CI)
- LLVM's test suite and widespread industry usage (clang, Swift, etc.)
- Miri detects some categories of UB that could indicate miscompilation
- Kani operates at the GOTO-program level (partial independence from LLVM codegen)
- Rust's type system prevents many classes of errors at source level, reducing the compiler's "attack surface"

**Gaps:**
- No certified Rust compiler exists (unlike GCC with AdaCore/Green Hills qualification kits)
- No diverse compilation path (gcc-rs / gccrs is not yet production-ready)
- No object-code-level verification integrated into CI
- Miri runs on MIR (pre-LLVM), not on generated machine code

**Mitigation strategies:**
1. **Ferrocene** (recommended for production): The Ferrocene project (AdaCore + Ferrous Systems) provides an ISO 26262 / IEC 61508 qualified Rust toolchain. Use Ferrocene for production builds targeting ASIL C/D.
2. **Diverse compilation**: When `gccrs` matures, compile with both rustc and gccrs and compare binary behavior (back-to-back testing). This would improve TD from TD3 to TD1, reducing TCL from 3 to 1.
3. **Binary-level testing**: Run integration tests on the compiled binary (QEMU-based tests already do this for embedded targets). Extend to run the full nextest suite on release-optimized binaries.
4. **Object code review**: For the most critical functions (safety module, CDR serialization), inspect generated assembly (`cargo asm`) and verify key invariants hold at the machine code level. The `just show-asm` recipe supports this.
5. **Pinned toolchain**: Pin the Rust toolchain version in `rust-toolchain.toml` and qualify a specific version through extensive testing before upgrading.

---

### 3.2 `cargo-kani` (Kani Verifier)

| Attribute                | Value                                                                                                                                                                                                                                              |
|--------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Version**              | 0.x (latest via `cargo install kani-verifier`)                                                                                                                                                                                                     |
| **Backend**              | CBMC 6.8.0 (C Bounded Model Checker)                                                                                                                                                                                                               |
| **Use case**             | Bounded model checking on 4 crates: `nros-serdes`, `nros-core`, `nros-params`, `nros-c`. 82 harnesses proving panic-freedom, roundtrip correctness, and bounded behavior.                                                                          |
| **Failure mode**         | False negative: Kani reports "VERIFICATION SUCCESSFUL" for a harness that actually contains a reachable assertion violation or panic.                                                                                                              |
| **Failure impact**       | **High** — a false negative means a verified property does not actually hold. However, Kani is a *detection* tool (does not modify production code), so it cannot *introduce* errors — it can only fail to detect them.                            |
| **Tool Impact**          | **TI2** — can fail to detect errors in the safety-related item                                                                                                                                                                                     |
| **Tool Error Detection** | **TD2** — medium confidence due to: (1) CBMC is a well-studied tool with >20 years of academic and industrial use; (2) Verus provides diverse verification of overlapping properties; (3) runtime tests independently validate the same properties |
| **TCL**                  | **TCL 2**                                                                                                                                                                                                                                          |

**Existing qualification evidence:**
- CBMC has extensive academic publication record and is used in AWS, Microsoft, and other safety/security-critical contexts
- Kani is developed by AWS and has its own test suite (>1,000 tests)
- nano-ros uses Verus (independent tool, different backend) to verify overlapping properties — this provides diverse tool redundancy
- 82 harnesses have been stable across multiple toolchain upgrades, providing usage evidence

**Gaps:**
- No formal qualification kit for Kani/CBMC per ISO 26262 Part 8
- Bounded model checking inherently cannot prove unbounded properties (addressed by Verus)
- Intermittent CBMC `goto-cc` crashes (exit code 70) indicate toolchain fragility, though these are non-silent failures

**Mitigation strategies:**
1. **Diverse verification**: Continue maintaining both Kani and Verus, ensuring key properties are proven by both tools independently. A false negative would need to occur in *both* tools simultaneously.
2. **Kani test suite validation**: Run Kani's own test suite as part of toolchain qualification when upgrading versions.
3. **Usage documentation**: Document specific Kani versions tested, harness coverage scope, and known limitations (bounds on loops, array sizes).
4. **Bound justification**: For each harness using bounded loops or array sizes, document why the chosen bound is sufficient for the production use case.

---

### 3.3 `verus` (Verus Verifier)

| Attribute                | Value                                                                                                                                                                                                                                  |
|--------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Version**              | Latest release (installed via `just setup-verus`)                                                                                                                                                                                      |
| **Backend**              | Z3 SMT solver (Microsoft Research)                                                                                                                                                                                                     |
| **Use case**             | Unbounded deductive verification. 67 proofs across scheduling, CDR serialization, time arithmetic, GoalStatus state machine, parameter types, and E2E data path.                                                                       |
| **Failure mode**         | False positive: Verus reports "verified" for a proof that contains a logical error (e.g., unsound axiom, incorrect specification). Z3 bug: the SMT solver accepts an unsatisfiable formula as satisfiable (or vice versa).             |
| **Failure impact**       | **High** — a false positive means a proven property does not actually hold. Like Kani, Verus is a detection tool and cannot introduce errors into production code.                                                                     |
| **Tool Impact**          | **TI2** — can fail to detect errors (by incorrectly verifying a flawed property)                                                                                                                                                       |
| **Tool Error Detection** | **TD2** — medium confidence due to: (1) Z3 is the most widely used SMT solver with extensive testing; (2) Kani provides diverse verification of overlapping properties; (3) ghost type validation tests confirm specification accuracy |
| **TCL**                  | **TCL 2**                                                                                                                                                                                                                              |

**Existing qualification evidence:**
- Z3 has >15 years of development, thousands of academic citations, and industrial use at Microsoft, AWS, and others
- Verus specifications are validated against production types via 17 ghost model tests in `nros-ghost-types`
- Kani independently verifies overlapping properties (diverse tool redundancy)
- 67 proofs with no `assume` statements in proof bodies — all obligations are fully discharged

**Gaps:**
- Verus is a relatively young tool (first public release 2023) with less industrial maturity than CBMC
- `external_type_specification` linkage between Verus specs and production types relies on naming conventions, not machine-checked linking
- Z3 has known edge cases in nonlinear arithmetic and quantifier instantiation
- No formal qualification kit

**Mitigation strategies:**
1. **Ghost model validation**: Maintain the `nros-ghost-types` test suite that validates ghost type specifications match production behavior. This provides an independent check that specifications are correct.
2. **No assume policy**: Enforce the existing policy that proofs contain no `assume` statements, ensuring all proof obligations are discharged by Z3.
3. **Diverse verification**: Key properties (CDR roundtrip, counter monotonicity) are proven by both Verus and Kani. Discrepancies would indicate a tool bug.
4. **Version pinning**: Pin the Verus version and Z3 version, re-running all proofs after any upgrade.

---

### 3.4 `cargo-nextest` (Test Runner)

| Attribute                | Value                                                                                                                                                                                                                                                      |
|--------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Version**              | 0.9.126 (0850c6559 2026-02-04)                                                                                                                                                                                                                             |
| **Use case**             | Executes all unit tests (429+) and integration tests. Replaces `cargo test` with better parallelism, JUnit XML output, and test grouping.                                                                                                                  |
| **Failure mode**         | Test masking: nextest reports a test as passed when it actually failed (e.g., due to process management bug, output capture error, or incorrect exit code interpretation).                                                                                 |
| **Failure impact**       | **Moderate** — a masked test failure means a defect goes undetected. nextest does not modify production code.                                                                                                                                              |
| **Tool Impact**          | **TI2** — can fail to detect errors by masking test failures                                                                                                                                                                                               |
| **Tool Error Detection** | **TD2** — medium confidence due to: (1) nextest is a thin wrapper around `libtest` — it spawns test binaries and checks exit codes; (2) JUnit XML output provides independent audit trail; (3) CI environments also run tests, providing diverse execution |
| **TCL**                  | **TCL 2**                                                                                                                                                                                                                                                  |

**Existing qualification evidence:**
- nextest is widely used in the Rust ecosystem (>5M downloads)
- JUnit XML output is independently parseable and auditable
- Test failures in CI have been reliably detected and reported throughout nano-ros development
- nextest's own test suite validates process management, signal handling, and output capture

**Gaps:**
- No formal qualification kit
- nextest test grouping (`max-threads`) could theoretically mask concurrency-dependent failures if misconfigured

**Mitigation strategies:**
1. **JUnit XML audit**: The JUnit XML output (`target/nextest/default/junit.xml`) serves as an independently verifiable test execution record. Include this in release artifacts.
2. **Diverse execution**: Run tests with both nextest and standard `cargo test` periodically to detect any nextest-specific masking.
3. **Exit code verification**: nextest correctly propagates non-zero exit codes from test binaries. The `run-test.sh` wrapper provides independent PASS/FAIL verification for non-nextest tests.

---

### 3.5 `miri` (MIR Interpreter)

| Attribute                | Value                                                                                                                                                                                                                                  |
|--------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Version**              | 0.1.0 (a423f68a0d 2026-02-13)                                                                                                                                                                                                          |
| **Use case**             | Detects undefined behavior (UB) in Rust code at the MIR level. Runs on 3 crates: `nros-serdes`, `nros-core`, `nros-params`. Detects: use-after-free, out-of-bounds access, invalid alignment, data races, use of uninitialized memory. |
| **Failure mode**         | False negative: Miri fails to detect actual UB (e.g., UB pattern not yet modeled in Miri's abstract machine).                                                                                                                          |
| **Failure impact**       | **Moderate** — undetected UB could manifest as runtime misbehavior. However, Miri operates on MIR (pre-LLVM), so its coverage is limited to Rust-level UB, not LLVM codegen bugs.                                                      |
| **Tool Impact**          | **TI2** — can fail to detect errors in the safety-related item                                                                                                                                                                         |
| **Tool Error Detection** | **TD2** — medium confidence due to: (1) Miri is an official Rust project tool, maintained by the Rust team; (2) it is the most comprehensive UB detector for Rust; (3) Kani independently checks for UB via CBMC                       |
| **TCL**                  | **TCL 2**                                                                                                                                                                                                                              |

**Existing qualification evidence:**
- Miri is developed and maintained by the Rust project (official tool)
- Extensive use across the Rust ecosystem for UB detection
- Miri has its own test suite validating detection of each UB category
- Regular updates aligned with Rust nightly (tracks MIR changes)

**Gaps:**
- Miri cannot detect all forms of UB (e.g., some LLVM-level UB, hardware-specific behavior)
- Miri runs tests under interpretation (slower), limiting the scope of what can be tested
- FFI code (`unsafe extern "C"`) is only partially modeled — Miri cannot interpret C code called via FFI

**Mitigation strategies:**
1. **Complement with Kani**: Kani's CBMC backend independently checks for UB patterns, providing diverse detection.
2. **FFI boundary testing**: For FFI-heavy crates (`nros-c`, `zpico-sys`), use ASAN/MSAN/TSAN in addition to Miri for compiled-binary UB detection.
3. **Scope documentation**: Document which crates and which `unsafe` blocks are covered by Miri, and which require alternative UB detection.

---

### 3.6 `cargo-llvm-cov` (Coverage Measurement)

| Attribute                | Value                                                                                                                                                                                                                                            |
|--------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Version**              | Not yet integrated (planned)                                                                                                                                                                                                                     |
| **Backend**              | LLVM source-based code coverage instrumentation                                                                                                                                                                                                  |
| **Use case**             | Measures statement, branch, and (future) MC/DC coverage for safety-critical crates. Required for ISO 26262 Part 6, Table 12 compliance at ASIL B+ (branch) and ASIL D (MC/DC).                                                                   |
| **Failure mode**         | Incorrect coverage measurement: reports higher coverage than actual (e.g., dead code marked as covered, branches not instrumented).                                                                                                              |
| **Failure impact**       | **Moderate** — inflated coverage could mask untested code paths. Does not modify production code.                                                                                                                                                |
| **Tool Impact**          | **TI2** — can fail to detect errors by reporting incorrect coverage                                                                                                                                                                              |
| **Tool Error Detection** | **TD2** — medium confidence due to: (1) LLVM instrumentation is a well-studied approach used by clang/gcc; (2) coverage reports are human-reviewable (line-by-line annotated source); (3) known gaps (e.g., macro-generated code) are documented |
| **TCL**                  | **TCL 2**                                                                                                                                                                                                                                        |

**Existing qualification evidence:**
- LLVM source-based coverage is the standard approach for C/C++ and Rust
- `cargo-llvm-cov` is widely used (>3M downloads)
- Coverage reports are independently auditable HTML/LCOV output

**Gaps:**
- Not yet integrated into nano-ros CI
- MC/DC support requires LLVM 18+ and nightly Rust (`-Cinstrument-coverage=mcdc`) — not yet stabilized
- Coverage of `#[cfg(kani)]` and `#[cfg(test)]` code may inflate numbers

**Mitigation strategies:**
1. **Integration**: Add `cargo-llvm-cov` to the `just quality` pipeline for safety-critical crates (`nros-serdes`, `nros-core`, `nros-params`).
2. **Exclude test-only code**: Configure coverage to exclude `#[cfg(test)]` modules and `#[cfg(kani)]` harnesses from coverage denominators.
3. **Manual review**: For safety-critical modules, manually review coverage reports to confirm that all decision branches are exercised, not just lines.
4. **MC/DC tracking**: Track Rust stabilization of LLVM MC/DC instrumentation (rust-lang/rust#124032) for ASIL D compliance.

---

### 3.7 `qemu-system-arm` (QEMU ARM Emulator)

| Attribute                | Value                                                                                                                                                                                                                                                                                 |
|--------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Version**              | 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.27); Docker uses QEMU 7.2                                                                                                                                                                                                                           |
| **Use case**             | Executes bare-metal ARM Cortex-M3 firmware (MPS2-AN385 machine) for integration testing. Validates that compiled binaries run correctly on the target architecture, including CDR serialization, WCET measurement, and networked pub/sub communication.                               |
| **Failure mode**         | Emulation inaccuracy: QEMU does not perfectly model the target hardware (timing, peripheral behavior, interrupt latency). A test passing on QEMU could fail on real hardware, or vice versa.                                                                                          |
| **Failure impact**       | **Moderate** — QEMU tests validate functional correctness but not real-time timing properties. Timing-critical bugs may be masked by QEMU's non-cycle-accurate emulation.                                                                                                             |
| **Tool Impact**          | **TI2** — can fail to detect errors by masking hardware-specific behavior                                                                                                                                                                                                             |
| **Tool Error Detection** | **TD2** — medium confidence due to: (1) QEMU is the most widely used open-source emulator with >20 years of development; (2) physical hardware testing (STM32F4, ESP32-C3) provides diverse execution environment; (3) semihosting output provides independent PASS/FAIL verification |
| **TCL**                  | **TCL 2**                                                                                                                                                                                                                                                                             |

**Existing qualification evidence:**
- QEMU is used extensively in the embedded industry for pre-silicon testing
- MPS2-AN385 machine model is used by ARM for official Cortex-M3 testing
- nano-ros validates QEMU results against physical hardware (STM32F4) for key functionality
- Semihosting-based test output is independently verifiable

**Gaps:**
- QEMU is not cycle-accurate — WCET measurements on QEMU are approximate (DWT CYCCNT emulation is imperfect)
- TAP networking in QEMU 6.2 has known issues (Ubuntu); Docker with QEMU 7.2 is recommended
- Peripheral emulation (LAN9118, ESP32 WiFi) may not match physical hardware behavior exactly
- No formal qualification kit for QEMU

**Mitigation strategies:**
1. **Hardware-in-the-loop**: Validate QEMU test results against physical hardware (STM32F4 NUCLEO-F429ZI) for critical functionality. Use QEMU for CI and development; use hardware for release validation.
2. **WCET on hardware**: Collect production WCET baselines on physical hardware, not QEMU. Use QEMU WCET measurements only as approximate sanity checks.
3. **Version pinning**: Pin QEMU version (Docker image uses Debian bookworm = QEMU 7.2) and document known emulation limitations per machine model.
4. **Diverse emulation**: For ESP32-C3 targets, validate on both QEMU and physical hardware. The `just test-qemu-esp32` and physical board tests provide this diversity.

## 4. Summary Matrix

| Tool              | Version   | TI  | TD  | **TCL**   | Qualification Path                                     |
|-------------------|-----------|-----|-----|-----------|--------------------------------------------------------|
| `rustc`           | 1.93.0    | TI2 | TD3 | **TCL 3** | Ferrocene for production; diverse compilation (future) |
| `cargo-kani`      | latest    | TI2 | TD2 | **TCL 2** | Usage documentation + Verus diversity                  |
| `verus`           | latest    | TI2 | TD2 | **TCL 2** | Ghost model validation + Kani diversity                |
| `cargo-nextest`   | 0.9.126   | TI2 | TD2 | **TCL 2** | JUnit XML audit + diverse execution                    |
| `miri`            | 0.1.0     | TI2 | TD2 | **TCL 2** | Complement with Kani + ASAN for FFI                    |
| `cargo-llvm-cov`  | (planned) | TI2 | TD2 | **TCL 2** | Manual review + standard LLVM approach                 |
| `qemu-system-arm` | 6.2/7.2   | TI2 | TD2 | **TCL 2** | Hardware-in-the-loop validation                        |

## 5. Key Findings

### 5.1 Critical gap: `rustc` at TCL 3

The Rust compiler is the only tool at TCL 3, meaning it requires the most stringent qualification. This is an **ecosystem-wide** gap — no certified Rust compiler exists in the open-source ecosystem. The recommended mitigation path:

1. **Short-term**: Pin toolchain version, increase binary-level testing, inspect assembly for safety-critical functions
2. **Medium-term**: Adopt Ferrocene (qualified Rust toolchain by AdaCore + Ferrous Systems) for production builds
3. **Long-term**: Diverse compilation with `gccrs` when mature (reduces TD from TD3 to TD1, achieving TCL 1)

### 5.2 Strength: diverse verification tools

The combination of Kani (bounded, CBMC) and Verus (unbounded, Z3) provides **diverse tool redundancy** for verification. A false negative in one tool would need to be independently present in the other. This is a strong argument for TD2 classification (rather than TD3) for both tools.

### 5.3 All detection tools achieve TCL 2

All 6 non-compiler tools achieve TCL 2 or better. TCL 2 can be satisfied through:
- **Increased confidence from use** (§11.4.6): documented usage history, version control, known limitations
- **Development process evaluation** (§11.4.7): open-source development with CI, test suites, peer review
- **Validation of the tool** (§11.4.8): running tool test suites as part of qualification

For ASIL A/B applications, TCL 2 tools can typically be qualified through usage documentation alone.

## 6. Recommendations

### 6.1 Immediate actions (ASIL A/B readiness)

1. Document tool versions and usage scope in a version-controlled manifest
2. Add `cargo-llvm-cov` to the CI pipeline for statement + branch coverage
3. Create a tool validation checklist that runs tool self-tests on version upgrade

### 6.2 Medium-term actions (ASIL C readiness)

1. Evaluate Ferrocene as the production compiler for ASIL C+ targets
2. Integrate hardware-in-the-loop testing into the CI pipeline
3. Add ASAN/MSAN testing for FFI-heavy crates as complement to Miri
4. Document qualification rationale for each TCL 2 tool per §11.4.6

### 6.3 Long-term actions (ASIL D readiness)

1. Adopt Ferrocene or equivalent qualified Rust toolchain
2. Implement diverse compilation (rustc + gccrs back-to-back) when feasible
3. Obtain or develop tool qualification kits for Kani and Verus
4. Integrate MC/DC coverage measurement when Rust stabilizes support

## 7. Methodology Compliance

This assessment follows ISO 26262:2018 Part 8 §11.4:

- **§11.4.2** (Tool classification): Each tool is classified by TI and TD per Table 4
- **§11.4.3** (TCL determination): TCL derived from TI × TD matrix
- **§11.4.4** (Qualification need): TCL 1 tools need no qualification; TCL 2/3 tools require qualification measures
- **§11.4.5** (Qualification methods): For each TCL 2/3 tool, specific qualification methods are proposed (increased confidence from use, development process evaluation, or validation)
- **§11.4.6-8** (Qualification details): Gap and mitigation sections identify which §11.4.6/7/8 method applies per tool

The assessment will be reviewed and updated when:
- Tool versions are upgraded
- New tools are added to the workflow
- Target ASIL level changes
- Ferrocene or gccrs become viable alternatives
