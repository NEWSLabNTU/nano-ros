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
| Unsafe/FFI code      | Full support (raw pointers, `extern "C"`) | Cannot verify — THIR erasure crash on fn pointers |
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

- **THIR erasure crash** — Verus panics at `erase.rs:237` when encountering function pointers, `dyn Trait`, or closures during the THIR erasure phase. This runs *before* `#[verifier::external]` annotations are processed, so problematic items cannot be excluded. Production crates like `nano-ros-node` contain `fn(&[bool]) -> bool` (TriggerFn), `Box<dyn Fn(...)>` (Trigger::Boxed), and closures (`ready.iter().any(|&r| r)`). Even `#[cfg(not(feature = "verus"))]` exclusion fails because `[package.metadata.verus] verify = true` causes cargo-verus to compile deps through the Verus pipeline too.
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

## Verification Approach

Every proof falls into one of three trust levels, depending on how it connects to production code:

| Level | Mechanism | What's trusted | Strength |
|-------|-----------|----------------|----------|
| Formally linked | `assume_specification` + `external_type_specification` | The spec matches the impl (human audit of ~4 lines) | Strongest |
| Ghost model | Manual struct/enum mirror | Line-by-line correspondence with production source | Medium |
| Pure math | Arithmetic identities | Only the math itself | Weakest (no code link) |

### External type specifications (transparent vs opaque)

Types defined outside `verus! { }` must be registered with `external_type_specification`. The presence or absence of `external_body` controls Verus's access:

- **Without `external_body`** → **transparent**: Verus sees the full variant/field structure. Enums can be pattern-matched in specs and proofs. Structs with public fields are field-accessible. This is the preferred approach for types with public interfaces.
- **With `external_body`** → **opaque**: Verus treats the type as a black box. Cannot match on variants or access fields. Use for types with private fields, complex internals, or when you only need to pass them around.

### `assume_specification` (formally linked)

Axiomatically declares a contract on a production function. The contract is **trusted** — a human auditor must confirm the spec matches the implementation.

