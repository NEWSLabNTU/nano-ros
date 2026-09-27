# phase-457 — payload-class sizing on every road, and the registration fact both halves need

**Status (2026-09-21). Queued, not started.** Successor to
[phase-454](phase-454-contract-states-facts-backends-derive.md), which
implemented [RFC-0100](../design/0100-rmw-agnostic-sizing-model.md) and named its
own ceiling. Closes [issue 1393](../issues/1393-cmake-road-has-no-bound-inventory.md)
and [issue 1340](../issues/1340-arena-budgets-a-receive-region-an-in-place-backend-never-claims.md),
and issue 1407 (not linked — its file is on PR #1123 until that merges).

**Do not start before phase-454 W9 lands** — W9 retires the
`NROS_DECLARED_*` / `NROS_DERIVED_*` carriers that the descriptor can already
replace, and this phase changes what the descriptor can state. Doing them in the
other order means W9 re-deciding a boundary this phase moves.

## Why one phase and not two

Both issues are blocked on the same missing fact, and RFC-0100 says so:

> *"Issue 1340 is blocked on the same missing fact from the Rust side, and the
> two should be settled together rather than twice."*

`registration_path` is refused on the workspace and cmake roads because nothing
knows which subscribe spelling an image writes. That is exactly what 1340 needs
to take its saving: the Rust **generic** registration claims `RX_BUF` while the
in-place one claims 672 bytes, and an endpoint row cannot today say which an
image uses. Fixing one produces the fact the other is waiting for, so splitting
them means building the same producer twice and choosing its shape twice.

## What is true going in

phase-454 W14 gave the descriptor a second producer. It states counts, topics,
types and all four QoS policies from the resolved SystemModel, and **refuses five
fields by name**, every reason naming issue 1393:

| field | what it needs |
| --- | --- |
| `wire_bound_bytes` | the message-bound inventory |
| `storage_bytes` | that bound, plus the board descriptor resolved for THIS image |
| `[types]` `max_fields` / `max_kinds` / `max_nested_depth` | codegen's per-type schema walk |
| `registration_path` | which subscribe spelling each node writes |

The refusals are correct — `Fact::stated()` is the only accessor that yields a
value, so a consumer cannot read one as a number. They are also not free.

## What the refusals cost, measured in phase-454

The count-derived knobs are real and small — uORB −1,512 B, cffi −280 B. The
numbers behind the refusals are not:

| wave | saving | class |
| --- | --- | --- |
| W6.b (XRCE) | −355,008 B (83 %) | payload + reliability |
| W6.a (zenoh) | −124,032 B (22 %) | payload |
| W12 (listener, end to end) | −101,504 B (37 %) | payload, via declared depth |
| issue 1340 | ~9,096 B **per subscription** | a region an in-place backend never claims |

Every one of those lands today on a single-package cargo leaf only.

## An obstacle that is not road-specific

Even on the road that HAS a bound inventory, **no service or action endpoint can
get a `wire_bound_bytes`**: `BoundInventory::record_message` runs for `.msg`
files only, so `pkg/srv/Name_Request` has no bound row. phase-454 W6.a found this
and correctly declined to fix it inside a backend wave.

So "give the other roads a bound inventory" is two things, and the first is owed
everywhere.


## phase-454 W9 widened this phase, and the widening is not more of the same

W9 applied the retirement test per fact and **kept 26 of 26 carriers**. Only four
are blocked on 1393. Nine entity counts and four queryable raw inputs are blocked
on **issue 1407**, which is a DIFFERENT axis and survives 1393's remedy untouched:

> *1393 is per-FIELD and its remedy is a bound inventory plus a board triple for a
> model image. Every mechanism here survives that remedy untouched: a richer set
> of FIELDS still comes from the poorer set of COMPONENTS, is still absent for a
> leaf with no model, and is still withheld in a multi-entry configure.*

So this phase has two axes, and finishing one leaves the carriers in place:

| axis | question | issue |
| --- | --- | --- |
| **fields** | what may a descriptor STATE? | 1393 |
| **coverage** | which images GET one, and composed from what? | 1407 |

**The root of the coverage axis, measured by W9**, is that the two producers do
not share an inventory. `nros ws entity-inventory` composes `nros-metadata.json`
WITH the model (`merged_per_kind_max`); `nros ws sizing-descriptor --from-model`
builds `EntityInventory::from_model` ALONE. A component
`nano_ros_node_register` put in the metadata that the contract does not describe
is `Declaration::Absent`, and `derive()` refuses for the whole image on exactly
that — **a refusal the descriptor's producer cannot reach, because it never sees
the metadata.**

That is why "retire the counts first" was wrong: it would swap a mechanism that
refuses on incomplete data for one that cannot tell the data is incomplete.

**W0 below comes first for that reason** — it is largely plumbing, it is what
1407 itself calls "the one worth doing first", and it unblocks thirteen carriers
without touching a bound.

Three of the nine counts could not be stated even with 1407 closed, each for its
own structural reason (timers and guard conditions dropped by `endpoint_kind`;
the schedule; per-component attribution). Those are ledgered separately and are
NOT in this phase's scope — do not quietly absorb them.

### W0 — one inventory behind both producers — **LANDED**

Give `--from-model` the composition the inventory verb already has: take
`--metadata` beside `--model` and run `merged_per_kind_max`, so the two producers
share one inventory **and one refusal**.

Acceptance: a component present in the metadata and absent from the contract
makes the model-written descriptor REFUSE, exactly as `entity-inventory` refuses
today — with a reproduction that fails first, since the current behaviour is to
state a number from a poorer set without noticing.

**The composition landed as specified. The acceptance as WRITTEN was wrong
against the code, and the correction matters more than the wave.** "A component
in the metadata and absent from the contract" does not refuse on EITHER producer
today, so there was no refusal to share:

* **issue 1402** (fixed after 1407 was filed) reclassifies exactly that shape to
  `Declaration::NotLaunched`, and `derive` deliberately does not filter on it —
  "no model row" is its proxy for "this image does not launch it". So once the
  model describes wiring, composing metadata in can no longer introduce a
  `derive()` refusal at all.
* **phase-454 W9** retired the `"entities"` key of `nros-metadata.json`, so every
  metadata row is `Declaration::Absent` to begin with.

What the poorer component set actually cost is an **UNDER-COUNT**, which is the
same mechanism in the direction that ships a failure: `max_nodes` IS
`components().len()`, so the model-only producer stated `[image] node_count = 1`
for an image whose configure registered three components — and a short
`NROS_EXECUTOR_MAX_NODES` is `NodeError::NodeTableFull` at boot. That is the
reproduction, and it was **watched failing** (`Stated(1)`, wanted `Some(3)`)
before the composition existed:
`cmd::sizing_descriptor::tests::a_component_the_contract_does_not_describe_still_counts_toward_the_node_table`.

The sharing is bound by
`the_descriptor_states_what_the_inventory_verb_derives`, which compares the
written descriptor against `merged_per_kind_max(...).derive()` over the same two
inputs — so whatever either producer derives from the component set is held
equal, without the test enumerating the fields. If 1402's proxy for "launched"
ever changes, that is where the refusal reappears on both roads at once.

### W0.b — a descriptor for a leaf with no model — **LANDED, carriers KEPT**

A standalone leaf declaring `[[component]] entities` in `system.toml` has no
model, so it can never have a model-written descriptor — and that is the road
issue 1378 measured failing. `facts_from_leaf` already reads exactly that
declaration through the same `EntityDecl` grammar.

Acceptance: such a leaf gets a descriptor, and its queryable carriers retire.

**The descriptor landed; the carriers did NOT retire, and that is the measured
answer rather than unfinished work.** `nros ws sizing-descriptor --from-leaf`
writes one from the same `EntityDecl` grammar, reached from
`nros_record_leaf_entity_facts`; `sizing_descriptor_leaf_road.rs` runs it over
both leaves issue 1378 was filed against and requires the descriptor's
`transient_local_publishers` to EQUAL the carrier's
`NROS_DECLARED_TL_PUBLISHERS` (and `node_count` to equal `NROS_DECLARED_NODES`).
Mutation-tested: stating the declaration as `None` reports `descriptor Absent,
carrier 1`.

Two roads still reach neither producer, so retiring would re-open 1378 on
exactly the images that describe themselves (phase-454 W9's lesson):

* a **multi-entry cmake configure** names no descriptor to cargo — and W0.c below
  re-affirmed that refusal as the PERMANENT answer, so this is not a gap waiting
  on a wave;
* a **standalone Zephyr leaf** reaches neither, because `nano_rosConfig.cmake`'s
  Zephyr arm returns before the leaf-facts call by design (the Kconfig derive
  sentinel is that road's own front-end, RFC-0049).

Both are recorded per row in `check-knob-single-reader`'s KEPT ledger against the
still-open 1407. One more finding worth keeping: `NROS_DECLARED_SERVICE_SERVERS`
is blocked by something else entirely — **no consumer reads that count off the
descriptor at all** (`queryable_floor_from` takes it from the carrier alone), so
the descriptor stating it buys nothing until a consumer ranks it first the way
`transient_local_publishers` already does.

Two structural notes for whoever extends this road. A `system.toml` with no
package manifest beside it is a workspace BRINGUP, not a leaf
(`leaf_system::is_package_dir`), so the producer is "a `system.toml` beside a
`CMakeLists.txt`". And it **stands down for a cargo leaf**, decided from the
`[package]` manifest rather than from whether a file is on disk: twelve leaves in
the tree carry both a `CMakeLists.txt` and a `[package]` manifest, and for those
the two producers can resolve one path — overwriting `nros sync`'s richer file
with one that refuses the whole payload class is an UNDER-statement.

### W0.c — per-entry descriptors in a multi-entry configure

Today the descriptor is withheld from cargo entirely when a configure has several
entries. 1407 notes this is "really a question about the shared staticlib rather
than about the descriptor" — so **answer that question before building
anything**, and record the answer.

## Design answers, recorded before building (2026-09-27)

W0.c and W2 each say to answer a question *before* building anything. Both are
answered here, from the code rather than from the plan.

### W0.c — keep the refusal; the condition does not occur

The question ("per-entry descriptors in a multi-entry configure") is already
decided in `cmake/NanoRosSizingDescriptor.cmake`, with the reason at the code:

> **EXACTLY ONE OR NONE.** A configure that declared several entries has several
> descriptors and one shared staticlib, and `NROS_SIZING_DESCRIPTOR` names a
> single file: handing cargo one of N would size the shared archive from one
> image and call it derived. The entity facts take a MAX across models for the
> same collision; a descriptor is a whole per-endpoint table and has no max, so
> this refuses instead and says so.

**Measured: no configure in this tree declares more than one entry.** All nine
`nano_ros_entry(` call sites are one per project — `examples/templates/{cpp-port-minimal-publisher,rclcpp-compat-smoke}`,
`examples/workspaces/{c,realtime-c,realtime-cpp}/src/zephyr_entry`,
`packages/testing/nros-tests/fixtures/{cmake_add_subdirectory_smoke,multi_pkg_workspace_cpp/src/demo_entry}`,
and `zephyr/` (the `multi_pkg_workspace_cpp` root match is a comment, not a
call). So the refusal path is unreachable by any in-tree build and costs no
image its knobs today.

**Decision: no work. Revisit when a multi-entry configure exists**, and the thing
to measure then is named: a conservative ENVELOPE over the entries' endpoint
tables (union the rows; where two entries state the same `(kind, type, name)`
at different depths, take the larger) is constructible and is the safe
direction — but it sizes every image in the configure for the worst case of all
of them, and whether that beats each image keeping crate defaults is empirical.
It cannot be measured without such a configure, so building it now would be
speculative generality for a shape nobody has.

### W2 — EXPORT, not re-derive; and both halves already exist

The question was whether the cmake entry's interface closure is re-derived or
exported from codegen. **Exported** — re-deriving is a second opinion about a
bound, issue 0196's class and the one this campaign kept finding. That is not a
new decision: the tree already made it, twice over.

* codegen emits a cmake projection beside the JSON —
  `rosidl_codegen::bounds::INVENTORY_CMAKE_NAME` (`nros_message_bounds.cmake`),
  the sibling of `INVENTORY_JSON_NAME` the leaf road reads;
* `cmake/NanoRosMessageBounds.cmake` already AGGREGATES those per-package
  fragments into an image-wide closure —
  `nros_message_bounds_register_fragment` appending to the
  `NROS_MESSAGE_BOUNDS_FRAGMENTS` global property, called by both generator
  lanes so there is one place to look, with refusal handling and a lifetime
  argument (a global property, not the cache, so a package removed from the
  closure cannot keep pricing a type nothing links).

**So W2 is not "decide and build a mechanism". The mechanism is there and the
descriptor producer has no input for it:** `git grep -n bounds
packages/cli/nros-cli-core/src/cmd/sizing_descriptor.rs` returns NOTHING, and
`write_for_model` passes `bounds: Vec::new()` with a `bounds_error` that carries
issue 1393's reason.

**Decision: give `--from-model` a bounds input fed from
`NROS_MESSAGE_BOUNDS_FRAGMENTS`, and read it through the existing composer.**
Do not add a second aggregator, and do not teach the descriptor producer to walk
types itself. The wave's acceptance is unchanged; what changes is that its first
step is a seam, not a design.

## Work items

### W1 — bounds for service and action member messages, on every road

`record_message` covers `.msg` only. A service's `_Request`/`_Response` and an
action's three pairs are messages with bounds; nothing records them.

Acceptance: a service endpoint on the LEAF road gets a `wire_bound_bytes`, and
zenoh's `ZPICO_SERVICE_BUFFER_SIZE` derives from it rather than keeping its
builtin. Measured delta on a named image.

### W2 — a per-image bound inventory and schema shape off the leaf road

The cmake entry knows its interface closure at configure time
(`nros_generate_interfaces`), which is the information codegen walks.

**The design question, and it is the one to answer first:** is that closure
re-derived, or exported from codegen? Re-deriving it is a *second opinion about
the bound* — issue 0196's class, and the class this campaign kept finding. Decide
with the reason recorded, not by whichever is easier to reach.

Acceptance: a cmake or Zephyr image with a contract derives the same
payload-class knobs a cargo leaf with the same contract derives.

### W3 — `registration_path`, settled once for both halves

Phase-454 W5 measured five rows, not the four issue 1319 assumed, and left the
over-stating row alone deliberately:

> *the Rust **generic** registration on the same backend does NOT reach that
> capability test and does claim `RX_BUF` — and an endpoint row cannot say which
> of the two an image writes.*

This wave gives the row a producer. Both the C/C++ question (does a given call
site pass `rx_size_bound<M>`?) and the Rust one (generic vs in-place) are the
same question: **which spelling does this endpoint's registration use**.

Acceptance: issue 1340's ~9 KiB/subscription is taken on an image that provably
registers in place, and NOT taken on one that registers generically — with a
reproduction that fails first in the second case, since taking it there is an
UNDER-size.

### W4 — `storage_bytes`, which needs the board per image

`[target]` already resolves per image on the leaf road. Decide whether the cmake
road can, or whether this field stays refused with a narrower reason.

Acceptance: either the field is stated on all three roads, or its refusal names
something more specific than 1393.

## Acceptance for the phase

Not "it builds". The measurable form, stated by issue 1393:

> a cmake or Zephyr image with a contract derives the same payload-class knobs a
> cargo leaf with the same contract derives, and `mem-report --baseline` shows a
> comparable delta on a named image.

Plus, on both axes:

* **zero refusals naming 1393 remain** in a descriptor written from a model, or
  each surviving one names a narrower, still-open reason; and
* **the thirteen carriers 1407 blocks retire** — the nine entity counts and the
  four queryable raw inputs — each registered in `check-knob-single-reader` with
  its single legitimate reader, per phase-454 W9's ledger.

W9's gate derives ledger completeness from `check-declared-fact-carriers.produced()`
and requires each row's issue to be open, so **closing 1393 or 1407 automatically
re-opens the question for exactly the carriers it blocked**. Use that rather than
re-auditing by hand.

## Carried rules, not to be re-decided

- **Refusal is per field and never a default.** D6, and it is what makes a
  partial descriptor safe to publish. Widening what is stated must not weaken it.
- **Demand is published unfloored; the floor lives at the consumer** (issues 1015
  + 1033).
- **A guess at a payload class is an UNDER-size**, the direction that ships
  `NodeError::BufferTooSmall`. Where a fact cannot be sourced, it stays refused.
- The descriptor stays **relative and self-contained** — the owner's ruling, and
  phase-454 W12's copy-out test is its executable form.
