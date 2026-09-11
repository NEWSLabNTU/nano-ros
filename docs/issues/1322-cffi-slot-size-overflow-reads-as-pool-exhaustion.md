---
id: 1322
title: "A backend state struct outgrowing cffi's 1 KiB `SLOT_SIZE` returns the
  same `BAD_ALLOC` as a full pool — one cause has a knob, the other cannot be
  fixed by any knob, and raising it makes the real one worse"
status: open
type: bug
area: rmw, embedded
severity: medium
found: 2026-09-11
related: [issue-0271, issue-0739, rfc-0100, phase-454]
---

## What is true

`packages/rmw/cffi/src/rust_adapter.rs` parks a bare-metal subscriber handle in
a static slot pool:

```rust
// nros-pool: SLOTS = NROS_RMW_SUBSCRIBER_SLOTS * 1024
static SLOTS: [Slot; SLOT_COUNT] = [const { Slot::new() }; SLOT_COUNT];

pub unsafe fn insert<T>(value: T) -> Option<*mut T> {
    if mem::size_of::<T>() > SLOT_SIZE || mem::align_of::<T>() > SLOT_ALIGN {
        return None;
    }
    for index in 0..SLOT_COUNT { /* … claim a free slot, else None */ }
```

`None` has **two causes** and the caller cannot tell them apart:

```rust
#[cfg(all(target_os = "none", not(feature = "std")))]
{
    let Some(ptr) = (unsafe { static_subscriber_storage::insert(sub_handle) }) else {
        return crate::NROS_RMW_RET_BAD_ALLOC;
    };
```

| cause | what it means | remedy |
| --- | --- | --- |
| every slot claimed | the image has more subscriptions than `NROS_RMW_SUBSCRIBER_SLOTS` | raise the knob |
| `size_of::<T>() > 1024` | a backend's per-subscription state outgrew the slot | **no knob fixes it** — `SLOT_SIZE` is a bare literal |

Raising `NROS_RMW_SUBSCRIBER_SLOTS` against the second cause buys nothing and
costs 1 KiB per added slot, on the platform with the least RAM. The knob is the
only lever the message points at, and against half the failures it is the wrong
lever applied in the expensive direction.

## Not silent — that was an earlier misreading

An earlier draft of RFC-0100 called this a silent `create_subscription` failure.
It is not: the return is `NROS_RMW_RET_BAD_ALLOC` and the caller sees it. The
defect is the **conflation**, not the silence, and the RFC is being corrected.
Recorded here because the wrong description is the more alarming one and would
have sent the next reader looking for a missing error path that is present.

## Why a runtime return is the wrong mechanism anyway

`size_of::<T>()` is known at COMPILE time. The size arm of that `if` can never
depend on anything the runtime learns — a given build either fits or never
fits. So the size check is a `const` assertion wearing a runtime return, and
the image that cannot possibly work still links, boots, and fails at
registration.

This is the shape `check-c-array-pool-floors` and its build-tier twin
`check-c-array-guard-probe` already enforce one layer down: a pool's constraint
belongs beside the pool, stated so the compiler refuses. The same argument
applies here, with the extra force that `SLOT_SIZE` guards a Rust generic where
a `const { assert!(…) }` is available and needs no probe harness.

## Scope

Bare metal only — `#[cfg(all(target_os = "none", not(feature = "std")))]`. The
hosted arm `Box`es the handle and has neither limit.

No backend exceeds 1 KiB today, so this is latent. It becomes live the moment a
backend grows its per-subscription state, which is exactly what a sizing
campaign that adds per-endpoint QoS to a handle would do — so the two are worth
landing in the same window.

## What a fix has to decide

* a `const` assertion naming the offending type and its size, so an over-large
  backend state is a BUILD error rather than a boot-time `BAD_ALLOC`; and
* whether `SLOT_SIZE` becomes a knob at all. It is a bare literal today. RFC-0100
  D5 says explicitly that it must NOT be modelled as a derivation — no user fact
  answers "how big is a backend's private state struct" — so if it becomes a
  knob it is an authored one with a compile-time floor, not a derived size.

Separating the two return causes (a distinct code, or a diagnostic beside the
`None`) is worth doing regardless, for the exhaustion case that legitimately
survives to runtime.

## Reproduction

Not reproduced — analysis from the source. A backend whose subscriber handle is
padded past 1 KiB, built for a bare-metal target, registers no subscription and
reports `BAD_ALLOC` with every slot free.
