# Phase 412 — every count and size the image can state about itself

**Status (2026-09-02). Opened from the mr-canhubk344 island audit.** phase-403
proved an image can derive its own buffer sizes; phase-409 proved the same for
the `Executor` value. This phase is the campaign that finishes the job: no
hand-picked count or size in a board `.conf` that the image already knows.

## Why a campaign rather than another knob

The island's board `.conf` was, until this week, twenty-odd numbers a person
chose. Every one of them is a claim about the image, and every one can be wrong
in two directions. Too small halts the board; too large costs RAM on a part
that has 320 KiB. Neither failure names the knob that caused it, and none of the
numbers carries the reasoning that produced it.

Four of them now derive (phase-403 W8/W9). The audit that followed found the
rest split cleanly into two groups, and the split is the point of this phase:

* **The number is already computed and published, and nothing consumes it.**
  Six knobs. No new arithmetic — a consumer and a precedence entry each.
* **No derivation exists yet.** Six more, each blocked on something specific
  and nameable.

The first group is the recurring defect this repo keeps producing: a mechanism
that is correct, tested, and unreachable from a real build. `rx_buffer_hint`
sized nothing (0896). The bound inventory had no reader (0963). A cap could not
reach codegen (#152). The entity inventory publishes eight per-kind counts and
exactly one of them becomes a knob (this phase). Each was found by an audit, not
by a gate, which is why W4 below exists.

## Where the numbers already are

`nros_entity_inventory_knobs_file` publishes, per configure, for the island:

```
NROS_ENTITY_INVENTORY_COMPONENT_COUNT 4
NROS_ENTITY_INVENTORY_ENTITY_TOTAL   28
NROS_ENTITY_COUNT_SUBSCRIPTION       10
NROS_ENTITY_COUNT_PUBLISHER          14
NROS_ENTITY_COUNT_TIMER               4
NROS_ENTITY_COUNT_SERVICE_SERVER      0
NROS_ENTITY_COUNT_SERVICE_CLIENT      0
NROS_ENTITY_COUNT_ACTION_SERVER       0
NROS_ENTITY_COUNT_ACTION_CLIENT       0
NROS_ENTITY_COUNT_GUARD_CONDITION     0
```

Of these, one knob is derived: `NROS_DERIVED_EXECUTOR_MAX_CBS` (14 — the
subscriptions, timers and service servers that claim a callback slot;
publishers claim none).

## W1 — wire the six. LANDED 2026-09-03, five of six

Each is a `_nros_entity_publish` of a value that is already in the file, plus
W8's precedence ladder (env > Kconfig/board > derived > crate default) and its
refusal rule (derive nothing when the inventory's own status is not `derived`).

| knob | island hand-set | published input | derives to |
| --- | ---: | --- | ---: |
| `NROS_MAX_SUBSCRIBERS` | 12 | `COUNT_SUBSCRIPTION` | 10 |
| `NROS_RMW_SUBSCRIBER_SLOTS` | 12 | `COUNT_SUBSCRIPTION` | 10 |
| `NROS_MAX_PUBLISHERS` | 16 | `COUNT_PUBLISHER` | 14 |
| `NROS_MAX_QUERYABLES` | 4 | `COUNT_SERVICE_SERVER` | 0 |
| `NROS_EXECUTOR_MAX_NODES` | 6 | `INVENTORY_COMPONENT_COUNT` | 4 |
| `NROS_EXECUTOR_ACTION_CLIENTS` | 0 | `COUNT_ACTION_CLIENT` | 0 |

**Both blocking questions, answered by reading the code.**

*Do the session pools need shim headroom?* No addend, verified rather than
assumed: `ZenohSubscriber::new` has exactly ONE caller (`create_subscription`),
and the two things that looked like they might share the pool do not -- the
graph cache lives in its own `graph_cache.sub` field, liveliness tokens in
`liveliness[ZPICO_MAX_LIVELINESS]`.

But the RAW per-kind count was still the wrong input, which is what made the
question worth blocking on. A declared ACTION is one entity that costs several
session slots:

    action server -> 3 queryables + 2 publishers
    action client -> 1 subscription

Wiring `MAX_QUERYABLES = COUNT_SERVICE_SERVER` would have under-sized every
image with an action. The multipliers now live beside the calls that decide
them (`ACTION_SERVER_QUERYABLES`, and new here `ACTION_SERVER_PUBLISHERS`,
`ACTION_CLIENT_SUBSCRIPTIONS`), held there by `check-infra-queryable-counts`.

Deliberately NOT counted: `PARAM_SERVICE_QUERYABLES` (6) and
`LIFECYCLE_SERVICE_QUERYABLES` (5). A feature enables those and the inventory
cannot see it, so counting them would guess. An image carrying either states
the knob, which is what "the derived value is a DEFAULT" is for.

*Is a component always a node?* One `ComponentNode` is one `Node::create` is
one name, and the executor keys node slots by NAME -- "a repeated name must
reuse its record". But `nros_create_node_on` (the bridge) creates TWO nodes per
bridge, OUTSIDE the component model, so `COMPONENT_COUNT` is a lower bound.

**So `NROS_EXECUTOR_MAX_NODES` is NOT wired.** Under-counting halts the board,
phase-403's rule is refuse rather than under-derive, and the island would save
6 -> 4. It moves to W2 with the bridge as its blocker.

**Measured on the island**, both configures (issue 0991):

| knob | hand-set | derived |
| --- | ---: | ---: |
| `NROS_MAX_SUBSCRIBERS` | 12 | 10 |
| `NROS_RMW_SUBSCRIBER_SLOTS` | 12 | 10 |
| `NROS_MAX_PUBLISHERS` | 16 | 14 |
| `NROS_MAX_QUERYABLES` | 4 | 0 |

    RAM   324834 (99.13%) -> 312088 (95.24%)   -12746 B
    DTCM   93744 (71.52%) ->  89320 (68.15%)    -4424 B

17170 bytes, and RAM headroom goes from 0.87% to 4.76%.

## THE RULE W1 COST, and it binds the rest of this phase

**The `-1` DERIVE sentinel is safe only where the consumer supplies its own
default.** `_nros_resolve_derivable_knob`'s rung 4 deliberately leaves a knob
UNRESOLVED so the reading build script falls to its own literal
(`env_usize("ZPICO_MAX_SUBSCRIBERS", 8)`) -- "the one place that literal is
written". A C compile definition has no such literal. An unresolved knob
expands to nothing, `-DZPICO_MAX_SUBSCRIBERS=` reaches the compiler, and
`zpico.c` reports `flexible array member not at end of struct` on a struct
nobody edited.

It only appears once a knob GAINS the sentinel, because before that Kconfig
always carried a number. Every knob W2 converts must therefore be checked for a
consumer with no default of its own, and that check belongs before the switch,
not after.

## THREE DELIVERY FAILURES, and what they say about W4

W1's derived values were RIGHT at every step -- 10, 10, 14, 0 sat correctly in
the fragment throughout. All three failures were in DELIVERY, and every gate
stayed green through all three, because the gates check the inventory and the
resolver and all three failures were downstream of both:

1. **A second consumer.** The zpico C defines read raw `CONFIG_*`, bypassing the
   resolver, and ran BEFORE it. Symptom: `size of array 'subscribers' is
   negative`.
2. **A name that resolved to empty.** A `foreach` building
   `NROS_DERIVED_${_pool}` produced `NROS_DERIVED_NROS_MAX_SUBSCRIBERS`, which
   names nothing; CMake yields EMPTY for an unknown name rather than failing.
3. **A consumer with no default**, the rule above.

W4 as first written -- "every published symbol has a consumer" -- would have
caught NONE of them. The symbol had a consumer in all three cases; a different
consumer was reading around it, or the name never matched, or the value was
legitimately absent. **The gate that catches all three asserts, per knob, that
the value reaching the COMPILE equals the value the resolver produced.** That is
the form W4 should take, and it is now backed by three instances rather than by
the argument that opened this phase.

## W2 — the ones with no derivation, and what each is blocked on

Named here so nobody re-audits them, with the blocker rather than a shrug.

| knob | island | blocked on |
| --- | ---: | --- |
| `NROS_EXECUTOR_ARENA_SIZE` | 40960 | **phase-403 step 3**, which is blocked on step 2 (QoS depth). Depth MULTIPLIES the bound — 86108 B at depth 10 against 24516 at depth 1 for the same ten subscriptions — and the entity record carries `kind`, `type_name`, `name` and no depth. Defaulting is the worst option: ROS's default is 10, so assuming it inflates tenfold and assuming 1 under-sizes in the unsafe direction |
| `CONFIG_MAIN_STACK_SIZE` | 16384 | needs frame analysis, not an inventory. **phase-409** established the method (`objdump`, summing every `sub`/`sub.w`/`subw sp`) and the numbers for one call chain; nothing turns that into a knob |
| `NROS_ZEPHYR_HEAP_SIZE` | 94208 | runtime allocation. No static model, and the honest first step is a high-water reporter, not a derivation |
| `NROS_GRAPH_CACHE_SIZE` | 4096 | sized by the PEER graph. Not a property of this image and probably never derivable from it |
| `NROS_MAX_LIVELINESS` | 32 | same — remote peers |
| `NROS_EXECUTOR_MAX_NODES` | 6 | the BRIDGE creates two nodes per bridge outside the component model, so `COMPONENT_COUNT` is a lower bound. Needs the bridge to declare, or the derivation to refuse when one is present |
| `NROS_ZEPHYR_TASK_SLOTS`, `..._TASK_STACK_SIZE` | 5, 8192 | transport tasks, not entities. Derivable in principle from the transport's own declaration; nothing declares it today |

`NROS_SUBSCRIBER_LARGE_SIZE` is a seventh case and a different one: it is
DELIBERATELY not derived (see `NanoRosMessageBounds`), and on the island it now
sizes nothing because `MAX_LARGE_SUBSCRIBERS` derives to 0. A number that sizes
nothing should be deleted from a `.conf`, not derived.

## W3 — the arena, once phase-403 step 2 lands

The arena is the single largest hand-set number on the island (40960 B) and the
last big one. It is listed here rather than duplicated: **phase-403 owns steps 2
and 3**, and this phase consumes them. When depth reaches the entity record,
W3 is the consumer that turns it into `NROS_DERIVED_EXECUTOR_ARENA_SIZE`.

Ordering is not a preference. An under-sized arena halts during entity creation,
BEFORE the first spin, so 0900's advisory never prints — the failure cannot
report itself. `MAX_CBS` was the right first consumer for the mirror-image
reason: it fails at registration with `ExecutorFull`, which names the knob.

## W4 — a gate that fails when a published number has no consumer

The reason this phase exists is that eight counts were published and one was
read, and no test noticed for a month. The gate asserts the inverse of what the
current ones assert: not "is the derived value right" — they already check that
— but "is every published input READ by something".

Concretely: enumerate `NROS_ENTITY_COUNT_*` and `NROS_DERIVED_*` from a fixture
configure, and fail on a published symbol that no `.cmake` consumes. A new count
then arrives with either a consumer or a deliberate exemption naming why.

This is the "correct but unreachable" gate proposed during phase-403 and never
built. Four instances have now been found by hand (0896, 0963, #152, and this
one). It should stop being an audit.

## Acceptance

1. The island board `.conf` states no NROS count or size that W1 covers, and the
   image builds and boots with every one of them derived.
2. Each W1 knob's derived value is READ FROM THE BUILD and compared against the
   hand-set number it replaces — the value, never the exit code. A cap or a join
   that silently does nothing has misled this campaign twice already.
3. W2's table is in the board `.conf` as a comment, so the next person reads the
   blocker instead of re-deriving the audit.
4. W4's gate fails on a deliberately unread published symbol, proving it can.

## Known: the first configure derives the wrong basis

`docs/issues/0991`. On a CLEAN build dir the payload classes derive over the
linked closure, because W9's producer runs later in the configure than W8's
reader. On the island that over-approximation overflows RAM by 103160 bytes at
LINK. Every measurement in this phase must therefore be taken from a build that
has configured at least twice, and W1's acceptance test must state which.

Anything in this phase that adds a knob derived from the entity inventory
inherits that lag. It is worth fixing before W1 multiplies it by six.

## Issues homed here (survey 2026-09-03)
Every open issue was checked for a home phase; these had none, or were
mentioned here only in passing. A mention is not an owner — an issue with
no work item is an issue nobody is accountable for, which is the same shape
as a gate sitting in a lane no CI job runs. Each row is a work item: the issue
holds the evidence, the item is *close it*.

| issue | why it belongs here |
| --- | --- |
| [#0991](../issues/0991-a-clean-build-of-an-entity-declaring-image-does-not-link.md) | a clean build of an entity-declaring image derives the WRONG payload basis and does not link |

