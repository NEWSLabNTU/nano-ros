---
id: 965
title: "Nothing states which entities an image creates, so the arena, MAX_CBS and the payload classes stay hand-set"
status: open
area: codegen, memory, build
severity: high
related: [0900, 0939, 0940, 0941, phase-403, phase-403-W4, phase-403-W5]
---

# The bound inventory prices a type; nothing says which types an image receives

## The distinction that blocks three consumers

phase-403 W6 exports every type's derived bound and W8 gave it a reader. That
answers "how big is `nav_msgs/Odometry`". It cannot answer "does this image
subscribe to it", and three consumers need the second question:

| consumer | needs | status at filing | status 2026-09-03 |
| --- | --- | --- | --- |
| zenoh payload classes | the sizes actually RECEIVED | derived over the linked closure — an over-approximation | **DERIVED.** `NROS_MESSAGE_BOUNDS_BASIS subscribed` joins against `NROS_ENTITY_SUBSCRIBED_TYPES` (`cmake/NanoRosMessageBounds.cmake:526-566`, refusal at `:754-768`) — landed `36622cadd`, phase-403 |
| `NROS_EXECUTOR_MAX_CBS` | total handle count | hand-counted; a grep gave 17 when the answer was 33 | **DERIVED** via `NROS_DERIVED_EXECUTOR_MAX_CBS` (`cmake/NanoRosEntityInventory.cmake:240`) — landed `afeab01d5`. See the count correction below: 33 was the ENTITY count; 14 are publishers claiming no slot, so the slot demand is **19** |
| `NROS_EXECUTOR_ARENA_SIZE` | entities that will occupy slots | six bisections against a board | **STILL A FORMULA.** 0900 narrowed the worst case (`NROS_DERIVED_EXECUTOR_ACTION_CLIENTS`), but the arena needs the per-subscription `(depth+1)*bound` and the entity record carries no QoS depth (`cmake/NanoRosEntityInventory.cmake:52-58`). Owned by phase-403 steps 2 and 3, in flight |

**Two of the three are done.** This table said otherwise for three days while
`changelog.d/0965.feat.md` sat in the tree recording the opposite — a
HIGH-severity issue whose text overstates what is broken misdirects as surely as
one that understates it. The issue stays open for the arena alone.

## Measured cost of the missing half

On the island entry (11 packages, 84 types, 10 subscriptions), deriving the
payload classes over what is LINKED rather than what is RECEIVED:

| basis | SMALL | LARGE | total |
| --- | ---: | ---: | ---: |
| hand-set today | 49152 | 20480 | 69632 |
| derived over the linked closure | 71808 | 0 | 71808 |
| derived over the SUBSCRIBED types | 42240 | 0 | 42240 |

Deriving from the closure COSTS 2176 bytes; deriving from the subscribed set
SAVES 27392. The difference is one `std_msgs/Float64MultiArray`, linked and
never received, whose 1496-byte bound sets the small class for every
subscription in the image.

So W8's reader is correct and currently unprofitable, and it stays that way
until something states the subscribed set.

## Why it is not already available

* `entity_facts.rs::describes_wiring` is the natural hook and abstains on all
  115 resolved SystemModels -- none carries topic wiring.
* RFC-0043 C++ components register their entities in CONSTRUCTORS, at runtime.
  There is no build-time artifact naming them.
* A package's type count is not an image's entity count, and deriving one from
  the other would produce exactly the confident-and-wrong number this campaign
  exists to remove.

## What a source would have to provide

Per image, and each bound to a type NAME the bound inventory can price:

* subscriptions, publishers, timers
* service servers and clients
* action servers and clients

Two plausible producers, neither chosen:

1. **Codegen at registration time.** The `NROS_SUBSCRIBE` / `create_publisher`
   call sites are visible to the C++ front end, and `M` is in scope there --
   that is how phase-403 W3 got the per-type bound to the arena in the first
   place. An emitted manifest per component would name every entity and type.
2. **An author-stated manifest**, checked against the runtime's own count on
   first spin so a stale manifest fails loudly rather than silently under-sizing.

(1) cannot see entities created conditionally; (2) can drift. A hybrid -- emit
what codegen can see, and have the executor's first-spin advisory report any
delta -- is likely the honest answer, and issue 0900 W1 already landed the
reporting half.

## Partly fixed 2026-08-31 (phase-403 W9): the inventory exists and has a consumer

Still `open`, and the remaining half is named below rather than implied.

### The fork, decided

The HYBRID, with the halves assigned by what each can actually do rather than
by preference.

