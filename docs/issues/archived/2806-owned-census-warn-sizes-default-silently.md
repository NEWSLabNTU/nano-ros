---
id: 2806
title: "An owned entry whose census was not taken under on_missing = \"warn\" sizes its pools at their defaults"
status: resolved
type: bug
area: [cli, census, sizing]
severity: medium
found: 2026-11-21
resolved: 2026-11-22
resolved_in: "fix(cli, issue 2806): a census not taken refuses every count it would state"
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

## Resolved, 2026-11-22

The user ruled for option (A) on 2026-11-22: write the descriptor even with
nothing else stated, as a narrow exception to RFC-0100's "no stated fact ⇒ no
file". It is recorded in RFC-0100 as
[Exception, 2026-11-22](../../design/0100-rmw-agnostic-sizing-model.md#exception-2026-11-22--a-census-not-taken-earns-a-file).

- **The take** records why it could not take the census, as
  `owned/<bringup>/<image>.not-taken` beside where the census would be (the
  take's first error line). A successful take removes the record.
- **The configure**, finding the census missing, still calls
  `nros dev image sizing-descriptor --owned-entry … --census-not-taken`
  instead of returning.
- **The descriptor** is `Wiring::Unwired` with `census_refused`. Its `[meta]`
  is the existing Unwired shape. Every `[image]` count an inventory states is
  refused with "the census of the owned entry `<pkg>` was not taken
  (<cause>)…", naming the take that makes one. With no record (a configure
  run with `NROS_LEAF_CENSUS_TAKE=OFF`), the cause reads "this configure did
  not take it".
- **The defaults report** (`defaults_in_use`, so `nros build --explain-sizing`)
  gains one entry for a refused `node_count`. It names the node table, the
  entity pools and the callback slots as keeping their builtins.

**Sizes are unchanged.** Every consumer reads a value only when it is stated,
so a refused count sizes exactly as an absent one. The zenoh pools read
endpoint rows, and none exist, so they answer `Absent`, as with no file.
nros-node's arena guard turns a `basis = closure` descriptor away, as it did
no file. That last point is tested in
`nros-node/tests/arena_model_in_place.rs`
(`a_census_not_taken_descriptor_is_turned_away_by_the_basis_guard`). The
descriptor is tested in `nros-cli-core`
(`a_census_not_taken_writes_a_descriptor_refusing_every_count_it_would_state`,
with a no-refusal negative control and a mutation check). The cause it
carries is tested by
`an_owned_entry_whose_census_was_not_taken_refuses_by_the_takes_cause`.

Not changed: with `NROS_LEAF_CENSUS_TAKE=OFF`, the configure reaches this
path whatever `on_missing` says, and that is unchanged behaviour. The policy
still decides only at the take.