```rust
use vstd::prelude::*;
use nano_ros_node::TriggerCondition;
use nano_ros_core::time::Duration;

verus! {

// Transparent enum — Verus can match on Any, All, Always, One(usize)
#[verifier::external_type_specification]
pub struct ExTriggerCondition(TriggerCondition);

// Transparent struct — Verus can access pub fields (sec, nanosec)
#[verifier::external_type_specification]
pub struct ExDuration(nano_ros_core::time::Duration);

// Spec function that matches on enum variants (only possible with transparent type)
pub open spec fn trigger_eval_spec(cond: TriggerCondition, ready: Seq<bool>) -> bool {
    match cond {
        TriggerCondition::Any => exists|i: int| 0 <= i < ready.len() && ready[i],
        TriggerCondition::All => ready.len() > 0 && forall|i: int| 0 <= i < ready.len() ==> ready[i],
        TriggerCondition::Always => true,
        TriggerCondition::One(index) => if (index as int) < ready.len() { ready[index as int] } else { false },
    }
}

// Trusted contract: links production fn to verified spec
// Note: &self becomes self_: &Type (named parameter, not method syntax)
pub assume_specification[ TriggerCondition::evaluate ](
    self_: &TriggerCondition,
    ready: &[bool],
) -> (ret: bool)
    ensures
        ret == trigger_eval_spec(*self_, ready@);  // ready@ converts &[bool] to Seq<bool>

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

### Ghost models (medium trust)

For types with `pub(crate)` fields or behind feature gates that prevent import, create a manually maintained mirror:

```rust
verus! {

/// Ghost representation of TimerState (mirrors nano_ros_node::timer::TimerState).
pub struct TimerGhost {
    pub period_ms: u64,
    pub elapsed_ms: u64,
    pub mode: TimerModeGhost,
    pub canceled: bool,
}

/// Model of TimerState::update() — auditor compares with timer.rs:302-310.
pub open spec fn timer_update_ready(s: TimerGhost, delta_ms: u64) -> bool {
    if s.canceled || s.mode is Inert { false }
    else { sat_add(s.elapsed_ms, delta_ms) >= s.period_ms }
}

} // verus!
```

Ghost model correctness relies on manual comparison with the production source code. This is weaker than `assume_specification` because the mirror can drift from the real implementation.

## Proof Targets

Proofs are organized by what they guarantee to the application developer, not by source crate.

### Tier 1: Real-Time Scheduling Guarantees (16 proofs — Done)

These prove that the executor has **bounded, predictable behavior** — the prerequisite for WCET analysis and schedulability proofs. An embedded developer needs to know: "if I call `spin_once()`, how much work can it possibly do?"

**Timer correctness** — ghost model (nano-ros-node `timer.rs`):

| Proof                             | Property                                                               | Real-time relevance                                                | Trust |
|-----------------------------------|------------------------------------------------------------------------|--------------------------------------------------------------------|-------|
| `timer_saturation_safety`         | `elapsed_ms.saturating_add(delta_ms)` never panics for all u64         | No overflow crash in timer accumulation                            | Ghost |
| `timer_oneshot_fires_once`        | OneShot: fire() → mode becomes Inert → update() returns false forever  | Safety-critical one-time actions can't repeat                      | Ghost |
| `timer_repeating_drift_free`      | Repeating: `elapsed -= period` preserves excess → no cumulative drift  | Control loops fire at t=0, P, 2P, 3P... not t≈0, t≈P+ε, t≈2P+2ε... | Ghost |
| `timer_repeating_elapsed_bounded` | After fire(), `elapsed_ms < period_ms` (excess is always < one period) | Timer state stays in a well-defined range                          | Ghost |
| `timer_canceled_never_fires`      | `canceled == true → update() returns false` regardless of elapsed      | Canceled timers are truly dead                                     | Ghost |

**Trigger conditions** — formally linked (nano-ros-node `trigger.rs`):

| Proof                          | Property                                                                               | Real-time relevance                                          | Trust |
|--------------------------------|----------------------------------------------------------------------------------------|--------------------------------------------------------------|-------|
| `trigger_eval_spec_complete`   | Unified spec correctly dispatches to per-variant spec functions                        | Spec is complete and unambiguous                             | Linked |
| `trigger_any_semantics`        | `Any.evaluate(ready) ⟺ ∃i. ready[i]`                                                   | Scheduling condition is logically correct                    | Linked |
| `trigger_all_semantics`        | `All.evaluate(ready) ⟺ (len > 0 ∧ ∀i. ready[i])`                                       | Sensor fusion trigger works as documented                    | Linked |
| `trigger_monotonicity`         | `All` true → `Any` true (never the reverse)                                            | Condition hierarchy is consistent                            | Linked |
| `trigger_one_in_bounds`        | `One(i)` true → `i < len` (out-of-bounds always false)                                 | Index-based triggers can't access invalid handles            | Linked |
| `trigger_one_out_of_bounds`    | `One(i)` false for empty mask, regardless of index                                     | One-based trigger safe when no handles registered            | Linked |
| `trigger_any_empty_false`      | `Any` false for empty mask                                                             | No spurious wake when no handles registered                  | Linked |
| `trigger_all_empty_false`      | `All` false for empty mask                                                             | Empty mask can't satisfy All condition                       | Linked |
| `trigger_always_unconditional` | `Always` true for any mask (empty, partial, or full)                                   | Timer-only executors always process callbacks                | Linked |
| `trigger_gating_correctness`   | trigger false → only timers fire, subscriptions_processed == 0 ∧ services_handled == 0 | Trigger controls callback scheduling without starving timers | Math  |
| `spin_once_result_consistency` | `any_work() ⟺ total() > 0` and `total() == subs + services + timers` (saturating)      | Callers can trust the result for scheduling decisions        | Math  |

### Tier 2: Communication Reliability Guarantees (14 proofs — Done)

These prove properties about message handling that applications depend on for correct communication.

**CDR round-trip integrity** (`cdr.rs`, 9 proofs — math + ghost):

| Proof                       | Property                                                          | Trust | Communication relevance                                    |
|-----------------------------|-------------------------------------------------------------------|-------|------------------------------------------------------------|
| `roundtrip_u8`              | `v as u8 == v` identity (no encoding needed)                      | Math  | u8 fields in ROS messages are preserved                    |
| `roundtrip_u16`             | `from_le_bytes_u16(le_bytes_u16(v)) == v` for all u16             | Math  | u16 fields survive serialization                           |
| `roundtrip_u32`             | `from_le_bytes_u32(le_bytes_u32(v)) == v` for all u32             | Math  | u32 fields (sequence numbers) survive serialization        |
| `roundtrip_u64`             | `from_le_bytes_u64(le_bytes_u64(v)) == v` for all u64             | Math  | u64 fields (timestamps) survive serialization              |
| `roundtrip_i32`             | `(v as u32) as i32 == v` for all i32 (signed cast roundtrip)      | Math  | i32 fields (std_msgs/Int32) are preserved                  |
| `roundtrip_bool`            | `(v as u8 != 0) == v` for all bool                                | Math  | Bool fields in ROS messages are preserved                  |
| `string_length_encoding`    | CDR string length = `s.len() + 1` (null terminator), decode subtracts 1 | Math | ROS 2 string messages have correct length framing          |
| `header_origin`             | `new_with_header` sets pos=4, origin=4 for buf.len() >= 4         | Ghost | CDR encapsulation header is valid for ROS 2 receivers      |
| `header_position_invariant` | After header: `position() + remaining() == buf.len()`             | Ghost | Buffer accounting is consistent from initialization        |

**Serialization safety + alignment** (`communication.rs`, 4 proofs — math + ghost):

| Proof                       | Property                                                                       | Trust | Communication relevance                                              |
|-----------------------------|--------------------------------------------------------------------------------|-------|----------------------------------------------------------------------|
| `align_padding_bounded`     | `cdr_align_padding(...) < alignment` for alignment > 0                         | Math  | Alignment never writes more than `alignment - 1` padding bytes       |
| `align_result_aligned`      | After alignment, `(pos + padding - origin) % alignment == 0`                   | Math  | Cross-platform CDR interoperability (ROS 2 CDR spec)                 |
| `serialize_never_corrupts`  | If `remaining < needed`, ghost state unchanged (pos stays same)                | Ghost | No silent data corruption — error path preserves writer state        |
| `position_monotonicity`     | Successful write: new_pos > old_pos (advances by at least 1)                   | Ghost | No backward seeks that could overwrite prior fields                  |

**Resource capacity** (`communication.rs`, 1 proof — ghost):

| Proof                          | Property                                                                                       | Trust | Communication relevance                              |
|--------------------------------|------------------------------------------------------------------------------------------------|-------|------------------------------------------------------|
| `param_server_count_invariant` | declare (count < max): count + 1 <= max; remove (count > 0): count - 1 >= 0; count <= max always | Ghost | Parameter server bookkeeping is correct              |

### Tier 3: Core Algorithm Correctness (13 proofs — Done)

These underpin the tier 1 and 2 proofs — e.g., the timer drift proof relies on Duration arithmetic being correct.

**Duration/Time arithmetic** (`time.rs`, 7 proofs — linked + math):

| Proof                           | Property                                        | Trust  | Status |
|---------------------------------|-------------------------------------------------|--------|--------|
| `remainder_bounded`             | `\|n % 1e9\| < 1e9` for all i64                 | Linked | **Done** |
| `duration_to_nanos_bounded`     | `to_nanos` output in [i32::MIN*1e9, i32::MAX*1e9+999999999] | Linked | **Done** |
| `duration_from_nanos_roundtrip` | `(n/1e9)*1e9 + n%1e9 == n` for non-negative n   | Linked | **Done** |
| `duration_components_valid`     | `0 <= n%1e9 < 1e9` for non-negative n           | Math   | **Done** |
| `time_add_sub_inverse`          | `t + d - d == t` at both nanos and component level | Math | **Done** |
| `time_ordering_consistent`      | Lexicographic `(sec,ns) < ⟺ sec*1e9+ns <` when ns < 1e9 | Math | **Done** |
| `time_from_nanos_bug`           | Negative remainder + u32 cast > 999999999 (proves Time::from_nanos bug) | Math | **Done** |

**GoalStatus state machine** (`action.rs`, 4 proofs — linked):

| Proof                      | Property                                                                        | Trust  |
|----------------------------|---------------------------------------------------------------------------------|--------|
| `terminal_active_disjoint` | `!(is_terminal(s) && is_active(s))` for all 7 variants                          | Linked |
| `valid_status_exhaustive`  | `from_i8(0..6)` maps to correct variants; 7, -1 return None                     | Linked |
| `transition_validity`      | Valid transitions strictly decrease rank → DAG (no cycles)                       | Math   |
| `from_i8_roundtrip`        | `from_i8(to_i8(s)) == Some(s)` for all 7 variants                               | Linked |

**Parameter types** (`params.rs`, 4 proofs — linked + ghost):

| Proof                             | Property                                                  | Trust  |
|-----------------------------------|-----------------------------------------------------------|--------|
| `integer_range_contains_boundary` | `contains(min) ∧ contains(max)` when min <= max           | Linked |
| `float_range_contains_boundary`   | Same for floating-point (ghost model with int, no f64)    | Ghost  |
| `parameter_value_roundtrip`       | `Integer(v)` extracts to `v`, `Bool(v)` extracts to `v`   | Ghost  |
| `parameter_value_type_tag`        | All 10 variants map to correct ParameterType discriminant | Ghost  |

## What Verus proves beyond Kani

| Property                         | Kani (Phase 30)          | Verus (Phase 31)                     | Status |
|----------------------------------|--------------------------|--------------------------------------|--------|
| Timer drift-free scheduling      | No Kani proof            | **Proved for all u64 inputs** (ghost) | Done |
| Timer oneshot fires exactly once | No Kani proof            | **State machine proof** (ghost)       | Done |
| Trigger gating correctness       | No Kani proof            | **Scheduling invariant** (linked)     | Done |
| Trigger semantics (all 4 variants) | No Kani proof          | **Formally linked** via `assume_specification` | Done |
| Duration to_nanos bounded        | No Kani proof            | **All Durations** (linked)           | Done |
| CDR align correctness            | offset ≤ 1024            | **All usize**                        | Done |
| Duration from_nanos roundtrip    | ±10B nanos               | **All i64**                          | Done |
| Time from_nanos bug              | Constrained non-negative | **Proves failure domain**            | Done |
| GoalStatus FSM                   | Exhaustive enum          | **Transition system model**          | Done |
| Serialization no-corruption      | Bounded buffer sizes     | **All buffer sizes**                 | Done |

## Running Verification

```bash
# Install Verus toolchain (downloads binary + required rustc)
just setup-verus

