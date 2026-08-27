# Phase 391 — one funnel, one arena, a constant-time allocator behind it, and a link-time gate that proves it

**Status (2026-08-26). Plan, one prerequisite landed.** Opened from a
memory-allocation review. [Issue 0817](../issues/archived/0817-platform-funnel-bypassed-in-zephyr-port.md)
(the sixteen Zephyr funnel bypasses) is fixed and archived; everything below is
unstarted. Depends on [phase 390](phase-390-storage-mode-rename-inline-heap-view.md)
for vocabulary only, not for code.

## Where this starts from

The architecture RFC-0034 D6 describes is already built, and mostly true:

- one `#[global_allocator]` in the tree — `nros-platform/src/lib.rs`
- one funnel — `nros_platform_alloc`
- `zpico-alloc`'s own docs: *"this crate is the ARENA, not the allocator"*
- bare-metal platforms already ship the arena in the platform package:
  `static HEAP: FreeListHeap<HEAP_SIZE>` in `nros-platform-{mps2-an385,stm32f4,esp32-qemu}/src/memory.rs`

Verified end-to-end on mr_canhubk3/s32k344 by disassembly: `z_malloc`
tail-calls `nros_platform_alloc`, so zenoh-pico's 42 allocation sites and the
Rust global allocator share one funnel.

**So this phase is not "build a funnel". It is: replace what sits behind the
funnel, and make the property checkable.**

## The gap: the arena is not real-time

`zpico-alloc::FreeListHeap` is first-fit with an address-ordered free list and
a 64-byte slab fast path:

```rust
let mut current = self.get_free_list();
while !current.is_null() {                  // O(n) walk
    if (*current).size >= aligned_size {
```

Good fragmentation behaviour — Robson (1977) showed first-fit is near-optimal
and best-fit is nearly worst-possible — but **O(n)**, so it has no worst-case
execution bound. That is the property a safety-island image needs and does not
have.

## rlsf, and what it costs (measured, not estimated)

