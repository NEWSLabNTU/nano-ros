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

### W4 — the descriptor artifact — **LANDED**

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

**What landed.** The schema is in RFC-0100 D4, updated to what is written rather
than what was sketched; later waves are briefed from there.

| piece | where |
| --- | --- |
| schema + the one reader | `packages/tooling/nros-sizing-descriptor` — leaf by construction (`serde` + `toml`), so a `no_std` crate's build script can depend on it |
| producer | `nros_cli_core::sizing_descriptor` — joins the entity inventory, the bound inventory and the board facts; nothing is re-derived |
| write site | `cmd::leaf_settings::write`, so every `nros sync` and every `nros build` refreshes it beside `build/<image>/nros-cargo.toml` |
| cargo consumer | `nros-node/build.rs` takes the receive-ring length word from `[target] pointer_bytes` |
| cmake consumer | `cmake/NanoRosSizingDescriptor.cmake` → `nros ws sizing-descriptor --output-cmake` |

**The cargo consumer is the smallest honest one, and it closes a stated defect.**
`nros-node/build.rs` priced an `SpscRing`'s per-slot length array at a hard
`RING_LEN_BYTES = 8` under its own comment: *"taken at its 64-bit width. A 32-bit
target spends 4, so this over-states there."* That is not an oversight — it is
the correct answer for a build script, which structurally cannot know the target
ABI (phase-118-E), and the over-statement is the safe direction. The descriptor
carries the board's number, so the same image on `thumbv7em` now prices
`(depth + 1) * 4` where it used to pay `* 8`. With no descriptor the constant
stands and the build is byte-identical to every build before this wave.

Three outcomes, and the middle one is D6 working:

| state | result |
| --- | --- |
| no descriptor | `RING_LEN_BYTES_DEFAULT`, unchanged |
| `[target]` refused | the default, plus a `cargo::warning` naming the refusal — *"always the safe direction and always loud"* |
| corrupt / unknown schema | a hard build error naming the file. The NEGATIVE CONTROL: a descriptor exists, so defaulting would size from numbers a user believes they supplied |

**`[policy]` is empty and that is the W4 answer, not an omission.** D1: policy is
the kind of fact *nobody* can derive, so it is stated — and no image states a
burst depth, a graph size or an MTU today. The first consumers that want one
(W6.a's `SUBSCRIBER_RING_DEPTH`, W6.b's XRCE stream history) bring the rung that
states it. Writing a derived number there now would be the category error D1
exists to prevent.

**`[types]`'s `max_fields` / `max_kinds` / `max_nested_depth` are REFUSED with a
reason, not left absent.** They are derivable — from the schema walk codegen
already does — and W6.c is the wave that reaches it. A refusal says that; an
absence would say nobody ever asked.

**What W5 is handed.** `registration_path` is on every endpoint row, as one of
issue 1319's four measured values, composed from two halves the build script
cannot see (the entry's language, and whether the linked backend carries type
descriptors). `RegistrationPath::claims_closure_buffer()` is the predicate W5's
arena term wants. Where either half is unknown the field is REFUSED rather than
guessed — the two schemaless rows are 1,848 bytes per subscription in the UNDER
direction, so a guess there is the failure, not a conservative default.

One limit W5 should know: a C/C++ entry is credited with `c_typed_hint`, because
whether an individual call site passes `rx_size_bound<M>` is a property of that
call site and nothing this writer reads distinguishes them. W10 — which closes
the C half of the declared-QoS check — is where a per-call-site answer becomes
available.

