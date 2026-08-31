---
id: 942
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

| consumer | needs | status |
| --- | --- | --- |
| zenoh payload classes | the sizes actually RECEIVED | derives over the linked closure instead, which is an over-approximation |
| `NROS_EXECUTOR_MAX_CBS` | total handle count | hand-counted; a grep gave 17 when the answer was 33 |
| `NROS_EXECUTOR_ARENA_SIZE` | entities that will occupy slots | six bisections against a board |

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
