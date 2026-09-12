---
id: 1197
title: "FreeRTOS still budgets `configTOTAL_HEAP_SIZE` for an executor backing that
  moved to `.bss`, and neither of the two mechanisms that fixed Zephyr can be
  used here — the board crate cannot see the size, and stating it would fight
  the per-leaf derivation"
status: resolved
type: tech-debt
area: [embedded, core, build]
related: [1145, 1171, 1146, 0827, 1061, 1348, phase-392, phase-448]
resolved_in: phase-448 W4
---

## What

phase-392 W6 moved the executor's per-entry storage out of a `Box::leak` into
the named `.bss` static `EXECUTOR_BACKING`. On an RTOS the allocator it used to
come out of is itself a fixed reservation, so an image that does not lower that
reservation now holds the same bytes twice. Issue 1145 paired Zephyr; FreeRTOS
is not paired, and this issue is why it was not done by hand.

## Measured

`bash scripts/build/fixtures-build.sh freertos rust`, then
`arm-none-eabi-nm -S` on each `build/cargo-fixtures/freertos/thumbv7m-none-eabi/
nros-relwithdebinfo/<leaf>`:

| leaf | `EXECUTOR_BACKING` | `ucHeap` |
| --- | ---: | ---: |
| `talker` | 20,608 | 2,097,152 |
| `listener` | 20,608 | 2,097,152 |
| `service-server` | 20,608 | 2,097,152 |
| `service-client` | 21,832 | 2,097,152 |
| `action-server` | 32,512 | 2,097,152 |
| `action-client` | 32,512 | 2,097,152 |

So between 20 KiB and 32 KiB per image is reserved twice — about 1–1.5 % of the
2 MiB heap. Small, and that is part of the argument below.

## Why neither Zephyr mechanism transfers

**The size is already DERIVED per leaf, and correctly.** It varies across the
six because `nros sync` writes each leaf a `nros-managed-env.toml` carrying
derived executor knobs — the issue 0827 / 1061 channel:

```
# examples/mps2-an385-freertos/rust/talker/.cargo/nros-managed-env.toml
NROS_EXECUTOR_ACTION_CLIENTS = "0"      # action-server: "1"
NROS_EXECUTOR_MAX_CBS = "1"
```

`EXECUTOR_BACKING_DEFAULT_U64S` is `ExecutorSizing::DEFAULT.u64_len()` over
those knobs, so the backing already tracks the declaration. Nothing is broken on
that side.

**1171's answer — STATE the size — is wrong here.** On Zephyr it was right
because the derived size is target-dependent (87,256 B on mps2_an385 against
88,328 B on native_sim for one conf), so no single derived number could pair
both boards. FreeRTOS has one target, and stating a number would pin a value
that `nros sync` derives, so the next declaration change would leave the stated
number stale — with the const assert only catching the case where it falls below
`EXECUTOR_BACKING_DEFAULT_U64S`, not the case where it is needlessly large.
Stating here fights the derivation instead of fixing anything.

**Lowering the heap by a copied number is 1171's original defect.** The
subtrahend would be a literal in a leaf `[env] NROS_FREERTOS_HEAP_KB`, copied out
of `nm`, stale the moment an entity count moves — on a platform where the size is
per-leaf, so it would be six literals rather than one.

## The mechanism that WOULD work, and what blocks it

The board's heap default should subtract the backing itself. It already computes
one (`packages/boards/nros-board-freertos/build.rs`):

```rust
let zenoh_default_kb = (env::var("CARGO_FEATURE_RMW_ZENOH").is_ok()).then(|| 2048_usize);
```

`nros-node` declares `links = "nros_node"` and already exports build facts to
dependents (`cargo:max_cbs`, `cargo:arena_size`, `cargo:rx_buf_size` ->
`DEP_NROS_NODE_*`), so adding `cargo:backing_u64s` is one line.

**The blocker: `nros-board-freertos` does not depend on `nros-node`**, so the
`DEP_NROS_NODE_*` channel does not reach it. Cargo only exposes those variables
to direct dependents of the `links` crate. Fixing this means either giving the
board that dependency — a real layering change, not a knob — or carrying the
figure through the same file-based route the platform rungs already use
(`BuildRungs::from_build_env`, which this build script already consults).