Gates and tests: `just check sizing-descriptor-reader`
(`tests/cmake-sizing-descriptor-tests.sh`, CLI stubbed, 16 assertions),
`nros-sizing-descriptor`'s 18 unit tests (round-trip, the three parse rules, the
`keep_all` per-field refusal, zero surviving unfloored), the producer's 10, the
verb's 4, and `sizing_descriptor_portable.rs` for the issue-0320 acceptance.

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
| W6.b | XRCE | **Landed.** `reliability` gates the two 64 KiB `*_reliable_buf`; one global `BUFFER_SIZE = 1024` splits into three per-family sizes; the subscriber ring takes declared bound and depth. See the correction below — the streams shrink to a protocol floor rather than vanishing, and two of the three families are split but not derived |
| W6.c | Cyclone | **LANDED** — `MAX_DESCRIPTOR_TYPES` derived from the same count as `MAX_TYPES` (silent-drop overflow at ~86 types today); `MAX_FIELDS`/`MAX_KINDS`/`MAX_NESTED_DEPTH` from the schema walk codegen already does; heap budget asserted at boot (D11) |
| W6.d | uORB | `REGISTRY_CAPACITY` and `PX4_MAX_CALLBACKS` — both trivially derivable, neither wired |
| W6.e | cffi | `MAX_NODES` from `components().len()`, which is already computed and discarded |

Each sub-wave keeps RFC-0100 D7: publish demand **unfloored**, floor at the pool.
Issues 1015 and 1033 are the negative control — one derivation feeding two
consumers with opposite right answers, where the floor in the shared derivation
silently defeated the fix.

Acceptance per sub-wave: a measured byte delta on a named image, plus the pool
inventory regenerated.

#### W6.a — zenoh — **LANDED**

`nros-rmw-zenoh/build.rs` reads the descriptor through `nros-sizing-descriptor`,
exactly as `nros-node/build.rs` does, and owns its own formula (D5).

**`ZPICO_SUBSCRIBER_RING_DEPTH` is derived.** It is D2's `SLOTS` factor for both
payload pools, and it was AUTHORED. The demand is the MAXIMUM declared depth over
the subscription rows — one knob serves every subscriber, so a ring shorter than
a declared depth is the downgrade `shim/qos.rs` already reports and advertises to
the graph.

**Measured, on `packages/testing/nros-tests/bins/sim-clock-listener`** (native,
zenoh, `--release`) — a leaf chosen because it needs neither `nros sync` nor ROS,
so the number is reproducible anywhere:

| symbol | before | after | delta |
| --- | --- | --- | --- |
| `LARGE_PAYLOADS` | 131,072 | 32,768 | **−98,304** |
| `SMALL_PAYLOADS` | 32,768 | 8,192 | **−24,576** |
| `SUBSCRIBER_BUFFERS` | 2,496 | 1,344 | −1,152 |
| section RAM | 557,952 | 433,920 | **−124,032 (−22.2 %)** |

for a descriptor stating the one thing the image keeps: `KEEP_LAST(1)` on
`/clock`. Nothing about the running code changed.

**`ZPICO_SERVICE_BUFFER_SIZE` takes the declared service/action bound as its
default, and may only RAISE it.** The floor is the builtin 1024 and the reason is
measured: a zenoh service server IS a queryable, and `[param_services]` (6) +
`[lifecycle]` (5) claim eleven of them before the app declares anything (issue
0460). Those servers receive through this very pool and appear in no
`[[endpoint]]` row, so sizing the slot DOWN to what the app declared would
under-size a surface the declaration structurally cannot mention. D7 is kept —
the descriptor publishes demand unfloored and the floor lives here, at the
consumer.

**It refuses on every in-tree image today, and says why.**
`BoundInventory::record_message` is called for `.msg` files and for nothing else,
so `pkg/srv/Name_Request` and `pkg/action/Name_Result` have no bound row and the
producer writes `wire_bound_bytes` REFUSED on every service and action endpoint.
The build prints that refusal naming the type. **Pricing srv/action types is the
prerequisite W6.b inherits** — it is a producer change (codegen records the two
member messages; the descriptor writer joins `srv/Name` to them), not a consumer
one, which is why it is not in this sub-wave.

