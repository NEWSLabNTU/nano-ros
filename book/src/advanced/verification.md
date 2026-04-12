# Formal Verification

nano-ros uses three complementary verification tools to ensure correctness of its core libraries:

- **Kani** -- bounded model checking with 160 harnesses across 6 crates. Checks memory safety, integer overflow, and panic-freedom within bounded inputs.
- **Verus** -- deductive (unbounded) proofs with 102 verified properties. Proves functional correctness of scheduling, serialization, time arithmetic, and safety protocols.
- **Miri** -- runtime undefined behavior detection for unsafe code.

All three run via just recipes:

```bash
just verify          # Run both Kani + Verus
just verify-kani     # Kani only (~3 min)
just verify-verus    # Verus only (~1 sec)
just test-miri       # Miri UB detection
```

## Kani

Kani is a bounded model checker that translates Rust code to a verification IR and exhaustively explores all possible executions up to a given bound.

```bash
just verify-kani
```

160 harnesses are spread across 6 crates:

| Crate | Focus |
|-------|-------|
| `nros-serdes` | CDR serialization round-trip, buffer bounds |
| `nros-core` | Time arithmetic, type invariants |
| `nros-params` | Parameter value constraints, range checking |
| `nros-c` | FFI boundary safety |
| `nros-ghost-types` | Buffer state machine transitions (overflow/lock) |
| `nros-node` | Executor scheduling, subscription handling |

Kani checks three properties by default:
1. **Memory safety** -- no out-of-bounds access, no use-after-free
2. **Overflow freedom** -- no integer overflow on arithmetic operations
3. **Panic freedom** -- no reachable `panic!`, `unwrap()`, or `assert!` failure

A typical run takes approximately 3 minutes on a modern machine.

## Miri

Miri is an interpreter for Rust's Mid-level IR that detects undefined behavior at runtime:

```bash
just test-miri
```

Miri catches issues that neither the compiler nor Kani can detect:
- Invalid pointer dereferences
- Uninitialized memory reads
- Violations of aliasing rules (Stacked Borrows / Tree Borrows)
- Data races in concurrent code

If Miri fails with "contains outdated or invalid JSON", clean the cache:

```bash
rm -rf target/miri
```

## Verus

Verus is a deductive verification tool for Rust. Unlike Kani (which checks bounded executions), Verus proves properties hold for all possible inputs using SMT solving.

### Quick Reference

```bash
just verification verus   # Download Verus binary to tools/ (or `just verification setup` for kani + verus)
just verify-verus    # Run all Verus proofs
just verify          # Run both Kani + Verus
```

Verification crate: `packages/verification/nros-verification/`

## Type Specifications

Every type used inside `verus! { }` that is defined outside the macro needs an
`external_type_specification`. How you write it determines whether Verus treats
the type as transparent or opaque.

### Transparent enums (variant matching allowed)

Use `external_type_specification` **without** `external_body`:

```rust
use nros_node::TriggerCondition;

verus! {

#[verifier::external_type_specification]
pub struct ExTriggerCondition(TriggerCondition);

// Now we can match on variants in spec functions:
pub open spec fn trigger_eval_spec(cond: TriggerCondition, ready: Seq<bool>) -> bool {
    match cond {
        TriggerCondition::Any => ...,
        TriggerCondition::All => ...,
        TriggerCondition::Always => true,
        TriggerCondition::One(index) => ...,
    }
}

} // verus!
```

This works for enums whose variants are visible (public). Verus sees the full
variant structure and allows pattern matching in specs and proofs.

### Opaque types (no internal access)

Use `external_type_specification` **with** `external_body`:

```rust
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExSomeOpaqueType(SomeOpaqueType);
```

Use this for types with private fields, complex internals, or when you only need
to pass them around without inspecting their structure. You cannot match on
variants or access fields.

### Structs with public fields

Use `external_type_specification` without `external_body` -- fields become
accessible in specs:

```rust
#[verifier::external_type_specification]
pub struct ExDuration(nros_core::time::Duration);

// Fields are accessible:
pub assume_specification[ Duration::to_nanos ](self_: &Duration) -> (n: i64)
    ensures n == (self_.sec as i64) * 1_000_000_000i64 + (self_.nanosec as i64);
```

