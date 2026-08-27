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

**Amended 2026-08-27 — "annotate the rest" is not achievable; the instrument
measures instead. Landed.** All four unpriced pools fail for the same reason,
and `__nros_comp_buf_N` is not the exception this wave assumed, it is the rule:
`SERVICE_BUFFERS` is a product including `ZPICO_MAX_QUERYABLES`, whose default is
*computed*, so there is no integer to write down; `MESSAGE_INFO_TABLE`'s element
gains three fields under `alloc` + `safety-e2e`, which is why [issue
0739](../issues/0739-static-pool-inventory-not-enumerable.md) declined to
annotate it and was right to; `SUBSCRIBER_BUFFERS` is an array of structs. The
size is known to the COMPILER, not to a comment, and a hand-written figure in a
comment is the drift class this tree already gates against
(`check-ffi-struct-mirrors`).

So W1 shipped as `scripts/nros-mem-report.py` / `just mem-report <elf>`: it reads
a built image's symbol table and attributes RAM by symbol, by crate and by
declared pool, with the unattributed gap called out. The declared and measured
mechanisms compose rather than compete — `--check` joins each `// nros-pool:`
formula to its measured symbol and requires agreement on a default-built image,
which turns the inventory's published figures from a claim into a checked fact
(gate `check-mem-report`, plus the fixture-backed test
`static_memory_declared_pools`). W3 is unblocked: a saving can now be reported as
a measured delta between two `--json` runs.

The first thing it measured is [issue
0827](../issues/0827-unused-rmw-pools-dominate-static-ram.md) — static RAM is a
property of the RMW, not of the node, identical to the byte across four roles,
and a talker reserves 80% of its static RAM in pools it cannot reach.

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

**Surveyed 2026-08-27. The mechanism is more built than this doc assumed, and
the missing piece has a language reason.**

What already exists, end to end: `rx_buffer_hint` on `TopicDesc` and on
`rmw_subscription_options_t`; `alloc_payload_block(hint)` in the zenoh shim,
which picks the large class when the hint exceeds
`ZPICO_SUBSCRIBER_SIZE_THRESHOLD` (2048); and, from phase 380,
`M::MAX_SERIALIZED_SIZE_XCDR1`/`_XCDR2` as PROVIDED consts computed from the
schema, plus `size::bound_fits::<M>` which takes the larger of the two.

What is missing is that **nothing sets the hint**. The only setter in the tree
is one bench site; `rust_adapter` passes a literal `0`. So every real
subscription takes the small class, and the large pool — 2 x 4 x 16384 =
131,072 B, already reserved — sits unused.

The cost of that shows up in the build error `create_subscription` raises when a
type does not fit: *"Raise the knob to at least the type's bound."* That knob is
GLOBAL. For a 4 KiB message type:

| remedy | SMALL_PAYLOADS | delta |
| --- | ---: | ---: |
| today: raise `ZPICO_SUBSCRIBER_BUFFER_SIZE` 1024 -> 4096 | 8 x 4 x 4096 = 131,072 | **+98,304 B** |
| route it to the large class instead | 8 x 4 x 1024 = 32,768 | **0** — the large pool is already there |

And it is charged twice: `NROS_SUBSCRIPTION_BUFFER_SIZE` sizes the executor
arena entry as well, so raising it grows every arena slot too.

**Why the split is not one wave.** The arena entry is
`SubInfoEntry<M, F, const RX_BUF: usize>`, and on stable Rust an associated
const of a type parameter cannot be used as a const-generic argument
(`error: generic parameters may not be used in const operations`, checked on
edition 2024). So:

- **W3a — route the zenoh block by the type's bound.** `rx_buffer_hint` is a
  runtime `usize`, so `create_subscription::<M>` can pass
  `max(XCDR1, XCDR2)` with no unstable feature. A type between the small size
  and `ZPICO_SUBSCRIBER_LARGE_SIZE` stops being a build error and starts being
  a large-class subscriber. Unbounded types keep the default: phase 380 is
  explicit that `None` means "no bound exists", never "unknown" — do not size a
  buffer from a fallback.
- **W3b — arena sizing, and only codegen can do it.** At a generated call site
  `M` is concrete, so codegen can emit the bound as the const-generic argument;
  a generic library function cannot. That is the real reason the size class is
  "decoupled from codegen" today, and it is a language constraint rather than an
  oversight.

