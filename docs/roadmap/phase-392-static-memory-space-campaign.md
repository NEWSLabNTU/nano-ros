# Phase 392 — 27% of a safety-island image is message buffers nobody can price

**Status (2026-08-26). Survey + plan, nothing landed.** Opened from a
memory-allocation review that measured a real 320 KiB-class board image. Sizes
below are `nm` output from `build-board/zephyr/zephyr.elf` on
mr_canhubk3/s32k344 (zenoh over serial), not estimates. Depends on
[phase 390](phase-390-storage-mode-rename-inline-heap-view.md) for vocabulary
and [phase 391](phase-391-allocation-unification-and-tier-model.md) for the
gate that verifies the claims.

## Where the RAM goes

| bytes | symbol | kind |
| --- | --- | --- |
| 49,152 | `nros_rmw_zenoh::shim::subscriber::SMALL_PAYLOADS` | wire buffers |
| 32,768 | `nros_thread_stacks` | stacks |
| 30,080 | `__nros_comp_buf_0..3` | deserialised components |
| 19,944 | `g_sessions` | zenoh-pico |
| 17,712 | `SERVICE_BUFFERS` | wire buffers |
| 16,460 | `kheap__system_heap` | the heap |
| 12,288 | `rust_adapter::static_subscriber_storage::SLOTS` | subscriber storage |
| 8,192 | `LARGE_PAYLOADS` | wire buffers |
| 3,584 | `MESSAGE_INFO_TABLE` | |
| 2,640 | `SUBSCRIBER_BUFFERS` | ring metadata |

**Message buffers total 123,648 B — 27% of the 458,752 B of SRAM+DTCM.**

A separate 27,760 B is Ethernet rings, `net_buf` pools and a TCP connection
slab, in an image whose only transport is a serial line.

For scale, one measurement already banked outside this phase: the libc malloc
arena was 24,576 B of `.bss`, `malloc_prepare` ran at boot to initialise it,
and `malloc` itself had been garbage-collected because nothing calls it. Setting
`CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=0` moved `.bss` from 367,566 to 343,010 —
**7.7% of SRAM held by a heap with no allocator**, invisible until someone
listed symbols by size. That is the shape of everything below.

## The three levers, in order of leverage

### 1. Wire buffers — 48 bytes of RAM per byte of knob

```
SMALL_PAYLOADS = MAX_SUBSCRIBERS x RING_DEPTH x SUBSCRIPTION_BUFFER_SIZE
               = 12 x 4 x 1024 = 49,152
```

Every byte of `SUBSCRIPTION_BUFFER_SIZE` costs 48 bytes, because the buffer is
uniform across every subscriber regardless of what each one carries.

Codegen already knows each subscription's type, and therefore its maximum
serialised size. **Sizing each subscriber's buffer to its own type** instead of
to a global constant is the largest single win available, and it needs no
allocator — the buffers stay static.

Half the mechanism already exists: `MAX_LARGE_SUBSCRIBERS` /
`SUBSCRIBER_LARGE_SIZE` is a two-class split (1x4x2048 large, 12x4x1024 small).
It is simply **decoupled from codegen**, so a human picks which subscribers are
"large".

### 2. Component buffers — 1:1 with per-field storage mode

```rust
// packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs:390
"alignas(::{cls}) static unsigned char __nros_comp_buf_{i}[sizeof(::{cls})];"
```

`sizeof(component class)`, which inlines its deserialised message members. This
is the storage that RFC-0033's per-field `mode` actually moves — `heap` and
`view` shrink it, `inline` does not.

**The distinction that decides this phase:** wire buffers hold *serialised* CDR
and are unaffected by `mode`; component buffers hold *deserialised* messages and
are affected 1:1. Conflating them is how a field-mode change gets predicted to
save 49 KiB and saves none of it.

### 3. Executor arena — a 4.9x hand-tuned guess

[Issue 0810](../issues/0810-executor-arena-sized-by-worst-case-shape.md): the
derivation budgets every slot at `sizeof(ActionClient)`, giving 254,720 B for a
board that registers no action clients; the image ships a hand-picked 52,224 B.
Unchecked in both directions, and undersizing fails at runtime.

## Waves

**W1 — pool inventory to full coverage.**
[Issue 0815](../issues/0815-pool-inventory-prices-3-of-46-knobs.md): 46 knobs
found, 3 priced, **66,304 bytes of unpriced pools** — more than the 57,344 that
is priced. Annotate the rest; add a gate rejecting new unannotated pools.
`__nros_comp_buf_N` cannot carry a static annotation (it is generated from
`sizeof`), so the generator emits its figure instead. Do this first: it is the
instrument every later wave is measured with.

**W2 — precise executor arena.** Entry codegen emits `NROS_ARENA_REQUIRED` as
the sum of *actual* entry sizes; `static_assert` against `ARENA_SIZE` moves the
failure from runtime to build. Encoding the requirement as a linker symbol whose
*size* is the figure lets `nm` check it across the C/Rust boundary without
running anything.

Hand-written `main`s create entities at runtime, have no generated entry, and
cannot be sized statically. **This wave explores that case rather than assuming
it away**: the likely answer is a runtime high-water mark reported at teardown
plus a CI lane that fails when it exceeds the configured arena — the generated
path proves its number statically, the hand-written path measures it, and both
report through one figure.

**W3 — per-subscriber wire sizing.** Lever 1. Requires W1 so the saving is
measured rather than asserted.

**W4 — drop the network stack from serial images.** 27,760 B. Needs triage
first: whether zenoh-pico's Zephyr layer needs the net *headers* only, or the
pools too.

## Explicitly out of scope

**Moving payload buffers to the heap.** It would convert `12 x 4 x 1024` of
always-reserved RAM into peak-of-concurrent, which is a real saving, and it is
declined deliberately. A statically provable buffer would become an allocation
that can fail mid-callback, and it would widen the heap's block-size range from
infrastructure-only (~2^6) to payload-inclusive (~2^16) — which is precisely
what makes [phase 391](phase-391-allocation-unification-and-tier-model.md)'s
constant-time allocator sizeable. The two decisions are coupled; this is the
side of the coupling that keeps both defensible.