**`SERVICE_BUFFERS` gets a stated reason, not an annotation.** It is 144,128 B on
a native talker — a node with no service server — and the largest single RAM
symbol in a native zenoh image. `gen-pool-inventory.py` evaluates a pool as a
PRODUCT of knobs at literal defaults, and this one is a struct whose size is a
SUM with target-dependent terms (`[u8; 256]` keyexpr, four ring slots, four
atomics) over a count whose default is itself computed. Both halves are right for
one build and wrong for the next appended field. So it follows the three
documented deliberate non-annotations — `LendArena`, `MESSAGE_INFO_TABLE`,
`executor::backing` — and their shared principle: *the size is known to the
compiler, so read it from the compiler's output*. `just mem-report` prices the
symbol from the ELF exactly. `scripts/nros-mem-report.py`'s own header had
already argued this; the argument now lives beside the static that needs it.

Three outcomes, the same three W4 established: no descriptor → byte-identical to
every build before this wave (verified: both constants unchanged, no warning
printed); a refused field → the builtin plus a `cargo::warning` naming the
refusal; a corrupt descriptor → a hard build error naming the file.

#### W6.b — XRCE — **LANDED**

**"Stops paying both buffers" was not available, and the reason is structural.**
Both reliable streams stay LIVE on a best-effort-only image, because this
backend sends *every control message* on `st->output_reliable` — participant,
topic, publisher, datawriter and datareader CREATE, DELETE,
`uxr_buffer_request_data` — and `uxr_buffer_request_data` names
`st->input_reliable` as the stream the Agent delivers on, for **every reader,
whatever its QoS**. A declaration about data delivery is not a licence to make
session setup lossy, so what `reliability` gates is the `SLOTS` term, down to
the protocol floor of 4 that D2 already writes into its own table. Measured on
`xrce_session_state_t` (x86-64, defaults otherwise): **427,968 → 329,664**, i.e.
**98,304 bytes of the 131,072**, and the residue is a protocol requirement
rather than money on the table.

**Only the subscriber family is derived; the two service families are split and
not.** The parameter (6 per node) and lifecycle (5) families open service
servers that no `[[endpoint]]` row itemises, and an action server opens three
more — so a size taken from the declared service rows alone is short for exactly
those, in the UNDER direction. That is D6's `undeclared_endpoints != 0` rule
reaching a set the field does not count. The split still lands in full: three
macros, three arrays, three knobs, each settable.

**The subscriber pool's two factors move TOGETHER or not at all,** and the
mutation control is measured rather than argued: a declared `std_msgs/msg/String`
prices its ring entry at 1,174 bytes against the literal 1,024 while a declared
`depth = 10` prices the ring at 10 entries against the literal 32, so taking the
bound WITHOUT the depth is **427,968 → 466,880, a 38,912-byte growth**. Taking
both, at depth 1: **427,968 → 72,960**, and with the reliability gate as well
**→ 72,960 from 171,264**. A pool sized from one declaration and one literal
describes no image at all.

**`mem-report --baseline` cannot show any of this**, and that is a property of
the pool rather than of the tool: `xrce_session_state_t` is ONE
`nros_platform_alloc` at session open, and `mem-report` joins declared pools to
ELF SYMBOLS. The figures above are `sizeof` under the two `-D` sets. The knobs
themselves ARE enumerable — all three reach `static-pool-inventory.md` with
their defaults, which needed `gen-pool-inventory.py` to follow a `#ifndef`
default that is another guarded MACRO rather than a literal.

#### W6.c — Cyclone — **LANDED**

Cyclone reads `[types]` and `[target].heap_budget_bytes`, and nothing else — no
entity counts, no QoS depth, no MTU, no reliability. That is the model working
rather than a carve-out, and `subscriber.cpp:78-99` is why: this backend
allocates every payload with `ddsrt_malloc` and owns no receive buffer to class,
so *"a Cyclone consumer can set the hint, do everything the sizing campaign asks,
and correctly observe nothing change in this backend."*

