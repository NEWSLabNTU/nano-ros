# Phase 103 — RMW typed-loan path: real zero-copy where transport supports it

**Goal:** add a parallel typed-shape lending surface to the RMW vtable
alongside the existing CDR-byte lending (Phase 99). Backends with
intra-process or shared-memory transport (uORB today; future iceoryx
or zenoh-pico SHM link) can hand the publisher a typed-buffer slot
instead of a CDR-byte slot, eliminating the CDR encode/decode step
entirely and reaching **true zero-copy**.

**Status:** Not Started.
**Priority:** Medium. uORB intra-process pubsub is the immediate user
— Phase 90's PX4 RMW currently CDR-encodes every sample even though
both publisher and subscriber live in the same address space. Future
shared-memory transports (iceoryx, zenoh-pico SHM link) need this
surface too.
**Depends on:** Phase 99 (CDR-byte lending) for the trait + arena
infrastructure; Phase 102 for the `loan_caps.supports_typed_loan` bit
already laid out in the entity struct.

## Background

Phase 99 added per-publisher arena lending: each publisher embeds a
fixed-size byte buffer; `try_lend_slot` hands the caller a writable
`&mut [u8]` slice into it; the caller writes CDR-encoded bytes;
`commit_slot` ships them. This saves the application→backend
memcpy. It does **not** save the CDR encode step — the bytes the
caller writes are CDR-encoded.

Upstream `rmw.h` exposes `rmw_borrow_loaned_message` /
`rmw_publish_loaned_message` which loan **typed memory** — a
`void * ros_message` slot the caller fills with the typed message
struct directly (no CDR encode). Backends that route the typed memory
straight through (iceoryx SHM, ROS 2 intra-process, future zenoh-pico
SHM) achieve actual zero-copy. Backends that don't (DDS over RTPS, all
network transports) serialize during commit and the API degrades to
"one fewer copy."

Phase 99's CDR-byte lending and the typed-memory lending have
disjoint use cases:

| Concern | CDR-byte loan (Phase 99) | Typed loan (Phase 103) |
|---------|--------------------------|------------------------|
| Eliminates `app → backend` memcpy | yes | yes |
| Eliminates CDR encode | no | yes |
| Eliminates the wire write | no | only with SHM transport |
| Works with any wire transport | yes | only with intra-process / SHM |
| Buffer payload | CDR bytes | typed POD struct |

This phase adds the typed-loan surface alongside, not in place of, the
CDR-byte loan. Backends that can do both implement both; backends that
can only do one implement that one; the runtime picks the path
per-publisher based on `loan_caps.supports_typed_loan` plus the
codegen-emitted `<MsgType>_is_loanable()`.

## Design

### Buffer ownership: publisher-embedded, neither caller nor backend allocates

Phase 99 already established the pattern: each publisher object embeds
a fixed-size buffer slot. The buffer is part of the publisher's own
storage — wherever the application chose to put the publisher (stack,
static, executor arena), the loan slot lives there too. This means:

- **No backend allocator required** — works on bare-metal where the
  backend has no heap.
- **No caller-buffer-passing dance** — caller doesn't need a separate
  storage strategy.
- **Compile-time sized** — slot capacity known at publisher
  construction time.

For typed loan, the same model applies. The publisher embeds a
typed-shaped buffer instead of (or in addition to) the CDR-shaped
buffer. The size is determined at compile time from the message type
via codegen.

```rust
// Phase 99 — CDR-shape arena
pub struct LendArena {
    busy: AtomicBool,
    buf: UnsafeCell<[u8; ZENOH_TX_BUF]>,    // 1024 bytes, CDR-encoded
}

// Phase 103 — typed-shape arena (per-message-type publisher)
pub struct TypedLendArena<M: Loanable> {
    busy: AtomicBool,
    buf: UnsafeCell<MaybeUninit<M>>,         // size_of::<M>(), typed
}
```

The same `busy` AtomicBool + UnsafeCell pattern applies; only the
buffer shape changes.

### Vtable surface

```c
/* CDR-byte loan (existing — Phase 99 / 102 shape). */
nros_rmw_ret_t (*loan_publish_cdr)(
    nros_rmw_publisher_t *pub,
    size_t requested_len,
    uint8_t **slot_out,
    size_t *cap_out);
nros_rmw_ret_t (*commit_publish_cdr)(
    nros_rmw_publisher_t *pub,
    uint8_t *slot,
    size_t actual_len);

/* Typed loan (new). The slot has the layout of the typed POD message
 * struct emitted by codegen. The backend must allocate slot space at
 * publisher creation time (size known via the create_publisher
 * `type_size_bytes` parameter); calling try_loan_typed when the slot
 * is in use returns NROS_RMW_RET_LOAN_NOT_SUPPORTED with errno-style
 * "would block" semantics handled by the runtime. */
nros_rmw_ret_t (*loan_publish_typed)(
    nros_rmw_publisher_t *pub,
    void **slot_out);
nros_rmw_ret_t (*commit_publish_typed)(
    nros_rmw_publisher_t *pub,
    void *slot);
```

