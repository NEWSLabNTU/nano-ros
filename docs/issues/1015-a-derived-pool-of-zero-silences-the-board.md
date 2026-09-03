---
id: 1015
title: "A derived session pool of ZERO silences the board, with no diagnostic"
status: open
area: cmake
severity: high
related: [1002, 0991, 0940, 0965, phase-403, phase-412]
---

# Zero is not a smaller pool, it is a different kind of object

## Measured

Same image, same derived knobs, one variable changed:

| `ZPICO_MAX_QUERYABLES` | serial output in 15 s |
| ---: | ---: |
| 0 (derived) | **0 bytes** |
| 4 (floored) | **110 bytes** |

At zero the board transmits NOTHING. No panic, no log line, no fault -- the
core sits in WFI and the ROS graph shows nothing. The Z4-verified image
(everything hand-set) transmits 108 bytes on the same board, cable and router,
so nothing outside the image is implicated.

## Cause

phase-412 W1 derives

    MAX_QUERYABLES = COUNT_SERVICE_SERVER + actions * ACTION_SERVER_QUERYABLES

The reference island declares no service servers and no actions, so it derives
exactly 0. In `zpico.c`:

    queryable_entry_t queryables[ZPICO_MAX_QUERYABLES];   // line 435

A zero-length array, and NOT the last member of the struct. It is a GNU
extension that compiles silently and changes what the struct is.

## The rule that was missing

phase-403 states the derived value carries NO headroom, deliberately: it is
exactly the declared demand, which makes the running image a checker of its own
declaration. That is right for a table the executor INDEXES -- registration
past the end returns `ExecutorFull`, which names the knob.

It is wrong when the number backs a FIXED-SIZE C ARRAY. There, zero is not a
smaller pool; it is a different kind of object, and the failure is a layout
change rather than a bounds check.

**A derived pool that backs a C array has a floor of 1.**

## Why every gate was green

This is the seventh delivery-class defect of the campaign and the first that
produced NO diagnostic at all:

* the image links;
* `check-knob-delivery` confirms the value ARRIVED, because it did -- 0 was
  delivered faithfully, it was simply the wrong answer;
* `check-knob-fixpoint` converges, because 0 is stable;
* the fast tier is green, because nothing here is a host-testable property.

Every check in the tree asks whether the number the image was built with is the
number that was derived. None asks whether the number is USABLE.

## Options

1. **Floor it in the derivation** (done in the fix commit): any pool backing a C
   array derives `max(1, demand)`. Cheapest, and it cannot regress a
   correctly-declared image because 0 was never a legal size for these arrays.
2. **Floor it in C**, `#if ZPICO_MAX_X < 1 #error`. Louder, and it fails at
   BUILD rather than at silence -- worth having as well, since a floor in one
   producer does not bind another.
3. **Make zero legal**, giving each pool a `[1]` placeholder. Rejected: it
   spends a slot on every image to make an illegal value representable.

## Not covered

Nothing asserts a derived knob is USABLE, only that it is faithfully delivered.
A gate that knows which knobs back fixed C arrays and requires them positive
would have caught this at configure time, and is cheap. The wider question --
what other derived value has an illegal special case at some boundary -- is
open, and 0 is the obvious one to check first for every pool this campaign has
touched.
