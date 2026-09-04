---
id: 1002
title: "A derived knob converges after THREE configures, not the two 0991 documents"
status: resolved
area: cmake
severity: medium
related: [0991, 0940, 0965, phase-403, phase-412, phase-424]
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

## Resolved 2026-09-05 — the count was right, the diagnosis was half right

Measured, not reasoned: a five-line cmake project of the real shape, driving
the REAL `nros_derive_message_bound_knobs` and `nros_derive_entity_inventory_
knobs` through the REAL three stages, with a `go` target echoing the value the
build was SIZED FROM rather than the value on disk.

    STAGE R   earliest reader   nros_resolve_knobs    (inside find_package(Zephyr))
    STAGE P1  mid producer      nros_find_interfaces  (bounds fragment)
    STAGE P2  latest producer   nano_ros_entry        (entity fragment)

| configure | entity fragment | bounds fragment | value DELIVERED to the build |
| --- | --- | ---: | ---: |
| 1 | placeholder | closure 1496 | nothing derived |
| 2 | real | subscribed **880** | 1496 |
| 3 | real | subscribed 880 | **880** |

Three configures, exactly as this issue reported. The reason is structural and
is not a bug: the producers are a CHAIN. The entity inventory is an INPUT to
the payload-class join, whose OUTPUT is what the knob resolver reads, and the
knob resolver runs before both. One pass per link, so the delivered value
settles one pass after the fragment does.

### What was NOT true: that a build ships the stale value

Ninja re-runs cmake until `build.ninja` stops being stale, so it runs all three
passes and then builds. Measured: `PROBE_BUILT_WITH=880 at_pass=3`, two
automatic re-configures, and a second `ninja` re-configures zero times. A
`west build` on a clean dir is correct today.

What is wrong is the INSTRUCTION, in three call sites and in 0991's own
write-up: each says "configure again", singular, which is right for one link
and one short for two. A HAND-driven `cmake -S -B` sequence that stops at two
-- which is what this issue's Reproduce section does, and what a person
debugging a size will do -- reads a fragment that already states 880 out of a
cache that still holds 1496, and both look authoritative. That prose is
corrected in `NanoRosReconfigure.cmake`, `NanoRosMessageBounds.cmake`'s
first-configure status line, `NanoRosEntry.cmake` and the Zephyr loader, and it
now states the RULE (one pass per producer upstream) rather than a number that
the next producer would move again.

### What WAS a defect, and it was the bound

`NROS_RECONFIGURE_MAX_PASSES` counted arms per fragment in the CACHE, whose
lifetime is the build dir. The comment called that "exactly the scope the bound
is about". It is not: the bound exists to stop a producer whose answer NEVER
SETTLES, which is a property of ONE convergence episode.

Counting over the directory's lifetime has a two-edit fuse. Measured on the
unfixed module, same project, editing which type the image subscribes to:

| step | declaration says | delivered | counter | |
| --- | ---: | ---: | ---: | --- |
| clean build dir | 880 | 880 | 2 | 2 of 3 spent just converging |
| edit 1 | 1496 | 1496 | 3 | the last one |
| edit 2 | 880 | **1496** | 3 | bound hit, WARNING, previous answer |
| edit 3 | 1496 | **880** | 3 | bound hit -- and this one is UNDER-sized |
| edit 4 | 880 | 880 | 3 | right by luck, the stale answer happens to match |

Edit 3 is the part that matters. This issue's point 2 said the staleness is
"only safe by accident", because the closure is a superset of the subscribed
set. An exhausted bound does not have that property at all: it ships whatever
the previous edit derived, so an edit that makes the image receive a LARGER
type leaves it sized for the smaller one. That is the unsafe direction, in a
build dir whose only sin is being a few edits old.

Fix: `nros_reconfigure_on_change` unsets the fragment's counter on the SETTLED
path -- the pass where the producer writes what the readers already had, which
is the fixpoint. The bound now counts CONSECUTIVE arms. A non-convergent
producer never reaches a settled pass, so case E still stops it.

After the fix, the same four edits deliver 880 / 1496 / 880 / 1496 / 880, each
matching its own declaration, with no warning.

### Options, against the measurement

Option 1 (re-resolve in the same configure) is unavailable for the reason 0991
already gave: the reader is inside `find_package(Zephyr)`, before the
application's first `add_subdirectory()`. Option 2 (loop to a fixpoint,
bounded) is what the mechanism already does -- the correction was to the bound,
not to the loop. Option 3 (say THREE everywhere) is rejected as this issue
predicted: the docs now name the rule, and the NUMBER is measured by a test.

### Gate

Two new cases in `tests/cmake-reconfigure-tests.sh` (`just check
reconfigure-on-change`, fast line, cmake+ninja only):

* **H** -- the three-stage chain including the EARLIEST reader. Asserts the
  value the BUILD USED is 880, that it took 2 automatic re-configures (the
  chain's depth, printed with the headroom against `MAX_PASSES` so a third
  producer moves a number in a test), and that the sequence reached a FIXPOINT.
  This is the "Not covered" section of this issue.
* **I** -- four declaration edits in ONE build dir, alternating so a stale
  answer is always visibly the wrong one, and failing if any of them hits the
  bound.

Both mutation-checked, and the mutations are orthogonal:

| mutation | G | H | I |
| --- | --- | --- | --- |
| `MAX_PASSES=1` (the module's old "exactly ONE extra configure" prose) | pass | **FAIL** | pass |
| the settle-reset removed (the lifetime counter) | pass | pass | **FAIL** |

The first row is why case G could not see this issue: G reads
`NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE` as the derivation left it in the caller's
scope -- the FRAGMENT, which is right on pass 2 -- and reported 13/13 green
against a module that delivers 1496 to the compile.

### At the bound

Not silent, and measured: a `message(WARNING)` naming
`NROS_RECONFIGURE_MAX_PASSES`, the fragment, and "THIS BUILD USES THE PREVIOUS
ANSWER". It does not FAIL the build, which is deliberate -- the answer on disk
is correct and the recovery is one more configure -- but it is a warning in a
long ninja log, so the reset above matters more than the wording does.

### Not covered

The island's own link, for the reason 0991 records: it needs a cross toolchain
and the 320 KiB part, and it lives out of tree. This closes the BASIS and the
DELIVERY; re-measuring the island on a clean build dir closes the byte count.
