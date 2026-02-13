# Ghost Model Validation Strategy

Ghost models are manually written mirrors of production types with private fields.
They enable Verus proofs over code that can't be imported directly (atomics,
lifetimes, heapless containers). The risk: if production code changes and ghost
models aren't updated, proofs still pass but no longer correspond to reality.

This document describes a three-layer validation strategy that detects structural,
behavioral, and semantic drift between ghost models and production code. It also
surveys Rust visibility mechanisms for compile-time field linkage and recommends
two concrete implementation approaches.

## Problem

Verus proofs using `assume_specification` or `external_type_specification` are
safe from drift -- if the production function signature changes, the Verus build
breaks. But ghost models have no such linkage. They rely on doc comments citing
source file line numbers, which are fragile.

**Types of drift:**

| Drift type | Example                                             | Consequence                                         |
|------------|-----------------------------------------------------|-----------------------------------------------------|
| Structural | Field renamed `len` -> `payload_len`                | Ghost model references a name that no longer exists |
| Behavioral | `if` branch reordered, new error path added         | Ghost model encodes stale control flow              |
| Semantic   | `len` now stores capacity instead of payload length | Ghost model field has wrong meaning                 |

## Ghost Model Inventory

**Already formally linked (no drift possible):**

| Type                      | Mechanism                                                                               | Crate           |
|---------------------------|-----------------------------------------------------------------------------------------|-----------------|
| `TriggerCondition`        | `external_type_specification` + `assume_specification[evaluate]`                        | nano-ros-node   |
| `Duration` / `Time`       | `external_type_specification` + `assume_specification[from_nanos, to_nanos]`            | nano-ros-core   |
| `GoalStatus`              | `external_type_specification` + `assume_specification[is_terminal, is_active, from_i8]` | nano-ros-core   |
| `IntegerRange`            | `external_type_specification` + `assume_specification[contains]`                        | nano-ros-params |
| `ParameterType`           | `external_type_specification`                                                           | nano-ros-params |
| `SerError` / `DeserError` | `external_type_specification`                                                           | nano-ros-serdes |

**Ghost models needing protection:**

| Ghost model             | Production type                   | Crate              | Mirrored fields                              | Blocker for formal linking                   |
|-------------------------|-----------------------------------|--------------------|----------------------------------------------|----------------------------------------------|
| `CdrGhost`              | `CdrWriter<'a>` / `CdrReader<'a>` | nano-ros-serdes    | buf_len, pos, origin                         | Lifetime parameters on `&'a mut [u8]`        |
| `ParamServerGhost`      | `ParameterServer`                 | nano-ros-params    | count, max                                   | `heapless::LinearMap` in fields              |
| `SubscriberBufferGhost` | `SubscriberBuffer`                | nano-ros-transport | has_data, overflow, stored_len, buf_capacity | `AtomicBool`/`AtomicUsize`; private type     |
| `PublishChainGhost`     | *(control flow)*                  | nano-ros-node      | header_ok, serialize_ok, publish_raw_ok      | No real struct; models multi-fn call chain   |
| `SpinOnceGhost`         | *(control flow)*                  | nano-ros-node      | trigger_result, subs, services, timers       | No real struct; models executor control flow |

## Visibility Constraints in Rust

The core challenge is that ghost models mirror **private** fields of production
types. Rust's visibility system limits what external crates can see. We surveyed
available mechanisms:

### `pub` is not an attribute

You cannot write `#[cfg_attr(feature = "verification", pub)]` on a field.
Visibility modifiers (`pub`, `pub(crate)`, etc.) are part of the Rust grammar,
not attributes. This rules out per-field conditional visibility without tooling.

### `#[cfg(test)]` child module access

Unit test modules (`#[cfg(test)] mod tests`) are child modules of their parent.
Rust allows child modules to access all private items of their parent via
`use super::*`. This gives free access to private fields, types, and functions.

