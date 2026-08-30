# Phase 392 — 27% of a safety-island image is message buffers nobody can price

**Status (2026-08-30). W1, W3 and most of W5 landed; W2 is now issue 0900.**
Opened from a memory-allocation review that measured a real 320 KiB-class board
image.

Landed since: **W1** (pool inventory to full coverage, plus `just mem-report`
and the `static_memory_declared_pools` test that makes a published pool figure
answer to the linker), **W3a/W3b** (a subscription buffer sized from the
message's own `MAX_SERIALIZED_SIZE_*` rather than by hand — Rust only; the
C/C++ half is [issue 0896](../issues/0896-c-cpp-subscriptions-never-state-a-buffer-hint.md)),
and **W5.b1/c/d/e/f** (the queryable table sized by declaration — 143,456 B off
a talker — with an exhausted table that names the declaration and a knob that is
checked at build time).

Still open: **W5.g**, which is the MEASUREMENT of W5 on a real cmake workspace
image and has not been run; W5.d's 143,456 B was measured on a pure-cargo leaf
that this wave changes by zero bytes. Sizes
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

## Amendment 2026-08-29 — four additions from a board measurement

Added after a session that took the mr_canhubk3/s32k344 action image from
98.73 % SRAM (non-functional: it could not be instrumented) to 85.60 % and
working. Three of the four were absent from this campaign entirely.

### A. Tightly-coupled memory was never a placement target

This document counts `458,752 B of SRAM+DTCM` in its denominator and then never
places anything in the DTCM half. Measured on the action image:

```
RAM   323 528 / 327 680   98.73 %
ITCM        0 /  65 536    0.00 %
DTCM        0 / 131 072    0.00 %
```

192 KiB idle on the same die while the image would not fit. Both regions were
already declared — the devicetree gives them `zephyr,memory-region` and Zephyr's
linker script emits matching `NOLOAD` sections — and nothing in the tree had
ever placed a symbol in either.

[Issue 0880](../issues/0880-tcm-unused-while-sram-exhausted.md) and
`CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM` land the first tenant: the 48 KiB task stack
array. SRAM 98.73 % → 85.60 %, DTCM 0 % → 37.5 %, board boots and reaches its
ready state with zero faults.

**80 KiB of DTCM is still free**, which changes the arithmetic of every lever
below: a pool that cannot be shrunk may still be *moved*.

The constraint that decides what may move: on Cortex-M7 the TCMs hang off the
CPU's private bus and are typically **not reachable by other bus masters**. A
buffer a DMA engine touches must not go there. Stacks are safe by construction;
`LARGE_PAYLOADS` is only safe while the link stays polled/ISR, and
[issue 0852](../issues/0852-zephyr-serial-rx-is-polled-and-overruns.md)'s fix
direction includes eDMA. **Verify reachability before moving any buffer.**

### B. Pools into the tiered arena — the sharing this campaign does not model

Every wire and component pool is sized for its own simultaneous worst case, and
the totals are then added. `SMALL_PAYLOADS` assumes twelve subscribers each four
deep each full; `SERVICE_BUFFERS` assumes every queryable in flight at once.
**They do not peak together, and nothing in this campaign captures that.**

Levers 1 and 2 above shrink each pool against its own worst case. They do not
address the worst cases being summed rather than overlapped. One arena behind
`nros_platform_alloc` (phase 391) sized for the *aggregate* peak instead of the
*sum of individual* peaks is a different and possibly larger win, and it is the
question this campaign has not asked.

What it costs, and why this is a wave rather than a decision:

- **Fragmentation.** Static pools cannot fragment. rlsf's bound is
  `1/SLLEN` internal, but external fragmentation across mixed lifetimes is a
  property of the traffic, not of the allocator. Needs measurement, not
  argument.
- **An allocator call on the RX hot path.** Bounded with rlsf, not free. The
  serial RX path already allocates twice per frame
  (`_Z_SERIAL_MAX_COBS_BUF_SIZE` + `_Z_SERIAL_MFS_SIZE`), so on that path this
  would not be a new class of cost — but on the subscriber path it would be.