## Two reductions target the same budget, and they must not be done separately

`build.rs` justifies the 2 MiB against a demand list that includes "the
`nros_app` task stack (now 384 KiB)". Issue 1146 lowers that default to
131,072 B — 256 KiB per task — and says so explicitly: *"Heap budget, not
`.bss`, until `configTOTAL_HEAP_SIZE` follows (issue 1145)."*

So there are two pending reductions to one number, one of them ~10x the other.
Doing them in separate hand-edited commits invites a double subtraction that
nothing would catch until an image malloc-fails at runtime — and issue 1146
measured exactly that failure mode, finding it reports `*** MALLOC FAILED ***`
rather than `*** STACK OVERFLOW ***`, because heap_4 hands out the task stacks.

## ATTEMPTED 2026-09-07 — the board cannot be the consumer, and here is why

The mechanism was built and the arithmetic validated. The board still cannot
use it, for a reason that is structural rather than plumbing.

### What works

`nros-node` can publish the number as a symbol whose STORAGE SIZE is the value
(`__NROS_LU_SZ_executor_backing`, behind `layout-size-markers`, off by default),
and `nros-sizes-build::extract_sizes` reads exactly that shape. Verified: the
symbol appears in `libnros_node.rlib` for `thumbv7m-none-eabi`.

The arithmetic is validated independently. Probing the fifteen per-region
`(size, align)` pairs out of the same rlib and running
`nros-executor-layout`'s placement over them reproduces **all four** measured
images to the byte:

| image | cbs | arena | computed | linked |
| --- | ---: | ---: | ---: | ---: |
| `talker` | 1 | 8,192 | 20,608 | 20,608 |
| `service-client` | 2 | 9,216 | 21,832 | 21,832 |
| `action-server` | 1 | 20,096 | 32,512 | 32,512 |
| defaults | 4 | 74,240 | 87,256 | 87,256 |

`alloc` is discriminating: with it off every row is 96 bytes low (8 sc x 12), so
the flag is doing real work rather than being decorative.

### Why the board still cannot consume it

**1. `nros-board-freertos` is in the root workspace's `exclude`, not its
`members`.** It is a cross-only crate and its own workspace root
(`cargo metadata` reports `workspace_root` = the board directory, 1 member). A
nested `cargo build -p nros-node` from its build script therefore fails with
`cannot specify features for packages outside of workspace`. It can be forced
with `--manifest-path <repo>/Cargo.toml`, and that does build.

**2. But the answer then depends on inputs the board cannot know.** Three probe
invocations against a leaf whose linked backing is **21,832** returned:

```
--features rmw-cffi,alloc, two env vars forwarded   ->  39,416
--features rmw-cffi,alloc, six env vars forwarded   ->  87,256   (the defaults)
```

The number is a function of the executor env AND the feature set the LEAF
resolved for `nros-node`. The board is not on the leaf's dependency path to
`nros-node` — it does not depend on it at all — so it can neither be told nor
discover either one. Guessing `rmw-cffi,alloc` is the issue-0665 hazard at full
strength, and 0665 was 16 bytes; this is tens of kilobytes.

**3. The probe cache cannot save it either.** `probe_key` hashes (target,
features). The backing varies by env, so two leaves with identical features and
different `NROS_EXECUTOR_MAX_CBS` collide on one cache entry.

### What this rules out, and what it points at

The consumer of this number **must be inside `nros-node`'s dependency graph**,
where the features and env are the ones that will actually link. The board is
structurally the wrong place, and no amount of wiring changes that.

That points at the entry, or at a crate the leaf already pulls in through
`nros-node`, publishing the heap requirement — rather than the board deriving
it. Whether `configTOTAL_HEAP_SIZE` can be set from there at all is the open
question, because it is baked into the board's C at the board's build time.

Which is the argument for the measurement route this issue already recommends:
`xPortGetMinimumEverFreeHeapSize()` answers "how much heap does this image
need" from inside the image, where every input is the real one.

