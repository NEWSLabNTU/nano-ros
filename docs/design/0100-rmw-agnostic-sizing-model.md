# RFC-0100 — One sizing model: the contract states facts, every backend derives its own buffers

**Status:** Draft (2026-09-11)

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
registration_path = "rust_typed_schemaless"
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

**Three fields the sketch above did not have, and one it did.**
`schema_version` and `[meta] entry` are new: the first because a reader that kept
going on a version it does not know sizes from numbers whose meaning has moved
(the rule that took the entity inventory to 5), the second so a copied file still
says what it is about. `registration_path` replaces the sketch's `buffer`, which
D9 rules out as a `SLOTS` source anyway; it is REQUIRED, and D1 says why.

A cargo consumer gets a real `rerun-if-changed` edge on this path —
`nros_sizing_descriptor::load_for_build_script` emits it, on the file's CONTENT
and never on the variable that names it (issue 0491). A cmake consumer reads it
through `nros_sizing_descriptor_read()` in `cmake/NanoRosSizingDescriptor.cmake`,
which registers BOTH the descriptor and the CLI in `CMAKE_CONFIGURE_DEPENDS`
(issue 1018's rule — `execute_process()` has already run by the time ninja
decides anything, so that list is the only thing that makes the emitted fragment
fresh). The descriptor is registered even when it does not exist yet, so the
first `nros sync` after a configure is what re-triggers one.

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
| zenoh | counts; `depth`; small/large bounds; service req/resp bounds; `[policy]` | `ZPICO_MAX_*`, `SUBSCRIBER_RING_DEPTH`, payload pools, `SERVICE_BUFFERS` | counts and payload classes derived; ring depth DERIVED from the declared depths (phase-454 W6.a, −124,032 B measured); `SERVICE_BUFFERS`'s slot size takes the declared service bound as its default and carries a stated non-annotation — but **no service or action type has a bound row to read** (`record_message` runs for `.msg` only), so it refuses on every image today |
| XRCE | counts (**zero legal**); per-family bounds; `depth`; MTU; `reliability` | `MAX_*`, per-family `BUFFER_SIZE`, ring depths, `STREAM_HISTORY` | two counts derivable via `-1`; one global `BUFFER_SIZE = 1024` serves three families; reliability unread |
| Cyclone | `[types]`, `[target].heap_budget_bytes`. **Nothing else** | `MAX_TYPES`, `MAX_DESCRIPTOR_TYPES`, `MAX_FIELDS`, `MAX_KINDS`, heap assertion | **all derived, phase-454 W6.c** |
| uORB | distinct topic count; subscription count | `REGISTRY_CAPACITY`, `PX4_MAX_CALLBACKS` | neither wired |
| cffi | subscription count; node count; backend count | `RMW_SUBSCRIBER_SLOTS`, `MAX_NODES`, `MAX_BACKENDS` | slots derived; `max_nodes = components().len()` computed and discarded |

**Not a derivation, and must not be modelled as one:** cffi's `SLOT_SIZE` is a
hard 1024, and `insert::<T>()` returns `None` both when every slot is claimed and
when `size_of::<T>() > 1024`. The caller maps both to `NROS_RMW_RET_BAD_ALLOC`,
so a backend whose per-subscription state outgrows the slot reports the same
code as a full pool — and only one of those has a knob. Raising
`NROS_RMW_SUBSCRIBER_SLOTS` against the size cause buys nothing and costs 1 KiB
a slot on the platform with the least RAM.

No user fact answers "how big is a backend's private state struct", so this is
**not** a sizing input. `size_of::<T>()` is a compile-time quantity, so it wants
a `const` assertion rather than a runtime return. Tracked as
[issue 1322](../issues/1322-cffi-slot-size-overflow-reads-as-pool-exhaustion.md).

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
| uORB | current literals |

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
| XRCE reliable streams | ~131,072 per session | over | reliability declared but unread |
| zenoh `SERVICE_BUFFERS` | 144,128 on a native talker | over | not derived, not in the inventory |
| island arena, depth 10 vs declared | 135,432 | over | 207,096 against 71,664 with the eleven `QoS(1)` declarations the code already makes |

The first row is the one that matters most, and it is the reason this RFC leads
with facts rather than savings: three of these are money left on the table and
one ships `BufferTooSmall`. A model that only pursued the savings would have
made it worse.

The last row is still the model's clearest argument: the running code did not
change. Only whether the build was told what it registers.
