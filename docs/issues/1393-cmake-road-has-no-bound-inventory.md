---
id: 1393
title: "The model-only sizing descriptor refuses five fields because a workspace
  image and a cmake entry reach no bound inventory, no per-type schema walk and
  no per-call-site registration spelling — the counts and the QoS travel, the
  payload sizes do not"
status: open
type: tech-debt
area: [build, core]
severity: medium
found: 2026-09-20
related: [1199, 1319, 0460, 1115]
---

## What is open

phase-454 W14 gave the sizing descriptor (RFC-0100 D4) a **second producer**:
`nros_cli_core::sizing_descriptor::write_for_model`, reached from the workspace
cargo road (`cmd::build`, stage 4) and from the cmake road
(`nano_ros_entry()` → `nros ws sizing-descriptor --from-model`). Before it, W11's
measurement stood: *"the descriptor reaches one road of three"*, and every
RFC-0100 D5 derivation was inert on the other two.

That producer has ONE input — the resolved SystemModel — so it states what the
contract declares and **refuses five fields by name**, each refusal naming this
issue:

| field | what it needs | who has it today |
| --- | --- | --- |
| `[[endpoint]] wire_bound_bytes` | the message-bound inventory (`nros_message_bounds.json`) | codegen, beside a LEAF |
| `[[endpoint]] storage_bytes` | that bound, plus the board descriptor resolved for THIS image | the leaf road's `write_for_leaf` |
| `[types] max_fields` | codegen's per-type schema walk (`schema_value::schema_shape_for`) | the same table |
| `[types] max_kinds` | ditto | ditto |
| `[types] max_nested_depth` | ditto | ditto |
| `[[endpoint]] registration_path` | which subscribe spelling each node writes | nobody, on a multi-package image |

The refusals are correct and they are not free. What each costs, measured on a
four-endpoint contract (2 pub, 2 sub, `std_msgs/msg/Int32`, all four QoS
policies stated) — see "Measured" below.

## Why it is a refusal and not a default

RFC-0100 D6. `Fact::stated()` is the only accessor that yields a value, so a
consumer cannot read a refusal as a number; the fallback is the consumer's own
literal, always the safe direction and always loud. That property is what makes
a PARTIAL descriptor safe to publish at all, and it is asserted directly by
`nros-sizing-descriptor`'s `a_refusal_yields_no_value_by_any_accessor`.

Inventing a bound here would be the opposite: a payload class is a per-TYPE
number, and a guess is an under-size in the direction that ships
`NodeError::BufferTooSmall`.

## Measured — what the refusals cost, per consumer

Each consumer built twice (no descriptor / model-only descriptor) and its
emitted knobs diffed. Every field the refusals gate keeps its pre-wave value:

| consumer | knob the refusal gates | stays at |
| --- | --- | --- |
| `nros-rmw-zenoh` | `SERVICE_BUFFERS`' slot size (`wire_bound_bytes`) | `ZPICO_SERVICE_BUFFER_SIZE`'s default |
| `nros-rmw-xrce-cffi` | the SUBSCRIBER family's buffer + ring (`wire_bound_bytes`, refused with `depth` deliberately coupled) | the header's defaults, with a `cargo::warning` naming this issue |
| `nros-node` | each subscription's receive slot (`registration_path`) | the CLOSURE buffer — a provable upper bound over all five paths, so the arena OVER-states rather than under-sizing |
| Cyclone (`[env]` projection) | `MAX_FIELDS` / `MAX_KINDS` / `MAX_NESTED_DEPTH` | `dynamic_type.rs`'s `option_env!` literals (64 / 256 / 8) |

Cyclone's is the one with no worst-case argument behind it: the three literals
are pre-existing defaults, not bounds derived from anything, so an image whose
largest schema exceeds 64 fields is under-sized by the fallback exactly as it
was before the descriptor existed. That is not a regression and it is the
weakest of the four.

## What CLOSING it looks like

Three independent pieces, in rough order of value:

