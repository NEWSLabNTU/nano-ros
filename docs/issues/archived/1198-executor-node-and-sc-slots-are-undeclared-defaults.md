---
id: 1198
title: "`MAX_NODES` and `MAX_SC` are undeclared defaults, so every executor backing
  carries a constant 12,416 B of tables sized for 4 nodes and 8 scheduling
  contexts whatever the image declares"
status: resolved
type: tech-debt
area: [core, embedded]
related: [1145, 1171, 0827, 0857, 0900, 1061, phase-392, phase-448]
---

## What

The executor backing is `arena + fixed tables`
(`nros-node/src/executor/storage.rs::compute_offsets`). Four inputs size it:

| input | source | default |
| --- | --- | ---: |
| `MAX_CBS` | **derived** per leaf by `nros sync` (`NROS_EXECUTOR_MAX_CBS`) | 4 |
| `ARENA_SIZE` | **derived** from `MAX_CBS`, `RX_BUF_SIZE`, `ACTION_CLIENTS` | floor 8192 |
| `MAX_SC` | `NROS_EXECUTOR_MAX_SC`, nothing derives it | **8** |
| `MAX_NODES` | `NROS_EXECUTOR_MAX_NODES`, nothing derives it | **4** |

The first two follow the declaration through the issue 0827 / 1061 channel. The
last two do not: no leaf states them, no sidecar derives them, so every image
gets 8 scheduling contexts and 4 node slots regardless of what it declares.

## Measured

Six FreeRTOS Rust leaves, `arm-none-eabi-nm -S` on
`build/cargo-fixtures/freertos/thumbv7m-none-eabi/nros-relwithdebinfo/<leaf>`,
against the `nros_node_config.rs` each `nros-node` build unit emitted:

| leaf | `MAX_CBS` | `ARENA_SIZE` | `EXECUTOR_BACKING` | backing − arena |
| --- | ---: | ---: | ---: | ---: |
| `talker` | 1 | 8,192 | 20,608 | **12,416** |
| `action-server` | 1 | 20,096 | 32,512 | **12,416** |

**The table term is identical to the byte** across leaves whose declarations
differ, because `cbs`, `sc` and `nodes` are the same in both. All per-leaf
variation is the arena. So ~12.4 KiB of every FreeRTOS image's backing is
insensitive to what the image actually contains.

`MAX_NODES` is the heavier of the two: it multiplies **seven** tables, by the
layout's own comment — "one worst-case extra session, extra-session id,
node-sched binding, dispatch slot, component slot and callback-group filter
entry per Node, so ONE count covers all seven".

## Why this is worth more than the pairing it was found under

Issue 1145 is about 20–32 KiB reserved twice on FreeRTOS. This is ~12.4 KiB
reserved **once but for nothing**, in every image on every platform, and it is
the larger and simpler target: a single-node talker pays for four nodes.

It is also the same defect class the campaign has already fixed twice —
issue 0827 (pools sized at the backend, where the entity set is unknown, instead
of at the image, where it is known) and issue 0857 (`ENTITY_BOUNDS` defaulting to
the knob caps because 81 of 99 classes declared nothing). Both were closed by
making the count follow the declaration. `MAX_NODES` and `MAX_SC` are the two
that were left.

## What has to be answered first

**Is the node count knowable at sync time?** `MAX_CBS` is, because the entity
declaration names publishers, subscribers, timers and services. Nodes are a
different axis: an entry may create several `Node`s, and the tiered boot paths
open one executor per tier. Whether the count is derivable from the model or the
contract sidecar, or must be declared by the entry, is the design question — and
it is close to issue 0973's answer (wiring is AUTHORED), so the honest first step
is to find out which artifact already states it.

**`MAX_SC` is scheduling, not entities.** Scheduling contexts come from the tier
model / RFC-0016, not from the entity inventory, so it may belong to a different
declaration than `MAX_CBS`.

## Do not

Lower the defaults by argument. The failure mode is a runtime registration
failure (`NodeError::BufferTooSmall`, the shape issue 0900's arena derivation
already hit) rather than a link error, and phase-392's standing rule is that no
wave claims a saving it did not measure. Whatever number replaces 4 and 8 must
be derived from a declaration or measured on a running image.

## Reproduce

```
bash scripts/build/fixtures-build.sh freertos rust
for f in $(find build/cargo-fixtures/freertos -name nros_node_config.rs); do
    grep -hE 'pub const (MAX_CBS|MAX_SC|ARENA_SIZE|MAX_NODES)' "$f"
done
arm-none-eabi-nm -S <leaf-elf> | grep EXECUTOR_BACKING
```

