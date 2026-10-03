# RFC-0100 — One sizing model: the contract states facts, every backend derives its own buffers

**Status:** Draft (2026-09-11; last reviewed 2026-10-03 — see
[Amendment 1](#amendment-1-2026-10-03--the-unified-build-path-moved-under-this-model),
which adds D12 and revises D4's producer and road language against RFC-0065's
unified build path)

Builds on RFC-0049 (the four-rung knob ladder supplies the precedence this model
needs and is not re-litigated), RFC-0033 (storage modes decide whether a cap
bounds the wire) and RFC-0065 D2 (refuse and name the remedy). Amends the
implicit model phase-403 and phase-412 built incrementally, by naming it and
closing three gaps it left. Absorbs issue 1256 (the contract carries depth only).

Home phase: [phase-454](../roadmap/phase-454-contract-states-facts-backends-derive.md).
Prior phases: 403 (type bound sizes every receive buffer), 412 (derived counts
and sizes), 408 (C++ message-derived buffers).

## The decision

> **A buffer's size is a FUNCTION of facts the user already declares. The
> contract file states those facts once; one generated descriptor carries them;
> each backend applies its OWN formula to the subset it needs. No backend name
> appears above the backend layer, and no size is a number a human typed unless
> nobody could have derived it.**

## Why this RFC exists

The tree already sizes this way in three places and has never said so, so each
place re-decided the shape and two of them disagree. Naming the model is what
makes the fourth place derivable instead of authored.

It also settles a number that is currently **road-dependent and contested**, which
is a sharper problem than a leak.

`nros-node/build.rs:348` reads `NROS_SUBSCRIBER_BUFFER_SIZE`. On the **cargo
road** nothing sets it, because the value exists only under the backend-named
spelling `ZPICO_SUBSCRIBER_BUFFER_SIZE` and the crate's own comment forbids
reading it:

> *"a BACKEND name, which this backend-agnostic crate must not read — so
> `NROS_SUBSCRIBER_BUFFER_SIZE` is absent here and this falls back to the closure
> knob."*

On the **Zephyr resolver road** it does arrive, and the term prices at 880
instead of the closure's 1,496. So one image sizes differently depending on which
lane built it — exactly what `check-declared-fact-carriers.py` exists to police:

> *"issue 1199 — the two roads a derived number takes into cargo … They must
> deliver the same KNOBS, or an image's sizing depends on which lane built it."*

**And neither number is simply right.** Issue 1319 measures that the runtime
allocation depends on the REGISTRATION PATH, which the build cannot see:

| path | slot size |
| --- | --- |
| C/C++ typed with a hint (`rx_size_bound<M>`) | the type's own `_RX` — matches the model |
| Rust typed, backend WITH descriptors (Cyclone) | `min(framed(bound), RX_BUF)` — at or below |
| **Rust typed, backend WITHOUT descriptors (zenoh, XRCE)** | **`RX_BUF`** |
| **C/C++ raw with no hint** | **`RX_BUF`** |

On the island at depth 1 the last two rows are **1,848 bytes per subscription the
model does not hold** — an UNDER-size, the direction that ships
`BufferTooSmall`. So "make the executor read the payload class" is not the fix;
the fix is that **the registration path is a fact the model must carry**, and
RFC-0100 D1 is where it acquires a home. Issue 1319's own framing:

> *"Not 'raise the term back to `RX_BUF`' — that gives up issue 1255's saving on
> the path where the per-type bound IS what is allocated."*

The layering observation still stands and is what D5 fixes: the payload class is
a property of the TYPES THE IMAGE SUBSCRIBES TO, a contract fact, not a zenoh
fact. It simply is not the whole of the number.

## D1 — three kinds of fact, three owners

| kind | examples | who can answer | home |
| --- | --- | --- | --- |
| **image** | entity counts by kind, per-endpoint QoS, bounds of subscribed/serviced types, distinct type count, **registration path** | the image's own declaration | contract file |
| **policy** | burst ring depth, domain graph size, transport MTU, session count, retransmit history | nobody — must be stated | contract file, tagged `policy` |
| **target** | pointer width, alignment, heap budget | the board | board descriptor |

Policy is its own kind because the tree already argues it correctly and would
otherwise lose the argument. XRCE's Kconfig:

> *"it is deliberately NOT derivable — no inventory knows how deep a burst a
> subscriber must survive. It is a policy, so it is stated."*

Correct — and a declared `qos.depth` is still a better **default** than a literal
32. So: a policy fact is *stated*, and a derived fact may supply its default.
That is RFC-0049's ladder, not a new mechanism.

**Registration path** is an image fact and a late addition, forced by issue 1319:
a subscription's slot size depends on whether it registers typed from C/C++ with
a hint, typed from Rust against a descriptor-carrying backend, or typed from Rust
against a schemaless one — and the third takes `RX_BUF` while the model budgets
the type's bound. The build cannot see this today. Of issue 1319's three candidate
fixes, this RFC takes the second — *"have the build model the REGISTRATION PATH,
which it cannot see today"* — because the third (*"state the shortfall as a
per-image margin"*) is explicitly *"the answer that goes stale next time a path
changes"*, and the first is a narrower repair that leaves the build blind.

**The path has FIVE rows, not four — phase-454 W5 measured the fifth.** Issue
1319 read the four out of the source and assigned zenoh and XRCE to the
`RX_BUF` row. Running it says otherwise: `register_subscription_buffered_on`
asks `handle.supports_process_in_place()` **before** it computes a slot size,
both schemaless backends answer an unconditional `true`, and the registration
returns through `SubInplaceEntry` having allocated no receive region at all.
Measured on `contract-monitor-sub` over zenoh, a `std_msgs/Header` subscription
claims **672 bytes** of arena against the 9,768-byte region the model budgets it
at `KEEP_LAST(10)`.

So `in_place` is its own row, and the two directions are not symmetric:

| direction | rows | what the model does |
| --- | --- | --- |
| UNDER — ships `BufferTooSmall` | `unbounded` | fixed in W5.b: each row is priced at what its path claims |
| OVER — wastes RAM | `in_place` | **phase-457 W3 takes the saving** — see below |

(Row names as of phase-456 W8, which collapsed the five to three and took the
caller's LANGUAGE out of their names: it was never a property of the
registration.)

**The over-statement was left alone because the row could not be attributed per
endpoint, and W3 is what fixed that.** The Rust **generic**
(`.generic(ty, hash)`) registration on the same backend does NOT reach the
capability test and does claim `RX_BUF` — and nine of the executor's eleven
subscription entry points are in the same position, so a row composed from the
`rmw` name alone credited every endpoint of a zenoh or XRCE image with a
capability most of them do not have. Free while the row was priced at the type's
bound; an UNDER-size the moment it is priced at nothing.

W3's answer is that the endpoint's own half is **observed**, never inferred:
`Executor::open_subscription` — the one site that reads the capability — reports
what the CALL SITE answered, the metadata probe records it per row, and a row
nothing observed is REFUSED
([issue 1522](../issues/archived/1522-registration-path-unobserved-on-roads-whose-probe-does-not-register.md)).
Only a STATED `in_place` claims no region
(`Endpoint::claims_no_receive_region`), so the one predicate whose `true` removes
bytes is reachable only from a fact somebody measured. Measured on
`examples/native/rust/listener` with a four-subscription `KEEP_LAST(10)`
contract: `ARENA_SIZE` 51,552 → 8,192 B, static RAM 195,402 → 152,042 B
(**−10,840 B per subscription**), and byte-identical to the baseline when the
rows are observed NOT capable or not observed at all.

Target facts are separate because **build scripts run for the host**
(phase-118-E), so `DEP_NROS_NODE_*` carry host sizes on a cross build. Storage
capacity is the target-ABI-dependent size; it cannot be inferred where it is
computed today.

## D2 — one formula, four backends

```
pool_bytes  =  Σ over classes [ COUNT(class) × SLOTS(class) × SLOT_BYTES(class) ]  +  fixed
```

The formula is agnostic. What a backend chooses is **which fact feeds each
factor**:

| pool | COUNT | SLOTS | SLOT_BYTES |
| --- | --- | --- | --- |
| zenoh `SMALL_PAYLOADS` | subscription count | QoS depth | small-class bound |
| zenoh `SERVICE_BUFFERS` | sessions × queryables | 1 | request/response bound |
| XRCE `*_reliable_buf` | **1, per session** | `STREAM_HISTORY`, policy floor 4 | **MTU** |
| XRCE `subscriber_slots` | subscription count | ring depth | payload bound |
| Cyclone type registry | distinct type count | 1 | descriptor slot |
| uORB registry | distinct topic count | 1 | entry |
| executor pub/sub arena | subscription count | `buffered_region(depth)` | rx class |

An earlier draft of this model said COUNT × DEPTH × SIZE and broke on two of four
backends. The defect was that **DEPTH conflated three quantities**. `SLOTS` is
the repair: it resolves from a QoS history depth, *or* a protocol floor, *or* 1.
XRCE's `STREAM_HISTORY >= 4` stops being an anomaly — it is a `SLOTS` source
whose floor is a retransmit requirement rather than a queueing choice. And
`SLOT_BYTES` resolving from **MTU** rather than a type bound is what admits
transport-framed backends to the same line.

CycloneDDS allocates every payload with `ddsrt_malloc` and is not an exception to
this model — it is a backend whose demand lands one layer up:

> `subscriber.cpp:96-99` — *"a Cyclone consumer can set the hint, do everything
> the sizing campaign asks, and correctly observe nothing change in this backend.
> What DOES change is the executor's arena … measure the arena, not the
> backend."*

That sentence is why an agnostic model is possible at all.

## D3 — the contract file is the single declaration surface

`nano_ros_node_register(... ENTITIES ...)` is already retired (phase-412); it is
parsed only to `FATAL_ERROR` naming the contract sidecar. This RFC finishes the
job: **a leaf `Cargo.toml`'s `[package.metadata.nros.component] entities = [...]`
stops being a QoS surface.** Today `@depth=` parses there and is silently
dropped, which is a declaration the author believes they made.

Three of four QoS policies are already accepted by the contract schema and
dropped at `depth_of` (`entity_inventory.rs:872`) — issue 1256. They become live:

| policy | effect on sizing |
| --- | --- |
| `history = keep_all` | **no static bound exists** → refuse (D6). Priced today as whatever `depth` says, which is a silent under-size |
| `reliability` | gates XRCE's two 64 KiB `*_reliable_buf`; a best-effort-only image pays ~128 KiB per session today |
| `durability = transient_local` | publisher-side retention — requires publisher depth (below) |
| `depth` | live already |

**Publishers must be able to declare depth.** `EntityInventory::from_model`
hardcodes `depth: None` for every publisher row, so a contract stating
`pub: { qos: { depth: 8 } }` cannot reach the build. That structurally blocks
transient-local pricing and is a prerequisite, not a nicety.

### The declaration surface reaches the DESCRIPTOR (phase-454 W12)

Landed. W11 measured that D3 was open in both directions at once — *"the road
that carries contracts writes no descriptor, and the road that writes
descriptors reads no contract"* — because the leaf producer filled its rows from
the leaf's `metadata/` PROBE. A probe knows an endpoint exists and nothing about
its QoS, so every row came out with the callback name in the `topic` column, no
`depth`, and `undeclared_endpoints` non-zero, which is the guard that switches
every per-endpoint consumer off.

**Two things had to exist, and only one of them was a join.**

A single-package leaf had nowhere to AUTHOR a contract. The resolver finds one
through the provider-sidecar channel, `<launch-file-dir>/<stem>.contract.yaml`,
and a leaf's launch file is SYNTHESISED by `nros sync` from its `[[component]]`
rows into a generated directory (RFC-0098 D3) — which is not a place a user can
put a file. So a leaf states its contract at **`<leaf>/system.contract.yaml`**,
beside the `system.toml` that states everything else about its deployment, and
sync carries it to where the resolver looks. A leaf with its own `launch/`
directory is unaffected: it authors the sidecar beside its launch file, like any
bringup.

**THE KEY, and the hazard it answers.** The two inventories key their rows
differently, and a wrong match publishes a depth against the wrong endpoint —
which is an UNDER-size, the direction that ships `BufferTooSmall`:

| side | row identity |
| --- | --- |
| probe | `(kind, type, id)`, and `id` is the CALLBACK name for a subscription |
| contract | `(kind, type, RESOLVED name)` |

There is exactly one key both sides can state: **the endpoint's name**. The
probe records it separately from `id` (`unresolved_topic` / `unresolved_name`,
now carried as `EntityDecl::source_topic`), AS THE SOURCE WRITES IT — which is
the resolved name only when nothing intervenes. Two things can:

* the node's **namespace**, which turns `chatter` into `/ns/chatter`;
* the launch **remappings**, which can turn any name into any other, and which
  the model records per NODE without saying which endpoint each one renamed.

So `nros_cli_core::contract_join` attributes a row in exactly one case — the
node declares no remaps, the written name is absolute, and `(kind, type, name)`
picks out ONE row **on each side** — and REFUSES in every other, naming the row
and what the contract does describe. The uniqueness is checked on both sides
because two registrations against one declaration is an under-description:
giving the declaration to whichever the probe listed first publishes a depth for
a registration nobody made.

A refusal is per row and per fact (D6). All four QoS policies refuse together,
because they come from one attribution and half an attribution describes no
image; the payload class beside them survives, because it is a property of the
TYPE. The row keeps counting toward `undeclared_endpoints`, so every consumer
keeps its worst case — measured end to end: a mis-keyed contract takes
`examples/native/rust/listener` back to 273,802 B with `LARGE_PAYLOADS` at
131,072, and `nros-rmw-zenoh`'s build prints the refusal verbatim.

**No contract, no change.** `from_model` returns `None` when the model describes
no wiring and the join then hands the probe's rows back untouched — measured
against `origin/main`'s CLI on the same leaf: the descriptor, the generated
`nros-cargo.toml` and the linked binary are byte-identical (`adab93c6…`, after a
forced recompile and relink). "Nobody said" (`Fact::Absent`) and "I looked and
could not tell" (`Fact::Refused`) stay different statements.

**Measured, on the ordinary flow.** `examples/native/rust/listener` — `nros
sync`, then the retypable `cargo build --config build/native/nros-cargo.toml`,
nothing exported by hand — with one contract row declaring `KEEP_LAST(1)`:

| | as `sync` wrote it before W12 | with the contract joined | delta |
| --- | --- | --- | --- |
| `.bss + .data` | 273,802 | **172,298** | **−101,504 (−37.1 %)** |
| `LARGE_PAYLOADS` | 131,072 | **32,768** | −98,304 |
| `SMALL_PAYLOADS` | 4,096 | 1,024 | −3,072 |
| `SUBSCRIBER_BUFFERS` | 312 | 168 | −144 |

which is W6.a's own saving, reached for the first time by a shipping image. The
running code did not change: the pair still delivers 11 of 11 messages against
`rmw_zenohd`, and the ring `shim/qos.rs` clamps every stock preset to was
already shorter than `QOS_PROFILE_DEFAULT`'s KEEP_LAST(10) before this wave.

## D4 — one descriptor, read by path

**Landed, phase-454 W4.** `nros sync` writes one file per entry; consumers read
it by path, never by environment. Env transport is what produced issues 0460 (a
knob reaching the Zephyr C lane and not the Rust one) and 0491 (a path variable
compared as text), and it cannot carry per-endpoint structure without encoding it
in a string.

The schema as SHIPPED — `packages/tooling/nros-sizing-descriptor` is the one
reader; do not hand-parse this anywhere:

```toml
# build/nros/sizing/<entry>.toml
schema_version = 1

[meta]
entry  = "talker"
status = "derived" | "partial" | "refused"
basis  = "contract" | "closure"
undeclared_endpoints = 0

[target]
pointer_bytes = 4
max_align = 8
heap_budget_bytes = 65536

[[endpoint]]
kind = "subscription"          # publisher | subscription | service_{server,client}
                               # | action_{server,client}
type = "std_msgs/msg/String"
topic = "/chatter"
history = "keep_last"
depth = 10
reliability = "reliable"
durability = "volatile"
registration_path = "unbounded"  # phase-456 W8: three tags, none named for a caller
storage_bytes = 12914
wire_bound_bytes = 1170

# A second endpoint, showing how a field with no number travels.
[[endpoint]]
kind = "subscription"
type = "sensor_msgs/msg/Image"
topic = "/image"
history = "keep_all"
wire_bound_bytes = 4096

[endpoint.refused]
depth = "history = keep_all on subscription /image: a KEEP_ALL queue has no static bound (RFC-0100 D6)"
storage_bytes = "`depth` is refused (history = keep_all), and a receive region is sized from it"

[image]
node_count = 2
backend_count = 1
subscriber_count = 3

# All four STATED since phase-454 W6.c. The three maxima come from codegen's own
# per-type schema walk (the one that prices the bounds), taken per column: the
# widest type and the deepest type need not be the same type. A type whose schema
# codegen could not build refuses all three and NAMES itself -- a maximum over a
# subset is a smaller number that reads exactly like the right one.
[types]
distinct_count = 7
max_fields = 14
max_kinds = 63
max_nested_depth = 4

[policy]
graph_max_entities = 64
transport_mtu = 4096
sessions = 1
```

**Per-field status is spelled as a `refused` sub-table beside each section's
values** (D6). A fully derived image's descriptor therefore reads exactly like
the sketch above it and costs no ceremony, while a refusal carries its prose to
the consumer that needs it rather than to a build log nobody kept. Three rules
are enforced at PARSE, and each is a shape that would otherwise read as derived:

* a key in BOTH the value slot and `refused` is a contradiction and an error;
* a `refused` key naming a field the section does not have is an error — a
  refusal nobody can read is worse than none, because the consumer defaults
  silently while the producer believes it warned;
* an unknown key, an unknown vocabulary spelling, and a `schema_version` this
  reader does not know are all errors. Never a best effort.

The reader's API has three states, not two: `Fact::Stated` / `Refused(reason)` /
`Absent`. `Absent` is "nobody said" — `EntityDecl::depth`'s own rule, *"`None`
means NOBODY SAID … It must never read as 0"* — and `Refused` is "I looked and
there is no number, here is why". `Fact::stated()` is the only accessor that
yields a value, so there is no spelling of "read it, and if that fails use 10"
that does not go through a `match`.

**`[image]` carries the counts no endpoint row can** (phase-454 W6.d/W6.e). Most
of D1's "entity counts by kind" are read off the rows — a consumer wanting
subscriptions counts `kind = "subscription"`. Three are not there to be counted:
the NODE count, the BACKEND count, and the session's SUBSCRIBER count, which is
declared subscriptions plus the feedback subscription each action client opens.
The third is a fact rather than a row count on purpose: the multiplier that turns
one declared action into several session slots lives beside the calls that make
it (`check-infra-queryable-counts` holds it there), and a build script counting
rows and multiplying would be a third mirror of it in a file no gate scans.

**Three fields the sketch above did not have, and one it did.**
`schema_version` and `[meta] entry` are new: the first because a reader that kept
going on a version it does not know sizes from numbers whose meaning has moved
(the rule that took the entity inventory to 5), the second so a copied file still
says what it is about. `registration_path` replaces the sketch's `buffer`, which
D9 rules out as a `SLOTS` source anyway; it is REQUIRED, and D1 says why.

A cargo consumer gets a real `rerun-if-changed` edge on this path —
`nros_sizing_descriptor::load_for_build_script` emits it, on the file's CONTENT
and never on the variable that names it (issue 0491), and **on the path whether
or not a file is there yet**, because CREATION is the edge that matters: the
first `nros sync` is what turns an image's defaults into its declaration.
Measured on cargo 1.98.1 — a watch emitted only for a file that already existed
saw neither the creation nor a later edit (phase-454 W11 carries the table). A
cmake consumer reads it through `nros_sizing_descriptor_read()` in
`cmake/NanoRosSizingDescriptor.cmake`, which registers BOTH the descriptor and
the CLI in `CMAKE_CONFIGURE_DEPENDS` (issue 1018's rule — `execute_process()` has
already run by the time ninja decides anything, so that list is the only thing
that makes the emitted fragment fresh). The descriptor is registered even when it
does not exist yet, so the first `nros sync` after a configure is what re-triggers
one; the cargo side answers the same way for the same reason.

**A consumer reading it is not a road delivering it.** `from_build_env()` answers
`Ok(None)` unless something names a descriptor to that build. Through
2026-09-13 exactly one road did: `cmd::leaf_settings::write` on a
single-package cargo leaf, as a `relative = true` `[env]` row (the path is
relative because the descriptor lives INSIDE the leaf and a package must stay
self-contained — proved by copy-out, not by reading; re-proved in W12, where a
copied-out leaf syncs and builds to the same 172,298 bytes with zero absolute
paths in the file).

### TWO producers, one composer (phase-454 W14)

*(Heading kept as landed. phase-457 W0.b added a third producer, `--from-leaf`;
Amendment 1 restates the set by INPUT and adds the multi-entry runtime, D12.)*

**Landed.** The other two roads — a workspace cargo image, and every cmake /
Zephyr west / NuttX entry — now write one too. They have ONE input where the
leaf has three: the resolved SystemModel. So the decision W11 left open —
*"what a descriptor written from a model alone may CLAIM"* — is settled:

> **Emit what the SystemModel knows. REFUSE every field you cannot source. Do
> not invent a number, and do not fall back to one.**

| field | model-only road | why |
| --- | --- | --- |
| entity counts, per-endpoint QoS (all four), topics, types | **Stated** | `EntityInventory::from_model` already resolves them |
| `wire_bound_bytes` | **Refused** | needs the bound inventory, which codegen writes beside a LEAF |
| `storage_bytes` | **Refused** | that bound, plus the board descriptor resolved for THIS image |
| `[types]` `max_fields` / `max_kinds` / `max_nested_depth` | **Refused** | needs codegen's own per-type schema walk |
| `registration_path` | **Per row** (phase-457 W3) | the `in_place` row needs only the BACKEND (a function of `rmw`, which this road has) and the endpoint's OWN observed answer, so a row the probe saw registering is STATED here. The two buffered rows still need the entry's language, which a model image of SEVERAL PACKAGES has no one answer for, and are refused. A row nothing observed is refused naming [issue 1522](../issues/archived/1522-registration-path-unobserved-on-roads-whose-probe-does-not-register.md) — the closure buffer is 1,848 bytes per subscription over the type's bound (issue 1319) and the in-place row claims NO region at all (issue 1340), so a guess is a failure in both directions |

Every refusal names [issue 1393](../issues/archived/1393-cmake-road-has-no-bound-inventory.md)
— except `registration_path`'s, which names
[issue 1522](../issues/archived/1522-registration-path-unobserved-on-roads-whose-probe-does-not-register.md)
because the input it lacks is a different KIND of thing: 1393's remedy is an
artifact some road can produce, while a registration spelling can only be
reported by a probe that actually registers. So the artifact itself says what is
missing and why, and the day either closes, the refusals in a written descriptor
are the checklist.

**Amended 2026-10-01 — issue 1393 is closed, and the table above is history.**
The model roads now receive the inputs the "Refused" rows named:
phase-457-payload W2 hands every producer the bound tables its closure
REGISTERED (so `wire_bound_bytes` and `[types]` are stated), phase-457 W4 the
board's triple, and `storage_bytes` runs the leaf road's own chain on every road.
A descriptor-carrying backend states `registration_path = "typed_bound"` without
the language, because every language arm gave it. So the rule is now one
sentence for all three producers — **compose with the same code, refuse per field
on the input that is actually missing, and name THAT input** — and a model road
and a leaf road handed the same inputs are asserted to AGREE field by field. What
still refuses on a model road is narrower than the road: a closure that
registered no table (said so), and a subscription on an in-place backend that no
CURRENT probe sidecar could be attributed to. The model road joins the
workspace's sidecars onto its rows by the contract join's own rule — no remaps on
the node, an absolute written name, a key unique on both sides — so the
observation the leaf road reads reaches it too
([issue 1594](../issues/archived/1594-model-road-subscription-rows-carry-no-registration-observation.md)).

**This is D6 doing the work it exists for.** A partial descriptor is safe to
publish precisely because `Fact::stated()` is the only accessor that yields a
value: a consumer cannot read one of those refusals as a default, so its
fallback is the literal it already had — always the safe direction and always
loud. Measured across all seven consumers (each built twice, knobs diffed):
**not one output difference is caused by a refusal.** Every difference is
attributable to a fact the contract STATED — `MAX_BACKENDS` 8→1 and
`SUBSCRIBER_SLOTS` 8→2 from the counts, `SUBSCRIBER_RING_DEPTH` 4→1 from the
declared depth, `XRCE_STREAM_HISTORY` 16→4 from a declared `best_effort`,
`arena_size` 26,800→10,240 from a declared `KEEP_LAST(1)`. The refusals cost a
`cargo::warning` and nothing else.

**No contract, no file.** `EntityInventory::from_model` returns `None` for a
model that describes no wiring — 109 of 114 resolvable models — and the producer
is not reached for one. An all-refused descriptor would move the `[meta] basis`
every consumer guards on in order to say nothing, which is W12's own control
held on a second road.

**One composer, two callers.** `write_for_leaf` and `write_for_model` both go
through `sizing_descriptor::build`, differing by a `ModelHorizon` and nothing
else; the counts and all four QoS policies come out identical for one image on
either road (asserted). A second composer is how two producers of one schema
come to disagree.

**What that one road CARRIES is a separate question from what it delivers, and
W12 is the answer to it.** Through W11 the live road delivered the probe's rows:
a descriptor that reached every consumer correctly and stated nothing any of
them could size from. See D3's "The declaration surface reaches the DESCRIPTOR"
above.

**The descriptor carries no absolute path** — issue 0320's rule. Two checkouts of
one tree at different paths render byte-identical bytes, which is what keeps
every freshness comparison against it honest;
`sizing_descriptor_portable.rs` measures it and `render` carries a tripwire for
the way it would actually go wrong (an interpolated `Path::display()` in a
refusal reason).

**One descriptor collapses the two roads, which retires a gate by construction.**
`check-declared-fact-carriers.py` exists to keep the cmake road and the cargo
road delivering the same knobs, and maintains a hand-written `ROAD_PAIRS` map —
*"the one place that pairing is written down"*. With one artifact there is one
road, so the pairing has nothing to drift and the map has nothing to hold. Around
35 `NROS_DECLARED_*` / `NROS_DERIVED_*` carriers across nine producers go with
it (D3's retirement, sequenced in the home phase).

## D5 — the backend owns its formula, in its own build

Each backend's build reads the descriptor and computes its own knobs. Adding a
fifth backend touches no shared code, and each backend's arithmetic stays where a
maintainer of that backend reads it.

| consumer | reads | computes | status today |
| --- | --- | --- | --- |
| executor | counts; per-endpoint `depth`+`history`; rx class; `[target]` | `MAX_CBS`, `ARENA_SIZE`, `BACKING_U64S` | partly derived; rx class is the dead knob above |
| zenoh | counts; `depth`; small/large bounds; service req/resp bounds; `[policy]` | `ZPICO_MAX_*`, `SUBSCRIBER_RING_DEPTH`, payload pools, `SERVICE_BUFFERS` | counts and payload classes derived — the three subscriber classes computed in `nros-rmw-zenoh`'s own build from the subscription rows' `wire_bound_bytes` (`subscriber_payload_classes`, issue 1595), the `NROS_DECLARED_*` carrier only where no descriptor is named; ring depth DERIVED from the declared depths (phase-454 W6.a, −124,032 B measured); `SERVICE_BUFFERS`'s slot size takes the declared service bound as its default and carries a stated non-annotation — but **no service or action type has a bound row to read** (`record_message` runs for `.msg` only), so it refuses on every image today |
| XRCE | counts (**zero legal**); per-family bounds; `depth`; MTU; `reliability` | `MAX_*`, per-family `BUFFER_SIZE`, ring depths, `STREAM_HISTORY` | phase-454 W6.b: `BUFFER_SIZE` split into three family knobs; `reliability` gates `STREAM_HISTORY` down to its protocol floor; the SUBSCRIBER family's bound and ring depth derive together (separately they can GROW the pool — measured). The two SERVICE families are split and not derived: the parameter and lifecycle server families are not `[[endpoint]]` rows |
| Cyclone | `[types]`, `[target].heap_budget_bytes`. **Nothing else** | `MAX_TYPES`, `MAX_DESCRIPTOR_TYPES`, `MAX_FIELDS`, `MAX_KINDS`, heap assertion | **all derived, phase-454 W6.c** |
| uORB | distinct topic count; subscription count | `REGISTRY_CAPACITY`, `PX4_MAX_CALLBACKS` | **both derived, phase-454 W6.d** |
| cffi | subscription count; node count; backend count | `RMW_SUBSCRIBER_SLOTS`, `MAX_NODES`, `MAX_BACKENDS` | **all three derived, phase-454 W6.e** |

**Not a derivation, and must not be modelled as one:** the cffi subscriber pool's
slot WIDTH. No user fact answers "how big is a backend's private state struct",
so it is not a sizing input — and `size_of::<T>()` is a compile-time quantity, so
it wants a `const` assertion rather than a runtime return.

**Settled, issue 1322 (archived).** It was a hard `1024`, and `insert::<T>()`
returned `None` both when every slot was claimed and when `size_of::<T>()`
exceeded it, with the caller mapping both to `NROS_RMW_RET_BAD_ALLOC` — one cause
answerable by `NROS_RMW_SUBSCRIBER_SLOTS` and one answerable by no knob at all,
so against half the failures the only lever the message named was the wrong one,
applied in the expensive direction. Two quantities had one lever; they are two
knobs now:

* the COUNT stays DERIVED (`[image] subscriber_count`, the row above), and
  `None` from `insert` now means exhaustion and nothing else, said out loud with
  that knob named and its per-slot cost;
* the WIDTH is `NROS_RMW_SUBSCRIBER_SLOT_BYTES`, AUTHORED over the RFC-0049
  ladder (env → `$DOTCONFIG` → `[knobs.rmw] subscriber_slot_bytes` → builtin
  1024). Its FLOOR is D7's shape and lives at the pool, as a `const` block in
  `insert::<T>()` — the only place that can see `T`. A short value is a BUILD
  error naming the type and both numbers; measured on `thumbv7m-none-eabi`,
  `ZenohSubscriber` is 112 bytes, `64` fails and `112` links.

The two halves therefore resolve by different mechanisms on purpose, which is the
point of separating them rather than raising the literal.

## D6 — refusal is per-fact, never global

Today refusal is coarse: all five `NROS_ENTITY_COUNT_*` or the whole arena falls
to the undeclared branch. That is wrong for a shared descriptor — a `keep_all`
subscription says nothing about Cyclone's type table, and a global refusal would
degrade it anyway. **Each derived field carries its own status.**

| trigger | refuses | why |
| --- | --- | --- |
| `history = keep_all` | depth-derived fields | no static bound exists |
| `undeclared_endpoints != 0` | fields needing per-endpoint attribution | absence is not zero |
| type unbounded (`mode = view`/`heap`) | payload-class fields | `cap_bounds_the_wire()` is false; there is no number |
| type not priced | payload-class fields, **naming the type** | existing `NanoRosMessageBounds` behaviour |
| `[target]` absent | storage-size fields | must not infer from the host |

What a refusal never does:

* **Never silently widens the basis.** A refused `subscribed` basis does not fall
  back to `closure` — that publishes the wrong row while every status still reads
  "derived", which is the shape that looks like it worked.
* **Never floors** (D7).
* **Never degrades another consumer's facts.**

Worst case when refused, always the safe direction and always **loud** — the
build prints what declaring would save:

| consumer | falls back to |
| --- | --- |
| executor | `PUBSUB_QOS_DEPTH = 10` × closure bound (today's number; every existing image builds unchanged) |
| zenoh | ROS default depth, closure bound, `UNDECLARED_HEADROOM` queryables |
| XRCE | **assume reliable** — pay both 64 KiB buffers |
| Cyclone | `DEFAULT_MAX_TYPES = 32` floor |
| uORB | current literals — the `#ifndef … 64` in each source, so an image with no descriptor is byte-identical to every build before W6.d |

## D7 — derivation publishes demand, unfloored; the floor lives at the consumer

Carried unchanged from issues 1015 and 1033, which are the same derivation
feeding two consumers with opposite right answers: `ZPICO_MAX_QUERYABLES = 0`
left a board transmitting nothing for 15 s with no diagnostic, while
`XRCE_MAX_SUBSCRIBERS = 0` is worth 33,296 bytes of heap a slot and is the
answer. 1015's floor landed in the shared derivation a day before 1033's fix and
silently defeated it, with every knob gate green because the number was derived
correctly and delivered faithfully.

Zero is a legitimate demand. Whether zero is a legal SIZE is a property of the
storage, so it is decided at the pool.

## D8 — the contract and `qos_overrides` must agree, exactly

`qos_overrides.<topic>.<role>.<policy>` is ROS 2's own mechanism for overriding
QoS declared in code, delivered as ordinary node parameters — so it reaches a
build from launch XML, a params YAML, or `system.toml [[component]] params`.

Today it feeds the baked **runtime** table (all eight policies) and the contract
feeds **sizing** (depth), and the two never meet. So
`qos_overrides./t.subscription.depth=64` is baked into the runtime and the arena
never hears about it — issue 1190's `BufferTooSmall` with a config-shaped cause
and no diagnostic.

**Ruling: they are two statements about one fact, so any divergence is a build
error naming both sites.** Not a bound check — narrowing is still a
disagreement, and honouring it silently means the image runs QoS the contract
does not describe. Where an override states a policy the contract omits, the
error names the contract line to add; the contract stays the single authoritative
producer.

This extends the rule that module already holds itself to:

> *"**Errors are values, not silence.** Before this module both producers
> `filter_map`ed an unrecognised role or policy away … the image ran different
> delivery semantics than the model declared."*

### As implemented (phase-454 W7)

`nros_orchestration_ir::qos_agreement::check_model` is the rule, one function
over a resolved `SystemModel`, called from all four roads a model reaches a bake
or a sizing derivation on — the two CLI sizing roads W3 already guards
(`nros ws entity-inventory`, fatal; `nros build`'s seed, a refusal reason) and
the two BAKE roads (`codegen::entry::plan_from_model`, `nros::main!`). Only the
bake road is guaranteed to see an override, and only the sizing road is
guaranteed to see the contract, so a check on either alone is issue 1199's shape.
It lives in `nros-orchestration-ir` because the proc-macro cannot depend on
`nros-cli-core`.

Three refinements the ruling did not have to state, each measured:

* **Scope is CAPACITY.** `qos_override::MODELLED_POLICIES` flags each of the
  eight policies `lower` accepts as capacity or occupancy, and only the four
  capacity ones are compared: `reliability`, `durability`, `history`, `depth` —
  exactly the four the contract's `qos:` block can state. `deadline`, `lifespan`
  and the two liveliness policies bound occupancy or liveness of a queue whose
  size is already decided; the schema has no key for them, so comparing them
  would refuse every legal `deadline` override.
* **It abstains where no contract was authored.** No topic wiring means no
  contract statement about any topic, hence ONE producer and nothing to diverge
  from — the same abstention `EntityInventory::from_model` makes by returning
  `None`. 109 of 114 resolvable models are in that state, including both in-tree
  producers of a `qos_overrides.*` parameter, which is why the wave moves no
  sizing number.
* **One vocabulary, structurally.** The comparison reads a parameter with
  `qos_override::sizing_statement`, which shares the key parser and the value
  functions with `lower`; a test asserts the two accept the same policy names,
  read one value, and refuse the same keys. A second `match policy` would be the
  drift that module's own header was written about.

## D9 — `buffer:` is a cross-check, not a `SLOTS` source

`buffer: latest | queue` is fault-analysis vocabulary, not queue sizing:

> *"`latest` (default) — staleness is the failure mode; `queue` — backlog is the
> failure mode, drained batch-wise by the consuming timer. Only meaningful
> alongside `state: true`; parse-time error otherwise."*

It is not deterministic in either direction — `queue` states no depth, `latest`
does not forbid depth > 1 — so it must not feed `SLOTS`. It earns two cheap
diagnostics instead: `queue` with `depth = 1` declares backlog as the failure
mode and then sizes for none; `latest` with a large depth pays for history the
author declared they do not read. Neither is an error alone.

But it selects a derivation. `queue` is drained by the consuming timer, and the
contract already carries both rates (`topics.<t>.rate_hz`,
`paths.<p>.trigger.timer.rate_hz`):

```
depth_default(queue endpoint) = ceil(pub_rate_hz / drain_rate_hz) + margin
```

A derived default under the ladder — a stated `depth` still wins — for exactly
the endpoints where a hand-typed depth is most likely wrong, using keys already
in the schema.

### The margin is ONE slot, and the argument is the phase relationship

`ceil(publish / drain)` is the number of samples emitted during one drain
period, and it is the right answer only for a queue whose drain instants are
ALIGNED with its arrivals. They are not: a `queue` subscription and the timer
that drains it are two independent periodic streams with no phase relationship
the contract states, and for an unaligned window of length `T` the arrivals of a
`p`-periodic stream are bounded by `floor(p·T) + 1`. One slot is exactly that
straggler — the sample that landed just after a drain instant and is still
queued when the next period's own samples arrive.

Not two, and not a percentage. Every extra slot is a whole message: the arena
charges `(depth + 1) * bound + (depth + 1) * pointer` per subscription, so
inflating a DEFAULT is the over-size direction the table above keeps measuring.
And the things a larger margin would absorb — timer jitter, drain overrun, a
burst above the declared rate — are bounded by no number that reaches the
derivation, so a margin covering them would be a guess wearing arithmetic's
clothes, which is precisely what this decision refuses to let `buffer:` itself
be. The contract does carry `paths.<p>.max_jitter` and `paths.<p>.miss`; when
those travel, a jitter-aware margin can be DERIVED and the constant retired.

### MEASURED: all three inputs reach the build (issue 1339, CLOSED)

phase-454 W8 implemented this and measured what a contract actually delivers,
resolving `packages/cli/nros-cli-core/tests/fixtures/queue_buffer/` through the
pinned `nros-launch-resolve`. The retraction above was about what `buffer:`
MEANS; this is about where it GOES, and for two waves it was the more immediate
constraint:

| fact | contract key | in the SystemModel? |
| --- | --- | --- |
| publish rate | `topics.<t>.rate_hz` | yes — `TopicContract::rate_hz` |
| publish rate | `<n>.pub.<ep>.min_rate_hz` | yes — `PubContract::min_rate_hz` |
| drain rate | `<n>.paths.<p>.trigger.timer.rate_hz` | yes — `PathContract::trigger`, since rlm v0.1.37 |
| discipline | `<n>.sub.<ep>.buffer` | yes — `SubContract::buffer`, since rlm v0.1.37 |

The two lower rows were **no** when W8 landed, and both gaps were the MODEL
SCHEMA's rather than nano-ros's: the resolver parsed both, validated `buffer`
(outside `state: true` it is a parse-time error), and performed this decision's
own division to emit a `[queue-drain-rate]` warning — then wrote a
`sub_endpoints` entry with no discipline and a `node_paths` entry carrying
`output` alone. So the derivation shipped ARMED and inert. rlm v0.1.37 (design
issue #52) added `SubContract::buffer` and `PathContract::trigger`, the resolver
lowers both, and issue 1339's consumer half reads them: a contract stating all
three now derives a depth end to end
(`contract_queue_buffer_reaches_the_model.rs`).

Three consequences this RFC should be read with.

First, **the drain rate is the AUTHORED one now, not a substitute.** It used to
be the `min_rate_hz` of what a node's timer paths publish — the convention
`mapper_input::pub_rate_hz` used — which is absent for a drain timer that
publishes nothing, and which the resolver's own `[derivable-min-rate]` advice is
to delete. That reader is retired: a path with no `trigger` is `Unclassified`,
yields no drain rate, and reports `NoDefault::NoDrainRate` rather than a depth
derived from a number nobody stated.

Second, **the inertness is now a property of the CONTRACTS, not of the schema.**
No image in this tree writes `buffer:`, so no image's sizing moves — but that is
a measurement to re-run (`no_shipping_contract_derives_a_depth`) rather than
something the model boundary guarantees. The first contract that states
`buffer: queue` will change bytes, deliberately.

Third, a derived depth is not interchangeable with a stated one: `DeclaredDepth`
carries a `DepthSource`, the arena reads both and the compile-time
`NROS_ASSERT_DECLARED_DEPTH` table reads only `Stated`. A default that became a
`static_assert` would oblige every call site to spell this CLI's arithmetic, and
moving the margin by one slot would break every image at once — the ladder
inverted.

## D10 — single-source the ROS QoS defaults first

ROS defaults are literals in four places (`nros-node/build.rs:27`, the
`qos_profiles!` table, `nros-c/src/qos.rs`, the cffi layer, plus
`CONFIG_NROS_PUBSUB_QOS_DEPTH`), held equal only by gates against
`docs/reference/rmw-qos-profiles.txt`. Three more defaulted policies multiply
that drift surface. **The defaults move to one source before any new policy
reaches a build** — a sequencing constraint, not a preference.

Related and in scope: the declared-QoS check is C++-only
(`_nros_declared_qos_arm` returns early for Rust and INTERFACE targets; no C
equivalent of `NROS_ASSERT_DECLARED_DEPTH`). The descriptor is language-neutral,
so this is where that closes.

**Closed in three steps.** phase-454 W10/W13 gave C a compile-time check and
Rust a registration one, for the DEPTH. Issue 1256 widened all three languages
to the two POLICIES a contract can state for a subscription: the generated
table carries `reliability` and `durability` columns (as tokens, because C and
C++ number those enums differently), `NROS_ASSERT_DECLARED_QOS` /
`NROS_ASSERT_DECLARED_{RELIABILITY,DURABILITY}` fail the build on a
disagreement, `Node::check_declared_qos` refuses it at boot, and
`nros_node::declared_qos::{check,honour}` at registration — `honour` TAKING a
weaker declared value and REFUSING a stronger one, the depth's rule with the
order of the two values standing in for "deeper". The policies reach Rust on the
sizing descriptor only; no env knob carries them. Issue 1564 then removed the
reason nothing could adopt it: a configure with several entries used to ABSTAIN
from rendering the table, and it now renders the union of every model's table
per component, refusing (loudly) only where two models state different values
for one endpoint.

## D11 — Cyclone gets a heap budget and asserts it at boot

**Landed, phase-454 W6.c.**

Cyclone sizes nothing statically and has no heap-budget knob of its own; the
nearest thing is the platform's (`NROS_ZEPHYR_HEAP_SIZE`,
`NROS_FREERTOS_HEAP_KB`) and a baked XML `<Sizing>` block of hard-coded
constants. The model derives a required-heap number from the same facts, and the
image asserts its configured heap meets it at boot. That makes Cyclone a
first-class consumer without inventing a static pool it does not have.

**Which of the two numbers `[target].heap_budget_bytes` carries — a correction.**
An earlier draft of this decision said the derived REQUIREMENT is emitted into
that field. It is not, and cannot be: the schema as shipped (D4) defines
`heap_budget_bytes` as *"the heap this image is configured with"*, W4 populates
it from the board's `[board.knobs.memory] heap_bytes`, and an assertion needs
both numbers. So the descriptor carries the CONFIGURED heap — the thing only the
board knows — and the BACKEND derives its own requirement from `[types]`, in its
own build, which is what D5 says every consumer does. Nothing else would have a
second number to compare against.

**The requirement is a FLOOR, and that is what makes it safe to act on.** It is
bytes the image is certain to need before it publishes anything, not an
accounting of Cyclone's heap use — which depends on the graph it discovers, the
samples in flight and the peers it meets, none of which a build can know. Three
terms, each chosen so a real image needs at least this much:

* the `<Sizing>` receive buffers, which `cyclone_config.hpp` bakes as literals;
* one `dds_topic_descriptor_t` and one mangled type name per REGISTERED TYPE —
  `dynamic_type_builder.cpp` allocates exactly three blocks per type and frees
  none of them, because the registry memoises for the process lifetime;
* the ops array of the LARGEST type, once, which is where `[types].max_kinds`
  becomes load-bearing rather than decorative.

Being a floor is what lets the check FAIL a boot: it can report a heap that is
too small and it can never refuse an image that would have worked. A ceiling
would have the opposite property, and the worse one. The failure it replaces is
not a quiet one to debug — Cyclone exhausting the ddsrt heap inside entity
creation has `ddsrt_mutex_init` swallowing the ENOMEM and the image dying as an
anonymous `abort()` twenty seconds later somewhere else (issues 0371 / 0496).

A board that states no heap gets no judgement: `kHeapBudgetStated` is the third
state, and "nobody said" is not "too small" (D6).

## What this does not change

* RFC-0049's ladder. Precedence is unchanged; this RFC only adds a rung's worth
  of derived defaults and says where they come from.
* The pool-inventory annotations. `// nros-pool:` stays a product-of-knobs
  grammar, and the three documented deliberate non-annotations keep their
  reasons — *"the size is known to the compiler, so read it from the compiler's
  output."* One gap is newly visible: zenoh's `SERVICE_BUFFERS` is a pure product
  of two knobs with neither an annotation nor a stated reason.
* Runtime behaviour. Every number here is a build-time size.

## Measured stakes

| gap | bytes | direction | cause |
| --- | --- | --- | --- |
| Rust typed on a schemaless backend | 1,848 per subscription | **UNDER** (issue 1319) | the registration path decides the slot size and the build cannot see it |
| XRCE reliable streams | ~131,072 per session, **98,304 of it recoverable** | over | reliability declared but unread. Recovered by phase-454 W6.b, measured: the two buffers drop to `SLOTS = 4` on a best-effort-only image. The residue is not a leak — both streams carry session CONTROL (every entity CREATE/DELETE, and the `uxr_buffer_request_data` that names the input stream for every reader whatever its QoS), so the `SLOTS` term shrinks to the protocol floor and the pool does not vanish |
| zenoh `SERVICE_BUFFERS` | 144,128 on a native talker | over | not derived, not in the inventory |
| island arena, depth 10 vs declared | 135,432 | over | 207,096 against 71,664 with the eleven `QoS(1)` declarations the code already makes |

The first row is the one that matters most, and it is the reason this RFC leads
with facts rather than savings: three of these are money left on the table and
one ships `BufferTooSmall`. A model that only pursued the savings would have
made it worse.

The last row is still the model's clearest argument: the running code did not
change. Only whether the build was told what it registers.

## Amendment 1 (2026-10-03) — the unified build path moved under this model

D4 was written when "a road" meant a hand-written entry and the build tool it
happened to use. Since then RFC-0065 made `nros build` the one front door
(five stages, the driver chosen by the board), phase-470 generated every
workspace entry but three, and phase-474 made one lowering (`LoweredEntry`) the
context every entry pack renders. None of that changed what a descriptor
STATES. It changed what D4 assumed about where one is CONSUMED, and the model
has to say so rather than carry the old premise in its phases.

### What the build path made true

**The cmake road builds ONE runtime for every image of a coordinate.** RFC-0065
D8: *"One configure per coordinate"* (platform, rmw, feature-sig). Every
`[image.*]` row of a bringup that resolves to the same coordinate is an entry of
the same `build/<coord>/cmake` configure, and they link ONE runtime staticlib
(`nros-cpp` / `nros-c` plus the backends, imported through Corrosion). The cargo
road does not do this: `build/<coord>/<image>_entry/` is each image's own cargo
root with its own target directory, so a cargo image always has a runtime of its
own. So the ratio of entries to runtime builds is a property of the DRIVER:

| road | entries per runtime build | descriptor named to the runtime's cargo |
| --- | --- | --- |
| cargo leaf | 1 | yes (`cmd::leaf_settings`) |
| cargo workspace image | 1 | yes (`cmd::build`, stage 4) |
| cmake standalone leaf | 1 | yes (`--from-leaf`, phase-457 W0.b) |
| west entry through `nano_ros_entry()` | 1 | yes (issue 1407, PR #1601) |
| generated Rust west application (`rust_cargo_application()`, phase-470 W5.a) | 1 | not measured here |
| **cmake workspace configure** | **N — every image of the coordinate** | **no** — `nros_sizing_descriptor_cargo_env()` names one of N or none, and says none (issue 1649) |

phase-457 W0.c kept that refusal on the measured premise that "no configure in
this tree declares more than one entry". The premise was true of hand-written
entries and is false of generated ones: `examples/workspaces/cpp`'s native
configure holds five entries and writes five descriptors (issue 1649, measured
2026-10-03), and [issue 1600](../issues/archived/1600-multi-entry-configure-sizes-runtime-from-last-entry.md)
had already met the same configure from the entity side. It is not an edge case
to revisit when it occurs; it is the cmake road's normal shape for any bringup
with more than one image per coordinate.

### D12 — the unit a descriptor sizes is the RUNTIME BUILD, not the entry

> **Every runtime build is named exactly one descriptor. Where entries and
> runtime builds are 1:1, it is the entry's descriptor, unchanged. Where they are
> N:1, it is the RUNTIME's descriptor: the same composer
> (`sizing_descriptor::build`) over the reduction issue 1600 already defined for
> the entity fragment.**

"Exactly one or none" becomes "exactly one". The per-entry descriptors are still
written — they are what `nros ws sizing-descriptor --descriptor` inspects and
what a per-entry reader would read — but in an N:1 configure none of them is
named to cargo.

Four rules, each chosen so the runtime descriptor cannot disagree with what the
configure already delivers:

1. **One reduction, three outputs.** The entity fragment (issue 1600,
   `EntityInventory::shared_runtime_over` over `NROS_ENTITY_INVENTORY_MODELS`),
   the declared-QoS header table
   ([issue 1564](../issues/archived/1564-declared-depth-check-has-no-adopter.md),
   the union per component) and the runtime descriptor fold the SAME model list
   by the SAME rule: a component any entry launches is counted once, at
   `merged_per_kind_max` of its declarations. Rows and `[image]` counts therefore
   agree by construction. A second reduction written for the descriptor would be
   issue 1025's shape — one formula, its inputs derived twice.
2. **A per-endpoint fact on which the entries AGREE is stated; one on which they
   DISAGREE is REFUSED, naming both models — never the max.** The max would be
   the safe direction for sizing alone, but the descriptor is also the ONLY
   carrier of the declared reliability and durability to the Rust registration
   check (D10), and an equality check fed a max refuses the image that declared
   less. A refusal costs sizing nothing it needs: the consumer prices that
   endpoint at its worst case (D6), which covers both declarations, and the
   check skips a row it was not given rather than enforcing a number nobody
   wrote. This is issue 1564's rule for the C/C++ table, held on the descriptor.
3. **The closure facts need no reduction.** `wire_bound_bytes` and `[types]` come
   from the bound tables the configure REGISTERED (phase-457-payload W2), and
   that registration is already configure-wide — it IS the shared runtime's
   closure.
4. **The file says what it covers.** `[meta] entry` names the runtime, and the
   descriptor lists the entries it composed — the reason `[meta] entry` exists at
   all ("a copied file still says what it is about"). The reader refuses unknown
   keys, so this is a `schema_version` bump, shared with the closure field below.

**The cost is the one the carriers already pay, so retiring them onto D12 moves
no byte.** Every entry of an N:1 configure is sized for the envelope of its
coordinate's images. That is what the `NROS_DECLARED_*` carriers deliver today
(issue 1600's union), so the retirement test for each KEPT row on issue 1649 is
the W14 method: build the configure with the runtime descriptor named and the
carriers removed, diff the knobs each consumer emits, require no difference.

**D12 closes the ROAD axis only.** Eleven KEPT rows carry a second reason in
their ledger text — the descriptor has no `[image]` field for the fact (callback
slots over timers and guard conditions, scheduling contexts, action clients,
publishers, monitor rows, the infra-queryable feature), no component attribution
on its rows, no consumer that ranks it first, or no file at all for a model with
no wiring. Those are a FIELD axis D12 does not touch, and they are
[issue 1655](../issues/1655-descriptor-has-no-field-for-eleven-kept-carriers.md).

**Not decided here: whether the cmake road should build a runtime per IMAGE**,
as the cargo road already does, so each image is sized for itself rather than
for its coordinate's envelope. That is RFC-0065 D8's trade — one runtime build
per coordinate against N — and neither side is measured: the bytes the smaller
images over-provision, and the build time of N Corrosion imports of the
runtime. D12 is correct under either answer; the measurement belongs to D8's
owner.

**Landed 2026-10-03 (issue 1649).** The runtime descriptor lives at
`<build>/nros/sizing/runtime/shared.toml` — a sub-directory, because an entry
is a CMake target name and no target name contains a `/`, so it can never be an
entry's file — with `[meta] entry = "shared-runtime"` and `[meta]
composed_entries` (schema 2, shared with `[types] max_wire_bound_bytes`). Rule
2's comparison is keyed on `(component, kind, type)` and NOT on the topic: a
remap gives one subscription a different topic in each image, so a topic-keyed
comparison would let `/a` at depth 1 and `/b` at depth 50 never meet and the
union price the second image at 1. A `history` disagreement refuses `depth`
too. Measured on `examples/workspaces/cpp` native (seven entries): the per-carrier
knob diff found six carriers zero-diff and eleven that are issue 1655's; and
naming the descriptor at all moves ten generated files against the carriers-only
build, every move a descriptor-first consumer answering from the union where the
multi-entry road had fallen to a builtin (issue 1649 has the table).

### D4 revised — producers are named by INPUT; the road is where they are CALLED

D4 says "THREE producers, one composer" in CLAUDE.md and "TWO producers" in the
heading above, because phase-457 W0.b added `--from-leaf` after W14 named the
section. Restated by what each READS, which is the distinction that survives the
build path changing under it:

| producer | reads | called from |
| --- | --- | --- |
| leaf (`write_for_leaf`) | the leaf's probe, its `generated/` bound tables, its synthesised model and `system.contract.yaml` | `nros sync` on a single-package cargo leaf |
| model (`write_for_model`) | the resolved SystemModel(s), the registered bound tables, the board's triple, the workspace's probe sidecars | `nros build` stage 4 on a cargo workspace image; `nano_ros_entry()` at configure on cmake and west |
| declaration (`--from-leaf`) | a standalone cmake leaf's `[[component]] entities` | `nros_record_leaf_entity_facts` |

D12's runtime descriptor is not a fourth producer; it is the model producer over
a list of models. Generated entries changed none of the INPUTS, so the split
stands. What the build path does change is where an input COULD come from:

* **The board is resolved once, at stage 2/4, for every road** — so a board
  fact the cmake configure cannot resolve (phase-457 W4's still-refused
  `heap_budget_bytes`) is a fact stage 4 should hand to the configure, never one
  the configure should learn to read.
  [Issue 1653](../issues/1653-cmake-road-descriptor-never-states-the-board-heap.md).
* **The component's language is on the plan.** phase-474 put each node's
  component KIND (`c` / `rust` / `rclcpp` / `configure`, read from
  `nros-metadata.json`'s `lang`) on `LoweredNode`. So the model road has a
  per-COMPONENT language without the probe — but a language is still a proxy for
  the registration call (a C/C++ registration with no type hint takes `RX_BUF`,
  [issue 1319](../issues/archived/1319-arena-prices-a-subscription-below-what-a-schemaless-backend-allocates.md)).
  The direction for
  [issue 1648](../issues/1648-model-road-observed-buffered-subscription-needs-probe-language.md)
  is to OBSERVE the buffered row the way phase-457 W3 observed the in-place one
  — the registration funnel computes the slot size, so it knows which row it
  claimed — with the plan's per-component language as the fallback, and never an
  image-wide one.
* **The declaration producer shrinks with the leaves it serves.** It exists for
  the twelve standalone NuttX C/C++ leaves
  ([issue 1556](../issues/1556-c-and-cpp-standalone-leaves-have-no-entity-probe.md))
  — phase-470 kept standalone leaves (its classes 3 and 4) and did not touch
  these. It retires when those leaves get a census or a synthesised model, not
  before.

### D4 extended — the closure field `RX_BUF` needs

[Issue 1595](../issues/archived/1595-subscriber-payload-carriers-have-no-descriptor-reader.md)
found the one payload carrier no `[[endpoint]]` row can replace: `RX_BUF` is a
CLOSURE fact (every type the image could receive or publish, because
`DEFAULT_TX_BUF` aliases it), and an undeclared endpoint's type is in the
closure and in no row. So `[types]` gains **`max_wire_bound_bytes`**, taken per
column like the three maxima beside it: the largest wire bound over every type
the closure registered. It refuses — naming the type — when a registered type is
unbounded or unpriced (D6's third and fourth rows), and naming the table when a
registered table is absent (the first-configure state below). Same inputs as
`[types]` today, so every producer road can state it.

### The fixed point is a descriptor property too

On the cmake road the bound tables are written by codegen DURING the build, so a
clean tree's first configure registers tables that do not exist yet and the
descriptor REFUSES its bound fields (`_nros_sizing_bound_args`: "the rest are
built by the first build"); the second configure states them, the consumers'
inputs change, and the runtime rebuilds. That is
[issue 1647](../issues/archived/1647-cmake-first-build-compiles-placeholder-message-bounds.md)'s
two-build fixed point seen from the descriptor, and its fix — a pre-configure
producer of the bound tables,
[issue 1252](../issues/1252-message-bound-knobs-have-no-pre-configure-twin.md)'s
direction — closes both at once. Acceptance for either includes the
descriptor's bytes, not only the binary's.
*(Issue 1647 resolved the same day: the generator runs codegen at CONFIGURE
time when a fragment is missing or stale, and the JSON table is written beside
the fragment by the same command, so the first configure already registers
tables that exist.)*

### The census is the evidence, and it is per MODEL

phase-463's census is what makes a stated fact trustworthy: the descriptor
carries what the contract says, and the census checks that the code agrees. It
is keyed per MODEL (`<model-dir>/<stem>.census.json`), which is what D12
composes over — so an N:1 runtime descriptor is trustworthy exactly when each of
its models' censuses is fresh and passing. Two gaps in that chain are the build
path's to close, not the descriptor's: the census hooks live in `nros-cpp`, so a
Rust node, and a C node that opens its own node through `nros-c`, are not
attributed; and only the cmake configure checks a census, so a cargo image is
never checked
([issue 1419](../issues/1419-no-layer-reconciles-contract-endpoints-with-the-code.md)).
The direction for both: the hooks move to `nros`, the crate every language's node
API sits on, and the cargo road checks at `nros build` stage 4, where it already
writes this descriptor.

### Ruling, 2026-10-03 — how `[image]` grows, and what earns a file (issue 1655)

Issue 1655 asked for two rulings to be made once rather than per carrier.

**1. `[image]` grows by one rule: a field is a fact a carrier already DELIVERS,
stated from the derivation that carrier comes from.** D4 admitted `[image]` for
the counts no endpoint row can carry; that kind is now closed under a test
rather than a list. A field enters only when (a) it is a field of
`DerivedEntityKnobs` — the `EntityInventory::derive` the `NROS_DECLARED_*`
carriers are written from, so the two cannot disagree — never a reduction a
consumer would redo over rows (D4's "third mirror"); (b) it is stated iff the
derivation derived, and refused with the derivation's own reason otherwise;
and (c) on an N:1 runtime it is D12's fold, with no per-field reduction. Seven
landed under it: `callback_slots`, `action_client_slots`, `publisher_count`,
`sched_context_count`, `monitor_rows`, `age_monitor_rows`, `cell_entities`.

Its corollary is the one that bit: **a field takes over a delivered fact; it
never introduces one.** The standalone-leaf DECLARATION road (`--from-leaf`)
had no carrier for any of these — its builtins sized those images — and its
declarations were checked against their runtimes for their queryables only
(issue 1378). Stating them there would shrink every executor and session table
the declaration does not mention (measured with `--from-leaf` over the twelve
NuttX C/C++ leaves: `publisher_count` 1 and `callback_slots` 1 for a talker,
against builtins of 8 and 4), so that road REFUSES the seven, naming
this ruling. Introducing a derived size on a road is its own change, with its
own runtime evidence.

**2. "No contract ⇒ no file" is restated as what both producers actually do:
no STATED entity fact ⇒ no file.** The model road states none for a model that
describes no wiring and writes nothing; the cargo LEAF road's probe states the
entities the image creates, so it writes a file with or without a contract.
That is the file observed for `examples/native/rust/talker` after `nros sync`
with no contract — `basis = "contract"`, no QoS fields, `undeclared_endpoints =
1`: neither stale nor a writer bug, and re-measured on a fresh sync.
`Basis::Contract` means "the rows are THIS IMAGE's endpoint set, not the link
closure", whichever input stated them (its vocabulary doc now says so).

What does NOT earn a file is a closure-only or model-only fact on its own — the
largest wire bound in the closure (`RX_BUF`), or the parameter-service node
count of a model with no wiring. A descriptor whose every entity fact refuses
is a second spelling of "nobody said" that every consumer must read through to
reach the one number it carries. So the two carriers that deliver such a fact
to a contract-less image — `NROS_DECLARED_SUBSCRIPTION_BUFFER_SIZE` and
`NROS_DECLARED_NODES` — stay by design: where a descriptor IS named, its own
field or count ranks first; where none is, the carrier is the delivery. They
are `ByDesign` rows in `check-knob-single-reader`, citing D4.

### What this amendment does not change

D1–D11 stand. Refusal stays per field and never a default; demand stays
unfloored; a descriptor stays relative and self-contained; the backend still
owns its formula. D12 adds a unit (the runtime) and a reduction it borrows; it
adds no fact and no carrier.