**One count, two tables.** `derive_max_descriptor_types` sits beside
`derive_max_types` and takes the same input, because it is the same SET:
`TypeRegistry::get_or_build` inserts into the Rust registry and then calls
`nros_rmw_cyclonedds_register_descriptor` for that type on the next line. The
arithmetic differs and must — the registry rounds to a power of two for
`heapless` and floors at 32, while `Entry g_entries[N]` is a plain C array that
wants the bare demand (D7), with the floor at the pool where a `#if ... < 1`
now sits.

**The three shape counters come from the walk that was already running.**
`schema_value::schema_shape_for` counts fields, flattened kinds and nesting depth
over the `&'static [Field]` graph `build_schema` produces for the bound — one
walk, not a second derivation. It mirrors `SchemaWalker::push_field_type`, and
three of its rules are places a plausible reading is wrong: a kind is one
FieldType NODE and nothing dedups; `Array`/`Sequence`/`BoundedSequence` each add
a level of depth exactly as `Nested` does; a flat message needs depth **1**, not
0, because the guard tests on ENTRY. The counters ride
`nros_message_bounds.json`, and `type_facts` takes the maximum PER COLUMN.

**The one thing that does not reach every road.** The knobs travel as cargo
`[env]` rows — `option_env!` for the Rust half, and a `cc::Build::define` in
`nros-rmw-cyclonedds-sys` for the C++ half, which has no other reach. The Zephyr
Cyclone lane compiles the backend into the app library per image and can take the
same values through `zephyr_compile_definitions`; the NATIVE CMake road cannot,
and that is structural rather than an omission: `nros_rmw_cyclonedds` is ONE
static target `add_subdirectory`'d once per build tree, and a build tree is shared
by every image at its coordinate, so a `-D` there would be last-configure-wins
across images. That road keeps the `#ifndef` fallbacks, which is exactly what it
had before. Wiring it needs a per-entry OBJECT library or a runtime cap, and
neither belongs in this wave.

### W7 — contract and `qos_overrides` must agree

Build error on any divergence, naming both sites (D8). Where an override states a
policy the contract omits, the error names the contract line to add.

This closes a live under-size path with no diagnostic:
`qos_overrides./t.subscription.depth=64` is baked into the runtime today and the
arena never hears about it — issue 1190's `BufferTooSmall` with a config-shaped
cause.

Acceptance: a divergent pair fails the build naming both; an agreeing pair
builds; a params-only policy fails naming the contract.

### W8 — `buffer:` earns its keep — LANDED, and ARMED rather than firing

Two diagnostics (`queue` at `depth = 1`; `latest` at a depth of 2 or more — the
arena's own break point, where an endpoint stops being a read-latest triple
buffer and starts paying by the message), and the rate-derived depth default for
`queue` endpoints. A stated `depth` still wins, per the ladder; the margin is
**one slot**, from the unaligned-window bound, argued in RFC-0100 D9.

`packages/cli/nros-cli-core/src/queue_depth.rs` is the whole vocabulary and the
whole arithmetic; `EntityInventory::{queue_depth_defaults, buffer_diagnostics}`
are the views, and `declared_depths()` is where the ladder runs. `DeclaredDepth`
gained a `DepthSource`, because the depth table has two consumers that want
opposite things from a default: `subs_arena` SIZES from it (a default is the
point) and `to_declared_qos_header` ASSERTS from it (a default there is a
requirement every call site must match). Schema 5 → 6, with
`NROS_ENTITY_DERIVED_DEPTHS` publishing the provenance.

**What the wave measured, and it is the finding rather than a caveat.** Of the
three contract facts the derivation needs, ONE reaches the SystemModel. The
resolver parses, validates and reasons about the other two — it emits a
`[queue-drain-rate]` warning performing exactly this division — and the model
schema has no field for either, so it writes neither. Issue **1339**; RFC-0100 D9
carries the table. Consequences:

