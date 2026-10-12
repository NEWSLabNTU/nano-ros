---
id: 1840
title: "An inherited `nano_ros_ROOT` from another checkout outranks the worktree — W2d stopped exporting it, which also stopped re-rooting it"
status: resolved
type: bug
area: [build, tooling]
related: [1280, 1391, 1593, 1638, phase-484]
---

## What happened

Measured 2026-10-12 by a verification agent, then re-measured here.

Agent shells in this repo inherit `nano_ros_ROOT=<main-checkout>`,
`NROS_REPO_DIR` and the source-tree paths (`FREERTOS_DIR`, `NUTTX_DIR`, …)
from the main checkout, whose older `activate.sh` / direnv exported them.

Since f039374550 (phase-484 W2d, RFC-0103 D6 amended 2026-10-12)
`scripts/sdk-env.sh` deliberately does not export the source-tree variables
(the index's `[source.*] env` names, W2c) or the `# just-only:` names
(`NROS_REPO_DIR nano_ros_ROOT`). Not exporting a name also meant not
re-rooting it, so issue 1280's rule never saw it: an inherited value naming a
DIFFERENT checkout survived `source ./activate.sh` in a worktree untouched.

`scripts/build/cmake-incremental.sh` derived `nano_ros_ROOT` only
`if [ -z "${nano_ros_ROOT:-}" ]` — a set value was a trusted one. So in a
worktree, outside `just`:

```
export nano_ros_ROOT=<main-checkout> NROS_REPO_DIR=<main-checkout>
source ./activate.sh
scripts/build/fixtures-build.sh linux c        # rc=2
```

configured the C fixtures with `nano_ros_DIR=<main-checkout>` and died
on

```
builtin_interfaces_msg_duration.h:24:2: error: #error "nros: this generated tree
was emitted at a codegen version the runtime does not accept …"
```

(the foreign runtime accepts codegen 10..11, the worktree CLI emits 12).
Reproduced on `origin/main` (53c7ea4482). Through `just` it was fine: the root
justfile exports `nano_ros_ROOT := _NROS_HERE`.

## Why a correct run did not repair it

The configure poisons the WHOLE cache, not one entry. A dir configured that
way held 11 distinct values in the foreign checkout — `nano_ros_DIR`,
`NROS_PLATFORM_CFFI_INCLUDE`, `NROS_PLATFORM_IMPL_DIR`, `NROS_RMW_CFFI_DIR`,
`CYCLONEDDS_SOURCE_DIR`, `NROS_RMW_CYCLONEDDS_MSG_TO_IDL`, `NanoRos_SOURCE_DIR`
and ~20 `_NROS_*:INTERNAL` module dirs — plus non-path answers the older cmake
gave (`NANO_ROS_RMW` / `NANO_ROS_FEATURES`, which this checkout's
`NanoRosDeprecatedNames.cmake` then refuses as disagreeing). `find_package`
never re-searches a found `nano_ros_DIR`, and the configure stamp did not
record the root (it reaches cmake through the environment), so nothing
reconfigured. Measured: poison `examples/native/c/talker/build-p1840-poison`
with the pre-fix scripts, then re-run them with every foreign variable unset —
still `nano_ros_DIR=<main-checkout>`, still the `#error`, rc=1. Issue
1593's stickiness class.

## Resolution

One rule, no new spelling: `scripts/lib/checkout-paths.sh` (outside any
checkout → keep; this checkout → keep; another checkout → re-root).

1. **Activation UNSETS a foreign value of an unexported name.**
   `scripts/sdk-env.sh` classifies every skipped name by the rule; one inside
   another checkout is unset, with one line naming them. Re-rooting would be
   activation exporting exactly what RFC-0103 D4 ("our tooling never EXPORTS
   a location into the user's shell") and D6 (`$NROS_REPO_DIR` and an exported
   `nano_ros_ROOT` "retire as inputs") delete; every resolver locates the tree
   itself once the value is gone. Out-of-tree values (a real SDK) are kept.
   `activate.fish` evals `sdk-env.sh --fish`, which now emits `set -e <name>`
   (and `--shell` emits `unset <name>`), with the note as code because fish
   discards that script's stderr. The name list has ONE derivation now,
   `scripts/lib/unexported-env-names.sh`, shared by both callers below.
2. **A SET value is not a trusted one.** `cmake-incremental.sh` re-roots every
   unexported name at source time for its own children — the same thing
   `just/sdk-env.just` does for a recipe, which D4 scopes out of "the user's
   shell" — and `nros_cmake_resolve_nano_ros_root` puts a set `nano_ros_ROOT`
   through the rule against the source dir's checkout. Sweep
   (`git grep -nE '\$\{(nano_ros_ROOT|NROS_REPO_DIR)[:-]' -- scripts just`, then
   the source-tree names): `codegen-stamp.sh`, `workspace-fixtures-build.sh`,
   `regenerate-bindings.sh` and `nuttx/build-nuttx.sh` trusted a set value too
   and now re-root it; `build-root.sh` and `cargo.sh` already did.
   `dev/measure-fixture-build.sh` used `NROS_REPO_DIR` as its "activated?"
   sentinel, which since W2d refused every activated shell and passed an
   inherited foreign one; it asks for `nros` on PATH instead. `just` recipes
   were already right (`sdk-env.just` re-roots them).
3. **The cache.** The configure stamp records the resolved `nano_ros_ROOT`, so
   a root move reconfigures (and every dir stamped before this did). On that
   configure path `cmake-cache-guard.sh` rule 3 drops `CMakeCache.txt` — only
   the cache, objects stay — when any cached absolute value is in another
   checkout by the rule. The cache and not a list of `-U` names, because the
   non-path entries are not attributable to a checkout one by one. Cost
   0.19 s per configure on a 778-line cache; nothing on the warm path.
   A configure-time `FATAL_ERROR` was the smaller alternative and was not
   taken: it would refuse the very run that is now correct.
4. **Gate.** `check-inherited-checkout-paths` gained two behaviour probes
   against a synthetic foreign checkout: activation (bash, and the `--fish`
   printer) must drop a foreign `nano_ros_ROOT` / `NROS_REPO_DIR` /
   `FREERTOS_DIR` and keep an out-of-tree one; `cmake-incremental.sh` must
   re-root a set foreign root and source-tree path, keep an out-of-tree root,
   stamp the root, and rule 3 must drop a foreign-bound cache and keep a clean
   one.

### Measured

| run | rc | cache `nano_ros_DIR` |
| --- | --- | --- |
| before: foreign env, `activate.sh`, `fixtures-build.sh linux c` | 2 (`#error` codegen version) | `<main-checkout>` |
| after: same command, those 8 poisoned dirs in place | 0 (8 caches dropped and reconfigured) | worktree, all 29 dirs |
| pre-fix poisoned dir, pre-fix scripts, clean env | 1 (sticky) | `<main-checkout>` |
| same dir, ONE run of the fixed scripts, clean env | 0 | worktree |

Mutation: the nine fixed scripts reverted (gate kept) → the gate reports 9
violations, rc=1; restored → rc=0.

Not covered, and named: a `PATH` entry into another checkout outside `just`
(issue 1638's rule runs only in `just/sdk-env.just`). Activation prepends this
checkout's `nros`, so an activated shell is fine; a shell that never activated
in the worktree still resolves the foreign `nros` and fails on it — loudly,
`unrecognized subcommand`, rather than building the wrong tree.

### Sweep

```
python3 scripts/check-inherited-checkout-paths.py
git grep -nE '\$\{(nano_ros_ROOT|NROS_REPO_DIR)[:-]' -- scripts just
```