Candidate 1 (emit a descriptor from the registration macros) cannot be the
producer, and the reason is the timing the issue already suspected, sharpened:
`NROS_EXECUTOR_MAX_CBS` and `NROS_EXECUTOR_ARENA_SIZE` are `const` sizes
compiled INTO `nros-node`, which is built before a single component TU is
compiled, let alone linked. A link-section manifest can VERIFY a count; it can
never SUPPLY one. That is the direction of the build graph, not a gap.

So the declaration supplies and the running image verifies:

* **Supply.** `nano_ros_node_register(... ENTITIES <spec>...)`, stated beside
  `CLASS` / `SHAPE` / `CALLBACK_GROUPS` and travelling the channel that
  declaration already travels, `nros-metadata.json`.
* **Verify.** The derived value carries NO headroom, so it is exactly the
  declared demand. One entity more than declared and registration returns
  `NodeError::ExecutorFull`, which names the knob; `ComponentNode`'s `ok()`
  flag then halts boot naming the failing node.

### What landed

* `nros_cli_core::entity_inventory` -- ONE data model, three transports (JSON,
  an `include()`able CMake fragment, `KEY=VALUE` env lines), shaped after
  `rosidl_codegen::bounds` rather than as a second mechanism. The env line is
  the cargo transport, because the knob is read from the environment by
  `nros-node/build.rs`.
* `nros ws entity-inventory` -- the verb, sibling of `ws entity-facts`.
* `cmake/NanoRosEntityInventory.cmake` -- the reader, sibling of
  `NanoRosMessageBounds.cmake`, invoked from `nano_ros_entry()` (the first
  point in a configure guaranteed to be after every registration).
* `NROS_EXECUTOR_MAX_CBS` moved onto the derivable ladder with `-1` as its
  Kconfig sentinel. Environment > Kconfig / board `.conf` > derived > crate
  default, unchanged from W8.

### The number the bring-up got wrong

A PUBLISHER CLAIMS NO CALLBACK SLOT. Every registration that claims one calls
`Executor::next_entry_slot()` -- 24 sites, all subscriptions, timers, services,
service clients, action servers, action clients and guard conditions.
`create_publisher` is not among them: on the C++ path it writes an
`RmwPublisher` into caller-owned storage, and on the C path there is no
`nros_executor_add_publisher` to increment `handle_count`.

So the "33 handles" this issue quotes is the ENTITY count, and 14 of those 33
are publishers. Measured over the island's own `build-board/nros-metadata.json`:

| | value |
| --- | ---: |
| hand-set in `mr_canhubk3_s32k344.conf` | 36 |
| entities declared | 33 |
| **executor callback slots** | **19** |

The bring-up table's per-node columns are mis-attributed too -- it reads 6
timers and 2 services where the source has 4 timers, 2 service servers and 2
service clients. The total comes out right by coincidence, which is the best
argument in this issue for not counting by hand.

`EntityKind::callback_slots()` is a MIRROR (the CLI cannot depend on the
`no_std` target crate) and is held to those 24 sites by
`just check entity-slot-costs`, the same way `ACTION_SERVER_QUERYABLES` is held
to its creation sites.

### An under-report cannot be silent

1. Composition REFUSES for the whole image if ANY component declared no
   entities -- W8's rule for an unbounded type, applied to an undeclared
   component. `ENTITIES NONE` is the explicit "creates nothing", so absence
   always means "nobody said".
2. The derived value has no headroom, which makes the image check its own
   declaration.
3. A short declaration is a named, boot-fatal `ExecutorFull`, not a silent
   mis-size. This is exactly why `MAX_CBS` was the right first consumer and the
   arena was not: an under-sized arena halts DURING entity creation, before the
   first spin, so issue 0900 W1's advisory never prints.

### What is still uncovered

* **The arena** and **the zenoh payload classes** need the two inventories
  JOINED per subscription -- this one's per-entity list against W8's per-type
  size. Either half alone yields the confident wrong number. Named as its own
  work rather than bolted onto either reader.
* **`NROS_MAX_SUBSCRIBERS` / `NROS_MAX_PUBLISHERS`** are a straight read of
  `NROS_ENTITY_COUNT_SUBSCRIPTION` / `_PUBLISHER`. Left out for scope
  discipline, not for a reason.
* **No image in this tree declares `ENTITIES` yet**, so nothing derives today;
  every existing image refuses and keeps its configured value. The island's own
  declaration is four `nros_components_register_node(... ENTITIES ...)` edits
  in the consumer repo.
* **Not measured on hardware.** The island was not flashed. The code claim (a
  publisher claims no slot) is verified by reading all 24 registration sites
  and by the gate; the byte saving is not.
