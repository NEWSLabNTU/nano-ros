# Phase-331 W1 baseline — captured 2026-08-02T06:42:37Z
base commit: 82b82a6d6 (docs(rfc-0066): example and fixture consolidation)

## Fixture manifest
single-node [[fixture]] rows : 251
workspace  [[workspace_fixture]] rows : 86

## Workspace directories
workspace dirs : 35
large four:
  rust   node pkgs: 10
  c      node pkgs: 9
  cpp    node pkgs: 9
  mixed  node pkgs: 10

## Lane selection
tier1 coords : 10
tier1 modules: native 
tier2 coords : 12

## Cold build (W1 measurement)

```
just build-test-fixtures lane=native   (workspace dirs wiped from the manifest first)
  BUILD_EXIT      0
  WALL_SECONDS    7051   (1h 57m)
  native stage    5912 s (84% of wall; remainder = generate-bindings,
                         setup-launch-resolve, zenoh-posix fixture, compile-checks)
  fixtures built  64
  errors          0
```

Prerequisites that had to be rebuilt first (the checkout had moved 51 commits):
`just setup-cli` (48 s) then `just setup-launch-resolve` (15 s). The stale-CLI
guard (issues 0363/0197) caught this in 1 s and refused to auto-rebuild.

Note: `setup-cli` warned the resolver was older and must be rebuilt, but
`setup-launch-resolve` returned 0 without rebuilding — its own probe watches
resolver sources, which those commits did not touch. The two recipes disagree;
the rebuild was forced by hand. Worth a fix in issue-0387 territory.
