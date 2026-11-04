---
id: 2249
title: "An existing west workspace never followed the checkout's manifest, so phase-524 W2's hal_stm32 never reached the tier-2 runner"
status: resolved
type: bug
area: [build, zephyr, ci]
severity: high
found: 2026-11-04
resolved: 2026-11-04
related: [1258, 1279, 1387, 2193, phase-466, phase-524]
---

## Measured

run-matrix 41784686434 (schedule, 2026-11-04T06:18Z, `main` at 8c49a148,
runner newslab-118): tier 2 named itself `NO VERDICT: stopped in the build`.
`just build tier2` failed only in the zephyr stage, on one row,
`build-nucleo-f767zi-c-talker-serial`:

```
nucleo_f767zi.dts:9:10: fatal error: st/f7/stm32f767zitx-pinctrl.dtsi: No such file or directory
```

That file is in `hal_stm32`, which df837f0174 (phase-524 W2) added to the 3.7
`west.yml`. The runner's persistent store workspace
(`~/.nros/workspaces/zephyr/3.7`) never fetched it. This host's store workspace
was in the same state: its manifest copy differed from the checkout's `west.yml`
and `modules/hal/stm32` was absent.

## Cause

Two gaps, one after the other:

1. Since issue 1258 the workspace's manifest project is a COPY of `west.yml`,
   not a link to a checkout. `unbind-manifest-project.sh` writes that copy only
   when it is MISSING, so it stays the manifest the workspace was created with.
2. `just zephyr setup` prints `Zephyr workspace already present` when
   `<ws>/zephyr` exists and runs no `west update`. `scripts/zephyr/setup.sh`'s
   own "Workspace exists, updating" path did run one, against the stale copy
   from gap 1, so it fetched nothing new either.

So a manifest change reached a workspace only through `--force`, a full
reprovision. Issues 1279 and 1387 were the same "already present skips more
than the workspace" shape, for the SDK registration and the manifest binding.

## Fix

`scripts/zephyr/sync-workspace-manifest.sh <ws> <checkout>`, called from both
paths:

- The copy is compared with the checkout's file byte for byte. If they are
  equal, nothing runs and nothing touches the network, so an offline host with
  a current workspace is unaffected.
- If they differ, the copy is replaced and `west update` runs in the workspace.
- If `west update` fails, the old copy is put back and the failure propagates,
  so the next run retries instead of reading the workspace as current.

The `[manifest]` reader is now one helper, `scripts/lib/west-config.sh`, shared
with `unbind-manifest-project.sh`.

A store workspace is shared by every checkout on its Zephyr line (RFC-0095 D2).
Two checkouts on different manifests therefore re-sync whenever they alternate.
`west update` never prunes, so a module a newer manifest added stays available
to an older checkout.

Test: `tests/zephyr-sync-workspace-manifest-tests.sh`, on the fast line as
`just check zephyr-sync-workspace-manifest`. It has 14 checks with `west`
stubbed; 9 of them fail against a no-op sync.

An already-provisioned runner picks this up on its next `just zephyr setup`
after the fix merges.
