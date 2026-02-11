# Phase 31: Verus Unbounded Deductive Verification

## Summary

Prove real-time scheduling guarantees, communication reliability properties, and core algorithm correctness for **all inputs** using [Verus](https://github.com/verus-lang/verus) SMT-based deductive verification. Complements Kani bounded model checking (Phase 30.4/30.5) — same properties, stronger guarantees.

## Context

Phase 30 established Kani verification with 82 proof harnesses across 4 crates. Kani proves properties up to a loop unwind bound — sufficient for bug finding but not for certification-grade assurance.

Verus proves properties for all executions, forever. For safety-critical deployments (ISO 26262, DO-178C contexts), unbounded proofs provide the strongest assurance. Kani and Verus are complementary:

|                      | Kani (Phase 30)                           | Verus (Phase 31)                                  |
|----------------------|-------------------------------------------|---------------------------------------------------|
| Approach             | Bounded model checking                    | Deductive verification (Z3 SMT)                   |
| Proof strength       | Up to unwind bound                        | **All inputs, unbounded**                         |
| Unsafe/FFI code      | Full support (raw pointers, `extern "C"`) | Cannot verify — uses `#[verifier::external_body]` |
| Specification burden | Low (harness + assertions)                | High (requires/ensures + proof hints)             |
| Integration          | `#[cfg(kani)]` — zero dependencies        | Separate crate — needs vstd + Verus toolchain     |
| Counterexamples      | Concrete failing input                    | "Verification failed" (no witness)                |

Kani stays for unsafe/FFI verification (nano-ros-c) and as a low-effort safety net. Verus adds unbounded proofs where Kani's bounds are a limitation.

## Architecture: Centralized Verification Crate

```
packages/verification/nano-ros-verification/
├── Cargo.toml          # edition = "2024", depends on nano-ros-* via path
├── rust-toolchain.toml # Verus-pinned rustc (currently 1.93.0)
└── src/
    ├── lib.rs          # Top-level imports
    ├── scheduling.rs   # Real-time scheduling proofs
    ├── communication.rs # Communication reliability proofs
    ├── cdr.rs          # CDR serialization correctness proofs
    ├── time.rs         # Duration/Time arithmetic proofs
    ├── action.rs       # GoalStatus state machine proofs
    └── params.rs       # ParameterValue + range proofs
```

### Why a separate crate (not in-crate like Kani)

- **Zero production impact** — no cfg flags, feature gates, or vstd references in production crates
- **Toolchain isolation** — Verus bundles its own modified rustc; excluded from workspace avoids conflicts
- **Cross-crate proofs** — properties spanning nano-ros-serdes + nano-ros-core + nano-ros-node live naturally in one place
- **Precedent** — matches the Asterinas [vostd](https://github.com/asterinas/vostd) pattern (OSDI 2024 best paper)

### Why edition 2024 works

Verus's `rust_verify` driver and internal crates already use edition 2024 (since rustc 1.93.0). The `--edition` pass-through in the driver defaults to 2021 only when no edition is specified — `cargo verus` reads `edition = "2024"` from Cargo.toml natively. This means the verification crate uses the same edition as nano-ros, with direct path dependencies and no cross-edition concerns.

### Workspace integration

```toml
# Root Cargo.toml — excluded from main workspace
exclude = [
    # ... existing excludes ...
    "packages/verification/nano-ros-verification",
]
```

```toml
# packages/verification/nano-ros-verification/Cargo.toml
[package]
name = "nano-ros-verification"
version = "0.1.0"
edition = "2024"

[package.metadata.verus]
verify = true              # Opt this crate into cargo verus verify

[lints.rust]
unexpected_cfgs = { level = "allow", check-cfg = ['cfg(verus_keep_ghost_body)', 'cfg(verus_keep_ghost)'] }

[dependencies]
vstd = "0.0.0-2026-02-08-0120"
nano-ros-serdes = { path = "../../core/nano-ros-serdes", default-features = false }
nano-ros-core = { path = "../../core/nano-ros-core", default-features = false }
nano-ros-params = { path = "../../core/nano-ros-params", default-features = false }
nano-ros-node = { path = "../../core/nano-ros-node", default-features = false, features = ["alloc"] }
```

```toml
# packages/verification/nano-ros-verification/rust-toolchain.toml
[toolchain]
channel = "1.93.0"
```

## Verification Approach: `assume_specification`

Contracts are declared on real nano-ros functions as trusted axioms, then proof functions verify mathematical properties:

```rust
use vstd::prelude::*;
use nano_ros_core::time::Duration;

verus! {

// Tell Verus about the external Duration type (pub fields become accessible)
#[verifier::external_type_specification]
pub struct ExDuration(nano_ros_core::time::Duration);

// Trusted contract on the real function (not re-implemented)
pub assume_specification[ Duration::from_nanos ](nanos: i64) -> (d: Duration)
    ensures
        0 <= d.nanosec < 1_000_000_000,
        d.sec == ((nanos / 1_000_000_000) as i32);

// Note: &self becomes a named parameter in assume_specification
pub assume_specification[ Duration::to_nanos ](self_: &Duration) -> (n: i64)
    ensures
        n == (self_.sec as i64) * 1_000_000_000i64 + (self_.nanosec as i64);

// Unbounded proof — holds for ALL Durations with valid nanosec
proof fn duration_to_nanos_bounded(d: Duration)
    requires
        0 <= d.nanosec < 1_000_000_000,
    ensures
        (d.sec as i64) * 1_000_000_000 + (d.nanosec as i64)
            >= i32::MIN as i64 * 1_000_000_000,
{
    // Z3 proves from integer arithmetic
}

} // verus!
```

## Proof Targets (~35 proofs)

Proofs are organized by what they guarantee to the application developer, not by source crate.

### Tier 1: Real-Time Scheduling Guarantees (~10 proofs)

These prove that the executor has **bounded, predictable behavior** — the prerequisite for WCET analysis and schedulability proofs. An embedded developer needs to know: "if I call `spin_once()`, how much work can it possibly do?"

**Timer correctness** (nano-ros-node `timer.rs`):

| Proof                             | Property                                                               | Real-time relevance                                                |
|-----------------------------------|------------------------------------------------------------------------|--------------------------------------------------------------------|
| `timer_saturation_safety`         | `elapsed_ms.saturating_add(delta_ms)` never panics for all u64         | No overflow crash in timer accumulation                            |
| `timer_oneshot_fires_once`        | OneShot: fire() → mode becomes Inert → update() returns false forever  | Safety-critical one-time actions can't repeat                      |
| `timer_repeating_drift_free`      | Repeating: `elapsed -= period` preserves excess → no cumulative drift  | Control loops fire at t=0, P, 2P, 3P... not t≈0, t≈P+ε, t≈2P+2ε... |
| `timer_repeating_elapsed_bounded` | After fire(), `elapsed_ms < period_ms` (excess is always < one period) | Timer state stays in a well-defined range                          |
| `timer_canceled_never_fires`      | `canceled == true → update() returns false` regardless of elapsed      | Canceled timers are truly dead                                     |

**Trigger and scheduling** (nano-ros-node `trigger.rs`, `executor.rs`):

| Proof                          | Property                                                                               | Real-time relevance                                          |
|--------------------------------|----------------------------------------------------------------------------------------|--------------------------------------------------------------|
| `trigger_gating_correctness`   | trigger false → only timers fire, subscriptions_processed == 0 ∧ services_handled == 0 | Trigger controls callback scheduling without starving timers |
| `trigger_any_semantics`        | `Any.evaluate(ready) ⟺ ∃i. ready[i]`                                                   | Scheduling condition is logically correct                    |
| `trigger_all_semantics`        | `All.evaluate(ready) ⟺ (len > 0 ∧ ∀i. ready[i])`                                       | Sensor fusion trigger works as documented                    |
| `trigger_monotonicity`         | `All` true → `Any` true (never the reverse)                                            | Condition hierarchy is consistent                            |
| `spin_once_result_consistency` | `any_work() ⟺ total() > 0` and `total() == subs + services + timers` (saturating)      | Callers can trust the result for scheduling decisions        |

### Tier 2: Communication Reliability Guarantees (~10 proofs)

These prove properties about message handling that applications depend on for correct communication.

**Serialization safety** (nano-ros-serdes `cdr.rs`):

| Proof                      | Property                                                                       | Communication relevance                                                  |
|----------------------------|--------------------------------------------------------------------------------|--------------------------------------------------------------------------|
| `serialize_never_corrupts` | Buffer overflow → `Err(BufferTooSmall)`, position unchanged (no partial write) | No silent data corruption in the serialization layer                     |
| `position_invariant`       | `position() + remaining() == buf.len()` after any operation                    | Buffer accounting is always consistent                                   |
| `position_monotonicity`    | Successful writes only advance position                                        | No backward seeks that could overwrite prior fields                      |
| `align_correctness`        | padding < alignment, result aligned, padding bytes are zero                    | Cross-platform CDR interoperability (alignment matters for ROS 2 compat) |

**Round-trip integrity** (nano-ros-serdes `cdr.rs`):

| Proof                                 | Property                                                          | Communication relevance                                    |
|---------------------------------------|-------------------------------------------------------------------|------------------------------------------------------------|
| `roundtrip_{u8,u16,u32,u64,i32,bool}` | `write(v); read() == v` for all values                            | Message content is preserved through serialize/deserialize |
| `string_roundtrip`                    | write_string + read_string preserves content and null-termination | String messages arrive intact                              |
| `header_origin`                       | `new_with_header` sets origin=4, pos=4 correctly                  | CDR encapsulation header is valid for ROS 2 receivers      |

**Resource capacity** (nano-ros-node, nano-ros-params):

| Proof                          | Property                                                                                  | Communication relevance                              |
|--------------------------------|-------------------------------------------------------------------------------------------|------------------------------------------------------|
| `capacity_enforcement`         | `subscriptions.len() >= MAX → create returns Err` (never silent overflow)                 | Resource exhaustion is always reported at setup time |
| `param_server_count_invariant` | `count == entries.filter(Some).count()` and `count <= MAX_PARAMETERS` after any operation | Parameter server bookkeeping is correct              |

### Tier 3: Core Algorithm Correctness (~15 proofs)

These underpin the tier 1 and 2 proofs — e.g., the timer drift proof relies on Duration arithmetic being correct.

**Duration/Time arithmetic** (nano-ros-core `time.rs`):

| Proof                           | Property                                        | Kani Bound → Verus                                         |
|---------------------------------|-------------------------------------------------|------------------------------------------------------------|
| `duration_from_nanos_roundtrip` | `to_nanos(from_nanos(n)) == n`                  | ±10B → **all i64**                                         |
| `duration_components_valid`     | `nanosec < 1e9` always                          | ±10B → **all i64**                                         |
| `time_add_sub_inverse`          | `(t + d) - d == t`                              | bounded → **unbounded**                                    |
| `time_ordering_consistent`      | `t1 < t2 ⟺ t1.to_nanos() < t2.to_nanos()`       | —                                                          |
| `time_from_nanos_bug`           | Formally demonstrates missing `.unsigned_abs()` | constrained non-negative → **proves failure for negative** |

**GoalStatus state machine** (nano-ros-core `action.rs`):

| Proof                      | Property                                                                                           |
|----------------------------|----------------------------------------------------------------------------------------------------|
| `terminal_active_disjoint` | `is_terminal ∧ is_active` is impossible for all variants                                           |
| `valid_status_exhaustive`  | `from_i8(s as i8) == Some(s)` for all 7 variants                                                   |
| `transition_validity`      | Valid transitions form a DAG (Accepted→Executing→{Succeeded,Aborted,Canceling}→{Canceled,Aborted}) |
| `from_i8_roundtrip`        | `from_i8(to_i8(s)) == Some(s)`                                                                     |

**Parameter types** (nano-ros-params `types.rs`):

| Proof                             | Property                                                |
|-----------------------------------|---------------------------------------------------------|
| `integer_range_contains_boundary` | `contains(from) ∧ contains(to)`, step divides interval  |
| `float_range_contains_boundary`   | Same for floating-point                                 |
| `parameter_value_roundtrip`       | `i64→ParameterValue→i64` identity, `bool→bool` identity |
| `parameter_value_type_tag`        | Each variant returns correct ParameterType discriminant |

## What Verus proves beyond Kani

| Property                         | Kani (Phase 30)          | Verus (Phase 31)              |
|----------------------------------|--------------------------|-------------------------------|
| Timer drift-free scheduling      | No Kani proof            | **Proved for all u64 inputs** |
| Timer oneshot fires exactly once | No Kani proof            | **State machine proof**       |
| Trigger gating correctness       | No Kani proof            | **Scheduling invariant**      |
| CDR align correctness            | offset ≤ 1024            | **All usize**                 |
| Duration from_nanos roundtrip    | ±10B nanos               | **All i64**                   |
| Time from_nanos bug              | Constrained non-negative | **Proves failure domain**     |
| GoalStatus FSM                   | Exhaustive enum          | **Transition system model**   |
| Serialization no-corruption      | Bounded buffer sizes     | **All buffer sizes**          |

## Running Verification

```bash
# Install Verus toolchain (downloads binary + required rustc)
just setup-verus

# Run Verus verification
just verify-verus

# Run both Kani and Verus
just verify-kani && just verify-verus
```

The `verify-verus` recipe adds `tools/` to PATH and runs `cargo verus verify` in the verification crate. Verus requires `tools/cargo-verus`, `tools/verus`, `tools/rust_verify`, and `tools/z3` — all downloaded by `just setup-verus`.

## Work Items

| ID   | Task                                            | Effort  | Status      |
|------|-------------------------------------------------|---------|-------------|
| 31.1 | Verus toolchain setup + crate scaffolding       | 0.5 day | **Done**    |
| 31.2 | Tier 1: Real-time scheduling proofs (~10)       | 1.5 day | Not started |
| 31.3 | Tier 2: Communication reliability proofs (~10)  | 1 day   | Not started |
| 31.4 | Tier 3: Core algorithm correctness proofs (~15) | 1.5 day | Not started |
| 31.5 | Integration + documentation                     | 2 hours | Not started |

### 31.1: Verus Toolchain Setup + Crate Scaffolding

**Tasks:**

1. Add `just setup-verus` recipe — downloads Verus binary from [GitHub releases](https://github.com/verus-lang/verus/releases) to `tools/verus`, makes it executable
2. Update `just setup` step 5 ("Installing cargo tools") to call `just setup-verus` alongside Kani
3. Create verification crate at `packages/verification/nano-ros-verification/`:
   - `Cargo.toml` — edition 2024, depends on vstd + nano-ros-{serdes,core,params,node} via path
   - `rust-toolchain.toml` — `channel = "1.93.0"`
   - `src/lib.rs` — top-level module declarations
   - Empty module stubs: `scheduling.rs`, `communication.rs`, `cdr.rs`, `time.rs`, `action.rs`, `params.rs`
4. Add `"packages/verification/nano-ros-verification"` to root `Cargo.toml` `exclude` list
5. Add `just verify-verus` recipe (see [Running Verification](#running-verification))
6. Write one smoke-test proof (e.g., `duration_from_nanos_roundtrip`) to validate the full toolchain pipeline

**Acceptance criteria:**

- [ ] `just setup-verus` downloads Verus binary; `./tools/verus --version` succeeds
- [ ] Verification crate compiles: `cd packages/verification/nano-ros-verification && cargo verus verify` exits 0
- [ ] Smoke-test proof passes (at least 1 `verified: 1` in output)
- [ ] `just quality` still passes (workspace not affected by excluded crate)
- [ ] `just verify-verus` runs end-to-end

### 31.2: Tier 1 — Real-Time Scheduling Proofs (~10)

**Depends on:** 31.1

**Tasks:**

1. Write `assume_specification` contracts for `TimerState::update()` and `TimerState::fire()` in `scheduling.rs`
2. Implement timer proofs:
   - `timer_saturation_safety` — `saturating_add` never panics for all u64
   - `timer_oneshot_fires_once` — OneShot fire → Inert → update returns false forever
   - `timer_repeating_drift_free` — excess preserved across fire, no cumulative drift
   - `timer_repeating_elapsed_bounded` — after fire, `elapsed_ms < period_ms`
   - `timer_canceled_never_fires` — canceled flag implies update returns false
3. Write `assume_specification` contracts for `TriggerCondition::evaluate()` in `scheduling.rs`
4. Implement trigger proofs:
   - `trigger_any_semantics` — `Any.evaluate(ready) ⟺ ∃i. ready[i]`
   - `trigger_all_semantics` — `All.evaluate(ready) ⟺ (len > 0 ∧ ∀i. ready[i])`
   - `trigger_monotonicity` — All true → Any true
   - `trigger_gating_correctness` — trigger false → only timers fire
5. Implement `spin_once_result_consistency` — `any_work() ⟺ total() > 0`

**Acceptance criteria:**

- [ ] All 10 proofs listed in the Tier 1 tables pass with `just verify-verus`
- [ ] Each proof function has `ensures` clauses matching the Property column
- [ ] No `assume` statements (other than `assume_specification` on external functions)

### 31.3: Tier 2 — Communication Reliability Proofs (~10)

**Depends on:** 31.1

**Tasks:**

1. Write `assume_specification` contracts for `CdrWriter::{write_u8, write_u16, write_u32, write_u64, write_i32, write_bool, write_string, align}` and `CdrReader::{read_u8, read_u16, read_u32, read_u64, read_i32, read_bool, read_string}` in `cdr.rs`
2. Implement serialization safety proofs in `communication.rs`:
   - `serialize_never_corrupts` — overflow → Err, position unchanged
   - `position_invariant` — `position() + remaining() == buf.len()` after any op
   - `position_monotonicity` — successful writes only advance position
   - `align_correctness` — padding < alignment, result aligned, zero-filled
3. Implement round-trip integrity proofs in `cdr.rs`:
   - `roundtrip_{u8,u16,u32,u64,i32,bool}` — write then read preserves value for all inputs
   - `string_roundtrip` — content + null-termination preserved
   - `header_origin` — `new_with_header` sets origin=4, pos=4
4. Implement resource capacity proofs in `communication.rs`:
   - `capacity_enforcement` — subscriptions at MAX → create returns Err
   - `param_server_count_invariant` — count bookkeeping correct after any op

**Acceptance criteria:**

- [ ] All 10 proofs listed in the Tier 2 tables pass with `just verify-verus`
- [ ] CDR round-trip proofs cover all primitive types (u8, u16, u32, u64, i32, bool)
- [ ] No `assume` statements (other than `assume_specification` on external functions)

### 31.4: Tier 3 — Core Algorithm Correctness Proofs (~15)

**Depends on:** 31.1

**Tasks:**

1. Implement Duration/Time arithmetic proofs in `time.rs`:
   - `duration_from_nanos_roundtrip` — `to_nanos(from_nanos(n)) == n` for all i64
   - `duration_components_valid` — `nanosec < 1_000_000_000` for all i64 input
   - `time_add_sub_inverse` — `(t + d) - d == t` unbounded
   - `time_ordering_consistent` — `t1 < t2 ⟺ t1.to_nanos() < t2.to_nanos()`
   - `time_from_nanos_bug` — proves failure domain for negative nanos without `.unsigned_abs()`
2. Implement GoalStatus state machine proofs in `action.rs`:
   - `terminal_active_disjoint` — `is_terminal ∧ is_active` impossible
   - `valid_status_exhaustive` — `from_i8(s as i8) == Some(s)` for all 7 variants
   - `transition_validity` — valid transitions form a DAG
   - `from_i8_roundtrip` — `from_i8(to_i8(s)) == Some(s)`
3. Implement parameter type proofs in `params.rs`:
   - `integer_range_contains_boundary` — `contains(from) ∧ contains(to)`, step divides interval
   - `float_range_contains_boundary` — same for f64
   - `parameter_value_roundtrip` — `i64→ParameterValue→i64` identity
   - `parameter_value_type_tag` — each variant returns correct discriminant

**Acceptance criteria:**

- [ ] All 15 proofs listed in the Tier 3 tables pass with `just verify-verus`
- [ ] Duration/Time proofs use unbounded quantifiers (not bounded like Kani)
- [ ] GoalStatus proofs cover all 7 variants exhaustively
- [ ] No `assume` statements (other than `assume_specification` on external functions)

### 31.5: Integration + Documentation

**Depends on:** 31.2, 31.3, 31.4

**Tasks:**

1. Update `just setup` banner text to mention Verus alongside Kani
2. Mark Phase 31 complete in `CLAUDE.md` phases table
3. Update Phase 30 doc: mark 30.9 cross-reference as complete
4. Document any Verus subset limitations discovered during implementation (append to Limitations section)
5. Verify full pipeline: `just verify-kani && just verify-verus`

**Acceptance criteria:**

- [ ] `just setup` mentions Verus installation in its banner
- [ ] `just verify-kani && just verify-verus` passes with all ~117 proofs (82 Kani + ~35 Verus)
- [ ] Phase documentation is up to date

## Setup Integration

The Verus toolchain is installed via `just setup-verus` and integrated into `just setup` step 5.

**What `setup-verus` does:**

1. Queries the GitHub API for the latest Verus release
2. Downloads the platform-specific zip (e.g., `verus-*-x86-linux.zip`)
3. Extracts to `tools/` (verus, cargo-verus, z3, rust_verify, vstd, builtin crates)
4. Installs the required Rust toolchain (currently 1.93.0) via `rustup`

**`just setup` integration** — step 5 calls `just setup-verus` after Kani (non-fatal on failure).

**`tools/` directory** is gitignored. The full Verus distribution is ~80 MB (includes Z3 solver, rust_verify, vstd source).

## Limitations

- High annotation burden (4:1 to 7:1 proof:code ratio)
- Cannot verify unsafe/FFI code (nano-ros-c stays with Kani)
- Verus supports a subset of Rust (no `dyn Trait`, limited complex borrowing)
- SMT solver can be unpredictable on complex proofs (timeouts)
- No C support — only applies to Rust code
- `cargo verus` is still maturing (known stability issues, fallback to direct binary)
- Transport layer (zenoh-pico FFI) is outside verification scope — Verus proves properties of nano-ros's own logic, not network behavior
- User callback execution time is unbounded by definition — proofs cover the framework, not application code

## References

- [Verus](https://github.com/verus-lang/verus) — deductive verification for Rust (CMU/MSR)
- [Verus Guide](https://verus-lang.github.io/verus/guide/) — official documentation
- [vstd on crates.io](https://crates.io/crates/vstd) — Verus standard library
- [vostd (Asterinas)](https://github.com/asterinas/vostd) — verified OS components with Verus (OSDI 2024)
- [Atmosphere](https://dl.acm.org/doi/10.1145/3731569.3764821) — verified microkernel built with Verus (SOSP 2025)
- [AutoVerus](https://dl.acm.org/doi/10.1145/3763174) — LLM-driven automated Verus proof generation (OOPSLA 2025)
- [Phase 30](phase-30-wcet-realtime-tooling.md) — Kani bounded model checking (82 harnesses)
