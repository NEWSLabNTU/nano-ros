---
id: 2900
title: "An owned entry's census reads fresh after its CMakeLists changes: the host binary is the only freshness input"
status: open
type: bug
severity: medium
area: [cli, census]
related: [phase-478, issue-2899, issue-2433]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
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
