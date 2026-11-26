---
id: 2980
title: "An image's configure reads as stale because ANOTHER bringup's census is stale or its resolve was refused, so its host configure gets no seed"
status: resolved
type: bug
severity: high
area: [cli, census, configure]
found: 2026-11-26
resolved: 2026-11-26
resolved_in: "fix/census-cross-bringup"
related: [2871, phase-530]
---

## What happened

The autoware safety island (branch `nano-ros-next` at 25cbfe7, nano-ros pin
1a638e9958) ran its first fresh configure of the S32Z270 image. The configure
stopped at `cmake/NanoRosPromise.cmake:402`: the entity inventory was
composed after the configure's readers ran, no `nros configure` had seeded
them, and the composed answer was derived.

The census the configure tripped on was NOT the S32Z image's. It belonged to
another bringup in the same workspace, `can_output_test_bringup` (image
`fvp-aemv8r-smp`), and was stale. The S32Z configure passed once the FVP lane
had re-taken that census. The exact FATAL text was not kept; the line number
and the sequence above are what the reporter recorded.

## Why

A host configure asks `nros locate configured` whether its image's
`nros configure` stamp is current (`cmake/NanoRosResolved.cmake`). Only a
current stamp publishes the seed; a stale one makes the configure compose
its own answer unseeded, held to what its readers read.

`configure_stamp::currency` decided that over the whole project in two
places:

- it walked every recorded census in the project, the models' censuses
  under `build/nros/models/<bringup>/` and the owned entries' under
  `build/nros/census/owned/<bringup>/`, and called the image stale if any
  of them was stale;
- it compared the stamp's recorded census digests with every census in the
  project.

`nros configure` does not re-take a census unless it is given `--census`. So
a stale census of one bringup stayed stale through every configure, and every
image of every other bringup read as stale until that census was re-taken,
by a lane that has nothing to do with them.

The same check made the same mistake one field over: a refused-resolve
marker (`<models>/<bringup>/<stem>.refused`) of any bringup made every image
stale, for as long as that bringup's resolve kept being refused.

## Fix

Both checks are scoped to the image's own bringup. The bringup comes from
the stamp key (`<bringup>:<id>`, or a shared configure's `@<bringup>:<coord>`)
and from where the census or marker lives. A leaf's bare image id and the
project-wide key still read every census and marker, and so does a census
or marker whose path names no bringup, so nothing that used to be checked is
dropped silently.

The other project-wide currency inputs were checked for the same class and
left as they are, because every `nros configure` refreshes them, so they
cannot hold another bringup's image stale. These are the input digest, the
external reads, the sidecar component sources and the ament inputs. Scoping
the digest would also be wrong: an image can read packages and launch files
outside its own bringup.

Unit tests in `packages/cli/nros-cli-core/src/configure_stamp.rs`:

- `another_bringups_stale_census_leaves_this_image_current`
- `this_bringups_stale_census_makes_the_image_stale`
- `another_bringups_census_digest_is_not_compared`
- `another_bringups_refused_model_leaves_this_image_current`
- `a_census_is_attributed_to_the_bringup_its_path_names`