- **The bare-metal tier must stay heap-free** (RFC-0034). So this is
  **tier-gated**, never universal: the `inline` tier keeps static pools, and
  only tiers that already admit an allocator may share the arena.

**Do lever 1 first regardless.** Per-type sizing shrinks the pools whether or
not they later share an arena, and a smaller worst case makes the sharing
question cheaper to answer.

**This REOPENS a question this document had closed, and the two texts must not
be left disagreeing.** "Explicitly out of scope" below declines moving payload
buffers to the heap, and it is the same move. Recorded 2026-08-29, on a rebase
that brought the two into one file.

The framing here is better — tier-gated rather than universal, aggregate peak
rather than risk appetite, a wave rather than a decision — and it answers one of
the two grounds the refusal rested on: an allocation that can fail mid-callback
is confined to tiers that already admit an allocator.

It does not answer the other, which is the stronger one. Sharing widens the
arena's block-size range from infrastructure-only (~2^6) to payload-inclusive
(~2^16), and that range is precisely what makes [phase
391](phase-391-allocation-unification-and-tier-model.md)'s constant-time
allocator sizeable. The refusal was not about whether pooling wastes RAM — it
plainly does — but about the cost landing on the allocator that phase 391 is
built around.

So the question is live, not settled either way, and the deciding measurement is
NAMED rather than argued: what the wider block-size range costs rlsf in control
words and in its `1/SLLEN` bound, on a real image, against the aggregate-vs-sum
saving it buys. Until that exists, neither section may be treated as the
campaign's position, and lever 1 proceeds regardless — which both texts already
agree on.

### C. Field storage mode does NOT shrink wire buffers — restated, because it keeps being proposed

Lever 2 above already draws this distinction. Restating it as a decision record
because the opposite has now been proposed twice:

> Offloading a large field to `heap` will reduce the message size and therefore
> the payload buffer.

**It will not.** A `heap` field changes where the *deserialised* value lives; the
*serialised* CDR on the wire is byte-identical. `SMALL_PAYLOADS`,
`LARGE_PAYLOADS` and `SERVICE_BUFFERS` hold serialised bytes and are unmoved by
any per-field mode. `__nros_comp_buf_N` holds the deserialised struct and shrinks
1:1.

The idea underneath it is still right, but it is lever 1, not lever 2: wire
buffers should be sized **per subscriber from its own type's maximum serialised
size**, instead of every subscriber paying a global constant. Today
`LARGE_PAYLOADS` is not computed from any message size at all — it is
`MAX_LARGE_SUBSCRIBERS x RING_DEPTH x SUBSCRIBER_LARGE_SIZE`, three constants a
human picks.

### D. Flash is 4 MiB at 8.3 %, and it is not fungible with RAM

Worth stating so it stops being re-proposed as capacity relief. There is no MMU
and no demand paging: **flash cannot back RAM on this part.** Code is already
XIP, `.data` is 3,564 B, and read-only data already lives in flash — so there is
no copy to eliminate.

What the spare flash IS good for, in order of value to this campaign:

1. **A post-mortem fault log.** Persist the fault dump — PC, LR, thread name,
   stack sentinel state — across reset. This directly serves issue 0852, whose
   whole difficulty has been that the board is at 96 %+ SRAM and cannot afford
   the instrumentation needed to observe its own crash. A flash log costs no
   SRAM at all.
2. **Parameter and configuration storage**, removing any temptation to hold
   defaults in RAM.
3. **ITCM relocation** (`CONFIG_CODE_DATA_RELOCATION`) — flash to ITCM for hot
   paths. A determinism lever, not a capacity one, and the 64 KiB of ITCM is
   still entirely unused.

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

**W2 IS [issue 0900](../issues/0900-arena-slots-budgeted-at-action-client-worst-case.md),
and its runtime half has landed.** The issue was filed from the other end —
measuring that `ARENA_SIZE` is 74,240 bytes on every generated config in the
tree, a talker included, while a timer-only executor claims **32 bytes** — and
only then met this wave. Recorded here so the two do not diverge into separate
efforts against one defect.

Delivered against the plan above:

