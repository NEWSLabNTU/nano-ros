---
id: 1680
title: "A census taken under one `[system] features` set reads FRESH after the features change, though the features change what the census records"
status: resolved
type: bug
area: [cli]
severity: low
found: 2026-10-05
related: [1679, 1556, 1649, phase-463]
resolved_in: "branch issue-1680 (fix(#1680) PR)"
---

## What

`cmd/entity_census.rs::is_freshness_input` decides freshness from four
recorded roles: `binary`, `source_tree`, `entry_tu`, `build_file`. The image's
`[system] features` (and `capabilities`) in `system.toml` are none of them, and
the doc comment there explains why the MODEL is deliberately not a freshness
input (a contract edit must not invalidate the evidence it is compared
against). That argument is about the contract. It does not cover the feature
selection, which is a BUILD input: it decides which code the native census
binary compiles.

Measured 2026-10-04 on `examples/workspaces/cpp` (issue 1649's parameter
measurement): a census taken before `features = ["param_services"]` was added
recorded no parameters (issue 1679 is why); after the edit, `nros build`
read that census as fresh and compared the contract against it, reporting
`param-phantom` for parameters the code does declare. Deleting the census file
and re-taking it fixed it. The `binary` digest did not catch it because the
check ran against the binary the census itself had recorded — nothing had
rebuilt it yet.

## Scope

The only feature-dependent recording known is issue 1679's (the C++ parameter
hook compiled only under `param-store`). If 1679 is fixed by making the hook
unconditional, the census may no longer depend on any feature, and this issue
closes as a RULING: say so in `is_freshness_input`'s comment, with the
measurement. If some feature still changes what the census records, the
feature set (or the cargo feature list the native sibling is built with) is a
freshness input and belongs in `inputs` with its own role.

## Acceptance

A test: take a census, change `[system] features`, and assert the verdict —
STALE if the features can change the record, FRESH with the ruling cited if
they cannot.

## Resolution (2026-10-05)

**Not a ruling. The features still change the record after issue 1679.**
1679 made the parameter HOOK independent of the feature set, but the axes
still decide what the census binary is BUILT with. `param_services`
registers the six `rcl_interfaces/srv/*` servers per node and `lifecycle`
registers the five `lifecycle_msgs/srv/*`, all through the RMW. The recording
backend records them, as it records every `create_service`. The contract check
excludes them by interface kind (`entity_census::is_infra_interface`, whose
own test `the_parameter_services_are_excluded_by_interface_kind` exists
because they ARE in the census). `census_callback_slots` sums every recorded
row, so those rows feed the sizing a fresh census answers. A census taken
under one axis set is therefore a wrong statement about an image built under
another, and the gap matters for sizing as well as for the check.

**Fix.** The census now records the bringup's capability axes as a
freshness input, with role `capabilities`, its path set to the bringup's
`system.toml`, and digest `capabilities:<axis>,<axis>`
(`metadata_refresh::capabilities_digest`).
- `collect_inputs` records it on the model road. `take` and `run --model`
  write it, and `check --require-fresh` and `census_callback_slots` both read
  it through `census_freshness`.
- `recompute_digest` recomputes it, and `is_freshness_input` admits the role.
  The doc comment there says why this input is unlike the contract.
- The axes are read the way the build reads them: `SystemToml::capability_enabled`
  over the `capability_resolver` registry, covering both `[system] features`
  and the deprecated typed blocks.
- Only the axis list is digested, never the file. `[census]` policy and
  `[census.waive]` live in the same `system.toml`, and editing them must not
  stale the evidence they apply to.
- The digest is readable rather than hashed, so a stale verdict says which axes
  moved, e.g. `recorded capabilities:, now capabilities:param_services`.
- A file that no longer parses is STALE, never "unchanged".

**Tests.**
- `metadata_refresh::tests::capabilities_digest_moves_with_the_axes_and_nothing_else`:
  - none gives `capabilities:`.
  - A `[census]` policy edit leaves the digest unchanged.
  - `features = [...]` and the typed `[param_services]` block both read.
  - Through `stale_recorded_inputs`, a features edit is STALE and names the
    new axes.
  - An unparseable file is STALE.
- `scripts/check-entity-census.sh` (`just check entity-census`, fast line)
  adds move 4c over the real verbs on the fixture workspace:
  - Adding `features = ["param_services"]` makes `check --require-fresh`
    refuse with `census stale` and `capabilities:param_services`, and it does
    not compare the stale census.
  - Restoring the file makes the same census fresh with no re-run.
  - Move 5, which rewrites the `[census]` policy in the same file, stays fresh.
- Not measured: the script against a CLI without this change. The only one
  on this host predates other gate moves and fails at an earlier move.

**Scope left as is: the standalone leaf road.** `take --leaf` configures the
leaf with `-DNANO_ROS_LEAF_BOARD=native` and nothing in `cmake/` reads the
leaf's `[system] features` into that build. They size the queryable budget
(`entity_facts::declared_infra`), not the census binary. So a leaf census does
not depend on them and records no `capabilities` input. If leaf features ever
reach the leaf build, that road needs the same input.
