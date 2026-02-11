# Phase 31: Verus Unbounded Deductive Verification

## Summary

Prove safety-critical algorithm properties for **all inputs** using [Verus](https://github.com/verus-lang/verus) SMT-based deductive verification. Complements Kani bounded model checking (Phase 30.4/30.5) — same properties, stronger guarantees.

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
    ├── cdr.rs          # CDR serialization proofs (~10)
    ├── time.rs         # Duration/Time arithmetic proofs (~5)
    ├── action.rs       # GoalStatus state machine proofs (~4)
    ├── params.rs       # ParameterValue + range proofs (~4)
    └── trigger.rs      # TriggerCondition proofs (~2)
```

### Why a separate crate (not in-crate like Kani)

- **Zero production impact** — no cfg flags, feature gates, or vstd references in production crates
- **Toolchain isolation** — Verus bundles its own modified rustc; excluded from workspace avoids conflicts
- **Cross-crate proofs** — properties spanning nano-ros-serdes + nano-ros-core live naturally in one place
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

[dependencies]
vstd = "0.0.0-2026-02-08-0120"
nano-ros-serdes = { path = "../../core/nano-ros-serdes", default-features = false }
nano-ros-core = { path = "../../core/nano-ros-core", default-features = false }
nano-ros-params = { path = "../../core/nano-ros-params", default-features = false }
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
use nano_ros_core::time::{Duration, NANOS_PER_SEC};

verus! {

// Trusted contract on the real function (not re-implemented)
pub assume_specification [Duration::from_nanos](nanos: i64) -> (d: Duration)
    ensures
        d.sec == (nanos / 1_000_000_000) as i32,
        d.nanosec == ((nanos % 1_000_000_000) as i64).unsigned_abs() as u32;

pub assume_specification [Duration::to_nanos](&self) -> (n: i64)
    ensures
        n == self.sec as i64 * 1_000_000_000 + self.nanosec as i64;

// Unbounded proof — holds for ALL i64, not just ±10B like Kani
proof fn duration_roundtrip_all_i64(nanos: i64)
    ensures
        Duration::from_nanos(nanos).to_nanos() == nanos,
{
    // Z3 proves: (n/d)*d + unsigned_abs(n%d) == n for all integers
}

} // verus!
```

## Proof Targets (~25 proofs)

### CDR Serialization (nano-ros-serdes, ~10 proofs)

| Proof                                 | Property                                         | Kani Bound → Verus              |
|---------------------------------------|--------------------------------------------------|---------------------------------|
| `align_correctness`                   | padding < alignment, result aligned, zero-filled | offset ≤ 1024 → **all usize**   |
| `position_invariant`                  | position() + remaining() == buf.len()            | ≤ 1024 → **all usize**          |
| `position_monotonicity`               | write operations only advance position           | bounded → **unbounded**         |
| `roundtrip_{u8,u16,u32,u64,i32,bool}` | write then read preserves value                  | bounded unwind → **all values** |
| `string_roundtrip`                    | write_string + read_string preserves content     | bounded → **all lengths**       |
| `header_origin`                       | new_with_header sets origin=4, pos=4             | —                               |

### Time Arithmetic (nano-ros-core, ~5 proofs)

| Proof                           | Property                                        | Kani Bound → Verus                                         |
|---------------------------------|-------------------------------------------------|------------------------------------------------------------|
| `duration_from_nanos_roundtrip` | `to_nanos(from_nanos(n)) == n`                  | ±10B → **all i64**                                         |
| `duration_components_valid`     | `nanosec < 1e9` always                          | ±10B → **all i64**                                         |
| `time_add_sub_inverse`          | `(t + d) - d == t`                              | bounded → **unbounded**                                    |
| `time_ordering_consistent`      | `t1 < t2 ⟺ t1.to_nanos() < t2.to_nanos()`       | —                                                          |
| `time_from_nanos_bug`           | Formally demonstrates missing `.unsigned_abs()` | constrained non-negative → **proves failure for negative** |

### GoalStatus State Machine (nano-ros-core, ~4 proofs)

| Proof                      | Property                                                                                           |
|----------------------------|----------------------------------------------------------------------------------------------------|
| `terminal_active_disjoint` | `is_terminal ∧ is_active` is impossible for all variants                                           |
| `valid_status_exhaustive`  | `from_i8(s as i8) == Some(s)` for all 7 variants                                                   |
| `transition_validity`      | Valid transitions form a DAG (Accepted→Executing→{Succeeded,Aborted,Canceling}→{Canceled,Aborted}) |
| `from_i8_roundtrip`        | `from_i8(to_i8(s)) == Some(s)`                                                                     |

### Parameters (nano-ros-params, ~4 proofs)

| Proof                             | Property                                                |
|-----------------------------------|---------------------------------------------------------|
| `integer_range_contains_boundary` | `contains(from) ∧ contains(to)`, step divides interval  |
| `float_range_contains_boundary`   | Same for floating-point                                 |
| `parameter_value_roundtrip`       | `i64→ParameterValue→i64` identity, `bool→bool` identity |
| `parameter_value_type_tag`        | Each variant returns correct ParameterType discriminant |

### TriggerCondition (nano-ros-node, ~2 proofs)

| Proof                   | Property                                         |
|-------------------------|--------------------------------------------------|
| `trigger_any_semantics` | `Any.evaluate(ready) ⟺ ∃i. ready[i]`             |
| `trigger_all_semantics` | `All.evaluate(ready) ⟺ (len > 0 ∧ ∀i. ready[i])` |

## What Verus proves beyond Kani

| Property                      | Kani (Phase 30)          | Verus (Phase 31)            |
|-------------------------------|--------------------------|-----------------------------|
| CDR align correctness         | offset ≤ 1024            | **All usize**               |
| Duration from_nanos roundtrip | ±10B nanos               | **All i64**                 |
| Time from_nanos bug           | Constrained non-negative | **Proves failure domain**   |
| GoalStatus FSM                | Exhaustive enum          | **Transition system model** |
| TriggerCondition              | No Kani proofs           | **First formal proofs**     |

## Running Verification

```bash
# Run Verus verification
just verify-verus

# Run both Kani and Verus
just verify-kani && just verify-verus
```

**Justfile recipe:**

```bash
verify-verus:
    cd packages/verification/nano-ros-verification && cargo verus verify
```

If `cargo verus` proves unreliable, fallback to direct binary invocation:

```bash
verify-verus:
    ./tools/verus packages/verification/nano-ros-verification/src/lib.rs
```

## Work Items

| ID   | Task                                          | Effort  | Status    |
|------|-----------------------------------------------|---------|-----------|
| 31.1 | Crate scaffolding + Verus toolchain setup     | 0.5 day | Not started |
| 31.2 | CDR serialization proofs (~10)                | 1 day   | Not started |
| 31.3 | Time arithmetic proofs (~5)                   | 0.5 day | Not started |
| 31.4 | GoalStatus state machine proofs (~4)          | 0.5 day | Not started |
| 31.5 | Parameter + TriggerCondition proofs (~6)      | 0.5 day | Not started |
| 31.6 | Justfile recipe + CI integration              | 2 hours | Not started |

## Limitations

- High annotation burden (4:1 to 7:1 proof:code ratio)
- Cannot verify unsafe/FFI code (nano-ros-c stays with Kani)
- Verus supports a subset of Rust (no `dyn Trait`, limited complex borrowing)
- SMT solver can be unpredictable on complex proofs (timeouts)
- No C support — only applies to Rust code
- `cargo verus` is still maturing (known stability issues, fallback to direct binary)

## References

- [Verus](https://github.com/verus-lang/verus) — deductive verification for Rust (CMU/MSR)
- [Verus Guide](https://verus-lang.github.io/verus/guide/) — official documentation
- [vstd on crates.io](https://crates.io/crates/vstd) — Verus standard library
- [vostd (Asterinas)](https://github.com/asterinas/vostd) — verified OS components with Verus (OSDI 2024)
- [Atmosphere](https://dl.acm.org/doi/10.1145/3731569.3764821) — verified microkernel built with Verus (SOSP 2025)
- [AutoVerus](https://dl.acm.org/doi/10.1145/3763174) — LLM-driven automated Verus proof generation (OOPSLA 2025)
- [Phase 30](phase-30-wcet-realtime-tooling.md) — Kani bounded model checking (82 harnesses)
