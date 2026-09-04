---
id: 1061
title: Entity budgets are derived for CMake consumers only, so pure-cargo leaves hand-set them
status: open
area: build
severity: medium
related: [1052, 0827, 0832, 0190]
---

## What

`nros-cli-core/src/entity_inventory.rs` computes how many entities of each kind
a component declares, and publishes the result as CMake variables:

```rust
"set(NROS_DERIVED_MAX_SUBSCRIBERS {})\n",
```

That reaches every leaf built through `cmake`. It reaches **no pure-cargo leaf**
— `examples/qemu-esp32-baremetal/rust/{talker,listener}` and
`packages/testing/nros-tests/bins/logging-smoke-esp32-qemu` are built by plain
`cargo build`, so they take `zpico-sys`'s compiled-in defaults (8 subscribers,
8 queryables) whatever they declare.

## Why it matters here specifically

On `qemu-esp32-baremetal` an over-budget static is not a footprint nicety. The
stack is the **linker leftover** after `.bss` (`link.x` fills DRAM up to
`_stack_start`, `.bss` grows up from below), so every byte of unused pool comes
straight out of the stack, and there is no runtime overflow guard: the image
writes frames into `.bss` and dies later as a wild jump somewhere unrelated.

That is issue 1052. The talker shipped with an **18,572 B** stack against
node.rs's ~67 KB budget and faulted with `sp` outside the stack, inside
`nros_smoltcp::TCP_RX_BUFFER_0`. Two of its pools were sized for entities it does
not declare.

## The workaround now in the tree, and why it is one

`examples/fixtures.toml` hand-sets the budgets per row:

| row | declares | hand-set |
| --- | --- | --- |
| talker | 1 pub, 1 timer | `ZPICO_MAX_QUERYABLES=2`, `ZPICO_MAX_SUBSCRIBERS=1` |
| listener | 1 sub | `ZPICO_MAX_QUERYABLES=2`, `ZPICO_MAX_SUBSCRIBERS=1` |
| logging-smoke | — | `ZPICO_MAX_QUERYABLES=2` |

Stacks recovered: talker 18,572 → 49,148 B, listener 22,060 → 52,636 B.

It works, and it is the third time the same edit has been made by hand. The
numbers restate what `register` already says, in a different file, with nothing
tying them together — so they go stale exactly when a leaf gains an entity, which
is the moment being wrong costs the most. The queryable half of this table was
already applied "a row too narrowly the first time" (its own comment says so),
which is this failure mode having happened once.

## Correction: the projection is not the blocker (2026-09-05)

The framing above ("publishes them only as CMake variables") is incomplete, and
the missing half is the one that decides the work. `nros ws entity-inventory`
does not read CMake variables — it reads `nros-metadata.json`, and for a Rust
leaf that file comes from a HOST PROBE: the CLI renders a harness crate that
depends on the component and runs it on the host to extract the declarations.

For these leaves the probe cannot run, for two independent reasons:

1. `.cargo/nros-board.toml` sets `[unstable] build-std = ["core", "alloc"]` with
   a foreign `[build] target`, which is exactly `ProbeBlocker::
   BuildStdForForeignTarget` — build-std is not target-scoped, so the host
   harness would rebuild `core`/`alloc` against that target's sysroot.
2. Even unblocked, the component crate depends on `nros-board-esp32-qemu`, which
   does not build for a host triple at all.

So the metadata is `metadata/<node>.json.unprobeable`, and there is no inventory
to project anywhere. Adding a JSON→cargo channel would deliver nothing for the
leaves that motivated this issue. Source parsing is explicitly rejected upstream
("a second parser for the same fact is how the two spellings drift").

## What landed instead (2026-09-05): the numbers moved to where they are true

Not derivation — placement, plus a real bug it exposed.

The budgets were in `examples/fixtures.toml`, so they applied only to the FIXTURE
build. `examples/` are standalone copy-out projects (CLAUDE.md), and this one did
not link on its own:

```
rust-lld: error: section '.bss' will not fit in region 'DRAM': overflowed by 11184 bytes
rust-lld: error: unable to move location counter (0x3fcd0fb0) backward to 0x3fcce400 for section '.stack'
```

A user copying the example out could not build it. The budgets now live in each
leaf's own `.cargo/config.toml [env]`, beside `ZPICO_SUBSCRIBER_LARGE_SIZE`,
which was already there for exactly this reason — its comment even reads
"Mirrors the esp32 workspace fixture env", documenting the duplication. The
fixture rows no longer state them.

Verified: the talker builds standalone (46,408 B stack, release profile), the
fixture build is unchanged (talker 49,148 B, listener 52,636 B, both clear of the
floor), and the talker now reaches its steady state and publishes
`Hello World: 1..7` — which it never did while it was faulting.

Note for whoever reads the build log: `just esp32 build-qemu` now exits 0 through
the pack step, and that is COINCIDENCE, not a fix. Empty row env means the group
slug is the bare `qemu-esp32-baremetal`, which happens to be where the packer's
invented-empty-args lookup points. Any row that regains an env breaks it again.
Issue 1025 is still real and its fix is still PR #303.

## Fix (still open)

Deliver the inventory to the cargo path as well as the CMake path, so a leaf's
budgets follow its declarations. The inventory is already computed; what is
missing is a channel `zpico-sys`'s `build.rs` can read — the same shape as
`nros_zephyr_build::knob_usize` reading `$DOTCONFIG`, and subject to the same
rule as issue 0460: a knob that reaches one lane and not the other is an ABI
split, not just a missing feature.

Doing it means giving the probe a way to see a component whose board crate is
target-only — a host stub behind a feature, or a declaration channel that does
not require compiling the component at all. That is the real work, and it is
larger than a projection change.

Meanwhile `scripts/check-stack-floor.py` is the backstop: it fails the build if
an ESP32 image drops below a 32 KB stack, so a leaf that gains an entity without
gaining budget is caught at build time rather than as a wild jump at runtime.
The gate bounds the damage; it does not remove the hand-maintenance.

## Not to do

Do not raise the ESP32 stack by shrinking the heap. node.rs documents that as a
two-sided constraint (issue 0190): 16 KB is too small for the executor backing
and 96 KB starves the stack. The heap is not where the slack is — the unused
pools are.