# Run Verus verification (currently: 57 verified, 0 errors)
just verify-verus

# Run both Kani and Verus
just verify

# Or separately
just verify-kani && just verify-verus
```

The `verify-verus` recipe adds `tools/` to PATH and runs `cargo verus verify` in the verification crate. Verus requires `tools/cargo-verus`, `tools/verus`, `tools/rust_verify`, and `tools/z3` — all downloaded by `just setup-verus`.

See [docs/guides/verus-verification.md](../guides/verus-verification.md) for coding practices (type specifications, trust levels, pitfalls).

## Work Items

| ID   | Task                                            | Effort  | Status      |
|------|-------------------------------------------------|---------|-------------|
| 31.1 | Verus toolchain setup + crate scaffolding       | 0.5 day | **Done**    |
| 31.2 | Tier 1: Real-time scheduling proofs (16) + time smoke tests (2) | 1.5 day | **Done** (18 verified) |
| 31.3 | Tier 2: Communication reliability proofs (14)   | 1 day   | **Done** (14 proofs in cdr.rs + communication.rs) |
| 31.4 | Tier 3: Core algorithm correctness proofs (13)  | 1.5 day | **Done** (13 proofs in time.rs + action.rs + params.rs) |
| 31.5 | Integration + documentation                     | 2 hours | **Done**    |

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

**Status: Done**

**Acceptance criteria:**

- [x] `just setup-verus` downloads Verus binary; `./tools/verus --version` succeeds (v0.2026.02.06.4a2b93e)
- [x] Verification crate compiles: `cd packages/verification/nano-ros-verification && cargo verus verify` exits 0
- [x] Smoke-test proof passes (`remainder_bounded` + `duration_to_nanos_bounded` in `time.rs`)
- [x] `just quality` still passes (418 tests, Miri clean, QEMU examples build)
- [x] `just verify-verus` runs end-to-end (57 verified, 0 errors)

### 31.2: Tier 1 — Real-Time Scheduling Proofs (16) + Time Smoke Tests (2)

**Depends on:** 31.1 — **Status: Done** (18 verified, 0 errors)

**What was implemented:**

Timer proofs use **ghost models** (`TimerGhost`/`TimerModeGhost`) because `TimerState` has `pub(crate)` fields that cannot be accessed from an external crate. The ghost models mirror `timer.rs` field-by-field, with spec functions modeling `update()` and `fire()`.

Trigger proofs use **formally linked** `assume_specification` on `TriggerCondition::evaluate()`, combined with **transparent** `external_type_specification` (without `external_body`). This allows Verus to match on all 4 enum variants (`Any`, `All`, `Always`, `One(usize)`) in spec functions — the strongest trust level.

**Proofs in `scheduling.rs` (16):**

1. `timer_saturation_safety` — `saturating_add` never panics for all u64 (ghost)
2. `timer_oneshot_fires_once` — OneShot fire → Inert → update returns false forever (ghost)
3. `timer_repeating_drift_free` — excess preserved across fire, no cumulative drift (ghost)
4. `timer_repeating_elapsed_bounded` — after fire, `elapsed_ms < period_ms` (ghost)
5. `timer_canceled_never_fires` — canceled flag implies update returns false (ghost)
6. `trigger_eval_spec_complete` — unified spec correctly dispatches to per-variant specs (linked)
7. `trigger_any_semantics` — `Any ⟺ ∃i. ready[i]` (linked)
8. `trigger_all_semantics` — `All ⟺ (len > 0 ∧ ∀i. ready[i])` (linked)
9. `trigger_monotonicity` — All true → Any true (linked)
10. `trigger_one_in_bounds` — `One(i)` true → `i < len` (linked)
11. `trigger_one_out_of_bounds` — `One(i)` false for empty mask (linked)
12. `trigger_any_empty_false` — `Any` false for empty mask (linked)
13. `trigger_all_empty_false` — `All` false for empty mask (linked)
14. `trigger_always_unconditional` — `Always` true for any mask (linked)
15. `trigger_gating_correctness` — trigger false → only timers fire (math)
16. `spin_once_result_consistency` — `any_work() ⟺ total() > 0` (math)

**Proofs in `time.rs` (2):**

17. `remainder_bounded` — `|n % 1e9| < 1e9` for all i64 (linked)
18. `duration_to_nanos_bounded` — `to_nanos` output bounded (linked)

**Acceptance criteria:**

- [x] All 18 proofs pass with `just verify-verus`
- [x] Each proof function has `ensures` clauses matching the Property column
- [x] No `assume` statements (other than `assume_specification` on external functions)
- [x] `just quality` passes (workspace unaffected)

### 31.3: Tier 2 — Communication Reliability Proofs (14)

**Depends on:** 31.1 — **Status: Done** (14 proofs, 42 total verified)

**What was implemented:**

CDR round-trip proofs use **pure math** — spec functions model `to_le_bytes()`/`from_le_bytes()` with bit-vector reasoning (`by (bit_vector)`) to prove invertibility for all values. CDR structural proofs use **ghost models** (`CdrGhost`) because `CdrWriter`/`CdrReader` have **private fields** (`buf`, `pos`, `origin`) and lifetime parameters — they cannot use transparent `external_type_specification`. `SerError`/`DeserError` are registered as transparent types (simple pub enums with no private fields).

Alignment proofs use **nonlinear arithmetic** (`by (nonlinear_arith)`) to prove modular arithmetic properties about CDR padding. Parameter server capacity uses the `ParamServerGhost` model mirroring `ParameterServer`'s private `count` field.

**Proofs in `cdr.rs` (9):**

1. `roundtrip_u8` — u8 identity (math)
2. `roundtrip_u16` — LE encode/decode roundtrip for all u16 (math, bit_vector)
3. `roundtrip_u32` — LE encode/decode roundtrip for all u32 (math, bit_vector)
4. `roundtrip_u64` — LE encode/decode roundtrip for all u64 (math, bit_vector)
5. `roundtrip_i32` — signed cast `(v as u32) as i32 == v` for all i32 (math, bit_vector)
6. `roundtrip_bool` — bool encode/decode via u8 for all bool (math)
7. `string_length_encoding` — CDR string length = content_len + 1, decode subtracts 1 (math)
8. `header_origin` — `new_with_header` sets pos=4, origin=4 (ghost)
9. `header_position_invariant` — `position() + remaining() == buf.len()` after header (ghost)

**Proofs in `communication.rs` (5):**

10. `align_padding_bounded` — padding < alignment for all positions (math, nonlinear_arith)
11. `align_result_aligned` — `(new_pos - origin) % alignment == 0` after padding (math, nonlinear_arith)
12. `serialize_never_corrupts` — error path preserves writer state (ghost)
13. `position_monotonicity` — successful writes advance position (ghost)
14. `param_server_count_invariant` — declare increments, remove decrements, count <= max (ghost)

**Acceptance criteria:**

- [x] All 14 proofs pass with `just verify-verus` (42 total verified)
- [x] CDR round-trip proofs cover all primitive types (u8, u16, u32, u64, i32, bool)
- [x] No `assume` statements (other than `assume_specification` on external functions)
- [x] `just quality` passes (418 tests, workspace unaffected)

### 31.4: Tier 3 — Core Algorithm Correctness Proofs (13)

**Depends on:** 31.1 — **Status: Done** (13 proofs, 57 total verified)

**What was implemented:**

Duration/Time proofs use **formally linked** `assume_specification` on `Duration::from_nanos` and `Duration::to_nanos`, combined with **transparent** `external_type_specification` for both `Duration` and `Time`. The `from_nanos` spec was strengthened with a nanosec clause for non-negative inputs, enabling the round-trip proof. The `time_ordering_consistent` proof required **nonlinear arithmetic** hints (`by (nonlinear_arith)`) inside conditional branches to help Z3 with multiplication bounds. The `time_from_nanos_bug` proof formally demonstrates that `Time::from_nanos` produces invalid nanosec fields for negative inputs (missing `.unsigned_abs()`).

GoalStatus proofs use **transparent** `external_type_specification` (without `external_body`) on the `#[repr(i8)]` enum, allowing Verus to match on all 7 variants. Spec functions mirror `is_terminal()`, `is_active()`, and `from_i8()`, linked via `assume_specification`. The transition DAG proof uses a **ranking function** where every valid transition strictly decreases rank (Accepted=3 → Executing=2 → Canceling=1 → terminal=0).