## Linking Specs to Production Code

### `assume_specification`

Axiomatically declares a contract on a production function. The contract is
**trusted** (not verified by Verus) -- a human auditor must confirm the spec
matches the implementation.

```rust
pub assume_specification[ TriggerCondition::evaluate ](
    self_: &TriggerCondition,  // &self becomes a named parameter
    ready: &[bool],
) -> (ret: bool)
    ensures
        ret == trigger_eval_spec(*self_, ready@);
```

Rules:
- `&self` must be written as `self_: &Type` (named parameter, not method syntax)
- `ready@` converts `&[bool]` to `Seq<bool>` (Verus view conversion)
- `*self_` dereferences `&TriggerCondition` to `TriggerCondition` for spec matching
- The return value must be named: `-> (ret: bool)`

### Ghost models

For types that can't be imported (private fields, feature-gated behind C FFI,
etc.), create a ghost model -- a manually maintained mirror:

```rust
/// Ghost representation of TimerState (mirrors nros_node::timer::TimerState).
pub struct TimerGhost {
    pub period_ms: u64,
    pub elapsed_ms: u64,
    pub mode: TimerModeGhost,
    pub canceled: bool,
}
```

Ghost models have weaker guarantees than `assume_specification`. Correctness
relies on manual comparison with the production source code.

## Trust Levels

Every proof falls into one of three trust levels:

| Level | Mechanism | What's trusted | Strength |
|-------|-----------|----------------|----------|
| Formally linked | `assume_specification` + `external_type_specification` | The spec matches the impl (human audit of ~4 lines) | Strongest |
| Ghost model | Manual struct/enum mirror | Line-by-line correspondence with production source | Medium |
| Pure math | Arithmetic identities | Only the math itself | Weakest (no code link) |

Document the trust level of each proof in the module-level doc comment.

## Pitfalls

### `verify = true` on production crates

**Never** add `[package.metadata.verus] verify = true` to a production crate
that contains function pointers, `dyn Trait`, or closures. Verus will attempt
THIR erasure on all items in the crate and panic at `erase.rs:237`.

Only the verification crate (`nros-verification`) should have `verify = true`.
Production crates are used as regular dependencies -- Verus compiles them with
standard rustc without attempting verification.

### vstd dependency

vstd is published on crates.io (`vstd = "0.0.0-2026-02-08-0120"`). Use the
registry version, not a path dependency. The pre-built Verus release does not
include the full source tree needed for path dependencies (missing
`dependencies/prettyplease` etc.).

### Items outside `verus!` are external

Any type, function, or trait defined outside the `verus! { }` macro is treated
as external by Verus. To use it in specs, you must register it with
`external_type_specification` or reference it via `assume_specification`.

### Closures and iterators

Verus cannot verify code containing closures (including `.iter().any(|&r| r)`)
or function pointers. Mark such items with `#[verifier::external]` if they're
in a crate being verified, or keep them in production code that Verus never
touches.

### Edition 2024

Verus's bundled rustc (1.93.0) supports edition 2024. The verification crate
uses `edition = "2024"` to match the nros workspace. No special
configuration is needed.

## Adding a New Proof

1. Identify the production function and its source location
2. Determine the trust level:
   - Can you import the type? Use `external_type_specification`
   - Is the type behind a feature gate with C FFI? Use a ghost model
   - Is it pure arithmetic? Pure math proof
3. Write the spec function inside `verus! { }`
4. If formally linking: add `assume_specification` and document which source
   lines the auditor should compare
5. Write the proof function with `ensures` clauses
6. Run `just verify-verus` to check
7. Update the module doc comment with the new proof's trust level

## File Organization

```
packages/verification/nros-verification/src/
├── lib.rs            # Module declarations
├── scheduling.rs     # Timer + trigger + executor proofs
├── communication.rs  # CDR serialization safety proofs
├── cdr.rs            # CDR round-trip integrity proofs
├── time.rs           # Duration/Time arithmetic proofs
├── action.rs         # GoalStatus state machine proofs
├── params.rs         # ParameterValue + range proofs
└── e2e.rs            # End-to-end data path proofs
```

Proofs are organized by what they guarantee to the application developer, not
by source crate. A single proof module may reference types from multiple
production crates.
