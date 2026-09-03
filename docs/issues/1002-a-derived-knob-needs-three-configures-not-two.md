---
id: 1002
title: "A derived knob converges after THREE configures, not the two 0991 documents"
status: open
area: cmake
severity: medium
related: [0991, 0940, 0965, phase-403, phase-412]
---

# The fragment updates on pass 2 and the resolved value on pass 3

## What happens

Issue 0991 established that a clean build dir derives over the wrong basis on
the first configure, because W9's producer runs later in a configure than W8's
reader. Its remedy is "configure again", and every recipe, comment and commit
message since says TWO passes.

Two is not enough for a knob whose resolved value is cached. Measured on the
mr-canhubk344 island, watching `NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE` (the
receive payload class) against the value that reaches the build:

| configure | fragment says | delivered |
| --- | ---: | ---: |
| 1 | placeholder | 1496 |
| 2 | **880** | 1496 |
| 3 | 880 | **880** |

Pass 2 writes the right answer into the fragment. The knob resolved from it
does not move until pass 3.

## Why it stayed invisible

The stale value is the CLOSURE basis, which is LARGER than the subscribed one.
So the image is over-sized rather than broken: it builds, it links, it runs,
and every gate is green. Four consecutive island builds in one session shipped
the closure class while their own inventory had derived the subscribed one, and
the only reason it surfaced at all is that a knob was renamed and the pairing
became checkable.

`check-knob-delivery` (phase-412 W4) now catches this class, but only for pairs
it names, and only because it was fixed to read BOTH inventories -- it read one
of the two for its whole first life, so every knob derived by the
message-bound inventory was outside it.

## Why it matters even though it over-sizes

1. The saving a phase reports is not the saving the image gets. phase-412
   published 38,114 bytes for a build that delivered the stale class; the
   figure with delivery confirmed is 72,314. A number measured on an image that
   did not receive the change is not evidence about the change.
2. It is only safe by accident. The stale value is larger HERE because the
   closure is a superset of the subscribed set. A knob whose stale value is
   smaller fails the other way, and nothing in the mechanism prevents that.
3. Every instruction in the tree says two passes, so a CI lane or a contributor
   following them ships the stale value and reports success.

## Reproduce

    rm -rf build-x
    west build -b <board> -d build-x <app>          # pass 1
    cmake -S <app> -B build-x                       # pass 2
    grep NROS_RESOLVED_NROS_SUBSCRIBER_BUFFER_SIZE build-x/CMakeCache.txt
    #   -> the CLOSURE value, while build-x/nros/message_bound_knobs.cmake
    #      already states the subscribed one
    cmake -S <app> -B build-x                       # pass 3
    #   -> now they agree

## Options

1. **Fix the convergence.** Whatever 0991 does for the fragment must also
   re-resolve the knobs derived from it in the same configure. Best outcome,
   and it retires the pass counting entirely.
2. **Make the build loop until fixpoint**, bounded, and fail if it does not
   converge. Honest, and it stops the count from being folklore.
3. **Say THREE everywhere** and gate it. Cheapest, and the worst of the three:
   the next knob with a longer chain moves the number again, and nobody will
   notice for the same reason nobody noticed this.

## Not covered

Nothing asserts that a configure sequence has REACHED a fixpoint. A gate that
configures twice, records every `NROS_RESOLVED_*`, configures once more and
requires no value to move would have caught this the day it appeared, and does
not depend on knowing the right pass count in advance.