ParameterValue proofs use **ghost models** because `ParameterValue` contains `heapless::Vec` and `heapless::String` types that Verus cannot import. `ParameterValueGhost` abstracts away heap-allocated payloads while preserving scalar variants (`Bool(bool)`, `Integer(i64)`). `FloatingPointRange` uses a ghost model with `int` fields because Verus has no `f64` support. `IntegerRange` and `ParameterType` are registered as **transparent** types with `assume_specification` on `IntegerRange::contains`.

**Proofs in `time.rs` (5 new, 7 total with 31.2):**

1. `duration_from_nanos_roundtrip` — Euclidean division identity for non-negative nanos (linked)
2. `duration_components_valid` — `0 <= n%1e9 < 1e9` for non-negative n (math)
3. `time_add_sub_inverse` — `t + d - d == t` at both nanos and component level (math)
4. `time_ordering_consistent` — lexicographic `(sec,ns) <` iff `sec*1e9+ns <` (math, nonlinear_arith)
5. `time_from_nanos_bug` — negative remainder + u32 cast > 999999999 (math)

**Proofs in `action.rs` (4):**

6. `terminal_active_disjoint` — `!(is_terminal(s) && is_active(s))` for all variants (linked)
7. `valid_status_exhaustive` — `from_i8(0..6)` maps correctly; 7, -1 return None (linked)
8. `transition_validity` — valid transitions strictly decrease rank → DAG (math)
9. `from_i8_roundtrip` — `from_i8(to_i8(s)) == Some(s)` for all 7 variants (linked)

