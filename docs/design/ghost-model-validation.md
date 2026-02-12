# Ghost Model Validation Strategy

Ghost models are manually written mirrors of production types with private fields.
They enable Verus proofs over code that can't be imported directly (atomics,
lifetimes, heapless containers). The risk: if production code changes and ghost
models aren't updated, proofs still pass but no longer correspond to reality.

This document describes a three-layer validation strategy that detects structural,
behavioral, and semantic drift between ghost models and production code.

## Problem

Verus proofs using `assume_specification` or `external_type_specification` are
safe from drift -- if the production function signature changes, the Verus build
breaks. But ghost models have no such linkage. They rely on doc comments citing
source file line numbers, which are fragile.

**Types of drift:**

| Drift type | Example | Consequence |
|---|---|---|
| Structural | Field renamed `len` -> `payload_len` | Ghost model references a name that no longer exists |
| Behavioral | `if` branch reordered, new error path added | Ghost model encodes stale control flow |
| Semantic | `len` now stores capacity instead of payload length | Ghost model field has wrong meaning |

## Ghost Model Inventory

**Already formally linked (no drift possible):**

| Type | Mechanism | Crate |
|---|---|---|
| `TriggerCondition` | `external_type_specification` + `assume_specification[evaluate]` | nano-ros-node |
| `Duration` / `Time` | `external_type_specification` + `assume_specification[from_nanos, to_nanos]` | nano-ros-core |
| `GoalStatus` | `external_type_specification` + `assume_specification[is_terminal, is_active, from_i8]` | nano-ros-core |
| `IntegerRange` | `external_type_specification` + `assume_specification[contains]` | nano-ros-params |
| `ParameterType` | `external_type_specification` | nano-ros-params |
| `SerError` / `DeserError` | `external_type_specification` | nano-ros-serdes |

**Ghost models needing protection:**

| Ghost model | Production type | Crate | Mirrored fields | Blocker for formal linking |
|---|---|---|---|---|
| `CdrGhost` | `CdrWriter<'a>` / `CdrReader<'a>` | nano-ros-serdes | buf_len, pos, origin | Lifetime parameters on `&'a mut [u8]` |
| `ParamServerGhost` | `ParameterServer` | nano-ros-params | count, max | `heapless::LinearMap` in fields |
| `SubscriberBufferGhost` | `SubscriberBuffer` | nano-ros-transport | has_data, overflow, stored_len, buf_capacity | `AtomicBool`/`AtomicUsize`; private type |
| `PublishChainGhost` | *(control flow)* | nano-ros-node | header_ok, serialize_ok, publish_raw_ok | No real struct; models multi-fn call chain |
| `SpinOnceGhost` | *(control flow)* | nano-ros-node | trigger_result, subs, services, timers | No real struct; models executor control flow |

## Three-Layer Validation

```
Production code
    |-- (Layer 1: accessors) -- compile-time link to ghost model fields
    |-- (Layer 2: contract tests) -- test-time link to ghost model behavior
    '-- (Layer 3: Verus proofs) -- prove properties of ghost model
```

### Layer 1: Feature-Gated Accessors (compile-time)

**Detects:** structural drift (field renamed, removed, retyped).

Add a `verification` Cargo feature to each production crate that has ghost models.
Under this feature, expose accessor functions for each private field that a ghost
model mirrors. The verification crate enables the feature and imports the accessors.
If a field is renamed or its type changes, the accessor fails to compile.

```rust
// nano-ros-serdes/src/cdr.rs
#[cfg(feature = "verification")]
impl<'a> CdrWriter<'a> {
    /// Verification accessor: mirrors CdrGhost.pos
    pub fn _verify_pos(&self) -> usize { self.pos }
    /// Verification accessor: mirrors CdrGhost.origin
    pub fn _verify_origin(&self) -> usize { self.origin }
    /// Verification accessor: mirrors CdrGhost.buf_len
    pub fn _verify_buf_len(&self) -> usize { self.buf.len() }
}
```

```rust
// nano-ros-params/src/server.rs
#[cfg(feature = "verification")]
impl ParameterServer {
    /// Verification accessor: mirrors ParamServerGhost.count
    pub fn _verify_count(&self) -> usize { self.count }
}
```

