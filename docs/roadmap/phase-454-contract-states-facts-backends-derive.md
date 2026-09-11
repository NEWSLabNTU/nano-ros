# phase-454 — the contract states the facts, every backend derives its own buffers

**Status (2026-09-11). Opened.** Home phase for
[RFC-0100](../design/0100-rmw-agnostic-sizing-model.md). Successor to
[phase-403](phase-403-type-bound-rx-sizing.md) (the bound inventory and the
entity inventory, both landed) and [phase-412](phase-412-derived-counts-and-sizes.md)
(derived counts). **Answers phase-412 W5**, which is still open and states the
question this phase exists to settle:

> *"what a standalone leaf derives FROM. … Answering that is a design decision —
> declare entities somewhere sync can read, or accept a stated knob for
> standalone leaves — and it is this phase's to make."*

The answer is RFC-0100 D3: the contract file, always.

Closes [issue 1256](../issues/1256-contract-qos-carries-depth-only.md) and issue
1319 (not linked: its file is on PR #951, so a link would dangle until that
merges — see the `.config/prose-issue-ref-baseline.txt` row).

## MUST READ FIRST — this phase does not start on `main`

phase-448 W7/W8 is in flight on `origin/feat/448-w7-w8-arena-model` and has
already landed, unmerged, two things this phase would otherwise redo:

| issue | what it did | status |
| --- | --- | --- |
| 1255 | the arena prices each subscription at its OWN type's bound, not one global maximum | resolved in phase-448 W7 |
| 1290 | `arena_size_for(cbs)` stopped scaling a declared model by `cbs / MAX_CBS` | resolved in phase-448 W8 |

That branch touches `nros-node/build.rs`, `nros-node/src/config.rs`,
`check-declared-fact-carriers.py`, `check-knob-delivery.py`,
`config-knob-census.py` and `nros_cargo_build.cmake` — the same files as W5/W6
here. **Base this phase on that branch, not on `main`**, and re-check the
overlap after it merges.

There is more shadow backlog: phase-412's `#4` (`arena_oracle.rs`), `#7`, W2
`#1130`, W5 `#1142` and `#1233` exist only in commit subjects on unmerged
branches and appear nowhere in the phase-412 doc. Reconcile before W5.

## The goal, in the owner's words

> I prefer to have an agnostic formula for the executor and various RMWs, and
> each RMW derives their internal buffer sizes based on user information.

and, on where the facts live:

> QoS always goes to the contract file. Retire ENTITIES.

## What this phase is not

Not a new sizing campaign. phase-403 built the two inventories and phase-412
wired counts; both work. This phase **names the model**, gives it one transport,
and closes the three gaps those phases left: the executor cannot read the payload
class it needs, three of four QoS policies are accepted and dropped, and each
backend's derivation was decided separately.

## Why the ordering is what it is

Two constraints fix the wave order, and neither is a preference.

**W1 before any new policy.** ROS QoS defaults are literals in four places
(`nros-node/build.rs:27`, the `qos_profiles!` table, `nros-c/src/qos.rs`, the
cffi layer, plus `CONFIG_NROS_PUBSUB_QOS_DEPTH`), held equal only by gates
against `docs/reference/rmw-qos-profiles.txt`. Adding three defaulted policies
before single-sourcing them multiplies the drift surface by four.

**W2 before transient-local pricing.** `EntityInventory::from_model` hardcodes
`depth: None` for every publisher row, so a contract stating
`pub: { qos: { depth: 8 } }` cannot reach the build at all. Durability is
publisher-side; without W2 there is nothing to price.

## Waves

### W1 — one source for the ROS QoS defaults

Move the four literal sites behind one declaration. No new policy reaches a build
in this wave; the deliverable is that adding one later touches one file.

Acceptance: the existing `rmw-qos-profiles.txt` gate still passes, and a
deliberate edit to the single source fails every consumer at once (negative
control — a gate that cannot fail proves nothing).

### W2 — a publisher can declare depth

Remove the `depth: None` hardcode in `EntityInventory::from_model`. Publisher
depth reaches the descriptor. Nothing prices it yet.

Acceptance: a contract with `pub: { qos: { depth: 8 } }` produces a publisher row
carrying 8, asserted on a fixture contract.

### W3 — the contract carries all four policies; `keep_all` refuses

`reliability`, `durability` and `history` stop being dropped at `depth_of`
(`entity_inventory.rs:872`). `history = keep_all` sets `status = "refused"` for
depth-derived fields, naming the endpoint.