## Recommended order

1. Land issue 1146 (the stack default).
2. Add `cargo:backing_u64s` to `nros-node` and route it to the board, or decide
   the layering question above.
3. Re-derive the heap default ONCE against both reductions, and verify by
   RUNNING every affected image on QEMU against a live router — the bar issue
   1146 already met for the stack, and the only bar that catches a too-small
   heap.

## Not done here, deliberately

No conf was edited. Issue 1145's Zephyr sweep is landed and this is the
FreeRTOS half of the same issue; recording the measurement and the blocker is
worth more than six hand-copied literals that would need re-deriving one PR
later.

---

# RESOLVED 2026-09-12 (phase-448 W4) — the layering decision, and the heap re-derived once

## The decision: the heap stops depending on the backing SIZE, because it stops budgeting for the FIRST executor at all

The 2026-09-07 attempt established that no consumer of the number can be given
it. That still holds, and re-checking it produced one more reason rather than a
way round:

* **`nros-node`'s build script cannot compute the size.** `EXECUTOR_BACKING_U64S`
  is emitted as the const EXPRESSION `EXECUTOR_BACKING_DEFAULT_U64S` precisely
  because the script cannot call `ExecutorSizing::DEFAULT.u64_len()` — the crate
  it would call lives in the crate it is building, and the per-region units are
  `repr(Rust)` layout facts only the TARGET compiler knows
  (`nros-executor-layout`'s own module doc says so). So `cargo:backing_u64s`
  cannot exist, and the `links` channel is empty however it is wired.
* **A DIRECT `nros-node` dependency on the board would not help.**
  `DEP_NROS_NODE_*` reaches direct dependents only, which is why the channel
  does not arrive today — but it would carry nothing, per the point above.
* **Reading the rlib is out for the reason issue 0464 removed it.** A build
  script that scans `deps/` picks the newest matching rlib by mtime, and
  phase-340 puts several leaves' units in ONE target dir, so two feature
  resolutions of `nros-node` coexist there. That is the 88680-vs-89392 ambiguity
  0464 measured, not a hypothetical.

So the answer is not to route the number. It is that **the heap no longer needs
it**, and the fact it does need — whether the backing is heap-resident — is a
BOOLEAN the board can see (`NROS_EXECUTOR_BACKING_U64S`, already read by
`nros-node`'s script and already the documented opt-out).

phase-392 W6 put ONE executor's storage in `.bss`. An image opens exactly one
executor unless it is TIERED, and `backing::take` is a latch, so the second and
later executors still `Box::leak` out of this heap — correctly and by design.
The residue is therefore not "the backing size" but "how many executors past the
first", which is a property of the image's own tier declaration.

### The one place the linker could answer it, and why not here

`platform.c`'s own header comment names the idiom —
`#define configTOTAL_HEAP_SIZE ( &_heap_end - &_heap_start )` — and with
`configAPPLICATION_ALLOCATED_HEAP` that is a RUNTIME expression, so the heap
could be "whatever RAM is left after `.bss`". That tracks `EXECUTOR_BACKING`
exactly, by construction, with no number anywhere. It is the real answer to
"move the computation to a layer that can see it", and it is not taken here
because it touches five boards' linker scripts, removes the ability to CAP heap
usage (which `mem-report` and the right-sizing campaign depend on), and would
leave `ucHeap` with no size in `nm -S` — which is how phase-448's acceptance
reads the result. Recorded so the next person does not have to find it again.

## The heap, re-derived ONCE against both reductions

`nros_board_common::freertos_config::default_heap_bytes(app_stack_bytes)`:

```
app_stack_bytes * SLOTS  +  SPARE_EXECUTOR * (SLOTS - 1)  +  WORKING_SET
    131 072     *   3    +     131 072     *      2       +    65 536     = 720 896
```

replacing the bisected literal `2048` KiB in `nros-board-freertos/build.rs`.
Every term is measured on running images and documented where it is defined:

| term | value | measurement |
| --- | ---: | --- |
| `DEFAULT_HEAP_APP_TASK_SLOTS` | 3 | the deepest in-tree image declares 2 tiers; stacks are charged per TASK |
| `DEFAULT_HEAP_SPARE_EXECUTOR_BYTES` | 131 072 | `realtime-rust` 389 064 less two 131 072 stacks less the single-executor overhead = **93 216** for its second, heap-resident executor |
| `DEFAULT_HEAP_WORKING_SET_BYTES` | 65 536 | worst **45 848** (`rust/action-client` 176 920 less its one stack); the C carrier agrees at **44 776** |

It **tracks the app stack**, which is the half this issue insisted could not be
done in a separate commit: lower `NROS_FREERTOS_APP_STACK_KB` and the heap falls
by three times as much, with no second edit to forget. Two unit tests hold the
arithmetic and the worst measured peak.

The cyclone / XRCE lane keeps `FreeRTOSConfig.h`'s 3 MiB. Those backends do not
enable `rmw-zenoh` on the board crate, DDS discovery's working set is a different
measurement, and this PR did not take it.

## Re-measured: the 2026-09-07 backing figures are all stale

`arm-none-eabi-nm -S`, after W6 (#957) and W7+W8 (#951):

| leaf | 2026-09-07 | 2026-09-12 |
| --- | ---: | ---: |
| `talker` / `listener` / `service-server` | 20 608 | **22 736** |
| `service-client` | 21 832 | **35 312** |
| `action-server` / `action-client` | 32 512 | **28 408** |
| `workspaces/rust`, `workspaces/realtime-rust` | — | **87 496** (declare nothing; the default sizing) |

`ucHeap` was 2 097 152 on every one of them and is **720 896** now.

## Found and fixed on the way: the heap knob reached ONE translation unit

`-DNROS_FREERTOS_HEAP_KB` was applied to the kernel `cc::Build` only, on the
reasoning that heap_4 is "the only TU that sizes it" — true of the ARRAY and
false of the MACRO. `nros-platform-freertos/src/platform.c` reads
`configTOTAL_HEAP_SIZE` too, so the platform ABI's
`nros_platform_heap_total_bytes()` / `nros_platform_heap_used_bytes()` answered
from `FreeRTOSConfig.h`'s 3 MiB default while `ucHeap` was 2 MiB: every zenoh
image reported a total it did not have and a phantom **1 048 576** bytes already
in use. It surfaced immediately, because the first thing the new instrument
printed was `heap peak … of 3145728 bytes` on an image whose `nm` says
`ucHeap = 0x200000`. The define now goes through one closure that every
`cc::Build` here is handed. Same class as issue 0135.

The C entry's own reporter deliberately asks `nros_platform_heap_total_bytes()`
rather than reading the macro: that TU is compiled by the CMAKE overlay, which
never forwards the define, so the macro would give a different answer per lane
with nothing to say which.

## The instrument

Every FreeRTOS image now prints, once, at the end of bring-up:

```
nros: heap peak 389064 of 720896 bytes (331832 free) — raise with NROS_FREERTOS_HEAP_KB
```

`xPortGetMinimumEverFreeHeapSize()` — heap_4's own high-water — sampled until it
stops moving, on both lanes (`heap_peak_task_entry` in
`nros-board-freertos/src/entry.rs`, `report_heap_peak` in
`c/freertos_c_entry.c`). This is the measurement route this issue recommended in
its last paragraph, and it is what the terms above were derived from. It is also
the check against a later backing change: the number a default has to cover is
now printed by the image itself rather than re-bisected.

## Verified by RUNNING, which is the only bar that catches a too-small heap

Every in-tree FreeRTOS mps2-an385 image, on QEMU against a live `rmw_zenohd`, at
the new 720 896-byte heap. Worst peak `examples/workspaces/realtime-rust`
**389 064** — 54 % of the budget, 1.85x margin. No `*** MALLOC FAILED ***`
anywhere. The Rust action SERVER does not register, for issue 1348's reason,
which predates this branch.

## Still open elsewhere

Issue 1145's other ports (NuttX, ThreadX, ESP32) are phase-448 W5 and untouched.