* No contract in this tree can produce a `queue` endpoint, so **no image's
  sizing moves**, which is acceptance 5 satisfied structurally rather than by
  luck. The targeted controls are asserted anyway (rates present + no
  discipline, and `buffer: latest`, both byte-identical).
* The drain rate that does arrive is a SUBSTITUTE: the `min_rate_hz` of what a
  node's timer paths publish (`mapper_input::pub_rate_hz`'s convention). Absent
  for a drain timer that publishes nothing.
* Acceptances 1, 2 and 4 are asserted over hand-built inventory rows, which is
  the only road that can reach a `queue` endpoint today; acceptance 3 is
  asserted end to end from a real contract through the real resolver, where the
  reported reason is `NotAQueue` precisely because both rates DID arrive.
* `tests/contract_queue_buffer_reaches_the_model.rs` holds two tripwires that go
  red the day 1339 closes, each naming the one line to wire.

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

### W10 — the C and Rust halves of the declared-QoS check — **LANDED**

`_nros_declared_qos_arm` returns early for Rust and INTERFACE targets, and there
is no C equivalent of `NROS_ASSERT_DECLARED_DEPTH`. The descriptor is
language-neutral, so this is where that closes.

**One of those two premises was wrong, and measuring it changed the shape of
the wave.** The descriptor is language-neutral and C already receives it —
`_nros_declared_qos_arm()` returns early for RUST components and for INTERFACE
targets, and a C component is a STATIC library like a C++ one, so a C component
has had `nros_declared_qos_generated.h` on its PRIVATE include path all along
and only lacked the check. **Rust cannot receive it at all**, and not for want
of wiring: `nano_ros_node_register(LANGUAGE RUST)` creates an *empty INTERFACE
library* (`NanoRosNodeRegister.cmake:626-651`) whose sources are compiled by the
workspace runtime crate, so there is no preprocessor in that lane and no
include path to put a header on. "Arm the header for Rust targets" is not a
thing that can be done. The two halves therefore travel different roads.

**C — compile time, plus registration.** `nros/declared_qos.h`, with
`NROS_ASSERT_DECLARED_DEPTH(type, topic, depth, topic_text)` mirroring the C++
macro's four arguments. Three measurements decided the mechanism:

* `"abc"[0]` is **not** an integer constant expression (gcc: *expression in
  static assertion is not constant*; clang: *not an integral constant
  expression*), so a character-wise macro chain cannot reach `_Static_assert`.
  `__builtin_strcmp` over two literals **does** fold — under `-std=c11
  -pedantic`, `-ffreestanding`, `-fno-builtin`, and on the pinned
  `arm-none-eabi-gcc 13.2`.
* A macro parameter of the caller is **not** substituted inside a separately
  defined row macro, so the obvious port of the C++ X-macro reads
  `use of undeclared identifier 'q_type'`. The query has to travel THROUGH the
  list, which is why the generator now emits a second, query-parameterised
  `NROS_DECLARED_QOS_ROWS_Q` beside the C++ `NROS_DECLARED_QOS_ROWS` — same
  rows, one loop, one assertion holding the two row counts equal.
* A `_Static_assert` message is a string literal and cannot interpolate an
  `int`, exactly as in C++. C's answer to what C++ does with
  `declared_depth_agrees<Declared, Passed>` is a pair of `extern char` array
  declarations of one name sized `declared` and `passed`: equal depths give
  `char[1]` twice and nothing happens, unequal ones give
  `conflicting types … have 'char[10]' / previous declaration … 'char[1]'`.
  The topic comes from the macro, the numbers from a type.

A C call site whose depth is not a constant expression — every
`nros_cpp_qos_t` built in a helper function — takes the registration check
instead.