**Limitation:** This only works for **intra-crate** tests. The `cfg(test)` flag
on one crate does not affect its dependencies. The verification crate
(`nano-ros-verification`) is a separate crate and cannot use this mechanism
directly. See [cargo#8379](https://github.com/rust-lang/cargo/issues/8379).

### `pub_fields` crate (conditional field visibility)

The [`pub_fields`](https://docs.rs/pub-fields) crate provides
`#[pub_fields]` which rewrites all struct fields to `pub`. Combined with
`cfg_attr`, this enables conditional field publicity:

```rust
#[cfg_attr(feature = "verification", pub_fields::pub_fields)]
pub struct CdrWriter<'a> {
    buf: &'a mut [u8],  // private normally, pub with verification feature
    pos: usize,
    origin: usize,
}
```

**Pros:** Minimal code change; fields only exposed with feature flag; `no_std`
compatible (proc-macro runs at compile time).
**Cons:** Adds a proc-macro dependency to production crates (only activated
with the feature). Does not help with types that are themselves private
(e.g., `SubscriberBuffer` -- the struct is `struct`, not `pub struct`).

### `core::mem::offset_of!` (Rust 1.77+)

Stabilized in Rust 1.77, `offset_of!` gives compile-time field offsets:

```rust
const _: () = assert!(core::mem::offset_of!(CdrWriter, pos) == 16);
```

**Limitation:** `offset_of!` respects visibility. You cannot use it on private
fields from an external crate
([RFC 3308](https://rust-lang.github.io/rfcs/3308-offset_of.html)). However,
inside the same crate (e.g., in `#[cfg(test)]` modules), it works on all fields.

### `core::mem::size_of` (cross-crate)

`size_of::<T>()` is const and works on any public type regardless of field
visibility:

```rust
const _: () = assert!(
    core::mem::size_of::<nano_ros_serdes::CdrWriter>() == 32
);
```

This catches field additions/removals (size changes) but cannot verify field-level
correspondence. Two structs can have the same size with completely different layouts.

### Summary

| Mechanism                   | Cross-crate?    | Field-level? | Compile-time? | Dependency              |
|-----------------------------|-----------------|--------------|---------------|-------------------------|
| `#[cfg(test)]` child module | No              | Yes          | Yes           | None                    |
| `pub_fields` + feature flag | Yes             | Yes          | Yes           | `pub_fields` proc-macro |
| `offset_of!`                | No (visibility) | Yes          | Yes           | None (std)              |
| `size_of` / `align_of`      | Yes             | No           | Yes           | None (core)             |
| Manual accessor functions   | Yes             | Per-accessor | Runtime       | None                    |

## Three-Layer Validation

```
Production code
    |-- (Layer 1: structural) -- compile-time link to ghost model fields
    |-- (Layer 2: behavioral) -- test-time link to ghost model behavior
    '-- (Layer 3: properties) -- Verus proofs over ghost model
```

### Layer 1: Compile-Time Structural Linkage

**Detects:** structural drift (field renamed, removed, retyped, added).

The goal is a compile-time assertion that the ghost model's fields correspond
to the production type's fields. Two approaches exist depending on whether
the production crate is modified:

**Approach A: Feature-gated `pub_fields`** — The production crate conditionally
makes fields public. The verification crate enables the feature and directly
accesses the fields. See [Approach A](#approach-a-feature-gated-pub_fields)
below.

**Approach B: Shared ghost type crate** — A separate crate defines the ghost
types. Each production crate has `#[cfg(test)]` assertions that construct the
ghost type from private fields. The verification crate imports from the shared
crate. See [Approach B](#approach-b-shared-ghost-type-crate) below.

### Layer 2: Contract Tests (test-time)

**Detects:** behavioral drift (same fields, different control flow), semantic
drift (field meaning changes), and structural changes missed by Layer 1.

For each spec function that encodes production behavior, add a corresponding
`#[test]` in the production crate that exercises the real code and asserts the
same state transitions the ghost model assumes. If someone changes the behavior,
the contract test fails.

Contract tests run as part of `just quality` (regular `#[cfg(test)]` functions),
so drift is caught before `just verify-verus` even runs.

These tests live inside the production crate's source files, where `#[cfg(test)]`
child modules have full access to private fields and functions. This is the
standard Rust pattern for testing internals.

#### Subscriber buffer contracts

```rust
// nano-ros-transport/src/shim.rs
#[cfg(test)]
mod verification_contracts {
    use super::*;

    /// Mirrors callback_post_fix spec: overflow path.
    /// Ghost model assumes: msg_len > capacity => has_data=true, overflow=true.
    #[test]
    fn contract_callback_overflow_sets_flag() {
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[0] };
        buffer.has_data.store(false, Ordering::Relaxed);
        buffer.overflow.store(false, Ordering::Relaxed);

        let data = [0u8; 2000];
        subscriber_callback_with_attachment(
            data.as_ptr(), 2000,
            core::ptr::null(), 0,
            0 as *mut core::ffi::c_void,
        );

        assert!(buffer.has_data.load(Ordering::Relaxed), "has_data should be set");
        assert!(buffer.overflow.load(Ordering::Relaxed), "overflow should be set");
    }

    /// Mirrors callback_post_fix spec: normal path.
    /// Ghost model assumes: msg_len <= capacity => has_data=true, overflow=false,
    /// stored_len=msg_len.
    #[test]
    fn contract_callback_normal_stores_data() {
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[0] };
        buffer.has_data.store(false, Ordering::Relaxed);
        buffer.overflow.store(true, Ordering::Relaxed);

        let data = [42u8; 100];
        subscriber_callback_with_attachment(
            data.as_ptr(), 100,
            core::ptr::null(), 0,
            0 as *mut core::ffi::c_void,
        );

        assert!(buffer.has_data.load(Ordering::Relaxed));
        assert!(!buffer.overflow.load(Ordering::Relaxed), "overflow should be cleared");
        assert_eq!(buffer.len.load(Ordering::Relaxed), 100);
    }

    /// Mirrors try_recv_post_fix spec: overflow clears both flags.
    /// Ghost model assumes: overflow=true => has_data=false, overflow=false,
    /// returns MessageTooLarge.
    #[test]
    fn contract_try_recv_clears_on_overflow() {
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[0] };
        buffer.has_data.store(true, Ordering::Relaxed);
        buffer.overflow.store(true, Ordering::Relaxed);

        // ... call try_recv_raw, assert MessageTooLarge ...
        // ... assert has_data=false, overflow=false ...
    }

    /// Mirrors try_recv_post_fix spec: BufferTooSmall clears has_data.
    /// Ghost model assumes: stored_len > rx_buf => has_data=false (no stuck state).
    #[test]
    fn contract_try_recv_clears_on_size_error() {
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[0] };
        buffer.has_data.store(true, Ordering::Relaxed);
        buffer.overflow.store(false, Ordering::Relaxed);
        buffer.len.store(500, Ordering::Relaxed);

        // ... call try_recv_raw with 100-byte buffer ...
        // ... assert BufferTooSmall, has_data=false ...
    }
}
```

#### CDR contracts

```rust
// nano-ros-serdes/src/cdr.rs
#[cfg(test)]
mod verification_contracts {
    use super::*;

    /// Mirrors CdrGhost header_origin spec.
    /// Ghost model assumes: new_with_header sets pos=4, origin=4.
    #[test]
    fn contract_header_sets_pos_and_origin() {
        let mut buf = [0u8; 64];
        let w = CdrWriter::new_with_header(&mut buf).unwrap();
        assert_eq!(w.pos, 4);     // direct private field access
        assert_eq!(w.origin, 4);  // direct private field access
    }

    /// Mirrors CdrGhost header_position_invariant spec.
    /// Ghost model assumes: pos + remaining == buf.len().
    #[test]
    fn contract_position_plus_remaining_equals_buf_len() {
        let mut buf = [0u8; 128];
        let mut w = CdrWriter::new_with_header(&mut buf).unwrap();
        w.write_u32(42).unwrap();
        assert_eq!(w.pos + w.buf.len() - w.pos, w.buf.len());
    }
}
```

#### Control-flow contracts

For ghost models that mirror control flow rather than a struct (`PublishChainGhost`,
`SpinOnceGhost`), contract tests exercise the production function and assert the
input-output relationship the ghost model encodes.

```rust
// nano-ros-node (executor tests)
#[cfg(test)]
mod verification_contracts {
    /// Mirrors spin_once_invariant spec.
    /// Ghost model assumes: trigger=false => subs_processed=0, services_handled=0.
    #[test]
    fn contract_trigger_false_skips_subscriptions() {
        // Build executor with TriggerCondition::All and one subscription with no data.
        // Call spin_once().
        // Assert: result.subscriptions_processed == 0, result.services_handled == 0.
    }
}
```

### Layer 3: Verus Proofs (verify-time)

**Detects:** property violations in the ghost model.

The Verus proofs themselves don't change. They continue to reason over ghost
models and prove properties like "no stuck subscription" and "no silent
truncation." Layers 1 and 2 ensure the ghost models stay in sync with production
code; Layer 3 ensures the properties hold on the model.

## Detection Matrix

| Change type                  | Layer 1 (structural) | Layer 2 (contracts) | Layer 3 (Verus) |
|------------------------------|----------------------|---------------------|-----------------|
| Field renamed                | **Build fails**      | --                  | --              |
| Field type changed           | **Build fails**      | --                  | --              |
| Field added/removed          | **Build fails** (A) or size mismatch (B) | Maybe | --     |
| Logic changed (same fields)  | --                   | **Test fails**      | --              |
| Field meaning changed        | --                   | **Test fails**      | --              |
| Ghost model was always wrong | --                   | **Test fails**      | --              |
| Property doesn't hold        | --                   | --                  | **Proof fails** |

Layer 2 (contract tests) is the most valuable layer because it catches behavioral
and semantic drift -- the categories that are hardest to detect mechanically and
most dangerous in practice. Layer 1 adds compile-time safety for the easy cases.

## Implementation Approaches

### Approach A: Feature-Gated `pub_fields`

Make production type fields conditionally public using the `pub_fields` crate.
The verification crate enables the feature and accesses fields directly.

```
nano-ros-serdes/
    Cargo.toml          # [features] verification = ["pub_fields"]
    src/cdr.rs          # #[cfg_attr(feature = "verification", pub_fields::pub_fields)]
                        # pub struct CdrWriter<'a> { buf, pos, origin }

nano-ros-verification/
    Cargo.toml          # nano-ros-serdes = { features = ["verification"] }
    src/cdr.rs          # Can directly access CdrWriter.pos, .origin, .buf
```

**Production crate changes:**

```rust
// nano-ros-serdes/src/cdr.rs
#[cfg_attr(feature = "verification", pub_fields::pub_fields)]
pub struct CdrWriter<'a> {
    buf: &'a mut [u8],  // private normally, pub with feature
    pos: usize,
    origin: usize,
}
```

**Verification crate usage:**

```rust
// nano-ros-verification/src/cdr.rs (outside verus! macro)
#[cfg(not(verus_macro))]
fn _structural_check() {
    // These field accesses fail to compile if fields are renamed/retyped
    let _: usize = core::mem::size_of_val(&CdrWriter::new(&mut []).pos);
    let _: usize = core::mem::size_of_val(&CdrWriter::new(&mut []).origin);
}
```

**Applicability per ghost model:**

| Ghost model             | `pub_fields` viable? | Notes                                       |
|-------------------------|----------------------|---------------------------------------------|
| `CdrGhost`              | Yes                  | `CdrWriter` is already `pub`                |
| `ParamServerGhost`      | Yes                  | `ParameterServer` is already `pub`           |
| `SubscriberBufferGhost` | Partial              | Struct itself is private; need to also make it `pub` |
| `PublishChainGhost`     | N/A                  | No real struct to expose                     |
| `SpinOnceGhost`         | N/A                  | No real struct to expose                     |

**For `SubscriberBuffer`** (private type): `pub_fields` only changes field
visibility, not the struct's own visibility. You would additionally need to
feature-gate the struct visibility, e.g., by placing it in a conditionally-public
module or adding `#[cfg_attr(feature = "verification", visibility::make(pub))]`
on the containing module (using the
[`visibility`](https://docs.rs/visibility) crate).

**Pros:**
- Strongest compile-time guarantee for struct-backed ghost models
- Field accesses in verification crate fail immediately on any structural change
- No separate crate needed

**Cons:**
- Adds `pub_fields` (and possibly `visibility`) as proc-macro dependencies
  to production crates, even if only activated by the feature flag
- Feature flag "leaks" -- any downstream crate can enable it and see private fields
- Cannot help with control-flow ghost models (no real struct)
- Cannot help with private types without additional visibility changes

### Approach B: Shared Ghost Type Crate

Create a separate crate that defines the ghost model types with all-public fields.
Each production crate has `#[cfg(test)]` assertions verifying its private fields
match the ghost type. The verification crate imports from the shared crate.

```
packages/verification/
    nano-ros-ghost-types/         # NEW: shared ghost type definitions
        src/lib.rs                # CdrGhost, SubscriberBufferGhost, etc.
    nano-ros-verification/        # Existing: Verus proofs
        src/cdr.rs                # Imports CdrGhost from ghost-types crate

packages/core/
    nano-ros-serdes/
        Cargo.toml                # [dev-dependencies] nano-ros-ghost-types
        src/cdr.rs                # #[cfg(test)] mod ghost_checks { ... }
```

**Ghost type crate:**

```rust
// nano-ros-ghost-types/src/lib.rs
#![no_std]

/// Ghost model of CdrWriter/CdrReader.
/// Mirrored fields: buf (as buf_len), pos, origin.
/// Production source: nano-ros-serdes/src/cdr.rs:9-13
pub struct CdrGhost {
    pub buf_len: usize,
    pub pos: usize,
    pub origin: usize,
}

/// Ghost model of SubscriberBuffer.
/// Production source: nano-ros-transport/src/shim.rs:853-876
pub struct SubscriberBufferGhost {
    pub has_data: bool,
    pub overflow: bool,
    pub stored_len: usize,
    pub buf_capacity: usize,
}
```

**Production crate assertions (using `#[cfg(test)]` private field access):**

```rust
// nano-ros-serdes/src/cdr.rs
#[cfg(test)]
mod ghost_checks {
    use super::*;
    use nano_ros_ghost_types::CdrGhost;

    /// Verify ghost model fields match production type fields.
    /// This test accesses private fields (child module privilege)
    /// and constructs the ghost type to confirm correspondence.
    #[test]
    fn cdr_ghost_matches_production() {
        let mut buf = [0u8; 64];
        let w = CdrWriter::new(&mut buf);

        // Construct ghost from production fields -- if any field is renamed
        // or retyped, this fails to compile
        let ghost = CdrGhost {
            buf_len: w.buf.len(),
            pos: w.pos,
            origin: w.origin,
        };

        assert_eq!(ghost.pos, 0);
        assert_eq!(ghost.origin, 0);
        assert_eq!(ghost.buf_len, 64);
    }

    /// Verify ghost model tracks production after operations.
    #[test]
    fn cdr_ghost_after_header() {
        let mut buf = [0u8; 64];
        let w = CdrWriter::new_with_header(&mut buf).unwrap();

        let ghost = CdrGhost {
            buf_len: w.buf.len(),
            pos: w.pos,
            origin: w.origin,
        };

        // These values must match the ghost model spec functions
        assert_eq!(ghost.pos, 4);
        assert_eq!(ghost.origin, 4);
    }
}
```

```rust
// nano-ros-transport/src/shim.rs
#[cfg(test)]
mod ghost_checks {
    use super::*;
    use nano_ros_ghost_types::SubscriberBufferGhost;

    #[test]
    fn subscriber_buffer_ghost_matches_production() {
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[0] };
        buffer.has_data.store(false, Ordering::Relaxed);
        buffer.overflow.store(false, Ordering::Relaxed);
        buffer.len.store(0, Ordering::Relaxed);

        // Construct ghost from production fields
        let ghost = SubscriberBufferGhost {
            has_data: buffer.has_data.load(Ordering::Relaxed),
            overflow: buffer.overflow.load(Ordering::Relaxed),
            stored_len: buffer.len.load(Ordering::Relaxed),
            buf_capacity: buffer.data.len(),
        };

        assert_eq!(ghost.has_data, false);
        assert_eq!(ghost.buf_capacity, 1024);
    }

    /// Verify ghost model after callback (overflow path).
    #[test]
    fn subscriber_ghost_after_overflow_callback() {
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[0] };
        buffer.has_data.store(false, Ordering::Relaxed);
        buffer.overflow.store(false, Ordering::Relaxed);

        let data = [0u8; 2000];
        subscriber_callback_with_attachment(
            data.as_ptr(), 2000,
            core::ptr::null(), 0,
            0 as *mut core::ffi::c_void,
        );

        let ghost = SubscriberBufferGhost {
            has_data: buffer.has_data.load(Ordering::Relaxed),
            overflow: buffer.overflow.load(Ordering::Relaxed),
            stored_len: buffer.len.load(Ordering::Relaxed),
            buf_capacity: buffer.data.len(),
        };

        // Must match callback_post_fix(2000, 1024) ghost spec
        assert!(ghost.has_data);
        assert!(ghost.overflow);
    }
}
```

**Verification crate imports from shared ghost type crate:**

```rust
// nano-ros-verification/src/cdr.rs
use nano_ros_ghost_types::CdrGhost;  // instead of defining its own CdrGhost

verus! {
    // Proofs use the shared CdrGhost type -- single source of truth
    pub open spec fn header_position_invariant(g: CdrGhost) -> bool {
        g.pos <= g.buf_len && g.pos >= g.origin
    }
}
```

**Applicability per ghost model:**

| Ghost model             | Shared crate viable? | Notes                                                     |
|-------------------------|----------------------|-----------------------------------------------------------|
| `CdrGhost`              | Yes                  | Production test constructs from private fields            |
| `ParamServerGhost`      | Yes                  | Same pattern                                              |
| `SubscriberBufferGhost` | Yes                  | Works even though type is private (test is in-crate)      |
| `PublishChainGhost`     | Partial              | Can define the ghost type; contract tests verify behavior |
| `SpinOnceGhost`         | Partial              | Same -- behavioral verification via contract tests        |

**Pros:**
- No proc-macro dependencies on production crates
- No feature flag that could leak private fields
- Works with private types (test module has in-crate access)
- Single source of truth for ghost types (shared between tests and verification)
- Construction-from-fields pattern catches renames AND type mismatches
- `no_std` compatible (ghost type crate has no dependencies)

**Cons:**
- Requires a new crate in the workspace
- Ghost type crate can't be compiled by Verus (it's outside `verus!` macro) --
  the verification crate would need to re-register types with
  `external_type_specification`, adding a layer of indirection
- Production crates gain a `dev-dependency`

### Comparison

| Criterion                        | Approach A (`pub_fields`) | Approach B (ghost crate) |
|----------------------------------|---------------------------|--------------------------|
| Production crate changes         | Feature flag + attribute  | `dev-dependency` + tests |
| Proc-macro dependency            | `pub_fields` (+ `visibility` for private types) | None |
| Private type support             | Needs additional work     | Works (in-crate tests)   |
| Control-flow ghost models        | N/A                       | Partial (behavioral)     |
| Single source of truth           | No (ghost defined in 2 places) | Yes (shared crate) |
| Feature flag leakage risk        | Yes                       | No                       |
| Verification crate complexity    | Direct field access       | `external_type_specification` indirection |

### Recommendation

**Use Approach B (shared ghost type crate) as the primary mechanism.** It handles
all ghost model types (including private types like `SubscriberBuffer`), requires
no proc-macro dependencies, and provides a single source of truth. The
construction-from-fields pattern in `#[cfg(test)]` modules catches both structural
and type mismatches without exposing private fields outside the crate.

Complement with `size_of` cross-crate assertions in the verification crate as
an additional safety net:

```rust
// nano-ros-verification (outside verus! macro)
const _: () = assert!(
    core::mem::size_of::<nano_ros_serdes::CdrWriter>() >= 3 * core::mem::size_of::<usize>()
);
```

## Implementation Checklist

For each ghost model:

1. Define the ghost type in the shared ghost type crate (`nano-ros-ghost-types`)
2. In the production crate, add `nano-ros-ghost-types` as a `dev-dependency`
3. Add `#[cfg(test)] mod ghost_checks` that constructs the ghost type from
   private fields (compile-time structural check)
4. Add contract tests in the same module that verify behavioral correspondence
   with spec functions (runtime behavioral check)
5. In the verification crate, import ghost types from the shared crate
   (or re-register with `external_type_specification`)
6. Add `size_of` assertions in the verification crate as a cross-crate safety net

### Naming Conventions

| Item             | Convention                        | Example                                      |
|------------------|-----------------------------------|----------------------------------------------|
| Ghost type crate | `nano-ros-ghost-types`            | `packages/verification/nano-ros-ghost-types` |
| Ghost type names | `{ProductionType}Ghost`           | `CdrGhost`, `SubscriberBufferGhost`          |
| Structural tests | `{type}_ghost_matches_production` | `cdr_ghost_matches_production`               |
| Behavioral tests | `{type}_ghost_after_{operation}`  | `subscriber_ghost_after_overflow_callback`   |
| Contract tests   | `contract_{spec_fn}_{scenario}`   | `contract_callback_overflow_sets_flag`       |

## Relationship to Other Verification

This strategy is specific to Verus ghost models. Other verification mechanisms
have their own drift protection:

- **Kani harnesses** (`#[cfg(kani)]`): Run against real production code. No drift
  possible -- if the code changes, Kani re-verifies.
- **`assume_specification`**: Compile-time linked to production function signatures.
  Signature changes break the Verus build.
- **`external_type_specification`**: Compile-time linked to production type
  definitions. Adding/removing variants or fields breaks the Verus build.
- **Pure math proofs**: No production code link. Cannot drift because they prove
  arithmetic identities independent of any implementation.