## Resolution (phase-448 W6, 2026-09-11)

Both knobs are DERIVED, on all three roads, and every FreeRTOS Rust leaf lost
**4,344 B** of executor backing — 3,672 B of it from the node table, which
[issue 1233](1233-derived-fact-road-gaps.md)'s fix delivered on 2026-09-09 while
this was in flight, and 672 B from the scheduling table, which was on no road at
all and is this work's own.

### The two design questions, answered from the tree

**1. Is the node count knowable at sync time? YES, and it was already being
computed — it just could not travel.** (Answered here; DELIVERED by issue 1233's
own fix, which landed on `main` from the other direction on 2026-09-09 while
this was in flight and reached the same conclusion. Recorded rather than
re-claimed.)

`DerivedEntityKnobs::max_nodes` has existed since phase-412: one slot per
declared component, `EntityInventory::derive`. It reached
`resolved.cmake` as `NROS_DERIVED_EXECUTOR_MAX_NODES` and the Zephyr resolver
road, and NO OTHER ROAD. Both cargo roads were
[issue 1233](1233-derived-fact-road-gaps.md)'s registered `OpenGap`, and the
reason was written not in the registry but at the CMake producer
(`cmake/NanoRosEntityFacts.cmake`): *"phase-412 withheld it from W1 on the
ground that under-counting HALTS the board."*

So the artifact that states the node count is the one the issue guessed at —
the component registration, which for these leaves comes from the resolved
**SystemModel** (`structure.nodes`, the same fact already carried as
`NROS_DECLARED_NODES` for the zenoh queryable sizing). Issue 0973's answer holds:
the wiring is AUTHORED, and a model that resolves has nodes.

