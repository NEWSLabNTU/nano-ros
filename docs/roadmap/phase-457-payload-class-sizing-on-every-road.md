# phase-457 — payload-class sizing on every road, and the registration fact both halves need

**Status (2026-09-21). Queued, not started.** Successor to
[phase-454](phase-454-contract-states-facts-backends-derive.md), which
implemented [RFC-0100](../design/0100-rmw-agnostic-sizing-model.md) and named its
own ceiling. Closes [issue 1393](../issues/archived/1393-cmake-road-has-no-bound-inventory.md)
and [issue 1340](../issues/archived/1340-arena-budgets-a-receive-region-an-in-place-backend-never-claims.md),
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

**Superseded 2026-09-27.** phase-461 W3 landed the pricing (`record_service` /
`record_action`) before this phase started, so a service row on the leaf road
already states its bound. W1 verified that by measurement and fixed what the
paragraph above did not see: the member SET was wrong for an action and for
every `[types]` maximum — see W1 below and issue 1506.


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

### W1 — bounds for service and action member messages, on every road — LANDED 2026-09-27

**The premise above was stale, and what was actually missing was worse.**
`record_message` covering `.msg` only stopped being true when **phase-461 W3**
landed `record_service` / `record_action` and `sizing_descriptor::wire_type_of`.
Verified end to end rather than read off the plan: on
`examples/native/rust/service-server` a `service_server` row states
`wire_bound_bytes = 24` from `example_interfaces/srv/AddTwoInts_Request`, and
`nros-rmw-zenoh`'s `SERVICE_BUFFER_SIZE` / `SERVICE_INBOX_BYTES` compile to
**24** instead of the builtin **1,024**. Built twice (once with
`NROS_SERVICE_INBOX_BYTES=1024` to reproduce the pre-W3 state) and diffed:
static RAM **188,890 → 156,890 B (−32,000 B, −16.9 %)**, all of it
`nros_rmw_zenoh::shim::service::USER_SERVICE_INBOX` at 33,536 → 1,536 B. So the
stated acceptance was already met.

**What W1 actually fixed is [issue 1506](../issues/archived/1506-action-inbox-sized-from-send-goal-only.md):
two UNDER-sizes, both from pricing a shared pool over one member of its
population.**

1. An action row's `wire_bound_bytes` was `<A>_SendGoal_Request` alone, on both
   producers (the descriptor's `wire_type_of`, and the cmake carrier's
   `action_request_types()`). The three queryables share ONE ring, and
   `action_msgs/srv/CancelGoal_Request` is **44 B rx against SendGoal's 36** on
   the in-tree Fibonacci, because a `GoalInfo` outweighs a small goal struct — so
   `ACTION_INBOX_BYTES` was 8 B short and every cancel request would have landed
   as `TransportError::MessageTooLarge`. `rosidl_codegen::action_received_types`
   names the three; both producers max over the set and the descriptor REFUSES
   the whole row when one member is unpriced. Measured on
   `examples/native/rust/action-server`: `ACTION_INBOX_BYTES` 36 → 44, static
   RAM 172,698 → 172,794 B (**+96 B** — the fix COSTS bytes, which is the
   direction).
2. `[types]`'s three maxima joined the schema shape on the endpoint's INTERFACE
   name, so `max_fields` / `max_kinds` / `max_nested_depth` were refused on every
   service and action image — and the refusal told the reader to run `nros sync`,
   a remedy that could not work, because codegen emits no shape for
   `pkg/srv/Name`. They now join on `registered_types_of`: a service's two
   halves, an action's eight members plus the `action_msgs` protocol types
   `RosAction::register_protocol_types` registers. Those last three were not
   optional — measured, they carry the DEEPEST schemas an action image holds
   (`CancelGoal_Response` 11 kinds, `GoalStatusArray` nested_depth 6, against 7
   and 3 for the envelopes). Emitted knob delta on the same image, built twice
   and diffed:

   ```
   + NROS_CYCLONEDDS_MAX_FIELDS = "4"          (was: no row, consumer kept 64)
   + NROS_CYCLONEDDS_MAX_KINDS = "11"          (was: no row, consumer kept 256)
   + NROS_CYCLONEDDS_MAX_NESTED_DEPTH = "6"    (was: no row, consumer kept 8)
   ```

   `dynamic_type.rs`'s own arithmetic for the builder's name arrays is
   `MAX_FIELDS × 64 + MAX_KINDS × 64`: 20,480 B → 960 B of stack frame.

