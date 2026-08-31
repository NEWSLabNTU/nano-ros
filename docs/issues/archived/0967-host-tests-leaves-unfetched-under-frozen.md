---
id: 967
title: "`host-tests` red for 20 consecutive runs — workspace-fixture leaves are never fetched, and `--frozen` forbids fetching them"
status: resolved
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

## Resolved — 2026-08-31

`nros_warm_leaf_cache` in `scripts/build/workspace-fixtures-build.sh`, called
before both `nros build … --offline` sites. It lives in the BUILD, not the
workflow, so the requirement travels with the thing that has it — a first clone
needs this as much as CI does.

**It warms TWO roots, and measurement is why.** The obvious fix — fetch the leaf
— is half a fix. Against a cold `CARGO_HOME`, a leaf fetch pulled
`allocator-api2` but **not** `toml 0.9.12+spec-1.1.0`:

```
leaf fetch (examples/workspaces/rust)   toml-0.9.12 absent   allocator-api2 PRESENT   185 crates
+ repo-root fetch                        toml-0.9.12 PRESENT  allocator-api2 PRESENT   434 crates
```

`toml` reaches the generated root through nano-ros PATH deps — `cbindgen`,
`nros-bridge`, `nros-tests` — whose registry deps live in the REPO ROOT lock,
not the leaf's. Warming only the leaf would have fixed one of the two crates CI
named and looked like a fix.

**`--locked` and `--frozen` are untouched.** The build stays hermetic, which is
the property issue 0676 wants; only this prepare step reaches the network,
exactly as `nros setup --source` already does. Permitting re-resolution instead
would have been the drift issues 0359/0378 exist to prevent.

A failed fetch is a loud WARNING, not fatal: an environment that is offline by
design with a warm cache is legitimate, and failing there would break it. The
warning names the symptom the build will otherwise print, because cargo's
"failed to download" is the misleading headline that cost this lane 20 runs.

Both roots are memoized, so 110 workspace rows pay two fetches, not 220. A warm
cache makes the whole thing a no-op — which is also why the defect never
appeared in a developer sweep.

**Not verified on the lane.** This cannot be confirmed locally: the failure
requires the offline condition CI imposes. What IS measured is the premise —
that the two crates cargo could not download are present in a cold cache after
these two fetches and absent after the leaf fetch alone. Whether `host-tests`
goes green needs a run on the lane.