**W4 — drop the network stack from serial images.** 27,760 B.

**TRIAGE ANSWERED (2026-08-27): headers only.** zenoh-pico's Zephyr layer needs
Zephyr's networking HEADERS at compile time and does not pull the pools. The
27,760 B is enabled by the image's own Kconfig, not by the transport.

Three independent lines of evidence:

*1. Kconfig dependency chains.* `config NROS_RMW_ZENOH` (zephyr/Kconfig) has NO
`depends on NET_SOCKETS` and selects nothing networking. Its siblings do —
`NROS_RMW_XRCE` is `depends on NET_SOCKETS`, `NROS_RMW_CYCLONEDDS` is
`depends on NET_SOCKETS && POSIX_API && CPP`. `NROS_ZENOH_LINK_SERIAL` has no
networking dependency either, and `NROS_TRANSPORT_SERIAL` only
`select NROS_ZENOH_LINK_SERIAL`. So nothing in our Kconfig requires networking
for a zenoh serial image.

*2. The #include graph.* In zenoh-pico's `src/system/zephyr/network.c`, `<netdb.h>`
and `<sys/socket.h>` are already guarded by `#if defined(CONFIG_NET_SOCKETS)`.
`<zephyr/net/net_if.h>` is NOT guarded — that is the one wart — but every
`net_if_*` USE is: all 19 call sites sit inside link-feature guards
(`Z_FEATURE_LINK_UDP_MULTICAST` and friends), 0 unguarded, checked by walking
the preprocessor stack rather than by eye. So on a serial build no networking
code is compiled; only a header is included.

*3. Symbols in a built image.* `zephyr-workspace/build-cortex-m-c-talker-zenoh`
(mps2/an385, zenoh over TCP) carries **22,580 B** of networking RAM — the same
order as the mr_canhubk3 figure on a different board/config. The largest are
`_k_mem_slab_buf_tcp_conns_slab` 9,600, `net_buf_data_rx_bufs` 4,096,
`net_buf_data_tx_bufs` 4,096. Every one is a Zephyr net-subsystem symbol; none
belongs to zenoh-pico.

And the pools have a named source: `examples/zephyr/c/talker/prj-zenoh.conf`
sets `CONFIG_NET_TCP=y`, `NET_PKT_RX/TX_COUNT=32`, `NET_BUF_RX/TX_COUNT=64`.
That is the image's config, correct for a TCP image and simply inherited by
anything that copies it.

**So the fix is conf-level, not code-level**, and needs no vendored change: a
serial image should not enable `NETWORKING`/`NET_TCP`/`NET_PKT_*`/`NET_BUF_*`.

**One caveat that still needs a build to settle.** Because
`#include <zephyr/net/net_if.h>` is unconditional, a serial image still needs
Zephyr's net headers to COMPILE with `CONFIG_NETWORKING=n`. Zephyr ships those
headers unconditionally and they are declaration-only, so this is expected to
hold — but it is not proven here, and if it does not hold the remedy is guarding
that include in zenoh-pico, which is VENDORED and must be reported rather than
patched in place.

**NOT MEASURED, and deliberately not guessed.** The mr_canhubk3/s32k344 board is
not in this tree — no board directory, no conf, no `build-board/` — so its image
cannot be built or measured here. `scripts/nros-mem-report.py` and
`just mem-report` do not exist in this tree or on `origin/main` either, so no
`--json --baseline` delta was available. Per this phase's own rule that no wave
claims a saving it did not measure, the 27,760 B remains the originally reported
figure and this wave contributes the triage plus the 22,580 B cross-check above,
not a new saving.

## Explicitly out of scope

**Moving payload buffers to the heap.** It would convert `12 x 4 x 1024` of
always-reserved RAM into peak-of-concurrent, which is a real saving, and it is
declined deliberately. A statically provable buffer would become an allocation
that can fail mid-callback, and it would widen the heap's block-size range from
infrastructure-only (~2^6) to payload-inclusive (~2^16) — which is precisely
what makes [phase 391](phase-391-allocation-unification-and-tier-model.md)'s
constant-time allocator sizeable. The two decisions are coupled; this is the
side of the coupling that keeps both defensible.
