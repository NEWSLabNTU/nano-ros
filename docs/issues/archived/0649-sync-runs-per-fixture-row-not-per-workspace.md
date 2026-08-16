---
id: 649
title: "`nros sync` ran once per fixture ROW while it is per-WORKSPACE, so one directory was synced 22 times per build"
status: resolved
type: performance
area: build
related: [issue-0641, issue-0645, issue-0646, phase-214, phase-244]
---

## Measurement

A transparent counting shim on `$NROS_CLI` (the documented override in
`nros_cli_bin`, so it intercepts every call site) across a clean
`just build-test-fixtures lane=native`:

```
185 invocations, 69 distinct targets   ->  116 repeats (63 %)

 42 targets x1     4 x6      2 x10
 12 targets x2     1 x7      1 x11
  1 target  x3     3 x8      1 x22
  2 targets x4
```

## Why

Two independent drivers loop over fixture ROWS and sync the row's DIRECTORY:

* `scripts/build/fixtures-build.sh` — inside `nros_fixture_build_one`, the
  function each make leaf runs;
* `scripts/build/workspace-fixtures-build.sh` — inside `build_workspace`.

`nros sync` is per-WORKSPACE. Its outputs — generated msg crates, the
`[patch.crates-io]` config, resolved SystemModels — do not vary by the
platform/rmw/lang coordinate that distinguishes one row from another. So the
count tracks `fixtures.toml` rows, not work:

| workspace | rows | syncs |
| --- | --- | --- |
| `features` | 24 | 22 |
| `rust` | 16 | 11 |
| `cpp` | 13 | 10 |
| `c` | 13 | 10 |
| `mixed` | 10 | 8 |
| `safety` | 7 | 7 |

The per-row placement was not simulating a user procedure, which was the
hypothesis worth testing: a user runs `nros sync` once and then builds. Running
it 24 times simulates nothing anybody does.

## Checked before hoisting, not assumed

**Nothing row-specific reached sync.** In `fixtures-build.sh` the call passed
only `NROS_REPO_DIR`, constant for the run — never the row's `envstr` or `args`.
In `workspace-fixtures-build.sh` the row's `env` is exported at line 250 and the
sync is at line 237, i.e. **above it**; the comment there records that env
reaching `codegen-system` was the fix for issue 0257, and sync was never in that
scope.

**Concurrent same-directory syncs were already safe.** Every row is an
independent `.PHONY` make target under `all:`, so rows sharing a directory could
sync it in parallel. Eight parallel syncs of one workspace, warm AND cold:
8/8 exit 0, no errors, and the resulting `generated/` tree hashed identically to
a reference sync. So this removes waste rather than a race — worth knowing,
because it means the code was wasteful and not broken. Hoisting makes it serial
regardless, which is better than depending on that.

## Fix

One pre-pass per driver, in the parent, before the rows fan out — the same move
(and the same rationale) as the Node-pkg pre-pass already in `fixtures-build.sh`:
*"Pre-sync every Node pkg … once, in the parent before the build fans out."*

`workspace-fixtures-build.sh` already computed `group_dirs`, "distinct workspace
dirs, order-preserving", for the shared cargo group; the pre-pass reuses it
rather than deriving a second copy of the same set.

```
185 -> 159 -> 101 invocations, 69 targets throughout
```

72 % of the redundancy gone, build green at each step.

## What remains, and why it is a different problem

101 invocations for 69 targets: 32 repeats, all 2–4x, and they are ACROSS
drivers rather than within one. `examples/native/rust/talker` is synced by
`regenerate-bindings.sh`, by `fixtures-build.sh`'s Node-pkg pre-pass, by its
row pre-pass, and by `just/native.just` — each legitimately ensuring its own
precondition, each in a separate process.

Removing those needs a cross-process freshness stamp: "this workspace's msg
inputs have not changed since the last sync". That is the other half of the
original suggestion, and it is a real design step rather than a hoist — a wrong
digest leaves a stale `generated/` tree compiling against the wrong shape, which
is exactly what phase-214.J was about, and what `nros_codegen_stamp_check_or_wipe`
exists to catch AFTER the fact. The existing stamp gates the WIPE, not the sync.

## Correction worth keeping

The first fix predicted 185 -> ~69 and delivered 185 -> 159, because only one of
the two drivers had been found: `features` was still at 22, from the loop in
`workspace-fixtures-build.sh`. The census is what caught it — the prediction
would have been reported as success without it.