**Proofs in `params.rs` (4):**

10. `integer_range_contains_boundary` — `contains(min) ∧ contains(max)` when min <= max (linked)
11. `float_range_contains_boundary` — same for float ranges (ghost, int model)
12. `parameter_value_roundtrip` — `Integer(v)` extracts to `v`, `Bool(v)` extracts to `v` (ghost)
13. `parameter_value_type_tag` — all 10 variants map to correct ParameterType discriminant (ghost)

**Acceptance criteria:**

- [x] All 13 proofs pass with `just verify-verus` (57 total verified)
- [x] Duration/Time proofs use unbounded quantifiers (not bounded like Kani)
- [x] GoalStatus proofs cover all 7 variants exhaustively
- [x] No `assume` statements (other than `assume_specification` on external functions)
- [x] `just quality` passes (418 tests, workspace unaffected)

### 31.5: Integration + Documentation

**Depends on:** 31.2 — **Status: Done**

Completed early alongside 31.2 because Verus patterns and limitations needed documentation immediately.

**What was done:**

1. Created `docs/guides/verus-verification.md` — coding practices guide covering type specifications, `assume_specification` syntax, ghost models, trust levels, pitfalls, and workflow
2. Updated `CLAUDE.md` — verification section, commands, phase status, doc index
3. Updated `MEMORY.md` — Verus patterns for session persistence
4. `just verify-verus` and `just quality` both pass

