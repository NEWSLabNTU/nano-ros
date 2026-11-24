---
id: 2900
title: "An owned entry's census reads fresh after its CMakeLists changes: the host binary is the only freshness input"
status: resolved
type: bug
severity: medium
area: [cli, census]
related: [phase-478, issue-2899, issue-2433]
found: 2026-11-23
resolved: 2026-11-25
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
I51. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`.

## What happens

`take_owned_census` returns at once when the stored census is current
(`census_freshness` in `packages/cli/nros-cli-core/src/cmd/entity_census.rs`).
For an owned entry the only freshness input recorded is the host binary the
census ran. The entry's own `CMakeLists.txt` is not an input, so editing it
(removing an `add_subdirectory`, adding a source, changing a define) leaves
the old census reading fresh, and the configure goes on with it.

## How it was found

Locally, removing the `add_subdirectory(asi_common)` fallback from the island's
FreeRTOS test entries configured and built: the census taken before the edit
still read fresh. CI, from a clean tree, failed (issue 2899). The stale census
masked issue 2899 on every warm build dir.

## Expected

An owned census is stale when the entry's `CMakeLists.txt` (and the
`package.xml` that names its `<depend>`s) changes, by content digest as the
other census inputs are.

## Acceptance

- A test: take an owned census, edit the entry's `CMakeLists.txt`, the next
  take reads it stale and re-runs.

## Resolution

Fixed 2026-11-25, together with issue 2899, from the same helper. The owned
census records as freshness inputs:

- the source tree of the entry's package;
- the entry's `CMakeLists.txt` and `package.xml`, named, so a stale verdict
  says which file moved;
- the source tree of each workspace package its census build adds
  (`owned_entry_closure`).

Every input is a content digest, as before. A census recorded before this
change holds the binary only. It digests as fresh, so `take_owned_census`
also requires the entry's build-file row and re-takes the census once.

The test is `an_owned_census_goes_stale_when_the_entrys_build_files_change`,
with a negative control: the binary-only census is fresh by digest and still
not believed. Measured on host 241 with `owned_two_programs_threadx`: the
first build after the change re-took the census, and so did each later edit
to `two_prog/CMakeLists.txt` and each revert. An unchanged tree read fresh.
