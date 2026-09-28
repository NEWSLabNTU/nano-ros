---
id: 1322
title: "A backend state struct outgrowing cffi's 1 KiB `SLOT_SIZE` returns the
  same `BAD_ALLOC` as a full pool — one cause has a knob, the other cannot be
  fixed by any knob, and raising it makes the real one worse"
status: resolved
type: bug
area: rmw, embedded
severity: medium
found: 2026-09-11
resolved: 2026-09-27
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

## Resolution — 2026-09-27

**The conflation was separated; the constant was not raised.** Two quantities
shared one lever, so each got its own:

| quantity | knob | mechanism | floor |
| --- | --- | --- | --- |
| how MANY slots | `NROS_RMW_SUBSCRIBER_SLOTS` | DERIVED — `[image] subscriber_count` (RFC-0100 D5) | none; zero is legal (issue 1033) |
| how WIDE a slot | `NROS_RMW_SUBSCRIBER_SLOT_BYTES` | **AUTHORED** over the RFC-0049 ladder: env → `CONFIG_NROS_RMW_SUBSCRIBER_SLOT_BYTES` → `[knobs.rmw] subscriber_slot_bytes` → builtin `1024` | a `const` block in `insert::<T>()`; range `[1, 65536]` |

D5 rules the width out as a derivation *by name*, so it is authored rather than
derived — and `NROS_RUNTIME_COMPONENT_SLOT_BYTES` is the same quantity one layer
up in the component pool, resolved exactly this way, which is the precedent the
ladder wiring follows.

### The size cause left the runtime return

`insert::<T>()`'s `if size_of::<T>() > SLOT_SIZE … return None` is gone. It was a
`const` assertion wearing a runtime return: a given build either fits or never
fits, so the check is now a `const` block, evaluated at monomorphisation. `None`
therefore means EXHAUSTION and nothing else, and the pool says so beside the
`None` with `nros_log` (issue 0589), naming `NROS_RMW_SUBSCRIBER_SLOTS`, the slot
count, and what a further slot costs. No new return code was needed: with one
cause left, `NROS_RMW_RET_BAD_ALLOC` is unambiguous.

**D7's shape, and where each floor landed.** The floor that matters —
`size_of::<R::Subscription>()` — is at the POOL, because the build script cannot
see which backend the image links, let alone that handle's target layout. The
measurement below is exactly why a build-script floor would have been wrong: a
host-side `size_of` is not a target fact.

There is a SECOND, smaller floor, and it is the one a build script *can* answer,
so it stayed there: the range's lower bound is **1**, not 0. This was ruled from
the storage rather than by analogy to 1033, and it comes out the other way from
the COUNT: at width 0 `Slot` is zero-sized, so every element of `SLOTS` shares one
address, and `take::<T>()` identifies a slot by POINTER EQUALITY — it would drop
slot 0 whatever it was handed. Zero also reclaims nothing that
`NROS_RMW_SUBSCRIBER_SLOTS=0` does not already reclaim, so unlike 1033 there is no
saving on the far side of the hazard, and the refusal names that knob as the way
to remove the pool. (At width 1 the `repr(align(16))` pads `Slot` back to 16 bytes
and the addresses are distinct again — so this is about 0 specifically.)

### Measured

`examples/mps2-an385-baremetal/rust/listener` (`qemu-bsp-listener`,
`thumbv7m-none-eabi`, `nros-relwithdebinfo`, zenoh) — the in-tree bare-metal Rust
image that creates a subscription, so the one that instantiates `insert::<T>()`.

**The guard fires, and its diagnostic carries everything a reader needs.** At
`NROS_RMW_SUBSCRIBER_SLOT_BYTES=1`:

```
error[E0080]: attempt to compute `1_usize - 112_usize`, which would overflow
   --> packages/rmw/cffi/src/rust_adapter.rs:180:57
180 |    let _raise_nros_rmw_subscriber_slot_bytes = SLOT_SIZE - mem::size_of::<T>();
    |                                               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
note: the above error was encountered while instantiating
      `fn insert::<ZenohSubscriber>`
```

So **`size_of::<ZenohSubscriber>()` = 112 bytes** on this target, printed by the
compiler rather than asserted in prose. The subtraction is deliberate: a const
panic message cannot be FORMATTED, so an `assert!` could state the remedy and
never the numbers, while the underflow states both. The remedy lives in the
BINDING NAME because rustc echoes the offending source line and **collapses the
rest of the const block to `...`** — measured; a first draft put the remedy in
comments inside the block and it never reached the error text.

