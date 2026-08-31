---
id: 940
title: "The derived-bound inventory is exported and nothing reads it, so every size downstream is still set by hand"
status: open
area: codegen, memory, build
severity: high
related: [0896, 0900, 0939, phase-403, phase-403-W6]
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

## Why this is severity high

A knob nobody can enumerate is a knob nobody sets, which is the 0271 / 0739
shape this repo keeps rediscovering. The difference here is that the number is
no longer unknowable: it is computed, written to disk in three formats, and
then ignored.