`record_action` also prices the three HALVES now (`_Goal`, `_Result`,
`_Feedback`) — **eight rows, not five** — because `RosAction` registers them with
the backend in their own right, so a shape join on them previously found nothing
about a descriptor the image holds.

**What W2 inherits.** Every spelling it needs is exported from
`rosidl_codegen` and has exactly one producer:
`service_member_types`, `action_member_types` (the eight suffixes, read by
`record_action` itself), `action_received_types` (the three requests),
`ACTION_CANCEL_SERVICE` and `ACTION_STATUS_TYPE`. The cmake road's carrier
already publishes the widened action list, so once `--from-model` gets the
bounds input W2 decided on, a model image joins the same sets a leaf does with
no second opinion about any of them.

**Not changed, deliberately:** `[types] distinct_count` still counts declared
interfaces, because the registered-type COUNT has its own producer
(`cyclonedds_type_sizing` → `NROS_CYCLONEDDS_MAX_TYPES`) and a second answer
here is what the single-writer rule beside `cyclonedds_env` refuses. The
divergence inside one section is recorded in a comment at `type_facts`.

### W2 — a per-image bound inventory and schema shape off the leaf road — LANDED 2026-09-29

**As built.** The decision recorded above (EXPORT, not re-derive) held, and its
first step was the seam it named: `--from-model` / `--from-leaf` take
`--bound-inventory <table>` (repeatable), and one cmake helper,
`_nros_sizing_bound_args`, passes every table the configure REGISTERED — the
JSON sibling of each `NROS_MESSAGE_BOUNDS_FRAGMENTS` entry, named by
`nros_message_bounds_files()`, the one function that owns both file names. Both
cmake producers call it; neither spells the loop.

ONE reader across all three model-road producers:
`leaf_payload_classes::bound_rows_from_tables` (read + parse, refuse the whole
set on a missing or malformed table) and `project_bound_rows` (the split into
bounds and schema shapes). The leaf road now calls the same two, through a
DISCOVERY split out as `generated_bound_tables(root)` — which the cargo
WORKSPACE road (`cmd/build.rs`) also uses, since a workspace keeps the same
`generated/<pkg>/` layout. So a type is priced by one reader on every road, and
the acceptance's parity is structural rather than a comparison two producers
happen to agree on.

**Measured on a real cmake image** — `examples/workspaces/cpp` `native`, via
`nros build`, `native_action_client_entry.toml`:

| field | before | after the second configure |
| --- | --- | --- |
| `wire_bound_bytes` (`/fibonacci`) | refused, 1393 | 44 |
| `[types] max_fields` | refused, 1393 | 4 |
| `[types] max_kinds` | refused, 1393 | 11 |
| `[types] max_nested_depth` | refused, 1393 | 6 |
| `registration_path` | refused | refused — needs an observed registration, not a table |

The lifecycle it was designed for, observed rather than assumed: the first
configure registered 5 tables and found 0 on disk (they are BUILD-time outputs
on this lane) and said so — `bounds REFUSED -- 0 of 5 registered bound table(s)
exist` — the first build produced all 5, and the next configure read them —
`bounds from 5 registered table(s)`. A registered table not yet on disk is a
per-field refusal NAMING its package, never an error, which is the aggregator's
own rule for the same list.

**Two things the work found.**

- **A refusal reason must not carry a path.** The first version named the
  missing table by absolute path and `write_for_model` refused to write the
  descriptor — issue 0320's rule that a descriptor be byte-identical across
  checkouts. The reader now names a table `<package>/<file>`, and so does its
  malformed arm, which had the same `path.display()`; asserted by
  `no_reader_message_names_the_checkout_path`.
