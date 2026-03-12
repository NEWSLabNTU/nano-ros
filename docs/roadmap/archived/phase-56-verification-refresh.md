# Phase 56 — Verification Refresh

## Context

Phases 40 (large message support), 47 (executor trigger overhaul), and 53 (UDP
transport) changed production code that the Verus and Kani verification suites
model.  An audit found:

1. **Verus proofs are broken** — 76 proofs fail to compile because
   `TriggerCondition` (4 variants, `evaluate(&[bool])`) was renamed to
   `Trigger` (8 variants, bitmask-based inline evaluation).
2. **Service buffer overflow proofs are missing** — the subscriber path has
   pre-fix / post-fix Verus specs, but the service callback post-fix spec
   (overflow detection added in Phase 40) was never written.
3. **Transport bridge staging buffers are unverified** — `SmoltcpBridge` has
   TCP + UDP staging buffers with `rx_pos/rx_len/tx_pos/tx_len` state machines
   that sit between the verified subscriber/service layer and the physical
   network.  Zero ghost model, zero harnesses.
4. **Ephemeral port counter wrapping is unverified** — the global
   `NEXT_EPHEMERAL_PORT` counter wraps via `wrapping_add(1)` with a manual
   floor check, but no proof that the result stays in `[49152, 65535]`.

Principle: **fix broken proofs first, then fill coverage gaps from the
subscriber/service layer outward toward the network.**

## Progress

| Item                                        | Status               |
|---------------------------------------------|----------------------|
| 56.1 — Fix Verus trigger specs              | **Done** (99 proofs) |
| 56.2 — Service buffer post-fix Verus proofs | **Done** (102 proofs) |
| 56.3 — Staging buffer ghost model + Kani    | **Done** (25 Kani)   |
| 56.4 — Ephemeral port Kani harness          | **Done** (28 Kani)   |

## Deliverables

### 56.1 — Fix Verus Trigger Specs

The `Trigger` enum (formerly `TriggerCondition`) was expanded from 4 to 8
variants and the `evaluate()` method was removed — evaluation is now inlined in
`spin_once()` as a `match` over a `ReadinessSnapshot` bitmask (not `&[bool]`).

#### Production code (current)

```
spin.rs:1050-1072

Trigger::Any    => bits & non_timer_mask != 0 || non_timer_mask == 0
Trigger::All    => bits & non_timer_mask == non_timer_mask
Trigger::One(id)      => snapshot.is_ready(id)        // bit test
Trigger::AllOf(set)   => snapshot.all_ready(set)       // bits & set == set
Trigger::AnyOf(set)   => snapshot.any_ready(set)       // bits & set != 0
Trigger::Always       => true
Trigger::Predicate(f) => f(&snapshot)                  // opaque
Trigger::RawPredicate => unsafe { callback(...) }      // opaque
```

#### Changes required

**`nros-verification/src/scheduling.rs`:**