* the runtime measurement this wave predicted — `Executor::arena_used()` /
  `arena_capacity()` plus a one-shot first-spin advisory naming the
  `NROS_EXECUTOR_ARENA_SIZE` value to set. The arena is a BUMP allocator, so
  that figure is exact rather than a high-water estimate;
* `NROS_EXECUTOR_ACTION_CLIENTS`, which stops every slot being budgeted at the
  ActionClient worst case (74,240 -> 16,384 at the defaults, with the default
  byte-identical to the old formula so no image moves).

Still owed by this wave: the STATIC half — `NROS_ARENA_REQUIRED` emitted by
entry codegen and checked by `nm` — and a CI lane. Note the correction issue
0900 records: the arena is **inline on the task stack**, not in `.bss`, so
`mem-report` cannot see it and a linker-symbol check will not either. Sizing it
needs a stack probe, which this wave's `nm` plan did not anticipate.

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

What was missing, at survey time, is that **nothing set the hint**. The only
setter in the tree was one bench site; `rust_adapter` passed a literal `0`. So
every real subscription took the small class, and the large pool — 2 x 4 x 16384
= 131,072 B, already reserved — sat unused.

**Corrected 2026-08-29: the RUST half of that is fixed and the C/C++ half is
not.** `2adf2b739` wired `create_subscription::<M>` to pass
`subscription_rx_hint::<M>(RX_BUF)` — the type's own bound, falling back to
`RX_BUF` only where no bound exists. A Rust subscription to a 4 KiB type now
routes large.

A C or C++ subscription to the same type still hints 0. The field is declared,
`rust_adapter` reads it, the shim routes by it — and no source under
`packages/api/nros-c`, `packages/api/nros-cpp`, `packages/cli/rosidl-*` or
`examples/` writes it. The producer is the Rust executor and nothing else, so
W3a's saving applies to half the tree.
[Issue 0896](../issues/0896-c-cpp-subscriptions-never-state-a-buffer-hint.md)
carries the survey and the constraint that decides the design: the bound is a
PROVIDED const computed from `FIELDS`, a C message has no such trait, and
whatever carries the number to the C call site must not become a SECOND
computation of it — that is the sizes-header mirror class
(0088 -> 0114 -> 0122 -> 0123 -> 0245 -> 0268) one lane over.

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

- **W3a — route the zenoh block by the type's bound. LANDED for Rust
  (`2adf2b739`); the C/C++ half is issue 0896 and is NOT done.**
  `rx_buffer_hint` is a
  runtime `usize`, so `create_subscription::<M>` can pass
  `max(XCDR1, XCDR2)` with no unstable feature. A type between the small size
  and `ZPICO_SUBSCRIBER_LARGE_SIZE` stops being a build error and starts being
  a large-class subscriber. Unbounded types keep the default: phase 380 is
  explicit that `None` means "no bound exists", never "unknown" — do not size a
  buffer from a fallback.
- **W3b — arena sizing, at any site where the type is named. LANDED.** The
  constraint is narrower than "only codegen": a *generic parameter* may not
  appear in a const operation, but a *concrete type's* associated const may, and
  that compiles on stable (checked, edition 2024). `emit_rust.rs` turns out to
  emit no subscriptions at all — the Rust call site is user code — so the fix is
  `nros::rx_buffer_for!(Msg)`, expanding at whatever site names the type:

  ```rust
  node.subscription::<PointCloud2>("points")
      .rx_buffer::<{ nros::rx_buffer_for!(PointCloud2) }>()
      .build(on_cloud)?;
  ```

  `.rx_buffer::<N>()` already existed; what was missing is a number that cannot
  drift. A literal is correct until a field is appended, after which the sample
  is received, ACKed and dropped at the transport — the failure
  `report_dropped_take` describes and that needs a packet capture to attribute.
  An unbounded type expands to `NROS_SUBSCRIPTION_BUFFER_SIZE`, not to an
  invented number, because phase 380 forbids sizing a buffer from a fallback.

  Tested from OUTSIDE the crate (a macro body resolves in the caller, so an
  in-crate test would see private names a consumer cannot), including use in
  const-generic position — the property the whole wave exists for.