Same shape on the receive side:

```c
nros_rmw_ret_t (*loan_recv_cdr)(...);     /* Phase 99 — existing */
nros_rmw_ret_t (*release_recv_cdr)(...);

nros_rmw_ret_t (*loan_recv_typed)(
    nros_rmw_subscriber_t *sub,
    const void **slot_out);
nros_rmw_ret_t (*release_recv_typed)(
    nros_rmw_subscriber_t *sub,
    const void *slot);
```

Backends that don't support typed loan leave the four function
pointers `NULL` and clear the `loan_caps.supports_typed_loan` bit.

### `<MsgType>_is_loanable()` codegen rule

Codegen emits:

```c
/* In <generated>/std_msgs/msg/int32.h */
typedef struct std_msgs__msg__Int32 {
    int32_t data;
} std_msgs__msg__Int32;

#define std_msgs__msg__Int32_IS_LOANABLE 1     /* fixed-shape POD */
```

A message type is `loanable = 1` iff every field is also loanable:

| Field shape | Loanable |
|-------------|----------|
| primitive (`int32`, `float64`, `bool`, …) | yes |
| fixed-size array of loanable | yes |
| struct of loanable fields | yes |
| `String` (ROS-side `string`) | no — internally a heap-y sequence |
| `Sequence<T>` / unbounded array | no — variable size |
| nested message containing `String` | no |

The flag is constant at compile time; the runtime gates path
selection on it.

### Runtime path selection (per-publisher, fixed at create time)

```rust
match (M::IS_LOANABLE, pub.loan_caps.supports_typed_loan,
       pub.loan_caps.supports_cdr_loan) {
    (true,  true,  _   ) => Path::TypedLoan,    // best — true zero-copy
    (_,     _,     true) => Path::CdrLoan,      // good — saves one memcpy
    (_,     _,     _   ) => Path::PublishRaw,   // baseline — full memcpy
}
```

Decision is made once at publisher creation; published as
`pub.path: PublishPath` in the runtime publisher state. No per-publish
branch.

### Backends touched

| Backend | CDR loan | Typed loan | Notes |
|---------|----------|------------|-------|
| zenoh-pico | yes (Phase 99 done) | no — leaves `NULL` | Wire transport requires CDR. |
| XRCE-DDS | yes (Phase 99 done) | no — leaves `NULL` | Wire transport requires CDR. |
| dust-DDS | yes (Phase 99 done) | no — leaves `NULL` | RTPS wire format = CDR. |
| uORB | n/a — never had CDR loan | **yes** | Same address space; typed pointer hand-off. |

Future:

| Backend | Plan |
|---------|------|
| iceoryx | typed loan first-class; CDR loan may also be worth doing for cross-machine fallback |
| zenoh-pico `Z_FEATURE_LINK_SHM` | typed loan over shared-memory link; falls back to CDR over TCP |

## Work Items

- [ ] **103.1 — Codegen: `<MsgType>_is_loanable()` flag + size constant.**
      Update `cargo nano-ros generate-c` and the Rust codegen to
      emit `<MsgType>_IS_LOANABLE` (C macro) /
      `impl Loanable for <MsgType> { const IS_LOANABLE: bool = …; }`
      (Rust trait). Loanability rule per the table above. Test
      vectors: `std_msgs/Int32` (loanable), `std_msgs/String` (not),
      `geometry_msgs/Twist` (loanable — all primitives), nested
      composite (depends).
      **Files:** `packages/codegen/cargo-nano-ros/`,
      `packages/codegen/rosidl-generator-*`.

- [ ] **103.2 — Add the `Loanable` Rust trait + `TypedLendArena`.**
      `nros-rmw::traits` gains:
      ```rust
      pub trait Loanable: Sized {
          const IS_LOANABLE: bool;
      }
      pub trait TypedSlotLending<M: Loanable>: Publisher {
          type TypedSlot<'a>: AsMut<MaybeUninit<M>> + 'a where Self: 'a;
          fn try_lend_typed(&self) -> Result<Option<Self::TypedSlot<'_>>, Self::Error>;
          fn commit_typed(&self, slot: Self::TypedSlot<'_>) -> Result<(), Self::Error>;
      }
      ```
      `TypedLendArena<M>` parallel to Phase 99's `LendArena`.
      **Files:** `packages/core/nros-rmw/src/traits.rs`,
      `packages/core/nros-rmw/src/lending.rs`.