```rust
// nano-ros-transport/src/shim.rs
#[cfg(feature = "verification")]
impl SubscriberBuffer {
    /// Verification accessor: mirrors SubscriberBufferGhost.has_data
    pub fn _verify_has_data(&self) -> bool {
        self.has_data.load(Ordering::Relaxed)
    }
    /// Verification accessor: mirrors SubscriberBufferGhost.overflow
    pub fn _verify_overflow(&self) -> bool {
        self.overflow.load(Ordering::Relaxed)
    }
    /// Verification accessor: mirrors SubscriberBufferGhost.stored_len
    pub fn _verify_stored_len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }
    /// Verification accessor: mirrors SubscriberBufferGhost.buf_capacity
    pub fn _verify_buf_capacity(&self) -> usize { self.data.len() }
}
```

**Scope:** Covers the 3 struct-backed ghost models. The 2 control-flow ghosts
(`PublishChainGhost`, `SpinOnceGhost`) have no struct to link -- they need Layer 2.

**Naming convention:** Accessors use the `_verify_` prefix to signal that they
exist solely for verification linkage and should not be used in production code.

### Layer 2: Contract Tests (test-time)

**Detects:** behavioral drift (same fields, different control flow), semantic
drift (field meaning changes), and structural changes missed by Layer 1.

For each spec function that encodes production behavior, add a corresponding
`#[test]` in the production crate that exercises the real code and asserts the
same state transitions the ghost model assumes. If someone changes the behavior,
the contract test fails.

Contract tests run as part of `just quality` (regular `#[cfg(test)]` functions),
so drift is caught before `just verify-verus` even runs.

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
        // Reset buffer 0 to known state
        let buffer = unsafe { &SUBSCRIBER_BUFFERS[0] };
        buffer.has_data.store(false, Ordering::Relaxed);
        buffer.overflow.store(false, Ordering::Relaxed);

        // Invoke real callback with oversized message (2000 > 1024)
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
        buffer.overflow.store(true, Ordering::Relaxed); // should be cleared

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
        assert_eq!(w.position(), 4);
        assert_eq!(w.remaining(), buf.len() - 4);
    }

    /// Mirrors CdrGhost header_position_invariant spec.
    /// Ghost model assumes: position() + remaining() == buf.len().
    #[test]
    fn contract_position_plus_remaining_equals_buf_len() {
        let mut buf = [0u8; 128];
        let mut w = CdrWriter::new_with_header(&mut buf).unwrap();
        w.write_u32(42).unwrap();
        assert_eq!(w.position() + w.remaining(), buf.len());
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

| Change type | Layer 1 (accessors) | Layer 2 (contracts) | Layer 3 (Verus) |
|---|---|---|---|
| Field renamed | **Build fails** | -- | -- |
| Field type changed | **Build fails** | -- | -- |
| Field added/removed | -- | Maybe | -- |
| Logic changed (same fields) | -- | **Test fails** | -- |
| Field meaning changed | -- | **Test fails** | -- |
| Ghost model was always wrong | -- | **Test fails** | -- |
| Property doesn't hold | -- | -- | **Proof fails** |

Layer 2 (contract tests) is the most valuable layer because it catches behavioral
and semantic drift -- the categories that are hardest to detect mechanically and
most dangerous in practice. Layer 1 adds compile-time safety for the easy cases.

## Implementation Checklist

For each ghost model, the developer should:

1. Add `verification` feature to the production crate's `Cargo.toml`
2. Add `_verify_*` accessor functions under `#[cfg(feature = "verification")]`
3. Add contract tests under `#[cfg(test)]` that assert the same state transitions
   as the corresponding spec functions
4. In the verification crate, enable the `verification` feature and import
   the accessors (outside `verus!`) to create compile-time linkage
5. Document which contract tests correspond to which spec functions

### Naming Conventions

| Item | Convention | Example |
|---|---|---|
| Accessor functions | `_verify_{ghost_field}` | `_verify_has_data()` |
| Contract tests | `contract_{spec_fn}_{scenario}` | `contract_callback_overflow_sets_flag` |
| Cargo feature | `verification` | `[features] verification = []` |

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