1. **A bound inventory for a model image.** The leaf road reads
   `generated/**/nros_message_bounds.json`, which codegen writes per LEAF. A
   workspace or cmake image knows its subscribed TYPE SET (it is in the
   descriptor's own `[[endpoint]]` rows) and the msg packages are resolvable;
   what is missing is a producer that prices that set without a leaf. Closing
   this one alone fills `wire_bound_bytes`, `[types]`'s three maxima, and — with
   `[target]` below — `storage_bytes`.
2. **`[target]` on the cmake road.** `nros_sizing_descriptor_from_model()`
   passes `--host-build` when the configure is not cross-compiling and no
   triple otherwise, so a CROSS cmake entry refuses `pointer_bytes` and
   `max_align`. The board descriptor is resolved elsewhere in the same
   configure; plumbing its triple and its `[board.knobs.memory] heap_bytes`
   through is small and independent of (1). The workspace cargo road already
   states both.
3. **`registration_path` for a multi-package image.** The leaf road composes it
   from the entry's LANGUAGE and the backend's two capabilities. A model image
   is several packages, so "the entry's language" has no single answer —
   W10's per-call-site declared-QoS machinery is where a real answer would come
   from, per endpoint rather than per image.

## Two smaller things this wave uncovered and did not fix

* **`NROS_ENTITY_COUNT_*` reaches the CMAKE road only.** It is produced by
  `cmake/NanoRosEntityInventory.cmake` and forwarded by
  `zephyr/cmake/nros_cargo_build.cmake`; nothing on the workspace CARGO road
  emits it. `nros-node`'s per-endpoint arena needs all five, so on that road the
  arena cannot consult the descriptor at all — measured: byte-identical with and
  without one. Not a defect this wave introduced, but it is why the arena saving
  shows up on the cmake road and not the cargo one.
* **`NROS_CYCLONEDDS_HEAP_BUDGET_BYTES` is written and never read.**
  `WrittenDescriptor::cyclonedds_env` emits it as a cargo `[env]` row, and
  `nros-rmw-cyclonedds-sys/build.rs`'s `KNOBS` list does not include it — so
  `heap_budget.hpp`'s `kHeapBudgetStated` is always false on the cargo road and
  D11's boot assertion is inert there.

## Do not

Do not "fill in a default" for any of the five. The whole point of the second
producer is that a partial descriptor is honest; a descriptor that guessed would
be the silent-default shape RFC-0100 exists to remove, and it would be worse
than the no-descriptor state it replaced, because a consumer that sees a stated
number stops printing the line that would have told a user to declare.

## It also gates phase-454 W9 — for FOUR of the 26 carriers, not all of them

Retirement removes the `NROS_DECLARED_*` / `NROS_DERIVED_*` carriers. While a
payload-class fact is refused on two roads of three, those carriers are still the
only road delivering it there — so retiring them would remove a working mechanism
in favour of one that cannot state the fact. That is
`check-knob-single-reader`'s own rule inverted: a mechanism that still resolves
is a mechanism people still use, and the converse bites just as hard.

**W9 ran that test per fact and found this issue blocks four carriers**
(`SUBSCRIBER_BUFFER_SIZE`, `SUBSCRIPTION_BUFFER_SIZE`, `LARGE_SUBSCRIBERS`,
`SUBSCRIBER_LARGE_SIZE`, plus `NROS_SUBSCRIBED_TYPE_BOUNDS` off the same
inventory) **— and that the other 22 are blocked by something else.** Closing
this issue therefore does NOT unblock the retirement:

* the entity counts and the queryable raw inputs are
  [issue 1407](1407-cmake-road-descriptor-coverage-narrower-than-its-carriers.md)
  — the model-only producer reads a POORER inventory than the carriers' verb
  does, and a standalone leaf has no model to read at all. Neither mechanism is
  touched by filling in a bound;
* the nine parameter-store carriers are
  [issue 1408](1408-sizing-descriptor-has-no-parameter-store-section.md) — the
  D4 schema has no section that could hold them, so they are not refused here,
  they are unspellable.

The full per-fact ledger is the `KEPT` registry in
`scripts/check/check-knob-single-reader.py`, and it is enforced: a carrier with
no row fails, and a row whose blocking issue is no longer `status: open` fails,
so closing this one re-opens the retirement question for exactly its four.
