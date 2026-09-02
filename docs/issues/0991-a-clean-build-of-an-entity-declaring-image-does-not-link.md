---
id: 991
title: "A clean build of an entity-declaring image derives the WRONG payload basis and does not link"
status: open
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

Option 3 is the smallest correct step and option 2 is what a user would want.
Whichever lands, the acceptance test is a CLEAN build dir: build once, from
nothing, and require a link.

## Not covered

No gate builds an entity-declaring image from a clean build dir and asserts it
links. The existing knob gates assert the DERIVED VALUES, which are right --
they are read after the fragment exists. This is the same shape as 0963 and
0896: a mechanism that is correct and, on the path a real user takes, not
reached.