**W3 has no MEASURED saving yet.** W3a changes which size class a subscription
routes to; it does not by itself shrink a pool, and the 98,304 B in the table
above is an avoided COST on a hypothetical 4 KiB type, not an observed delta on
a real image. Per this phase's rule, W3 claims nothing until `just mem-report
--json --baseline` shows it on an image with a type large enough to route
differently — and no example in the tree has one today, which is its own finding
about the fixture corpus rather than about the wave.

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

**W5 — queryable pools sized by declaration, not by guess.** Lever 1, and the
largest single figure this phase has measured: 144,128 B on a native talker,
39% of its static RAM, in service buffers for services it does not have.

`ZPICO_MAX_QUERYABLES` decides that pool (it sizes `SERVICE_BUFFERS` as
`ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES`, and the C shim's queryable table
alongside). Its default is `if hosted { 32 } else { 8 }` — a literal chosen for
headroom in `nros-zpico-build`, because at that point nothing knows the answer.
Six inputs decide the right number and none of them meet: the app's service
count (known only to the resolved model), the parameter services (6) and
lifecycle services (5) (known only to `nros-node`, behind cargo features),
`ZPICO_MAX_SESSIONS`, the hosted/embedded split, and the literal.

### The shape

One declaration site, two front-ends, one consumer.

```
                    system.toml + launch files
                              |
                       nros sync resolves
                              |
                    +---------v---------+
                    |   SystemModel     |  app service-server count
                    | (build artifact)  |  features = [param_services, lifecycle]
                    +----+---------+----+
              Rust entry |         | C/C++ entry
            nros::main!  |         | nano_ros_entry()
                         v         v
                  one declared figure, delivered as env to cargo
                              |
                              v
             nros-zpico-build  -- sizes --> C shim queryable table
                               `- sizes --> SERVICE_BUFFERS (Rust pool)
                                    ^
                                    | adds, from nros-node
                     PARAM_SERVICE_QUERYABLES / LIFECYCLE_SERVICE_QUERYABLES
```

DECLARED, from the model: how many service servers the application has, and
whether the infrastructure services exist. DERIVED, from Rust: how many
queryables each of those features costs. That split is what keeps issue 0460
closed — codegen sees the user's entities and never the runtime's, so it must
never own the second number.

### Why not const generics

Checked, because the W5 endgame in [phase
391](phase-391-allocation-unification-and-tier-model.md) sized component cells
exactly that way and the parallel is tempting. `SERVICE_BUFFERS` is a private
`static mut`: no header, no `#[no_mangle]`, no `repr(C)`. C round-trips one
opaque `*mut c_void` token (`session_index * ZPICO_MAX_QUERYABLES + local`) that
it never does arithmetic on, and bounds its own handles against its own table.
So the two tables are NOT layout-coupled — this is not the issue-0135 class, and
a const generic would reach no non-Rust consumer.

It is still the wrong tool. A const generic needs a TYPE to carry the bound. A
Rust entry has one (`Node::ENTITY_BOUNDS`); a C/C++ entry does not — it is
cmake-driven through `nano_ros_entry()`. Manufacturing one leaves two exits,
both bad: a non-generic C entry point that picks some N (a hand-picked number
again, which is the thing being removed), or a sizing parameter on the C API,
which stops it being a thin wrapper of Rust.

### The channel already exists

`*_OPAQUE_U64S` is the established thin-wrapper channel and it runs Rust -> C:
Rust owns the type, a build step computes `size_of`, a generated header carries
the number, C declares opaque storage. This need runs the other way — the
application declares, the backend consumes — which is the same direction W2's
`NROS_ARENA_REQUIRED` needs.

The delivery mechanism is proven, not hypothetical: phase-351 W5's
`nros_resolve_board_facts` resolves facts through a CLI verb and attaches them
with `corrosion_set_env_vars`, which reaches the cargo invocation where
`set(ENV{...})` does not (issue 0460). A declared entity figure is one more fact
on that path, and `nros ws model-dims` is the existing seam for asking the model
a question from one implementation rather than a second one in cmake.

Net C/C++ API change: none. No function, no parameter, no generic in a header.