**Remaining items (completed after 31.3/31.4):**

- [x] ~~Update `just setup` banner text to mention Verus alongside Kani~~ — already done in 31.1
- [x] Mark Phase 31 complete in `CLAUDE.md` phases table
- [x] Update `CLAUDE.md` verification description (18 → 57 proofs)
- [x] Final pipeline check: `just verify-kani && just verify-verus`

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

### Fundamental

- **THIR erasure crash (`erase.rs:237`)** — Verus panics during `setup_verus_ctxt_for_thir_erasure` when encountering function pointers (`fn(&[bool]) -> bool`), `dyn Trait` (`Box<dyn Fn(...)>`), or closures (`.iter().any(|&r| r)`). This runs *before* `#[verifier::external]` annotations are processed. Consequence: **in-crate verification is not feasible** for production crates containing these constructs. The separate verification crate pattern is required, not optional.
- **`[package.metadata.verus] verify = true` propagation** — when a crate has `verify = true`, cargo-verus also attempts to compile its dependencies through the Verus pipeline. Adding `verify = true` to any production crate that transitively depends on types with fn pointers/closures will trigger the THIR erasure crash. Only the dedicated verification crate should have `verify = true`.
- **Private/`pub(crate)` fields are inaccessible** — types like `TimerState` (with `pub(crate)` fields), `CdrWriter`/`CdrReader` (with private fields + lifetime parameters), and `ParameterServer` (with private fields) cannot be registered as transparent types from an external verification crate. Ghost models (manual mirrors) are the only option, which is a weaker trust level.

