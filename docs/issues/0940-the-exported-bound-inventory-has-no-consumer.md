---
id: 940
title: "The derived-bound inventory is exported and nothing reads it, so every size downstream is still set by hand"
status: open
area: codegen, memory, build
severity: high
related: [0896, 0900, 0939, phase-403, phase-403-W6, phase-403-W8]
---

# The build knows every number and cannot say any of them

## What exists

phase-403 W6 exports each package's derived bounds over three transports from
one data model (`rosidl_codegen::bounds::BoundInventory`):

* `<gen>/nros_message_bounds.json` -- canonical
* `<gen>/nros_message_bounds.cmake` -- an `include()`able fragment, written at
  configure time so it exists before anything downstream configures
* a generated `build.rs` + `links` key, so a dependent reads
  `DEP_NROS_MSGS_<PKG>_BOUNDS_JSON`

Measured on the island entry: 11 inventories, 84 types, 60 bounded.

## What consumes it

Nothing.

## What that costs, measured rather than argued

Every size downstream is still set by a human, and on one bring-up every one of
them was set wrong at least once:

| knob | how it was actually set |
| --- | --- |
| `NROS_EXECUTOR_ARENA_SIZE` | SIX bisections against a board, flashing each |
| `NROS_EXECUTOR_MAX_CBS` | counted by grep: 17, when the answer was 33 |
| `NROS_MAX_LARGE_SUBSCRIBERS`, `NROS_SUBSCRIBER_LARGE_SIZE` | read off generated C++ headers by eye |
| `NROS_SUBSCRIBER_BUFFER_SIZE`, `NROS_MAX_SUBSCRIBERS` | ditto |
| W4's payload class boundaries | blocked ON this inventory; the wave says so |

The grep failure is worth keeping: `MAX_CBS` counts total HANDLES, and most
subscriptions are the `NROS_SUBSCRIBE` macro rather than a literal
`create_subscription`, so grepping the obvious string found 1 of 11
subscriptions and none of the 6 timers.

Issue 0900 W1 landed the runtime half -- `arena_used()` / `arena_capacity()` and
a first-spin advisory naming the value to set. It cannot help in the case that
matters: an image whose arena is too small halts during entity creation, which
is BEFORE the first spin, so the advisory never prints.

## What would resolve it

A consumer per number, each reading the inventory rather than a human:

1. `MAX_CBS` from the entity count the model and codegen already know.
2. The executor arena from the entities that will occupy it -- phase-403 W5's
   remaining half, and 0939's multiplication makes a derived total more
   valuable, not less.
3. zenoh's payload class boundaries from the distinct sizes present, which is
   W4's stated goal and its stated blocker.

The transports exist and are verified end to end (a CMake `-P` run over five
packages composed 97 types and derived `NROS_MAX_LARGE_SUBSCRIBERS=3` /
`NROS_SUBSCRIBER_LARGE_SIZE=364`). What is missing is the reader.

## Partly fixed 2026-08-31 (phase-403 W8): consumer 3 of 3 exists

Still `open`, and the remaining half is named below rather than implied.

**What landed.** `cmake/NanoRosMessageBounds.cmake` composes every package's
fragment and derives the size knobs. `nros_find_interfaces()` runs it over the
image's whole interface closure and writes the answer plus its provenance to
`<build>/nros/message_bound_knobs.cmake`; the Zephyr knob resolver reads that,
with `-1` as the Kconfig spelling of "nothing here chose a number" and a
precedence ladder of environment > Kconfig / board `.conf` > derived > crate
default. Item 3 of "What would resolve it" -- zenoh's payload class boundaries
-- is done, plus `NROS_SUBSCRIPTION_BUFFER_SIZE`.

Measured on the island entry (11 packages, 84 types, regenerated from the args
files its build dir already holds): `NROS_MAX_LARGE_SUBSCRIBERS` **2 -> 0**,
`NROS_SUBSCRIBER_LARGE_SIZE` **2560 -> not derived**, because nothing in the
closure exceeds the 2048 B class split. The hand-set pair reserved
`2 x 4 x 2560 = 20,480` bytes of `.bss` for a class no type can route into.
It also found `CONFIG_NROS_SUBSCRIPTION_BUFFER_SIZE=512` against a derived
`nav_msgs/Odometry` bound of 880 -- an undersized take buffer on the image's
own largest subscribed type, whose failure mode is a silent drop.

**What is still uncovered, and why it is not a patch away.** Items 1 and 2 --
`NROS_EXECUTOR_MAX_CBS` and the arena -- are questions about WHICH ENTITIES AN
IMAGE CREATES, and this inventory answers only what every TYPE'S SIZE is. A
package's type count is not an image's entity count. Deriving one from the
other would produce exactly the plausible-wrong-number this issue exists to
remove, so W8 declined to.

A second source would have to supply, per image: the number of subscriptions,
publishers, timers, service servers/clients and action entities, each bound to
a type NAME the inventory can then price. `entity_facts.rs::describes_wiring`
is the extension point and abstains on all 115 resolved SystemModels today, and
the RFC-0043 C++ components register in constructors at runtime, so the wiring
would have to come from codegen at component-registration time or from an
author-stated manifest -- not from the resolved model as it exists.

**The same gap sets the price of what did land.** The derived numbers are
UPPER BOUNDS: the closure is what the image LINKS, not what it subscribes to.
On the island the derived `1496` comes from `std_msgs/Float64MultiArray`, which
it never receives, against `880` for its real worst case -- `29,568` B of
`SMALL_PAYLOADS` and `66,528` B of arena, spent on types nothing reads. Safe in
direction, expensive in magnitude, and the entity inventory is what would make
it tight.

## Why this is severity high

A knob nobody can enumerate is a knob nobody sets, which is the 0271 / 0739
shape this repo keeps rediscovering. The difference here is that the number is
no longer unknowable: it is computed, written to disk in three formats, and
then ignored.