The floor is exact, not approximate — a real mutation test of the guard:

| `NROS_RMW_SUBSCRIBER_SLOT_BYTES` | result |
| --- | --- |
| 0 | build-script refusal, `out of range [1, 65536]`, naming `NROS_RMW_SUBSCRIBER_SLOTS=0` as the way to remove the pool |
| 1 | const guard, `1_usize - 112_usize` |
| 64 | const guard, `64_usize - 112_usize` |
| 112 | links |
| 1024 (default) | links |

The two refusals are deliberately at different layers: the build script answers
what it can see (a width of 0 is never a legal SIZE for this storage), and the
const guard answers what only it can see (this backend's handle).

**At the default the image is byte-identical in RAM**, which is the acceptance for
splitting a constant into a knob. Same leaf, before (pre-change tree) and after:

| | before | after | delta |
| --- | --- | --- | --- |
| `.bss` | 539,440 | 539,440 | **0** |
| `.data` | 4,976 | 4,976 | **0** |
| section RAM total | 544,416 | 544,416 | **0** |
| RAM in symbols | 544,370 | 544,370 | **0** |
| every RAM symbol | — | — | **no symbol changed** |
| `.text` | 282,880 | 283,060 | +180 |
| `.rodata` | 44,932 | 45,320 | +388 |

The `.text`/`.rodata` growth is the new exhaustion diagnostic — the one thing this
change adds to a working image. (The only commit between the two trees was
`docs/roadmap/phase-457-*.md`, so the delta is attributable to this diff.)

**What the knob now buys, measured.** With 112 bytes of real handle against a
1024-byte slot there is 9.1× headroom, and lowering the width is now SAFE because
the compiler refuses a value the backend does not fit. At `=128`:

```
1,024  SLOTS  — DECLARED 8,192 at defaults
```

`nros_rmw_cffi::…::SLOTS` 8,192 → 1,024 bytes, **−7,168 bytes** of `.bss`, and
section RAM 544,416 → 537,248 — exactly −7,168. That is the saving the bare
literal made unreachable, on the platform class with the least RAM. It is NOT
applied to any shipped image here: the right width is a per-board statement, and
this issue was about the lever existing and being safe to pull.

### One thing the fix cannot claim

The guard is POST-MONOMORPHISATION, so **`cargo check` does not reach it**
(measured: `rustc --emit=metadata` over the same construct exits 0 — no codegen,
no instantiation collection). What catches an over-large handle is a `cargo build`
of a bare-metal image that creates a subscription, i.e. tier 2 / the nightly, not
the fast line. That is weaker than a source gate and strictly stronger than the
boot-time `BAD_ALLOC` it replaces — the image no longer links. Stated in the
source so the next reader does not assume `just check` covers it.

### The class sweep: one sibling, correctly NOT changed

`git grep 'size_of::<T>() >\|fn fits<'` over `packages/` finds exactly one other
place where a slot pool asks whether a type fits:
`nros::runtime_storage::Slot::fits<T>()` (the component pool, one layer up). It
must stay a RUNTIME check, and its own doc comment already says why: *"the pool is
heterogeneous and the FFI seam cannot name a generic"* — a C component's type is
not known to a `const` block, and different slots hold different types. The cffi
subscriber pool is the opposite: `insert::<T>()` is only ever instantiated with
`R::Subscription`, so per build there is exactly one `T` and its size is a
compile-time constant. Recorded so the next reader does not "finish the sweep" by
turning that one into a const assertion and breaking C component registration.

(`zpico-alloc`'s `SLAB_SLOT_SIZE` is a size CLASS in an allocator, with its own
count and a documented fall-through to the free list for anything larger — not
this class either.)

### A second defect, found by this fix and fixed with it

`gen-pool-inventory.py` recovered a knob's default with patterns ending `\s*\)`,
which only matches a call rustfmt kept on ONE LINE. rustfmt wraps a call whose
arguments exceed `fn_call_width` (60) and adds a trailing comma — and
`NROS_RMW_SUBSCRIBER_SLOT_BYTES`'s arguments are 67 characters, so merely naming
the knob took the `SLOTS` pool's byte figure off
`book/src/reference/static-pool-inventory.md`: it read "not priceable statically
(knob … has a computed default)". Issue 0271's enumeration failure reached through
a formatting rule rather than through a new wrapper.

Fixed in the SCANNER, not by a formatting rule nobody could see the reason for:
one shared `_C = r"\s*,?\s*\)"` on all five closing-paren patterns. The page now
reads `SLOTS | 8,192 | NROS_RMW_SUBSCRIBER_SLOTS * NROS_RMW_SUBSCRIBER_SLOT_BYTES`
and lists the new knob at its default. `scripts/nros-mem-report.py` imports this
scanner, so both consumers were fixed by the one edit.

### Files

* `packages/rmw/cffi/src/rust_adapter.rs` — `SLOT_SIZE` off the literal; the
  `const` fit guard; the runtime size/align arm deleted; the exhaustion
  diagnostic; the `// nros-pool:` formula now naming both knobs.
* `packages/rmw/cffi/build.rs` — `emit_subscriber_slot_bytes`, and the header's
  D5 note corrected from "not a knob" to "an authored one".
* `packages/tooling/nros-platform-config/src/platform_config.rs` — `RmwKnobs`
  gains `subscriber_slot_bytes` (struct, `RMW_KNOBS`, `rmw_env_key`,
  `platform_rmw_knobs`, `rmw_rungs`, `resolve_rmw`). Ladder count 46 → 47.
* `packages/cli/nros-cli-core/src/cmd/config.rs` — the `nros config explain` row.
* `scripts/check/check-knob-single-reader.py` — one reader, declared.
* `scripts/gen-pool-inventory.py` — the trailing-comma fix above.
* RFC-0100 D5 and phase-454's "explicitly not in this phase" now record the
  settled shape instead of the open defect.


---

# The measurement the fix landed without (2026-09-28)

PR #1346 shipped this split with its acceptance — a before/after RAM delta —
**not taken**: the agent that wrote it was terminated by a session limit one step
before measuring, and the PR said so rather than implying otherwise. Taken now.

## What the lever does, measured

The knob moves, through the RFC-0049 env rung, read off the build script's own
output:

```
$ touch packages/rmw/cffi/build.rs && cargo build -p nros-rmw-cffi
NROS_RMW_SUBSCRIBER_SLOT_BYTES=1024          # builtin

$ touch packages/rmw/cffi/build.rs \
    && NROS_RMW_SUBSCRIBER_SLOT_BYTES=256 cargo build -p nros-rmw-cffi
NROS_RMW_SUBSCRIBER_SLOT_BYTES=256
```

The `touch` is not ceremony: the build script is cached, and the first attempt at
this measurement read a stale `output` file predating the split and concluded the
knob was never emitted.

## What that is worth, and why the arithmetic IS the byte count

The pool is a fixed-size array:

```rust
static SLOTS: [Slot; SLOT_COUNT] = [const { Slot::new() }; SLOT_COUNT];
// nros-pool: SLOTS = NROS_RMW_SUBSCRIBER_SLOTS * NROS_RMW_SUBSCRIBER_SLOT_BYTES
```

so its size is exactly `COUNT x WIDTH` — not an estimate, and the published
inventory already prices it (`static-pool-inventory.md`: `SLOTS | 8,192`).

| `SLOT_BYTES` | pool | vs builtin |
| --- | --- | --- |
| 1024 (builtin) | 8 x 1024 = **8,192 B** | — |
| 256 | 8 x 256 = **2,048 B** | **-6,144 B** |

**The default is unchanged**, which is the other half of the result: an image
that does not set the knob is byte-identical across this fix. The split cost
nothing and bought a lever.

## Why the lever is the point, not the 6 KiB

Before it, `SLOT_SIZE` was a bare `1024` serving two quantities, so the only
response to a handle that did not FIT was raising `NROS_RMW_SUBSCRIBER_SLOTS` —
which buys more slots **of the same too-small width**, at 1024 bytes each, on the
platform with the least RAM. That is not a fix in any direction: the handle still
does not fit and the image is larger. Widening was unreachable and narrowing was
unreachable; both are one knob away now.

## Honest limit

**No end-to-end `mem-report` on a linked image.** The pool's size is a compile-time
constant in a `static` array, so the table above is exact for the pool itself —
but it is not the same evidence as a linked `.bss` delta, and this note should not
be read as if it were. What a linked image adds is whether `--gc-sections` drops
the pool in an image that creates no cffi subscription, which is a real question
and is unanswered here.