### Practical

- High annotation burden (4:1 to 7:1 proof:code ratio)
- Cannot verify unsafe/FFI code (nano-ros-c stays with Kani)
- Verus supports a subset of Rust (no `dyn Trait`, limited complex borrowing)
- SMT solver can be unpredictable on complex proofs (timeouts)
- No C support — only applies to Rust code
- `cargo verus` is still maturing (known stability issues, fallback to direct binary)
- Transport layer (zenoh-pico FFI) is outside verification scope — Verus proves properties of nano-ros's own logic, not network behavior
- User callback execution time is unbounded by definition — proofs cover the framework, not application code

### Mitigations discovered

- **Transparent `external_type_specification`** (without `external_body`) makes public enums and structs fully accessible for matching and field access from the verification crate. This is the preferred approach for types with public interfaces.
- **`assume_specification`** links production functions to verified specs without modifying production code. Combined with transparent types, this provides formally linked proofs at the strongest trust level.
- **Ghost models** handle the `pub(crate)` case with medium trust. These require manual auditing against production source but still provide unbounded proofs over the model.
- **vstd from crates.io** (`vstd = "0.0.0-2026-02-08-0120"`) works reliably. Path dependencies to `tools/vstd` do NOT work because the pre-built Verus release is missing `dependencies/prettyplease`.

## References

- [Verus](https://github.com/verus-lang/verus) — deductive verification for Rust (CMU/MSR)
- [Verus Guide](https://verus-lang.github.io/verus/guide/) — official documentation
- [vstd on crates.io](https://crates.io/crates/vstd) — Verus standard library
- [vostd (Asterinas)](https://github.com/asterinas/vostd) — verified OS components with Verus (OSDI 2024)
- [Atmosphere](https://dl.acm.org/doi/10.1145/3731569.3764821) — verified microkernel built with Verus (SOSP 2025)
- [AutoVerus](https://dl.acm.org/doi/10.1145/3763174) — LLM-driven automated Verus proof generation (OOPSLA 2025)
- [Phase 30](phase-30-wcet-realtime-tooling.md) — Kani bounded model checking (82 harnesses)