**Rust — registration.** There is no macro seam at the subscribe site
(`nros::main!` emits only the boot scaffold; there is no `nros::subscribe!`)
and the topic is a runtime `&str`, so no `const` assertion is possible.
`NROS_ENTITY_DECLARED_DEPTHS` (`type|topic=depth`) now reaches every lane —
the Zephyr cargo lane already forwarded it, and
`_nros_declared_depth_table_env` adds the rest — `nros-node/build.rs` renders
it as `config::DECLARED_QOS_ROWS` with **both** type spellings, and
`declared_qos::check` refuses a disagreement with
`NodeError::DeclaredDepthMismatch` at every one of the 14 places a
subscription is created from a topic and a QoS. The C/C++ FFI registration
seams call the same function, which is how a C component reaches it —
`nros_cpp_subscription_register` is what a C configure function calls, and it
returns the same `-403` the C++ boot check already used.

Note the two guards on one carrier, which is the reason
`_nros_declared_depth_table_env` is a sibling of `_nros_qos_depth_env` rather
than an extension of it: SIZING must refuse a partial picture (the arena bills
every slot the same price, so a table over the endpoints that happen to be
declared sizes an image from a subset of itself), while CHECKING per endpoint
does not — whether `/chatter`'s declared depth matches the depth registered on
`/chatter` is a question about `/chatter`. Every in-tree contract leaves at
least one subscription silent, so requiring the sizing guard would have meant
no image in the tree was checked at all.

Gates: `check-declared-qos-header` grew case F2 (the C negative control against
the header a real configure just rendered — 25 assertions, from 16),
`just check c` grew the positive/expected-failure pair, and
`check-declared-qos-registration` is new: it builds nros-node twice and asserts
that the declared-image test passes WITH `NROS_ENTITY_DECLARED_DEPTHS` and does
not exist without it, so a cfg that was always on cannot make the lane green
over nothing.

## Acceptance for the phase

Not "it builds". One correctness result and three measured recoveries, each on a
named image, each with a before/after from `just mem-report --baseline`:

| gap | direction | acceptance |
| --- | --- | --- |
| Rust typed on a schemaless backend (issue 1319) | **UNDER**, 1,848 B per subscription | a reproduction that fails with `BufferTooSmall` FIRST, then passes |
| XRCE reliable streams | over, ~131,072 B per session | **MET at 98,304 B** (W6.b, measured): a best-effort-only image drops both buffers to the protocol floor. It cannot drop them entirely — both streams carry session control for every image, whatever its QoS; see "W6.b, as built" |
| zenoh `SERVICE_BUFFERS` | over, 144,128 B (native talker) | derived, and either annotated or given a stated reason |
| island arena, declared vs default depth | over, 135,432 B (207,096 → 71,664) | already true on the Zephyr road; holds on the cargo road too |

The ordering is deliberate. Three of these are money on the table and one ships
a runtime failure — a phase that chased only the savings would have made the
fourth worse, which is exactly what this phase's first draft did.

The last row stays the clearest argument: the running code does not change. Only
whether the build was told what it registers.

## Explicitly not in this phase

* **cffi's `SLOT_SIZE`.** A hard 1024 where a size overflow and a full pool both
  return `NROS_RMW_RET_BAD_ALLOC`, and only one of the two has a knob. No user
  fact answers "how big is a backend's private state struct", so it is not a
  sizing input; it wants a `const` assertion. Filed as
  [issue 1322](../issues/1322-cffi-slot-size-overflow-reads-as-pool-exhaustion.md).
  Worth landing in the same window as W3, since adding per-endpoint QoS to a
  handle is exactly what would grow that struct past 1 KiB.
* **A static pool for Cyclone.** D11 gives it a heap budget and a boot
  assertion; inventing a pool would touch the vendored fork's ddsrt allocator.
* **`lifespan`, `deadline`, `liveliness`.** They bound occupancy or liveness, not
  capacity. They stay runtime-only.
* **RFC-0049's ladder.** Precedence is unchanged.
