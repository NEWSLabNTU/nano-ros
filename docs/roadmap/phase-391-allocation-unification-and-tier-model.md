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