The withheld ground is now MEASURED rather than assumed. Under-counting nodes
has exactly one source — `nros_pubsub_bridge_create`, whose two nodes are
runtime strings declared nowhere — and every exhaustion path NAMES the knob:
`NodeError::NodeTableFull` ("the executor's node table is full
(NROS_EXECUTOR_MAX_NODES)"), the bridge's own two diagnostics
(`packages/rmw/bridge/src/cffi.rs`), and the zenoh session's per-node liveliness
table ("Raise NROS_EXECUTOR_MAX_NODES, which is what sizes it"). That is the
condition this phase requires, and it was already satisfied.

**2. Where does `MAX_SC` come from? The SCHEDULE, not the entity inventory —
and the two producers of scheduling contexts disagree about the cost of a
tier.**

Measured over the tree, `create_sched_context` has exactly two non-test
producers:

* the **Rust runtime creates NONE.** `ExecutorNodeRuntime::apply_tier_sched_policy`
  (`packages/api/nros/src/node_runtime.rs`) ends in `set_default_sched_context`,
  which MUTATES slot 0. A tiered Rust boot opens one executor per tier and each
  spends its own reserved slot 0 — no table entry at all. This is why the six
  FreeRTOS Rust leaves need **2** and were given 8.
* the **C / C++ entry pack creates one per tier**, through
  `nros_cpp_create_sched_context_from_policy` into `__nros_sc_ids[N]`, where `N`
  is `SchedView::n` — the resolved tier table's length
  (`packages/cli/nros-cli-core/src/codegen/entry/mod.rs`).

Plus slot 0, which `Executor::create_sched_context` reserves for the default
Fifo context and never hands out (it searches `1..MAX_SC`).

So the declaration is `execution.tiers` (RFC-0016 / `[tiers.*]` in
`system.toml`), which is a DIFFERENT declaration from the entity one exactly as
the issue suspected, and the inventory now carries it beside `infra`. The
derivation is:

```
max_sc = 1 + max(authored tiers, nodes)
```

The second term is the LARGER of the two, not the authored tier count alone,
because a bringup that authors no tiers does not thereby have none:
`derive_tiers_from_contracts` synthesises a `derived-<node>` tier per
schedulable node, so an empty `[tiers.*]` can still resolve to one tier per
node. Taking the larger covers both shapes and over-counts for the Rust one,
which is the safe direction.

### The one thing that had to change before deriving was safe

`MAX_SC`'s undeclared source is application code calling `create_sched_context`
by hand — no artifact describes it, the same shape as the bridge is for
`MAX_NODES`. `NodeError::NoSchedContextSlot`'s Display already names the knob,
but all three FFI wrappers flattened it to a bare `NROS_RET_FULL` /
`NROS_CPP_RET_FULL`. They now log it, reusing that one Display so the text stays
in one place:

* `nros_executor_create_sched_context` (`packages/api/nros-c/src/executor.rs`)
* `nros_cpp_create_sched_context` and
  `nros_cpp_create_sched_context_from_policy` (`packages/api/nros-cpp/src/lib.rs`)

Sweep: `grep -rn 'create_sched_context(sc)' packages/` — three call sites
outside `nros-node` itself, all three fixed together.

### Measured — `arm-none-eabi-nm -S`, thumbv7m-none-eabi, nros-relwithdebinfo

`EXECUTOR_BACKING`, over every FreeRTOS Rust example leaf. **Three columns,
because the node half landed on `main` from another direction while this was in
flight** — [issue 1233](1233-derived-fact-road-gaps.md)'s own fix (`fix(#1233):
four road-gaps, closed by delivery and not by an excuse`, 2026-09-09) put
`NROS_DERIVED_EXECUTOR_MAX_NODES` on both cargo roads, reaching the same
conclusion this issue reached about the same withheld ground. So the middle
column is what `main` gives today and the last is what this work adds on top:

| leaf | undeclared (4 / 8) | `MAX_NODES` derived | + `MAX_SC` derived |
| --- | ---: | ---: | ---: |
| `talker` | 27,080 | 23,408 | **22,736** |
| `listener` | 27,080 | 23,408 | **22,736** |
| `service-server` | 27,080 | 23,408 | **22,736** |
| `service-client` | 39,656 | 35,984 | **35,312** |
| `action-server` | 32,752 | 29,080 | **28,408** |
| `action-client` | 32,752 | 29,080 | **28,408** |

`MAX_NODES = 4 -> 1` is **-3,672 B** (1,224 B a slot over three slots, because
it multiplies seven tables). `MAX_SC = 8 -> 2` is a further **-672 B** (112 B a
slot over six). Together **-4,344 B in every leaf**, and exactly additive.

The two columns on the right were measured from ONE tree, isolating the knob
rather than the commit: the same build with and without `NROS_EXECUTOR_MAX_SC=8`
in the environment, which is rung 1 and reproduces `main`'s compiled value. The
emitted `nros_node_config.rs` confirms the two builds differ in that constant and
nothing else:

```
main-equivalent  MAX_CBS 1  MAX_SC 8  ARENA_SIZE 20096  MAX_NODES 1
this work        MAX_CBS 1  MAX_SC 2  ARENA_SIZE 20096  MAX_NODES 1
```

The leftmost column's absolutes differ from the table at the top of this issue
because the arena derivation has moved since it was written (phase-412 #4 landed
the depth-aware arena). The TABLE term is what this issue is about, and it was
the same 12,416 B in every leaf -- which is the defect restated: it did not
depend on the declaration.

Note the two FreeRTOS rows with no `system.toml` (the wake-latency bench and the
logging smoke bin) still compile `MAX_NODES = 4` / `MAX_SC = 8`. That is
correct, and it is the rule this phase works to: a derived value is a DEFAULT,
absent means "no answer", and nothing was lowered by argument.

### Delivery, read from the build

`examples/mps2-an385-freertos/rust/talker/build/mps2-an385-freertos/nros-cargo.toml`
after `nros sync`:

```
NROS_EXECUTOR_MAX_CBS = "1"
NROS_EXECUTOR_MAX_NODES = "1"
NROS_EXECUTOR_MAX_SC = "2"
```

and the emitted `nros_node_config.rs` for that build unit reads
`MAX_NODES = 1`, `MAX_SC = 2`.

### Where each number now travels

| road | `MAX_NODES` | `MAX_SC` |
| --- | --- | --- |
| resolver (Zephyr) | `NROS_RESOLVED_NROS_EXECUTOR_MAX_NODES` (already) | `NROS_RESOLVED_NROS_EXECUTOR_MAX_SC` (new; Kconfig default 8 -> the `-1` DERIVE sentinel) |
| sidecar (cargo leaf) | `NROS_EXECUTOR_MAX_NODES` | `NROS_EXECUTOR_MAX_SC` |
| declared (CMake, no leaf) | `NROS_DECLARED_EXECUTOR_MAX_NODES` | `NROS_DECLARED_EXECUTOR_MAX_SC` |

`check-declared-fact-carriers` had 4 OPEN roads when this work started, all of
them issue 1233's. That issue closed its own four on 2026-09-09; `MAX_SC` was
never among them, because a fact that is never PUBLISHED cannot be recorded as
missing a road. It is published and carried now, so the registry describes it
too.
