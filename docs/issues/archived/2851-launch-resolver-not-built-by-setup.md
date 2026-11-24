---
id: 2851
title: "`nros-launch-resolve` is built by no step of the normal setup, so a fresh user's first build fails for a missing resolver"
status: resolved
type: bug
severity: medium
area: [cli, setup]
related: [phase-589, issue-0409, issue-0797]
found: 2026-11-23
resolved: 2026-11-24
---
Found by a downstream proof of concept on an embedded Linux camera SoC (HiSilicon Hi3519DV500: 2x Cortex-A55, Linux 5.10, musl 1.2.3, BusyBox), built as an out-of-tree hosted board; see phase-589.

## What
The resolver is built only by `just setup-launch-resolve`, by
`just post-rebase`, and by `scripts/bootstrap.sh`. `just setup-cli` only warns
when it is stale, and `nros setup` does not mention it. The proof of concept
followed `setup-cli` and its build failed for a missing resolver. Issue 0797
saw the same gap in a CI job; issue 0409 made the failure loud but not absent.

## Fix direction
`just setup-cli` builds the resolver as well, or `nros setup` provisions it,
so one documented step yields both binaries.

## Acceptance
From a fresh clone, the documented setup steps followed by `nros build` of a
launch-file example succeed with no separate resolver step.

## Resolution
`just setup-cli` now builds the resolver too, through `setup-launch-resolve`,
whenever the play_launch submodule is initialised and the resolver is missing
or stale. It runs on the no-op path as well, since a fresh CLI says nothing
about the resolver. With the submodule absent there is nothing to build, and
setup-cli prints the two commands that build it rather than skipping
silently (issue 0409's rule). Measured: with the resolver binary moved away,
`just setup-cli` rebuilt it; a second run was a no-op. AGENTS.md's install
line says so.