### Waves

* **W5.a — the counts get one definition.** LANDED. They had seven spellings and
  none was a definition; two were wrong, both saying lifecycle was 6 (it is 5,
  so the widely-quoted "twelve slots before the application declares anything"
  is eleven), including the message a user sees when the table overflows.
  `check-infra-queryable-counts` ties each constant to the number of creation
  sites, because a constant alone is still a hand-typed literal that drifts the
  same way the prose did.

* **W5.b — the model answers the question.** SPLIT, after looking at what the
  model actually contains. This wave was written assuming the resolved model
  could answer both halves. It cannot, and the difference decides who can do
  the work.

  MEASURED — every resolved model in the tree
  (`examples/*/build/nros/models/*/*.yaml`), full key set: `meta`, `structure`
  (`scopes`, `nodes` -> `pkg`, `exec`, `node_name`, `params`, `remaps`,
  `lifecycle_autostart`, `scope`), `execution` (`deploy`, `features`, `tiers`,
  `bridges`, `bindings`). There is NO entity inventory: a node's publishers,
  subscriptions and service servers appear nowhere.

  * **W5.b1 — the infrastructure flags.** LANDED. `execution.features` carries
    `param_services` and `lifecycle` verbatim, which is exactly the half a build
    script cannot otherwise see (cargo exposes no other crate's features).
    `nros ws entity-facts` reports it; W5.c delivers it; the consumer adds what
    those features COST rather than being told. Unknown names are ignored, not
    refused — `safety` is a real feature here and is not a queryable question.

    Measured over every resolved model in the tree: 92 `none`, 22
    `param+lifecycle`, 1 `lifecycle`. On a model declaring neither, the table
    goes 32 -> 8 slots; on one declaring both, 32 -> 19.

  * **W5.b2 — the application's own service-server count.** NOT AVAILABLE, and
    this is the THIRD reading of the same question. The first draft said the
    model could not answer it and proposed extending the spec. The second said
    `ros-launch-manifest` already modelled it, so it was available. Both were
    wrong, in opposite directions, and the truth is in between.

    The SPEC models it: `structure.services` maps a service FQN to
    `ServiceWiring { srv_type, server: Vec<String>, client }`, `structure.actions`
    is the same type, and rlm's own `service-wiring` rule already reasons over
    `!svc.server.is_empty()`. Extending it would have been wrong as well as
    unnecessary — **rlm is the platform-neutral SPEC, `play_launch` is its Linux
    implementation, and neither may carry nano-ros concerns.** Service wiring is
    a general ROS graph concept; a static-RAM sizing rule is not, and stays in
    this tree's build layer.

    The RESOLVER never emits it. MEASURED: zero of the 115 resolved models in
    this tree carry ANY layer-1 wiring — no `topics`, no `services`, no
    `actions`. A plain `<node>` launch element names a node; it does not say
    what that node serves, and nothing else in the resolver's inputs does
    either. `nros metadata`'s component sidecar does not either: it carries
    name, class, sources and callback groups, no endpoint inventory.

    So an absent `services` map does NOT mean "this system has no service
    servers". `examples/workspaces/c`'s `service_server_model.yaml` describes
    exactly one node, called `add_server`, and carries no `services` key at all.

    **`nros ws entity-facts` therefore ABSTAINS rather than reporting a zero it
    cannot support.** Reporting 0 there would size the table to the
    infrastructure alone and exhaust it the moment that node registers — a
    confident wrong number, sized exactly, which is this campaign's own failure
    shape rather than a new one. The discriminator is whether ANY wiring was
    described: if it was, an empty `services` map is a real zero; if nothing
    was, the question is unanswered and the verb says nothing. The app term
    stays `UNDECLARED_HEADROOM` in the consumer, labelled there as the guess it
    is.

    The counting machinery is landed and correct for the day the resolver does
    emit wiring: endpoint refs are counted per SERVER, not per service name (two
    nodes serving one name are two queryables), and an action server costs
    `ACTION_SERVER_QUERYABLES` because an action is three services on the wire.

  ### If `services[].server` arrives, it is AUTHORITATIVE

  DECIDED, and unchanged by the above — it is the rule for when the input
  exists. The launch declaration is the contract: an image is sized for exactly
  the service servers its model declares.

  The alternative was to treat it as a floor and add headroom, which is safe and
  wrong — it re-creates in miniature the guess this whole wave exists to delete,
  and a guess derived from something real is still a guess nobody can audit.

  What being authoritative COSTS is paid in W5.e, in this wave: a node that
  creates a service server its model does not declare exhausts the table, so
  "the declaration is wrong" and "the table is too small" become the same event
  and the runtime must name the first.

  ### Future work: counting in code

  The declaration is a stepping stone, not the destination — and after the
  measurement above it is not even the near-term path, since nothing populates
  it today. The count a node's CODE produces is the ground truth, and deriving
  it there would make a launch declaration checkable rather than trusted — the
  same move W5.a made for the infrastructure counts, where a constant beside the
  creation sites replaced seven prose spellings and a gate holds it to the sites
  themselves.

  Out of scope for W5 and deliberately not designed here: it needs the entity
  set to be visible at build time in BOTH languages, which is the asymmetry that
  ruled out const generics (`Node::ENTITY_BOUNDS` exists for Rust; C/C++
  entities are created at runtime in C with no declaration site). Solving that
  is the real end state; W5 gets the infrastructure saving without waiting for
  it.

* **W5.c — delivery.** LANDED. `NanoRosEntityFacts.cmake` runs the verb per
  entry and attaches the result with `corrosion_set_env_vars` — the phase-351 W5
  carrier, for the reason issue 0460 records: a workspace member's own
  `.cargo/config.toml` is never read (Corrosion runs cargo from the workspace
  root) and `set(ENV{...})` reaches only the configure-time process.

  One thing is different from board facts and it decides the shape. Exactly one
  BOARD is active per configure; several MODELS can be, one per entry. There is
  one runtime staticlib per configure and every entry links it, so entries
  ACCUMULATE: union of the infrastructure flags, max of the application counts,
  and ONE abstaining entry makes the whole configure's count unknown — a shared
  table has to satisfy the largest, and an unknown is not smaller than anything.
  Applied once by `nros_synth_runtime_umbrella`, which already runs after the
  SUBDIRS loop that processes the entries.

  Failure is soft throughout, deliberately, exactly as `nros_resolve_board_facts`
  is: a model that is not resolved yet, a CLI that is not built, an entry
  addressed the `MODEL` way at a path that does not exist. None of those is a
  configuration error; each means "this configure carries no entity facts",
  which is the state every build was in before this wave.

  Verified by a script-mode cmake probe over real models — the union, the
  abstention, a wired model scoring 1 service + 1 action = 4, and the soft skip
  when a model will not load. NOT yet verified end-to-end through a real
  configure; see W5.g.

* **W5.d — consumption.** LANDED. `nros-zpico-build` computes
  `app_declared + PARAM_SERVICE_QUERYABLES + LIFECYCLE_SERVICE_QUERYABLES` and
  sizes the C table and `SERVICE_BUFFERS` from ONE computation. Two sizings from
  one number, not two numbers that must coincidentally agree.

  MEASURED on `examples/native/rust/talker`, `nros-relwithdebinfo`, built twice
  and diffed with `nros-mem-report --baseline`:

  | | before | after | delta |
  | --- | ---: | ---: | ---: |
  | `SERVICE_BUFFERS` | 144,128 | 4,504 | **−139,624** |
  | `g_sessions` | 24,480 | 20,640 | **−3,840** |
  | RAM (.bss + .data) | 365,778 | 222,322 | **−143,456 (−39.2%)** |

  The `g_sessions` line was NOT predicted and is the confirmation that matters:
  the C shim's per-session `stored_queries[N][M]` and `last_reply_seq[N]` are
  sized by the same knob, so one number really does size both sides. A design
  that had left them independent would have moved only the Rust figure.

  The rule is a pure function (`queryable_default_from`) with the environment
  lifted out, because a build script reading env directly is untestable
  in-process and a sizing rule verified by reading is how this phase's other
  defects survived. Seven cases, including both refusals: a malformed count and
  an unknown infrastructure spelling PANIC rather than falling back to
  "undeclared", which is the `.max(1)` shape 0827 measured.

  It introduces TWO deliberate mirrors — `nros-zpico-build` cannot depend on
  `nros-node` to read the constants, nor see its features. `check-infra-queryable-counts`
  holds them to the definitions, which is the entire difference between these and
  the seven prose spellings W5.a replaced. Verified by drifting the lifecycle
  mirror to its historical wrong value of 6 and watching the gate name the file.

* **W5.e — an exhausted table names the right fault.** LANDED, in the half that
  authoritativeness makes urgent. The same exhaustion is now two different
  faults and the message says which. Sized from the backend's own budget, the
  table is too small and the fix is the knob — issue 0460's message, unchanged.
  Sized from a DECLARATION (`ZPICO_QUERYABLE_TABLE_DECLARED`, emitted beside the
  size it describes so the two cannot disagree), it holds what the model said
  this image would create, so reaching the limit means the image created an
  UNDECLARED service server, and pointing that reader at `ZPICO_MAX_QUERYABLES`
  sends them to enlarge a table that was already right. Both arms are `&'static
  str` constants, so the `no_std` half gets the same split — the half issue 0460
  found had no explanation at all.

  NOT landed: the build-time failure for an image that declares nothing. That
  still needs the hand-written-`main` question settled (see Open, below), and it
  is what blocks W5.f's deletion.

* **W5.f — RETIREMENT.** HALF LANDED, and the half that is not is blocked rather
  than skipped.

  LANDED: `ZPICO_MAX_QUERYABLES` stops being an independent opinion and becomes
  a CHECKED override. Two mechanisms deciding one number is how this phase's
  other defects were born. The check compares against a DERIVED floor, not the
  budgeted default, and that distinction is the point: an image carrying both
  service families provably claims eleven slots at boot, so below that it cannot
  start and the build is REFUSED; the default adds `UNDECLARED_HEADROOM` for an
  application count nothing can supply, and being under THAT is reported with a
  `cargo:warning` naming both numbers, because refusing a build for being under
  a guess is the defect this wave exists to remove. The floor and the default
  share one parser of the infrastructure spelling — two readings of one string
  is how a rule and its check come to disagree.

  This is issue 0460 caught a stage earlier: `zephyr/Kconfig` defaults
  `CONFIG_NROS_MAX_QUERYABLES` to 8, and an entry enabling both families needs
  eleven. That image used to build cleanly and die at boot.

  NOT LANDED: deleting `if hosted { 32 } else { 8 }` and the
  `CARGO_CFG_TARGET_OS` sniff behind it. The old path is still REACHABLE, which
  is this wave's own precondition for deletion — a bare `cargo build` of a leaf,
  the standalone `check-rmw-*` projects, `cargo test` from the repo root, and
  every hand-written `main` declare nothing and must still build. Deleting the
  literal before W5.e's build-time failure would not make those declare; it
  would make them fail.

* **W5.g — measure and gate.** NOT MEASURED, and recorded as such rather than
  estimated, per this phase's rule that no wave claims a saving it did not
  measure.

  The reason is specific. W5.d's 143,456 B was measured on
  `examples/native/rust/talker`, a standalone pure-cargo leaf with no bringup
  and no model — so nothing declares for it and this wave changes it by zero
  bytes. The images this wave DOES change are the cmake-configured workspaces,
  and those have no committed root `CMakeLists.txt`: the fixture builder
  synthesises it, so measuring means a fixture build rather than a reconfigure.
  That is the next session's first job, not an estimate to write down here.

  What to run: `just build-test-fixtures lane=native`, then `just mem-report
  --json --baseline` on a `workspaces/features` entry (declares
  `param+lifecycle`, so 32 -> 19 slots) and a `workspaces/rust` one (declares
  neither, so 32 -> 8). The gating property is that a role which declares
  services keeps exactly what it declares. Confirm the delivery reached cargo at
  the same time: `NROS_DECLARED_INFRA_QUERYABLES` must appear in the configure's
  `build.ninja`, where today it does not.

### Open, and deliberately not assumed away

**Hand-written `main`s.** They create entities at runtime, have no generated
entry, and cannot declare — so under W5.e they cannot build. W2 has the same
problem for the arena and proposes a runtime high-water mark plus a CI lane;
queryables should ride that answer rather than invent a second one. This couples
W5.e to W2's timing.

**Nothing populates the model's service wiring.** `ros-launch-manifest` models
it and the resolver never emits it: zero of 115 models here carry any layer-1
wiring. Until something does, W5.b2's application term is a labelled guess and
the sizing is exact only for the infrastructure half. Two ways out, and they are
not equivalent: teach the resolver to describe endpoints (a `play_launch`
question, and it can only describe what the launch inputs say), or count in code
(the ground truth, and the real end state — see Future work above). This is the
one that decides whether "sized by declaration" ever becomes "sized exactly".

**`ZPICO_MAX_SESSIONS`** multiplies the pool and has no declaration path at all.
Either it joins the model or it stays a knob and this phase says so explicitly.
It is currently 1 everywhere, which is why it has never been the visible term.

## Explicitly out of scope

**Moving payload buffers to the heap.** REOPENED by amendment B above
(2026-08-29) and no longer this campaign's settled position — read the two
together, and see B for the measurement that decides it. The original reasoning
stands as the case against, unchanged:

It would convert `12 x 4 x 1024` of
always-reserved RAM into peak-of-concurrent, which is a real saving, and it is
declined deliberately. A statically provable buffer would become an allocation
that can fail mid-callback, and it would widen the heap's block-size range from
infrastructure-only (~2^6) to payload-inclusive (~2^16) — which is precisely
what makes [phase 391](phase-391-allocation-unification-and-tier-model.md)'s
constant-time allocator sizeable. The two decisions are coupled; this is the
side of the coupling that keeps both defensible.

## W5.g measured 2026-08-30 — the delivery does not reach any workspace checked

W5.d's 143,456 B was measured on a pure-cargo leaf, which this wave changes by
zero bytes. W5.g was to re-measure on a real cmake workspace image. It did not
get as far as a byte count, because the ENV NEVER ARRIVES.

Method: reconfigure each workspace and look for `nros_entity_facts_env`'s own
status line, `"nano-ros: queryable table sized from the declaration"`. A
configure that applies the env prints it; one that does not, does not.

| workspace | umbrella created | entity-facts line |
| --- | --- | --- |
| `features` (Rust entries) | no | **no** |
| `c` (pure C) | no | **no** |
| `mixed` (C + a Rust node pkg) | YES | **no** |

Two of the three are explained and one is not.

**`c` / `cpp` are by construction.** `nros_synth_runtime_umbrella` returns early
for a pure-C/C++ workspace (`NanoRosRuntimeCrate.cmake:202`, "keep
nros_cpp-static as the umbrella"), and `nros_entity_facts_env` is called AFTER
that return. So the C and C++ workspaces — the ones whose images this campaign
is about — cannot receive the figure through this path at all. That is a design
consequence nobody wrote down, not a bug in the wave.

**`features` has no cmake runtime crate.** Its entries build through cargo
directly, so there is no `nros_ws_runtime-static` to attach env to. The models
that would benefit MOST are here: W5.b1 measured 22 of 115 models as
`param+lifecycle` (32 slots -> 19) and they are `features` models.

**`mixed` is UNEXPLAINED and needs one more step.** The umbrella IS created —
its own message prints — so the early return was passed, and
`nros_entity_facts_env(nros_ws_runtime-static)` sits a few lines further on in
the same function. It still prints nothing. Either the call is not reached for
a reason not yet found, or the status line is suppressed. **Not diagnosed;
recorded as the next action rather than guessed at.**

**So the honest state of W5:** the sizing logic, the exhaustion diagnostic and
the checked override all landed and are unit-tested, and the figure they consume
reaches no image that was checked. The 143,456 B in W5.d remains a measurement
of what the mechanism WOULD save, on a leaf where the mechanism does not run.
Nothing in this wave should be quoted as a shipped saving until the `mixed` case
is explained and a before/after `mem-report` exists.

