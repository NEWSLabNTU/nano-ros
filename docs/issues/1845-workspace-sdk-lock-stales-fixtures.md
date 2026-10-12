---
id: 1845
title: "A workspace's `nros-sdk.lock` is untracked and unignored, so each entry build re-stales its sibling workspace fixtures"
status: open
type: bug
severity: medium
area: [tooling, tests]
related: [1262]
found: 2026-10-12
---

## Measured

phase-485 / RFC-0104 D4 has `nros build` write `<ws>/nros-sdk.lock`. In the
in-repo example workspaces that file is neither tracked nor gitignored. Only
the checkout-root `/nros-sdk.lock` is ignored (`.gitignore`, the
`/nros-sdk.lock` line). After one fixture build:

```
?? examples/workspaces/c/nros-sdk.lock
?? examples/workspaces/cpp/nros-sdk.lock
?? examples/workspaces/mixed/nros-sdk.lock
?? examples/workspaces/features/nros-sdk.lock
... (9 workspaces)
```

`scripts/build/source-manifest.sh`'s `nros_source_manifest` hashes
`git ls-files --cached --others --exclude-standard`, so an untracked,
unignored lock is an input to every workspace fixture's signature
(`.nros-workspace-fixture.<id>.inputsig`). Each entry build adds its own
`[cargo."build/…"]` section to the lock, which changes the signature of
every sibling fixture built before it.

Effect, measured on `scripts/build/workspace-fixtures-build.sh linux
{rust,c,cpp,mixed}` followed by the `workspace_features_e2e` /
`workspace_metadata` / `roundtrip_xprocess_e2e` tests: 23 of 35 tests read
STALE fixtures that had been built minutes earlier. A second build pass
leaves the locks byte-identical (md5 checked), and the signatures then
settle, so a sweep needs two passes to be green.

## Fix direction

Decide what the lock IS for an in-repo workspace:

- a build output: ignore `examples/**/nros-sdk.lock` (and the templates'),
  so it leaves the source manifest; or
- a recorded input: track it and keep its content stable across entry
  builds (one write per workspace, not one section per entry build).

Either way, the manifest must not see a file that the build it fingerprints
writes. Sweep: `git status --short -- '*nros-sdk.lock'` after a fixture
build must print nothing.
