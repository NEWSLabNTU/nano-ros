---
id: 2806
title: "An owned entry whose census was not taken under on_missing = \"warn\" sizes its pools at their defaults"
status: open
type: bug
area: [cli, census, sizing]
severity: medium
found: 2026-11-21
related: [2433, 2805, 2794]
---

## What happens

A bringup may opt into `[census] on_missing = "warn"`. When the census of an
image's OWNED entry (issue 2433) then cannot be taken, the take warns and
writes no census, and the configure builds the image with every pool at its
embedded default:

- `owned_census_fallback`
  (`packages/cli/nros-cli-core/src/cmd/entity_census.rs`, ~line 1404) turns
  the take's error into a warning ending "so the image builds without census
  sizing (its pools at their defaults)" and returns Ok.
- `nros_record_owned_entity_facts`
  (`cmake/NanoRosLeafEntityFacts.cmake`, ~lines 290-300) finds no
  `<ws>/build/nros/census/owned/<bringup>/<image>.census.json`, warns that
  "its pools stay at their defaults" and returns before any sizing
  descriptor or entity inventory is written.

So the image is built from defaults nobody stated. A CMake warning in a long
configure log is the only trace; the descriptor, which is where a reader
looks for a pool's size and its provenance, does not exist for the image.

## Why it is wrong

Under the carried-or-refused rule a value the build depends on is either
carried (stated, with its source) or refused (with a reason). An unstated
default is neither. `on_missing = "warn"` was meant to let the build go on,
not to let it go on with silent numbers.

## Expected

The image's sizing descriptor is still written, and states each
census-derived pool (`subscriber_count`, `publisher_count`,
`callback_slots`, the entity-inventory knobs) as refused, reason "census not
taken" plus the take's first error line. A reader that needs one of them
then refuses or applies its own documented default visibly, as for any
other refused field.

## Ownership

nros 243 takes this after its census-fold branch (issue 2794) lands, since
that branch reshapes `take_owned_census`, the descriptor and
`NanoRosLeafEntityFacts.cmake`. Found while fixing issue 2805, where the
consumer's owned entries took this path.