- **The cargo-workspace wiring has no in-tree image to prove it on.** No cargo
  workspace image carries a contract: `realtime-rust`'s contract is in
  `derived_bringup`, whose only image is Zephyr, and `demo_bringup` has none —
  so the workspace road writes no descriptor for any shipping image ("no
  contract, no file"). The code it runs is the code the cmake image proves;
  the road itself is unexercised.

**Not run:** the Zephyr lane. Its codegen runs at CONFIGURE time, so its tables
should be read on the first configure; not measured here.

Tests: `bound_tables_turn_the_model_roads_payload_refusals_into_facts` (the real
committed `nros-std-msgs` table),
`a_registered_bound_table_not_yet_built_refuses_by_name`,
`both_roads_read_one_table_into_the_same_rows` (row equality, not counts),
`no_reader_message_names_the_checkout_path`; `nros-cli-core` 1640 passed.

The cmake entry knows its interface closure at configure time
(`nros_generate_interfaces`), which is the information codegen walks.

**The design question, and it is the one to answer first:** is that closure
re-derived, or exported from codegen? Re-deriving it is a *second opinion about
the bound* — issue 0196's class, and the class this campaign kept finding. Decide
with the reason recorded, not by whichever is easier to reach.

Acceptance: a cmake or Zephyr image with a contract derives the same
payload-class knobs a cargo leaf with the same contract derives.

### W3 — `registration_path`, settled once for both halves — LANDED 2026-09-28

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

**The premise was narrower than the defect.** The brief frames it as generic vs
in-place; measured against the executor, **nine of its eleven subscription entry
points cannot use an in-place dispatch** — the generic path, `.message_info()`,
`.safety()`, a borrowed view, and four of the five C/C++ entries — and until this
wave `registration_path` credited EVERY endpoint of a zenoh or XRCE image with
the in-place row, composed from the `rmw` name alone. The C/C++ half was also
already settled for the BOUND (phase-456 W7 made every nros-cpp registration site
state `rx_size_bound<M>`, gated), so what was open on both sides was the same
single thing: the in-place capability, per endpoint.

**Where the fact comes from: the PROBE, and it is an observation rather than a
derivation.** `SubscriptionRequest::in_place_capable` is stated at each entry
point and read at exactly one site — `Executor::open_subscription`, phase-456
W8's single prologue. That site now REPORTS it
(`nros_node::executor::registration_observer`, behind a feature only
`nros/metadata-mode` turns on), the recorder joins it onto the subscription row
the backend created in the same call
(`MetadataRecorder::observe_subscription_registration`), and the sidecar carries
it as `in_place` (schema v2 → v3, additive). What travels is the CALL SITE's half
alone and never the conjunction the executor computed: a probe links the
RECORDING backend, so reporting `SubscriptionOpen::in_place` would describe the
probe rather than the image. The consumer composes the two halves itself.

The three candidate sources, and why this one:

* the **contract** cannot state it — `system.contract.yaml` says what an image
  KEEPS, and which of eleven overloads its code calls is a property of the code;
* a **second opinion** derived in the CLI is what was already there and already
  wrong, and is issue 0196's class;
* the **probe** runs the code, so the one site that knows can simply say.

**The unlock nobody asked for, and it is what made the acceptance reachable.**
The MODEL road can now state the in-place row, which phase-454 W14 refused
outright. The in-place row needs the backend (a function of `rmw`, which that
road has) plus the endpoint's own observed answer; only the two BUFFERED rows
need the entry language, and those still refuse with the horizon's own prose. So
`write_for_model` supplies `backend_schema` / `backend_dispatch` and the
road-specific refusal moved INSIDE the composer, where the arm that runs out of
inputs is the one that reports. That matters because the arena's per-endpoint sum
runs only where all five `NROS_ENTITY_COUNT_*` arrive — the cmake/Zephyr road —
and that road's descriptor is written by this producer.

**The reproduction, watched failing.**
`executor::tests::a_generic_subscription_on_an_in_place_backend_still_claims_a_full_region`
is the runtime cost: on `MockSession::with_in_place_dispatch()` a typed
registration claims 2,304 bytes of arena and a GENERIC one 5,416, so an arena
priced at the first refuses the second with `NodeError::BufferTooSmall` — at a registration
`arena_oracle` passed. The descriptor-level pair is
`two_subscriptions_on_one_in_place_backend_take_different_paths` and
`an_unobserved_endpoint_refuses_the_path_rather_than_claiming_in_place`.
Mutation-tested both ways: restoring the pre-W3 arm (the in-place row from the
backend alone) reds 6 tests including both reproductions; making
`claims_no_receive_region` true for a REFUSED path reds the unobserved one.

**Measured, both directions, on `examples/native/rust/listener`.** The counts and
the descriptor delivered through the environment, which is how the cmake road
delivers both to cargo. One `KEEP_LAST(1)` subscription:

| the row says | `arena_model::REQUIRED` |
| --- | --- |
| observed in-place-capable | 3,072 |
| observed NOT capable (`unbounded`) | 6,144 |
| unobserved (refused) | 6,144 |

`ARENA_SIZE` does not move there: `FLOOR` is 8,192 and both numbers are below it,
which is how a one-subscription image absorbs the gap and why issue 1340 sat.
With a four-subscription `KEEP_LAST(10)` contract on the same leaf the floor
stops absorbing it:

| the row says | `ARENA_SIZE` | static RAM | `EXECUTOR_BACKING` |
| --- | --- | --- | --- |
| observed in-place-capable | 8,192 | **152,042** | 17,824 |
| observed NOT capable | 51,552 | 195,402 | 61,184 |
| unobserved (refused) | 51,552 | 195,402 | 61,184 |

**−43,360 bytes, 10,840 per subscription**, all of it one symbol; and
byte-identical in both directions where the saving must not be taken. That last
row is the acceptance's second half: an unobserved endpoint costs exactly what it
cost before the wave.

**End to end through the real toolchain**, not only through unit fixtures: the
C++ census fixture registers one borrowed-bytes and one `_with_info`
subscription through the real ABI and the sidecar carries `"in_place": true` and
`"in_place": false` on their own rows, with no key at all on the poll-style
subscription nothing registered. And `nros sync` over
`examples/templates/multi-node-workspace-cpp` writes `"in_place": true` for the
C++ listener's registration.

**What stays refused, and it is the honest half:**
[issue 1522](../issues/archived/1522-registration-path-unobserved-on-roads-whose-probe-does-not-register.md).
A **Rust** component's rows are unobserved, because `record_node_metadata::<C>`
runs `register()` against a recording `NodeContext` and opens no executor at all;
so are rows from the `ENTITIES` grammar, rows from a launch declaration, and
every service endpoint (`open_subscription` has no service sibling). Each is
priced at the buffering row — the number every image was built against — and the
issue records that closing the Rust half would state `unbounded`, not unlock the
saving, because both declarative arms answer `false` today. Taking it there is
issue 1340's own first candidate, which
`register_subscription_buffered_raw_on`'s `in_place_capable: false` already
writes down as deliberate and unfinished.

**No new `check-*` gate, deliberately.** The invariant is "only an OBSERVED row
may claim no region", and it is structural rather than textual: `in_place_capable`
is a non-defaulted struct field on `SubscriptionRequest`, so a twelfth entry point
cannot forget it and compile, and `open_subscription` is the only prologue that
reaches the capability. A script re-deriving "which shapes should be capable"
would be the second opinion this wave removed.

### W5 — the Rust probe states what it never registers — LANDED 2026-09-28

W3 left four populations unobserved and
[issue 1522](../issues/archived/1522-registration-path-unobserved-on-roads-whose-probe-does-not-register.md)
recorded them. This closes the first: a **Rust** component's subscription rows.

The route is the issue's second one — **one classifier**, not "teach the probe
to register". It is available here and nowhere else in the four because the
Rust declarative registrar is ONE function, `node_runtime`'s
`EntityKind::Subscription` arm, lowering to one of two entry points; the shape
is therefore a function of the DECLARATION, computed in one place.
`nros_node::executor::declared_shape::DeclaredSubscriptionShape` is that
function's codomain, and `MetadataRecorder::create_entity` — the one
`NodeRuntime` seam a Rust declaration crosses — states `in_place_capable` from
it. The C/C++ adapters reach the recorder through `push_entity`, so W3's
observation still wins on their road and this cannot overwrite it.

**W3 said "no new gate, deliberately", and this does not contradict it.** W3's
argument was that a script re-deriving *which shapes should be capable* would
be the second opinion the wave removed. `check-declared-subscription-shape`
re-derives nothing of the sort; it holds that the ONE derivation is spelled
once — the registrar branches on the classifier rather than on
`metadata.safety`, the `safety-e2e` mask has a single home, and each entry
point's `in_place_capable` is `DeclaredSubscriptionShape::<V>.in_place_capable()`
rather than a literal. That last clause is what keeps the classifier from being
a COPY of the entry point's answer, and it is why issue 1340's candidate is now
a one-line change that moves the executor and the probe together. Every
expectation is harvested from the enum's own variants and doc comments.

**Measured, both directions.** As issue 1522 predicted, this buys correctness
and not bytes: both declarative arms answer `false`, so a Rust subscription
moves from REFUSED to a stated `unbounded` row — the same price, since
`claims_no_receive_region` is false for both and `may_claim_closure_buffer` is
true for both. On `examples/native/rust/listener`, the only in-tree leaf with a
`system.contract.yaml` on a cargo road:

| | before | after |
| --- | --- | --- |
| RAM (`.bss` + `.data`), by section | 181,434 | 181,434 |
| RAM attributed to symbols | 157,554 | 157,554 |

No image changed size. An image on a backend that does not dispatch in place
never reaches the test, and an image with no contract has no descriptor, so
those two populations cannot move by construction.

**A defect W3 left, found by emitting the key from a second road.**
`SourceSubscriber` in `nros-cli-core/src/orchestration/source_metadata.rs` is
`deny_unknown_fields` and never declared `in_place`;
`metadata_refresh::stamp_provenance` parses EVERY producer's sidecar through
those structs, so the key fails the whole document rather than being ignored.
Invisible for a fortnight because no producer that reaches that reader emitted
it. Second occurrence of the class (issue 0518's `period_us` is the first), so
it is gated: `check-sidecar-endpoint-keys`.

**What stays refused**, and it is still the honest half: rows from the
`ENTITIES` grammar and from a launch declaration — neither has a registrar to
be consistent WITH, so a classifier there would be the parallel table issue
1522 forbids — and every service / action endpoint, which is 1522's second
piece and waits on phase-454 W6.b's pricing reading a service row at all.

### W4 — `storage_bytes`, which needs the board per image

`[target]` already resolves per image on the leaf road. Decide whether the cmake
road can, or whether this field stays refused with a narrower reason.

Acceptance: either the field is stated on all three roads, or its refusal names
something more specific than 1393.

### W4 — answered: the refusal SPLITS, and two thirds of it gets narrower than 1393

W4 asks whether the cmake road can resolve `[target]` per image, or whether
`storage_bytes` stays refused with a narrower reason. **Both, in different parts** —
the field is one name over three independent inputs, and they have three
different answers. Measured against the code, 2026-09-28:

| input | cmake road | reason |
| --- | --- | --- |
| `pointer_bytes`, `max_align` | **CAN be stated** | the triple is resolvable in a cmake configure — `_nros_resolve_rust_target()` in `NanoRosCodegenCore.cmake` — and `abi_for_triple` maps it. This is the "small and independent" plumbing issue 1393 predicted. |
| `heap_budget_bytes` | **stays refused, narrower** | the CLI seam already exists (`--from-model` takes `[board.knobs.memory] heap_bytes` "when the caller knows it"), but **nothing on the cmake road resolves a board heap knob at all**: `heap_bytes` and `knobs.memory` appear nowhere in `cmake/` or `zephyr/cmake/`. The blocker is a missing board-knob resolution on that road, NOT the bound inventory of 1393. |
| `storage_bytes` | **stays refused, narrower** | it needs `pointer_bytes` (this wave) **and** `wire_bound_bytes` (W2). Once the triple is plumbed it refuses on the BOUND, which is W2's own issue, rather than on `[target]`. |

`NanoRosSizingDescriptor.cmake` already documents the first row against itself:

> It does not pass a target triple. A cross cmake entry therefore gets a REFUSED
> `[target]`, naming the board rule (RFC-0100 D1) …

So the gap was known and explained; W4's contribution is deciding to close the
part that can be closed, and giving the other two parts reasons a reader can act
on.

**Decision.** Plumb the triple through `nros_sizing_descriptor_from_model()` so a
CROSS cmake entry states `pointer_bytes` and `max_align`. Leave
`heap_budget_bytes` refused, with a reason naming the missing board-knob
resolution on that road and NOT issue 1393. Leave `storage_bytes` refused, with a
reason naming W2's bound. The phase's "zero refusals naming 1393" criterion is
then met for `[target]` by making the two surviving refusals name something
truer — which is what that criterion asks for.

**Superseded 2026-10-01 (issue 1393's closure).** With W2's tables and this
wave's triple both landed, `storage_bytes` no longer needs a road-specific
refusal at all: the horizon's short-circuit is deleted and the leaf road's own
chain runs on every road, refusing on whichever input is actually missing. The
model road states the region wherever a depth is declared and is asserted to
equal the leaf road's (`bound_tables_turn_the_model_roads_payload_refusals_into_facts`).
Zero refusals name 1393.

**Do NOT** read the width in the CLI as a fallback for a cross entry. A build
script's `size_of` answers for the HOST (phase-118-E), and `target_facts` already
refuses rather than guesses for exactly that reason: a guessed pointer width
under-sizes a ring's length array.

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