This is the wave with a live defect behind it: `keep_all` is priced today as
whatever `depth` says, which is a **silent under-size**, and it is the one
trigger in RFC-0100 D6 that can ship a too-small buffer rather than a too-large
one.

Acceptance: a `keep_all` endpoint refuses with a message naming it; a
four-policy contract round-trips into the descriptor; issue 1256 closes.

### W4 — the descriptor artifact

`nros sync` writes `build/nros/sizing/<entry>.toml` per RFC-0100 D4. Schema,
writer, and a reader crate the consumers share. Per-field status (D6), not one
global status.

`[target]` is populated from the **board descriptor**, never from a host build
script — build scripts run for the host (phase-118-E), so `DEP_NROS_NODE_*` carry
host sizes on a cross build and storage capacity is the target-dependent size.

Acceptance: descriptor is byte-identical across two checkouts of the same tree at
different paths (the issue-0320 portability rule); a cargo consumer gets a
`rerun-if-changed` edge; a cmake consumer reading it at configure time registers
it in `CMAKE_CONFIGURE_DEPENDS` (issue 1018).

### W5 — the executor reads the descriptor, and the model learns the registration path

Two halves, and the second is the one with a live defect behind it.

**W5.a** — the executor takes its facts from the descriptor rather than from env
carriers. One road instead of two, so an image no longer sizes differently
depending on which lane built it (issue 1199's disease).

**W5.b** — close **issue 1319**. The arena budgets a subscription at the type's
bound, but a Rust typed registration on a *schemaless* backend (zenoh, XRCE) —
and any C/C++ raw registration with no hint — actually claims `RX_BUF`. On the
island at depth 1 that is **1,848 bytes per subscription the model does not
hold**, in the UNDER direction, landing as `NodeError::BufferTooSmall` at a
registration the arena oracle passed.

An earlier draft of this wave claimed the opposite — that lowering the term to
the payload class *recovers* 11,344 bytes. That would have **made issue 1319
worse on every zenoh and XRCE image**. Issue 1319 rules the fix out directly:

> *"Not 'raise the term back to `RX_BUF`' — that gives up issue 1255's saving on
> the path where the per-type bound IS what is allocated."*

So W5.b carries the registration path as a descriptor fact (RFC-0100 D1), which
is issue 1319's second candidate. Its third — a stated per-image margin — is
rejected there as *"the answer that goes stale next time a path changes"*.

Acceptance, **measured not asserted**: an image that declares its entities,
subscribes from Rust on zenoh, and has `NROS_SUBSCRIBER_BUFFER_SIZE` below
`NROS_SUBSCRIPTION_BUFFER_SIZE` — issue 1319 names this shape and notes it was
never reproduced — registers every subscription without `BufferTooSmall`, and
`mem-report --baseline` shows the arena at or above `arena_model::REQUIRED`.
A reproduction of the failure comes FIRST; issue 1319 is analysis-only today.

### W6 — each backend derives its own

One sub-wave per consumer; they are independent and can run in parallel.

| sub-wave | backend | lands |
| --- | --- | --- |
| W6.a | zenoh | `SUBSCRIBER_RING_DEPTH` from declared depth (authored today); `SERVICE_BUFFERS` derived from request/response bounds and given a `// nros-pool:` annotation or a stated reason — it is 144,128 B on a native talker and in neither |
| W6.b | XRCE | `reliability` gates the two 64 KiB `*_reliable_buf`; one global `BUFFER_SIZE = 1024` splits into three per-family sizes; ring depths take declared depth as their default |
| W6.c | Cyclone | `MAX_DESCRIPTOR_TYPES` derived from the same count as `MAX_TYPES` (silent-drop overflow at ~86 types today); `MAX_FIELDS`/`MAX_KINDS` from the schema walk codegen already does; heap budget emitted and asserted at boot (D11) |
| W6.d | uORB | `REGISTRY_CAPACITY` and `PX4_MAX_CALLBACKS` — both trivially derivable, neither wired |
| W6.e | cffi | `MAX_NODES` from `components().len()`, which is already computed and discarded |

Each sub-wave keeps RFC-0100 D7: publish demand **unfloored**, floor at the pool.
Issues 1015 and 1033 are the negative control — one derivation feeding two
consumers with opposite right answers, where the floor in the shared derivation
silently defeated the fix.

Acceptance per sub-wave: a measured byte delta on a named image, plus the pool
inventory regenerated.

### W7 — contract and `qos_overrides` must agree

Build error on any divergence, naming both sites (D8). Where an override states a
policy the contract omits, the error names the contract line to add.

This closes a live under-size path with no diagnostic:
`qos_overrides./t.subscription.depth=64` is baked into the runtime today and the
arena never hears about it — issue 1190's `BufferTooSmall` with a config-shaped
cause.

Acceptance: a divergent pair fails the build naming both; an agreeing pair
builds; a params-only policy fails naming the contract.

### W8 — `buffer:` earns its keep

Two diagnostics (`queue` at `depth = 1`; `latest` at a large depth), and the
rate-derived depth default for `queue` endpoints from `topics.<t>.rate_hz` and
`paths.<p>.trigger.timer.rate_hz` — both keys already in the schema. A stated
`depth` still wins, per the ladder.

### W9 — retirement

Per `check-knob-single-reader.py`'s own rule:

> *"Retirement is a wave, not a side effect. A mechanism that still resolves is a
> mechanism people still use, and a fallback left in place winning silently is
> how issues 0135 and 0316 happened."*

So each retired path is **registered in that gate**, not merely deleted. The
surface is smaller than it looks — most of it is already orphaned:

| surface | size | note |
| --- | --- | --- |
| `[package.metadata.nros.component] entities` | **2 leaves** (`examples/esp32-c3-baremetal/rust/{talker,listener}`) | neither uses `@depth=`; `reconcile()` compares kind counts only, so a depth there is checked against nothing |
| the `@depth=` string grammar (`entity_inventory.rs:362`) | already an orphan | its cmake producer fatals; the metadata-JSON reader has no producer left; the contract road never uses the grammar (`depth_of` builds the field directly). Its `depth=0` refusal is already duplicated on the surviving road |
| `NROS_DECLARED_*` / `NROS_DERIVED_*` carriers | **~35 names, 9 producers** | the big one. Retires `check-declared-fact-carriers.py` and its `ROAD_PAIRS` map by construction — one road has no pairing to drift |
| `orchestration/schema.rs`'s `QosProfile` | zero producers | nothing constructs one; `planner.rs` never builds a `PlanEntity`; real generated plans have no `entities` key. It is the vestigial version of the type the contract now owns — replace, don't keep a third apparent source |

Also stale and worth fixing while here: the hint text at
`entity_inventory.rs:1410` points users at `nano_ros_node_register(... ENTITIES
...)`, a verb that now fatals.

Acceptance: the gate lists each retired knob with its single legitimate reader,
and a second reader is a hard red.

### W10 — the C and Rust halves of the declared-QoS check

`_nros_declared_qos_arm` returns early for Rust and INTERFACE targets, and there
is no C equivalent of `NROS_ASSERT_DECLARED_DEPTH`. The descriptor is
language-neutral, so this is where that closes.

## Acceptance for the phase

Not "it builds". One correctness result and three measured recoveries, each on a
named image, each with a before/after from `just mem-report --baseline`:

| gap | direction | acceptance |
| --- | --- | --- |
| Rust typed on a schemaless backend (issue 1319) | **UNDER**, 1,848 B per subscription | a reproduction that fails with `BufferTooSmall` FIRST, then passes |
| XRCE reliable streams | over, ~131,072 B per session | a best-effort-only image stops paying both buffers |
| zenoh `SERVICE_BUFFERS` | over, 144,128 B (native talker) | derived, and either annotated or given a stated reason |
| island arena, declared vs default depth | over, 135,432 B (207,096 → 71,664) | already true on the Zephyr road; holds on the cargo road too |

The ordering is deliberate. Three of these are money on the table and one ships
a runtime failure — a phase that chased only the savings would have made the
fourth worse, which is exactly what this phase's first draft did.

The last row stays the clearest argument: the running code does not change. Only
whether the build was told what it registers.

## Explicitly not in this phase

* **cffi's `SLOT_SIZE`.** A hard 1024 where `insert::<T>()` returns `None` if
  `size_of::<T>() > 1024`, so a backend growing its per-subscription state
  silently fails `create_subscription`. No user fact answers it; it needs a
  compile-time assertion in cffi. Separate issue.
* **A static pool for Cyclone.** D11 gives it a heap budget and a boot
  assertion; inventing a pool would touch the vendored fork's ddsrt allocator.
* **`lifespan`, `deadline`, `liveliness`.** They bound occupancy or liveness, not
  capacity. They stay runtime-only.
* **RFC-0049's ladder.** Precedence is unchanged.