[TLSF](http://www.gii.upv.es/tlsf/files/papers/ecrts04_tlsf.pdf) is O(1) for
allocate and free regardless of heap state, via two-level segregated free lists
plus bitmaps and a `CLZ` — a single instruction on Cortex-M7. Internal
fragmentation is bounded at `1/SLLEN`.

Candidates surveyed:

| impl | license | cert claims | verdict |
| --- | --- | --- | --- |
| [rlsf](https://github.com/yvt/rlsf) 0.2.3 | MIT/Apache-2.0 | none | **chosen** |
| [o1heap](https://github.com/pavel-kirienko/o1heap) | MIT | MISRA C:2012, published WCMC formula | rejected — see below |
| [mattconte/tlsf](https://github.com/mattconte/tlsf) | BSD | none | unmaintained since 2016 |
| [UPV original](http://www.gii.upv.es/tlsf/main/license) / ros2/tlsf | **GPL/LGPL dual** | none | licence non-starter |

o1heap has the better certification story and was still rejected on merit: it
is **not TLSF**. It is single-level, one bin per power of two, and rounds every
request up:

```c
const size_t alloc_size = roundUpToPowerOf2(amount + O1HEAP_ALIGNMENT);
```

Worst-case internal fragmentation approaches 100%. A 6,220,800-byte 1080p frame
rounds to 8,388,608 — 2.17 MB wasted on one message. Disqualifying for the
large-payload case, whatever the paperwork says.

**Measured cost of rlsf on `thumbv7em-none-eabihf`, `opt-level="z"` + LTO:**

| FLLEN/SLLEN | `.text` | `.bss` | max internal frag |
| --- | --- | --- | --- |
| 8/8 | 608 B | 276 B | 12.500% |
| 12/8 | 608 B | 412 B | 12.500% |
| **12/16** | **600 B** | **796 B** | **6.250%** |
| 16/16 | 608 B | 1060 B | 6.250% |
| 16/32 (needs a `u32` SL bitmap) | 596 B | 2116 B | 3.125% |

Code size is flat; the fragmentation bound is bought with `.bss`, ~136 B per FL
class. Per-allocation overhead is an 8 B header
(`GRANULARITY/2`, `GRANULARITY = size_of::<usize>() * 4` = 16 on 32-bit) plus
16 B granularity rounding plus the class rounding above.

**Net against the current image**, replacing rather than adding — Zephyr's
`sys_heap` is 1,856 B of code that garbage-collects once nothing calls
`k_malloc`:

```
text  +600 (rlsf) +282 (glue, no realloc) -1856 (sys_heap)  =  -974 B
bss   +796 (control struct); the 16,384 B k_heap becomes the rlsf arena
```

**A net flash shrink and 796 B of RAM.** Implementation footprint is not the
argument in either direction.

## What makes this defensible now

The decision that payload buffers **stay static** (they do not move to the
heap) is what makes TLSF sizeable here. Robson's bound scales with the ratio of
largest to smallest block; a heap holding both 20-byte key expressions and
megabyte payloads has a ~2^16 spread and a punishing worst case. A heap that
holds only *infrastructure* — zenoh-pico's sessions, key expressions and
strings, and Rust `String`/`Vec` churn — has a narrow spread, and the bound
becomes cheap to defend.

## The tier model

Heap-freedom is not nano-ros's to give up or keep — **it is the vendor RMW that
requires a heap.** zenoh-pico reaches the allocator from 42 call sites and is
third-party C. A consumer who brings a heap-free RMW must still get a heap-free
image, so this is a tier, not a global choice:

| tier | rule | who can reach it |
| --- | --- | --- |
| `heap-free` | **no** allocation symbol in the linked image | consumers with a heap-free RMW; embassy/RTIC integrations |
| `unified` | allocation symbols only inside `nros_platform_*` backend objects | the zenoh and cyclone tiers |

The tree is already built for this: `alloc` is a Cargo feature and every core
crate gates `extern crate alloc` on it. What is missing is enforcement — the
book already promises "fully no-alloc" for embassy and RTIC with nothing
checking it ([issue 0816](../issues/0816-no-alloc-claimed-but-unenforced.md)).

## Waves

**W1 — link-time allocation gate.** `nm` the built image, deny
`malloc`/`calloc`/`realloc`/`free`/`k_malloc`/`k_free`/`pvPortMalloc`/... at the
tier's strictness. A symbol gate, not a source grep — a grep cannot see
vendored C, which is where 42 of the sites are. Closes issue 0816; would have
caught all sixteen sites in issue 0817. **Do this first** — it is what verifies
every later wave.

**W2 — rlsf behind the funnel.** Replace `FreeListHeap`'s internals in
`zpico-alloc` and the three `nros-platform-*/src/memory.rs` statics. The arena
and the `z_malloc`/`z_free` shim structure do not change; only the algorithm
does.

**Vetted 2026-08-27 — rlsf clears every prerequisite, and two costs the survey
above did not know are lower than it assumed.**

*Suitability.* `#![no_std]` unconditionally (`lib.rs:2`, not cfg-gated); its
`std` feature is opt-in and off by default. Edition 2021, `rust-version` 1.61.
Dependencies are `cfg-if`, `const-default` (`default-features = false`) and
`rustversion` — none reaches std. The API is the shape this funnel needs:
`Tlsf::new()` is a `pub const fn` (so it can back a `static`),
`insert_free_block_ptr(NonNull<[u8]>)` takes a static arena, and
`allocate(Layout) -> Option<NonNull<u8>>` / `deallocate(ptr, align)` match the
funnel's two operations. `reallocate` exists if the glue ever wants it.

*The tuning table is expressible and self-consistent with the source.*
`Tlsf<'pool, FLBitmap, SLBitmap, const FLLEN: usize, const SLLEN: usize>` is
generic over exactly the parameters the table varies, with the bitmaps as
separate type parameters — which is why the `16/32` row needs a `u32` SL
bitmap: `sl_bitmap: [SLBitmap; FLLEN]` has to be wide enough for SLLEN.

*Two things that lower the cost of adopting it:*

* **rlsf 0.2.3 is already vendored in the local cargo registry** (0.2.2 and
  0.2.3 both present), so adding it needs no network — which matters because
  `--locked` is injected project-wide by the `scripts/bin/cargo` shim.
* **It is already a transitive dependency of `nros-board-esp32-qemu`**, via
  `esp-alloc 0.9.0`. rlsf therefore already compiles for a bare-metal target in
  this tree. "Does it build for our targets" is retired as a risk, and
  `nros-platform-esp32-qemu` would be consolidating onto an allocator its own
  board already links rather than adding a second one.

*Measured on `thumbv7m-none-eabi`, `opt-level="z"` + LTO + `codegen-units=1`* —
a DIFFERENT target from the survey table above, which is `thumbv7em-none-eabihf`:

```
FLLEN=12 SLLEN=16, u16/u16 bitmaps
  control struct (.bss)   796 B     <- matches the table's 12/16 row exactly
  code + 3 C wrappers     720 B
```

The `.bss` figure reproduces the survey's 796 B independently, on M3 rather than
M7, which is expected — the control struct is `fl_bitmap` + `[SLBitmap; FLLEN]`
+ `[[Option<NonNull>; SLLEN]; FLLEN]`, and both targets are 32-bit. The harness
was validated by decomposition rather than asserted: total probe `.bss` was
4,892 B = 796 (control) + 4,096 (the probe's own arena).

The code figure is NOT directly comparable to the table's `600 B (rlsf) + 282 B
(glue)`: this 720 B is rlsf plus three thin `extern "C"` wrappers under LTO,
not the same glue. Same order; do not subtract them from each other.

**THE SURVEY'S CHOSEN 12/16 CANNOT SERVE TWO OF THE THREE PLATFORMS.** This is
the one measurement that contradicts the section above, and it is a correctness
constraint, not a tuning preference.

rlsf caps the pool it can hold (`tlsf.rs`):

```rust
const MAX_POOL_SIZE: Option<usize> = {
    let shift = GRANULARITY_LOG2 + FLLEN as u32;   // GRANULARITY_LOG2 = 4 on 32-bit
    if shift < usize::BITS { Some(1 << shift) } else { None }
};
```

`GRANULARITY = size_of::<usize>() * 4` = 16, so `MAX_POOL_SIZE = 1 << (4 + FLLEN)`
and the largest single block is `(GRANULARITY << FLLEN) - GRANULARITY`.

| FLLEN | max pool | verdict against our arenas |
| --- | --- | --- |
| **12** (the row this doc chose) | **64 KiB** | too small for two of three |
| 14 | 256 KiB | covers the 128 KiB arenas |
| 18 | 4 MiB | covers the 2 MiB arena |

The arenas, from the statics this wave is supposed to convert:

| platform | `DEFAULT_HEAP_SIZE` | needs |
| --- | --- | --- |
| `nros-platform-stm32f4` | 32 KiB | FLLEN >= 12 (12/16 is fine) |
| `nros-platform-mps2-an385` | 128 KiB | **FLLEN >= 14** |
| `nros-platform-mps2-an385` (one cfg) | 2 MiB | **FLLEN >= 18** |

So the `.bss` figure in the survey table is an understatement for the platforms
that matter: at ~136 B per FL class, 12 -> 18 is roughly +816 B on top of the
796 B, i.e. ~1.6 KiB rather than 796 B for the 2 MiB arena. That is still small
against a 2 MiB heap, but it is not the number this doc currently promises, and
picking 12/16 as written would fail to hold the pool at all.

**DESIGN REVISION (measured 2026-08-27): a DEFAULT const parameter, with the
adequacy of FLLEN for N asserted at compile time.** This supersedes the three
options first sketched here (one-size FLLEN / derive-from-N / split the arena).

```rust
pub struct FreeListHeap<const N: usize, const FLLEN: usize = 18> { .. }

impl<const N: usize, const FLLEN: usize> FreeListHeap<N, FLLEN> {
    const MAX_POOL: usize = 1usize << (4 + FLLEN);   // GRANULARITY_LOG2 = 4, 32-bit
    pub const fn new() -> Self {
        assert!(N <= Self::MAX_POOL,
                "NROS_HEAP_SIZE exceeds rlsf MAX_POOL_SIZE for this FLLEN");
        ..
    }
}
```

Every existing call site keeps working — `FreeListHeap<N>` takes the default —
and a platform that wants a smaller control struct names its own:
`FreeListHeap<{32 * 1024}, 12>`.

*Verified on `thumbv7m-none-eabi`, both directions:*

* `FreeListHeap<{32 * 1024}, 12>` and `FreeListHeap<{128 * 1024}>` compile.
* `FreeListHeap<{128 * 1024}, 12>` — a 128 KiB arena against a 64 KiB max pool
  — **fails the build**: `error[E0080]: evaluation panicked: NROS_HEAP_SIZE
  exceeds rlsf MAX_POOL_SIZE for this FLLEN`. The negative control was run
  deliberately, because a guard nothing can trip is this campaign's own named
  trap.

Default const parameters and `assert!` in a `const fn` both work on the pinned
toolchain, so this needs no `generic_const_exprs` — which is what blocked the
derive-FLLEN-from-N option.

*Measured control-struct cost, SLLEN=16, same target:*

| FLLEN | `.bss` | max pool | fits |
| --- | --- | --- | --- |
| 12 | **796 B** | 64 KiB | stm32f4 (32 KiB) |
| 14 | **928 B** | 256 KiB | mps2-an385 (128 KiB) |
| 18 | **1,192 B** | 4 MiB | mps2-an385 (2 MiB cfg) |

**This corrects the survey table's per-class figure.** The measured slope is
**66 B per FL class** at SLLEN=16, not the ~136 B this doc claimed — 136 is the
SLLEN=32 slope. So the worst case (FLLEN=18) is +396 B over the already-accepted
796 B, not the ~+816 B a reader would extrapolate. Sizing for the largest arena
is therefore much cheaper than it first appeared, and the default of 18 costs a
32 KiB board 1,192 B (3.6% of its arena) if it does not override.

**W2 LANDED — measured before/after on a named image (2026-08-27).**

`qemu-bsp-talker`, mps2-an385, `thumbv7m-none-eabi`, `nros-relwithdebinfo`,
built by `just qemu build-fixtures`, measured with
`scripts/nros-mem-report.py`. Both arms built from the SAME tree with only
`zpico-alloc` differing (the pre-W2 arm produced by
`git checkout <W2>~1 -- packages/rmw/zenoh/zpico-alloc/`), so the delta names
this wave and nothing else:

| | before (first-fit) | after (rlsf) | delta |
| --- | --- | --- | --- |
| `nros_platform_mps2_an385::memory::HEAP` | 131,608 B | 132,792 B | **+1,184 B** |
| RAM (`.bss` + `.data`) | 386,900 B | 388,084 B | **+1,184 B** |

RAM total moves by exactly the HEAP delta, so nothing else shifted. The
decomposition holds:

```
arena      131,072 -> 131,072   (unchanged, by design)
metadata       536 ->   1,720   (+1,184)
                             = 512 slab + 1,192 rlsf control + 16 padding/flags
```

**This is a COST, not a saving, and it should be read as one.** The survey above
projects a net flash shrink and 796 B of RAM, but that arithmetic is the ZEPHYR
case, where rlsf REPLACES `sys_heap` (−1,856 B of text) and the 16 KiB `k_heap`
becomes the arena. On bare-metal there is no `sys_heap` to remove, so the
control struct is added with nothing offsetting it. W3 is where the offset
appears; W2 alone buys a worst-case execution bound and pays 1,184 B for it on a
458,752 B part (0.26%).

A cross-check worth keeping: an image built 32 commits earlier measured the
identical 131,608 B / 386,900 B, so none of the intervening work touched this
image's RAM — which is why the isolated rebuild and the historical image agree.

**BLOCKED on file ownership, not on technique.** `FreeListHeap`'s implementation
is `packages/rmw/zenoh/zpico-alloc/src/lib.rs`, and the rlsf dependency would be
added to that crate's manifest. The three `nros-platform-*/src/memory.rs` files
are 94-line wrappers that only `use zpico_alloc::FreeListHeap` and size the
arena from `NROS_HEAP_SIZE`; changing them alone would leave the tree pointing
at a type whose O(n) first-fit walk is unchanged — a diff that reads as "W2
landed" while delivering none of its property.

**W3 — Zephyr tier: `CONFIG_HEAP_MEM_POOL_SIZE=0`.** Repoint
`nros_platform_alloc` at rlsf and let `sys_heap` garbage-collect. Requires that
no Zephyr subsystem calling `k_malloc` is enabled (fs, mcumgr, net,
`log_mgmt`, cfb — none are in the serial image, but this needs a link test, not
an assertion). Prerequisite: fix
[issue 0811](../issues/0811-zephyr-net-iptcp-allocator-provenance-mismatch.md),
whose two-allocators-one-free-path is survivable only while both bottom out in
`k_malloc`.

**W4 — declare the tier per image, and gate it in CI.** Tier becomes a build
input; the W1 gate reads it. `heap-free` gets at least one lane that actually
builds and links.

**W5 — a static component pool in `node_runtime`, so the `heap-free` tier is
USEFUL rather than merely reachable.**
[Issue 0843](../issues/0843-node-runtime-forces-alloc-on-every-cffi-image.md)
decoupled the allocation gate from the transport gate, so a cffi image now links
without `alloc`. What it did not do is leave anything useful behind: with
`alloc` off, `node_runtime` is gated out entirely, and it is the only path to a
running `Executor`. This wave removes the reason it needs a heap.

*Why this is nano-ros's problem and not a vendor's.* All four backends reach the
executor through ONE cffi seam, the seam allocates nothing, and the executor's
dispatch algorithm links heap-free. Every allocation between here and a working
heap-free image is ours:

| site | uses | why it allocates |
| --- | --- | --- |
| `node_runtime` registries | 35 `String`, 6 `Vec` | owned names + unbounded entity lists |
| `node_runtime` cells | 17 `Arc<ComponentCell>` | closures must outlive the executor |
| `node_runtime` slots | 3 `Box<dyn ComponentSlot>` | type-erased per-component state |
| `executor/spin.rs` | `leak_default_backing()` | leaks an arena when the caller supplies none |
| `executor/handles.rs` | `EventRegs` | boxed event callbacks |

*The count is not known at compile time; the BOUND is* — and that is all a pool
needs. `register_node::<C>()` is a runtime call, but the tree already answers
this shape twice, and `node_runtime` is the outlier:

| layer | bound | on overflow |
| --- | --- | --- |
| executor node table | `NROS_EXECUTOR_MAX_NODES` (build.rs, default 4) -> `config::MAX_NODES` | `NodeError::NodeTableFull` |
| `node_metadata` | `DEFAULT_MAX_METADATA_NODES` = 8, const-generic | bounded |
| **`node_runtime`** | **none** — `Vec<Arc<ComponentCell>>` grows without limit | — |

*Capacity comes from a BUILD-SCRIPT KNOB, not a const generic, and the reason is
FFI.* `node_runtime` carries nine `extern "C"` sites and backs
`__nros_component_<pkg>_install`, the uniform cross-language component-install
seam. A const generic would put a type parameter on a type that crosses into
C/C++; a baked `pub const` is invisible at the ABI. The tree already follows this
rule without stating it: `node_metadata` has ZERO `extern "C"` sites and uses
const generics freely; `node_runtime` has nine and uses none. So:
`NROS_RUNTIME_MAX_COMPONENTS`, emitted as a `pub const` exactly as
`NROS_EXECUTOR_MAX_NODES` is.

*What the pool buys, beyond the heap.* Cells in a `'static` pool outlive every
closure by construction, so the `Arc` refcount is proving a lifetime the pool
already guarantees. Closures and trampolines hold a `ComponentId` index instead.
All 17 `Arc` uses go, and they go because the ownership model got simpler, not
because they were worked around.

**Acceptance:** an image that CALLS runtime code — not one that names types —
links at tier `heap-free` and passes the W1 gate with `symbols read` well above
1. Three probes have already passed that gate vacuously at `symbols read: 1`
(`qos::DEFAULT`, `DEFAULT_MAX_METADATA_NODES`, and
`size_of::<internals::RmwSession>()`, which pulls no code even for a real type).
A pass without that symbol count is not evidence.

**OPEN, and it decides whether this is a port or a redesign: `Box<dyn
ComponentSlot>`.** The pool is heterogeneous — `TypedSlot<C>` is generic over
`C` — so cell storage cannot simply be `[ComponentCell; N]`. Two candidates,
neither chosen:

* caller-supplied `&'static mut dyn ComponentSlot` per slot, the shape
  `Executor::open_in` / `ExecutorSizing` already uses, which keeps storage with
  the caller and off the heap;
* slot storage sized to the largest `C::State` in the image, which codegen knows
  and a hand-written `main` does not — the same split W2 of
  [phase 392](phase-392-static-memory-space-campaign.md) hit, where generated
  entries can be sized statically and hand-written ones need a measured
  high-water mark.

Settle that before writing code. The rest of this wave is determined.

## Related, not owned here

- [issue 0812](../issues/0812-publisher-loan-heap-allocates-per-loan.md) —
  `Box::new` per loan. As written, `lending` and `heap-free` are mutually
  exclusive for no inherent reason. Fixing it is a precondition for the loan
  API existing on the heap-free tier.
- [issue 0814](../issues/0814-lending-never-exercised-on-hardware.md) — the
  whole zero-copy surface is posix-test-only.
- Whether `heap` survives as a storage mode at all. Payload buffers staying
  static means no payload field needs it; the question is whether infrastructure
  use justifies keeping a mode nobody applies to messages.
