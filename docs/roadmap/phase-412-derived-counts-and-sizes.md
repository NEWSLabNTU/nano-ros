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

## W1 — wire the six

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

**Two things to settle before wiring, not after.**

`NROS_MAX_SUBSCRIBERS` and `NROS_MAX_QUERYABLES` are ZENOH SESSION limits, not
executor ones. A session may declare entities the image never registers as
callbacks — liveliness tokens, an internal queryable for the graph. Read the
zenoh shim and count what it declares before equating the knob to
`COUNT_SUBSCRIPTION`; if the shim adds any, the derivation is `count + shim`
and the shim's contribution must come from the shim rather than from a constant
written here. Getting this wrong under-counts, which halts the board.

`NROS_EXECUTOR_MAX_NODES` is components, and a component is not always a node:
a multi-node component would break the equality. Check `nros_components` before
assuming `COMPONENT_COUNT` is the answer.

**Derived values carry NO headroom, deliberately** (phase-403's rule). Exact
demand makes the running image a checker of its own declaration: register past
the table and `NodeError::ExecutorFull` names the knob. That property is only
worth having if the count is right, which is why the two questions above are
blocking rather than advisory.

## W2 — the ones with no derivation, and what each is blocked on

Named here so nobody re-audits them, with the blocker rather than a shrug.

| knob | island | blocked on |
| --- | ---: | --- |
| `NROS_EXECUTOR_ARENA_SIZE` | 40960 | **phase-403 step 3**, which is blocked on step 2 (QoS depth). Depth MULTIPLIES the bound — 86108 B at depth 10 against 24516 at depth 1 for the same ten subscriptions — and the entity record carries `kind`, `type_name`, `name` and no depth. Defaulting is the worst option: ROS's default is 10, so assuming it inflates tenfold and assuming 1 under-sizes in the unsafe direction |
| `CONFIG_MAIN_STACK_SIZE` | 16384 | needs frame analysis, not an inventory. **phase-409** established the method (`objdump`, summing every `sub`/`sub.w`/`subw sp`) and the numbers for one call chain; nothing turns that into a knob |
| `NROS_ZEPHYR_HEAP_SIZE` | 94208 | runtime allocation. No static model, and the honest first step is a high-water reporter, not a derivation |
| `NROS_GRAPH_CACHE_SIZE` | 4096 | sized by the PEER graph. Not a property of this image and probably never derivable from it |
| `NROS_MAX_LIVELINESS` | 32 | same — remote peers |
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