- [x] Replace `use nros_node::TriggerCondition` — removed entirely (Trigger
      contains fn pointers, can't be registered with Verus)
- [x] Remove `ExTriggerCondition` type spec — Trigger modeled as pure math
      specs over `Seq<bool>` with audit contract documenting bitmask equivalence
- [x] Remove `trigger_eval_spec` dispatch function — individual specs called
      directly (`trigger_any`, `trigger_all`, `trigger_all_of`, `trigger_any_of`)
- [x] Update `trigger_one` → bitmask semantics (unchanged, already used index)
- [x] Add `trigger_all_of` spec: `forall|i| set[i] ==> ready[i]`
- [x] Add `trigger_any_of` spec: `exists|i| set[i] && ready[i]`
- [x] Update `trigger_any` — fires on empty mask (`ready.len() == 0 ||`)
- [x] Update `trigger_all` — vacuously satisfied on empty mask (removed len guard)
- [x] Remove `assume_specification[TriggerCondition::evaluate]` — replaced with
      audit contract in module doc
- [x] Update proofs: `trigger_any_semantics`, `trigger_all_semantics`,
      `trigger_monotonicity` (added `len > 0` precondition),
      `trigger_one_in_bounds`, `trigger_any_empty_true` (was `_false`),
      `trigger_all_empty_true` (was `_false`),
      `trigger_always_unconditional`, removed `trigger_eval_spec_complete`
- [x] Add `trigger_all_of_semantics` proof
- [x] Add `trigger_any_of_semantics` proof
- [x] Add `trigger_all_of_implies_any_of` proof (AllOf ⟹ AnyOf when set has
      a true element)
- [x] Add `trigger_all_of_superset_of_all` proof
- [x] Add `trigger_any_timer_only` proof (empty mask always fires)

**`nros-verification/src/e2e.rs`:**

- [x] Remove `use nros_node::TriggerCondition` and `trigger_eval_spec` import
- [x] Update `default_trigger_delivers` — direct `trigger_any(ready)` call
- [x] Update `all_trigger_starvation` — direct `trigger_all(ready)` call
- [x] Update `executor_progress_under_any` — direct `trigger_any(ready)` call

**`nros-verification/src/progress.rs`:**

- [x] Remove `use nros_node::TriggerCondition` and `trigger_eval_spec` import
- [x] Update `trigger_always_progress` — ensures `true` (no params needed)
- [x] Update `trigger_any_progress` — direct `trigger_any(ready)` call

**`nros-ghost-types/src/lib.rs`:**

- [x] No changes needed — `SpinOnceGhost` is decoupled from `Trigger` type

#### Verification

- [x] `just verify-verus` passes (99 proofs — up from 76 baseline)
- [x] New `AllOf`/`AnyOf` proofs included in count

### 56.2 — Service Buffer Post-Fix Verus Proofs

The Verus `service_callback_spec` in `e2e.rs:778` still models the **pre-fix**
callback (silent truncation, `overflow: false`).  But production and the Kani
ghost model (`ServiceBufferGhost::callback_write`) both now detect overflow.

#### Changes required

**`nros-verification/src/e2e.rs`:**

- [x] Add `service_callback_post_fix` spec — overflow detection on write
      (mirrors subscriber `callback_post_fix`)
- [x] Add `try_recv_request_full` spec — 4-path recv with overflow check
      (supersedes `try_recv_request_post_fix` which lacked overflow path)
- [x] Add proof 16: `no_silent_service_truncation` — overflow callback →
      recv returns overflow error, not truncated data
- [x] Add proof 17: `no_stuck_service_post_fix` — using post-fix callback,
      all 3 error paths (overflow/BufferTooSmall/success) clear has_request
- [x] Add proof 18: `service_overflow_then_normal` — full recovery cycle:
      overflow → consume → normal request accepted and delivered

#### Verification

- [x] `just verify-verus` passes with new service proofs (102 total)
- [x] New proofs added to the count (99 → 102, +3 proofs)

### 56.3 — Staging Buffer Ghost Model + Kani

The `SmoltcpBridge` staging buffers have invariants that are not formally
checked.  Both TCP and UDP use the same pattern:

```
SOCKET_RX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS]
SOCKET_TX_BUFFERS: [[u8; SOCKET_BUFFER_SIZE]; MAX_SOCKETS]

SocketEntry { rx_pos, rx_len, tx_pos, tx_len, ... }
```

#### New ghost type

**`nros-ghost-types/src/lib.rs`:**

- [x] Add `StagingBufferGhost` struct with `rx_pos`, `rx_len`, `tx_pos`,
      `tx_len`, `capacity` fields
- [x] Add `StagingBufferGhost::new(capacity)` constructor
- [x] Add `recv(&mut self, user_buf_len) -> usize` — models `socket_recv`:
      copies `min(available, user_buf_len)` bytes, advances `rx_pos`,
      resets both to 0 when fully consumed
- [x] Add `send(&mut self, data_len) -> usize` — models `socket_send`:
      appends `min(available_space, data_len)` bytes at `tx_len`
- [x] Add `compact_rx(&mut self)` — models `copy_within` compaction in
      `poll()`: `rx_len = rx_len - rx_pos`, `rx_pos = 0`
- [x] Add `drain_tx(&mut self, sent)` — models TX transfer in `poll()`:
      advances `tx_pos`, resets both when fully drained
- [x] Add `fill_rx(&mut self, received)` — models RX fill in `poll()`:
      `rx_len += received`

#### Kani harnesses

- [x] `staging_invariant_after_recv` — `rx_pos <= rx_len <= capacity`
- [x] `staging_invariant_after_send` — `tx_pos <= tx_len <= capacity`
- [x] `staging_compact_preserves_data_length` — `new_rx_len == old_available`,
      `new_rx_pos == 0`
- [x] `staging_recv_progress` — if `rx_len > rx_pos`, recv returns > 0
- [x] `staging_send_progress` — if `tx_len < capacity`, send returns > 0
- [x] `staging_full_cycle` — send → drain → compact → fill → recv preserves
      all invariants
- [x] `staging_no_overlap` — compact + fill never exceeds capacity;
      send never exceeds capacity
- [x] `staging_empty_recv_returns_zero` — `rx_pos == rx_len` → recv returns 0
- [x] `staging_full_send_returns_zero` — `tx_len == capacity` → send returns 0

#### Verification

- [x] `cargo kani -p nros-ghost-types` passes (16 existing + 9 new = 25)

### 56.4 — Ephemeral Port Kani Harness

The ephemeral port counter in `bridge.rs` uses `wrapping_add(1)` with a manual
floor check:

```rust
NEXT_EPHEMERAL_PORT = NEXT_EPHEMERAL_PORT.wrapping_add(1);
if NEXT_EPHEMERAL_PORT < EPHEMERAL_PORT_START {
    NEXT_EPHEMERAL_PORT = EPHEMERAL_PORT_START;
}
```

This is used by both `register_socket` (TCP) and `register_udp_socket` (UDP).

#### Changes required

**`nros-ghost-types/src/lib.rs`:**

- [x] Add `ephemeral_port_next(current: u16) -> u16` function + `EPHEMERAL_PORT_START` const
- [x] Add Kani harness: `ephemeral_port_stays_in_range` — for any `current: u16`,
      result is in `[49152, 65535]`
- [x] Add Kani harness: `ephemeral_port_wraps_correctly` — `65535` → `49152`
- [x] Add Kani harness: `ephemeral_port_increments` — in-range inputs increment
      by 1, below-range inputs floor to `49152`

#### Verification

- [x] `cargo kani -p nros-ghost-types` passes (25 + 3 = 28)

## Implementation Order

```
56.1 (fix trigger specs)  ───→  56.2 (service post-fix proofs)
                                       │
56.3 (staging buffer Kani) ── parallel ─┤
56.4 (ephemeral port Kani) ── parallel ─┘
```

56.1 must be first (unblocks Verus compilation).
56.3 and 56.4 are independent Kani work, parallel with 56.2.

## Key Files

| File | Change |
|------|--------|
| `packages/verification/nros-verification/src/scheduling.rs` | Trigger spec overhaul |
| `packages/verification/nros-verification/src/e2e.rs` | Trigger import + service post-fix proofs |
| `packages/verification/nros-verification/src/progress.rs` | Trigger import fix |
| `packages/verification/nros-ghost-types/src/lib.rs` | StagingBufferGhost + ephemeral port + Kani |

## Verification

1. `just verify-verus` — all proofs pass (102 after 56.2)
2. `cargo kani -p nros-ghost-types` — all harnesses pass (28 from 16 baseline)
3. `just quality` — no regressions
