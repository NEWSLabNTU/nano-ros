---
id: 1197
title: "FreeRTOS still budgets `configTOTAL_HEAP_SIZE` for an executor backing that
  moved to `.bss`, and neither of the two mechanisms that fixed Zephyr can be
  used here — the board crate cannot see the size, and stating it would fight
  the per-leaf derivation"
status: open
type: tech-debt
area: [embedded, core, build]
related: [1145, 1171, 1146, 0827, 1061, phase-392, phase-448]
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