- [ ] **103.3 — uORB backend: typed loan implementation.**
      `nros-rmw-uorb` is the first backend to implement
      `TypedSlotLending`. uORB's `orb_publish` already accepts a
      typed `void *` payload pointer — match its calling convention.
      Per-publisher `TypedLendArena<M>` embedded in the publisher
      struct.
      **Files:** `packages/px4/nros-rmw-uorb/src/publisher.rs`,
      `packages/px4/nros-rmw-uorb/src/subscriber.rs`.

- [ ] **103.4 — C vtable: typed-loan function pointers.**
      Four new function-pointer fields in `nros_rmw_vtable_t`:
      `loan_publish_typed`, `commit_publish_typed`,
      `loan_recv_typed`, `release_recv_typed`. Backends that don't
      implement them leave `NULL`. The runtime probes for `NULL` at
      session open and clears
      `loan_caps.supports_typed_loan` accordingly.
      **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`.

- [ ] **103.5 — Runtime path selection.**
      `nros-node`'s `Publisher<M>` constructor probes
      `M::IS_LOANABLE` and `pub.loan_caps.supports_typed_loan` once,
      stores the resulting `PublishPath` enum on the
      publisher. `publish(msg)`, `try_lend_typed()`, etc. dispatch
      on `PublishPath` without per-call branches that hit the cold
      path.
      **Files:** `packages/core/nros-node/src/publisher.rs`,
      `packages/core/nros-node/src/subscription.rs`.

- [ ] **103.6 — User-facing API.**
      `Publisher<M>::loan_typed() -> Result<TypedLoan<'_, M>, Error>`
      gives the user a `TypedLoan` smart-pointer that derefs to
      `&mut MaybeUninit<M>`; `commit()` consumes it. C / C++ API
      mirrors with `nros_publisher_loan_typed(&pub, &slot_out)` /
      `nros_publisher_commit_typed(&pub, slot)`.
      **Files:** `packages/core/nros-node/src/publisher.rs`,
      `packages/core/nros-c/src/publisher.rs`,
      `packages/core/nros-cpp/include/nros/publisher.hpp`.

- [ ] **103.7 — uORB SITL E2E test: zero CDR encode on the publish
      path.** Builds on the Phase 90 SITL fixture. Test asserts that
      a `geometry_msgs/Twist` publish from a nano-ros uORB module
      lands in the consumer's typed buffer without going through
      the CDR encode path (verified via a feature-flagged trace
      counter on the encode call site).
      **Files:** `packages/testing/nros-tests/tests/uorb_typed_loan.rs`.

- [ ] **103.8 — Documentation.**
      New section in `book/src/design/rmw-vs-upstream.md` "Loaned
      messages first-class" updated to describe both shapes and
      when each fires. Doxygen on the four new vtable fields. README
      update on `nros-rmw-uorb` showing the typed-loan worked
      example.

## Acceptance Criteria

- [ ] `<MsgType>_IS_LOANABLE` constant emitted by codegen for every
      generated C / Rust type.
- [ ] uORB backend's `Publisher<geometry_msgs::msg::Twist>` reports
      `PublishPath::TypedLoan` at construction.
- [ ] uORB SITL E2E test passes: publish path traces zero CDR-encode
      calls.
- [ ] Existing Phase 99 CDR-loan tests on zenoh-pico / XRCE / dust-DDS
      continue to pass — typed-loan addition does not regress them.
- [ ] Doxygen + book updated; book builds clean.

## Notes

- **No `rosidl_message_type_support_t` analogue.** The interface is
  just `(void *slot, size_t bytes, bool is_loanable)`. The codegen
  flag plus the size constant is enough; the runtime never inspects
  the message structure beyond byte size.
- **Why publisher-embedded buffers, not backend allocator.** Embedded
  targets often have no allocator with predictable behaviour. Embedding
  the buffer in the publisher object (whose own storage the
  application chose) sidesteps the allocator question entirely. Same
  reasoning Phase 99 used for CDR loan.
- **Concurrent loan attempts** — single-slot per publisher, same as
  Phase 99. `try_lend_typed` returns `Ok(None)` on contention; never
  blocks. Apps that want multiple concurrent in-flight publishes need
  multiple publishers.
- **`MaybeUninit<M>` discipline.** The slot is uninitialised on lend;
  the user's `commit_typed` is unsafe iff the slot is not fully
  initialised. C side documents this as "you must write every field
  before commit." Rust side enforces via `MaybeUninit::assume_init`
  in the smart-pointer's commit path.
