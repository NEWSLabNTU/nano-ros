---
id: 991
title: "A clean build of an entity-declaring image derives the WRONG payload basis and does not link"
status: resolved
area: cmake
severity: high
related: [0965, 0940, phase-403]
---

# The first configure always answers `closure`, and on a small part that does not fit

## What happens

phase-403 W9's producer (`nano_ros_entry()`) runs LATER in a configure than
W8's reader (`nros_derive_message_bound_knobs`, reached from
`nros_find_interfaces`). The reader therefore reads the fragment the PREVIOUS
configure wrote. On a clean build dir there is no previous configure, so it
reads the placeholder:

    set(NROS_ENTITY_INVENTORY_STATUS "refused")
    set(NROS_ENTITY_INVENTORY_REASON "no entity inventory composed yet")

and falls back to `NROS_MESSAGE_BOUNDS_BASIS "closure"`.

`NanoRosMessageBounds.cmake` calls that fallback an over-approximation "in the
safe direction". It is safe for CORRECTNESS. It is not safe for a part that is
nearly full, and that is the case the whole derivation exists to serve.

## Measured, mr-canhubk344 island

Reproduced in TWO independent, freshly created build dirs. Identical byte count
both times:

    ld.bfd: region `RAM' overflowed by 103160 bytes

Because the closure basis sets the small payload class from a
`std_msgs/Float64MultiArray` the image links through `geometry_msgs` and never
receives:

| configure | basis | small class | result |
| --- | --- | ---: | --- |
| 1st (clean build dir) | `closure` | 1496 | RAM overflow by 103160 B, no link |
| 2nd (same dir) | `subscribed` | 880 | links; RAM 98.83%, DTCM 68.15% |

The image declares 28 entities across four components and the fragment resolves
correctly -- `NROS_ENTITY_INVENTORY_STATUS "derived"`,
`NROS_ENTITY_SUBSCRIBED_TYPES_STATUS "resolved"`, 9 subscribed types,
`NROS_DERIVED_EXECUTOR_MAX_CBS 14`. Nothing is wrong with the declaration. The
first configure simply cannot see it.

## Why the documented recovery does not fire

The module says the fragment's rewrite "re-runs cmake". Empirically a single
`west build` on a clean dir does NOT reach a second configure before linking:
it configures once, writes the real fragment at the end of that configure, then
runs ninja, which builds and fails at link. A later explicit configure over the
same dir does flip the basis and the image links.

So the lag closes eventually, but not within the build that first needs it.

## Why it is worse than a slow convergence

1. It fails at LINK, far from the cause, naming a byte count and no knob. The
   phase-403 message chain -- "a bound must EXIST before a buffer can be sized
   from it" -- does not appear, because every bound does exist. Only the BASIS
   is wrong.
2. The failure is order-dependent, so it reproduces for a new contributor and
   for CI (both start clean) and NOT for anyone with a warm build dir. That is
   the worst distribution for a bug.
3. It punishes exactly the images the feature targets. An image with slack in
   RAM never notices; an image tight enough to need derived payload classes
   cannot build.

## Options

1. **Compose the entity inventory before the bound reader runs.** Removes the
   lag rather than tolerating it. Biggest change; the producer currently has to
   run after every `nano_ros_node_register()`.
2. **Make the producer force a reconfigure when the fragment's answer CHANGES**,
   so the pass that first learns the subscribed set is followed by one that uses
   it, inside the same `west build`.
3. **Refuse rather than over-approximate when the fragment is a placeholder AND
   the image declares entities.** A clear "configure again" beats a link error
   naming 103160 bytes. Cheapest, and honest, but leaves two passes as the
   contract.

## Resolved 2026-09-03 by option 2 — and the recovery it relied on never existed

Option 1 is not available on the platform this bug is about. In a Zephyr build
the earliest reader is `nros_resolve_knobs()`, which runs inside
`find_package(Zephyr)` — before the application's first `add_subdirectory()`,
so before any `nano_ros_node_register()` can have happened. Moving the producer
earlier than that reader is not a reordering, it is a rewrite of the Zephyr
module's include order, and `nros_rmw_zenoh.cmake` bakes `zephyr_compile_
definitions()` immediately after. Option 3 fails this issue's own acceptance
("build once, from nothing, and require a link") and would leave the payload
classes at crate defaults, which are SMALLER than the derived answer — silently
under-sized, the unsafe direction.

So: option 2. Implementing it found that the thing it was supposed to build on
was not there.

### The mechanism three call sites documented does not fire

`NanoRosCodegenCore.cmake`, and both loaders in `zephyr/cmake/nros_cargo_build.
cmake`, each stated the same recovery at length:

> `CMAKE_CONFIGURE_DEPENDS` plus a write-if-changed producer, so ninja re-runs
> cmake by itself once the entity lane writes different bytes.

MEASURED on this tree (cmake 3.22, ninja), with a five-line project of exactly
that shape:

| attempt | result |
| --- | --- |
| write the file during the configure, register `CMAKE_CONFIGURE_DEPENDS` | **0 re-runs.** `build.ninja` is written at the END of the generate step — 1 ms after the fragment — and ninja's regeneration rule fires only on an input NEWER than `build.ninja` |
| delete that input instead | **0 re-runs.** A missing dependency of `build.ninja` is not "dirty" to ninja |
| `file(GENERATE)` the stamp | **0 re-runs.** It lands in the same millisecond as `build.ninja` |

So the lag never closed slowly. It never closed at all: not on that build, not
on the next one, only on an explicit re-configure — which is exactly what this
issue observed from the other end ("a later explicit configure over the same dir
does flip the basis"). The 103160-byte overflow was not a race that a warm build
dir happened to win; a warm build dir won it because someone had configured
twice by hand.

That is the wider defect. Both fragments and both readers relied on it, so the
fix is one shared mechanism, not a patch at the reported site.

### What landed

`cmake/NanoRosReconfigure.cmake` — the one spelling of "the answer changed after
its readers ran; run cmake again before this build proceeds":

* `nros_reconfigure_snapshot` hashes the fragment's CONTENT before the producer
  writes, so a producer that rewrites identical bytes arms nothing.
* `nros_reconfigure_on_change` future-dates the fragment when the digest moved.
  That is the only lever left: `build.ninja` does not exist yet, so "newer than
  it" means the future. Ninja then finds the manifest stale, re-runs cmake and
  RESTARTS — inside the same `west build`.
* `nros_reconfigure_settle` clears that date at the first reader of the next
  pass. This is what makes it terminate: measured at **100** re-configures
  without it, **exactly 1** with it.
* Two bounds, because a mechanism that can re-run the configure must not be able
  to do it forever: `NROS_RECONFIGURE_MAX_PASSES` (3) stops a non-convergent
  producer with a WARNING naming the knob, and `NROS_RECONFIGURE_FUTURE_SECONDS`
  (120) must exceed the remainder of the configure — too small degrades to the
  OLD behaviour, never to a wrong answer.

Wired at both producers (`nano_ros_entry()` for the entity inventory, the end of
`nros_find_interfaces()` for the message-bound sizes) and settled at every
reader, with the Zephyr loaders settling first because they are the earliest
readers in the configure — which bounds the window in which any file is
future-dated.

The three comments that asserted the inert mechanism are corrected rather than
deleted, since "why doesn't CONFIGURE_DEPENDS do this?" is the first question a
reader will have.

### Gate

`just check reconfigure-on-change` → `tests/cmake-reconfigure-tests.sh`, on the
fast line. cmake + ninja only (`project(... NONE)`), ~2 s.

It is a real configure plus a real `ninja`, not `cmake -P` like its two
siblings, because the claim is about what ninja does with an mtime — and the
thing being fixed READ as working in every review while firing never. **Case A
is a control that reproduces the bug**: the same project with bare
`CONFIGURE_DEPENDS` builds with the placeholder and re-runs cmake 0 times. A fix
that quietly stopped working would not be able to hide behind it.

Case G is this issue's acceptance, at the level a gate without a cross toolchain
can reach: a CLEAN build dir, configured once and built once, drives the REAL
`nros_derive_message_bound_knobs` and `nros_derive_entity_inventory_knobs` in
the real order and must come out `basis=subscribed` with the small payload class
at **880** — the type the image receives — rather than the closure's **1496**,
which is the `std_msgs/Float64MultiArray` it links through and never receives.
1496 is the number that overflowed the island's RAM by 103160 bytes.

## Not covered

The island's LINK itself. That needs a cross toolchain and the 320 KiB part, and
it lives out of tree (`NEWSLabNTU/autoware-safety-island`). What is gated here
is the BASIS, which is the cause; the byte count was the symptom. Re-measure the
island on a clean build dir to close that half.
