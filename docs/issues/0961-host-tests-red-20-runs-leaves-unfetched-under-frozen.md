---
id: 961
title: "`host-tests` red for 20 consecutive runs — workspace-fixture leaves are never fetched, and `--frozen` forbids fetching them"
status: open
area: build
severity: high
found: 2026-08-31
related: [0359, 0378, phase-407]
---

# The only CI lane that runs E2E has not run a test in 20 runs

Every completed `host-tests` run in the last 20 is `failure`, and every one dies
in the same step — **Build workspace fixtures** — so the integration tests after
it have not executed at all.

The visible error looks like a network flake:

```
error: failed to download `toml v0.9.12+spec-1.1.0`
error: failed to download `allocator-api2 v0.3.1`
```

It is not. The cause is one line further down:

```
Caused by:
  attempting to make an HTTP request, but --frozen was specified
```

`--frozen` is `--locked --offline`. `--locked` is injected project-wide by the
`scripts/bin/cargo` PATH shim via `NROS_CARGO_FLAGS` (issues 0359/0378), which is
correct and should stay. The defect is that the crates are **not in the registry
cache when that step runs**.

## Why these crates specifically

`examples/workspaces/*` leaves are SEPARATE cargo workspace roots. CI's
provisioning (`nros setup --source …`, the root build) populates the cache for
the ROOT workspace's graph — and the root `Cargo.lock` does contain
`toml 0.9.12+spec-1.1.0`. But the leaves resolve their own graphs, including
crates the root never pulls, and nothing fetches for them before the
`--frozen` build.

`allocator-api2 v0.3.1` is the sharper evidence: it appears in NO tracked
lockfile except `packages/cli/*`. So the leaf graph wants a crate that no
lockfile CI has fetched from mentions.

## Why it was not obvious

* it reproduces only under `--offline`; a developer's local
  `just build-test-fixtures` succeeds because it is allowed to download, which
  is why this never showed up in local sweeps;
* the surface message says "failed to download", which reads as a network
  problem and invites a retry that cannot help;
* the lane is UNIFORMLY red, so it has no signal capacity — a regression landing
  in it looks exactly like yesterday's failure (CLAUDE.md's red-lane class).
  `post-submit`'s tier-2 job is separately skipped on the self-hosted interlock,
  so between them **no fixture or E2E test runs anywhere in CI**.

## Work

1. Fetch for the leaves before the `--frozen` build — a `cargo fetch` per
   workspace root, driven by the same list `build-workspace-fixtures` walks, so
   the two cannot disagree about which roots exist.
2. Do NOT relax `--locked`. A lockfile that changes because CI could not resolve
   is the drift 0359/0378 exist to prevent; the fix is fetching, not permitting
   re-resolution.
3. Make the diagnosis legible: the step should say "not in the cache and
   `--offline` forbids fetching" rather than leaving `failed to download` as the
   headline.
4. Consider whether a leaf SHOULD pull a crate the root never sees — if the
   answer is no for `allocator-api2`, there is a second defect underneath this
   one.

## Verification note

Fixing this cannot be confirmed locally: the failure requires the offline
condition CI imposes. Reproduce with `--offline` against a cold `CARGO_HOME`, or
verify on the lane itself.
