---
id: 2467
title: "`nros build --zephyr-workspace <ws>` runs its configure without the flag, so a host with only that workspace refuses with \"no Zephyr found\""
status: resolved
resolved: 2026-11-12
resolved_in: "the configure that `nros build` runs carries the caller's `--zephyr-workspace` (SyncArgs, OutputFlags)"
type: bug
area: [cli, zephyr, ci]
severity: medium
found: 2026-11-12
related: [phase-530, phase-531, 2193, 1475]
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)**, whose
W1 made `nros build` configure first (RFC-0120 D3). This is a flag that W1's
configure did not carry.

## Measured

The nightly's `zephyr copy-out check (4.4)` job failed on every scheduled run
from 2026-11-10 to 2026-11-12 (runs 42760153000, 42963678320,
43159304017). The 11-09 failure was a separate 504 on an SDK download.

```
Error: configure: image `nros_zephyr_talker_c_bringup:zephyr_zenoh`

Caused by:
    no Zephyr found, so `west build` cannot be run for `zephyr_zenoh`.
    ...
    (searched: --zephyr-workspace, $ZEPHYR_BASE, $NROS_ZEPHYR_WORKSPACE,
     $NROS_HOME/workspaces/zephyr/<line>, ...; <line> is $NROS_ZEPHYR_VERSION,
     default 3.7)
FAIL: copy-out build did not reach zephyr.elf (rc=1)
```

`scripts/zephyr/check-copy-out.sh` resolves the 4.4 workspace itself and
passes it: `nros build <bringup>:zephyr_zenoh --workspace <copy>
--zephyr-workspace <4.4 ws>`. The container has only the 4.4 workspace
(`NROS_ZEPHYR_VERSION=4.4 just setup zephyr`). The job's own step sets no
`NROS_ZEPHYR_VERSION`.

## Why

`nros build` runs `configure::ensure_current` before it plans (RFC-0120 D3).
For each image, configure's stage 4 plans the build with `configure::build_args`,
which set `zephyr_workspace: None` ("Stage 4 never runs west, so no Zephyr
is needed and none is looked for"). That planning does resolve a Zephyr, in
two places:

- stage 3's preflight (`check_zephyr_lang_rust`, `check_zephyr_runtime`'s
  remedy), and
- the west handoff, which refuses with "no Zephyr found" on any run that is
  not `--dry-run` (`build.rs`, "Resolved here, ENFORCED at exec").

Without the flag, both fall back to the ladder. On the default line (3.7)
the ladder's store rung is `$NROS_HOME/workspaces/zephyr/3.7`, absent on a
4.4-only host, so configure refused for a workspace the user had named. The
same flag was also dropped from the stamp's skipped-image preflight
(`configure_stamp::real_preflight`).

It started on 2026-11-10. Phase-531 W6 (`3eefb44f93`) made
`examples/zephyr/c/talker` a one-package workspace, so the copy-out check goes
through `nros build` and its implicit configure. Before that, it built the leaf
with `west` directly.

## Fix

The caller's Zephyr workspace now travels with the configure. It is a
`#[arg(skip)]` field on `ws::SyncArgs`, like Ruling R45's interface search
path: an argument set by the caller, not a CLI flag of `nros configure`, and
never a process variable.

- `configure::ensure_current(.., zephyr_workspace)` sets it, and
  `nros build` passes `args.zephyr_workspace`. The census verb passes `None`,
  because it plans a host image.
- `configure_project` plans each image with it.
- `OutputFlags` carries it outside its digest `pairs`, so
  `real_preflight` re-runs a skipped image's preflight against the same
  Zephyr. It changes no stamp digest: a unit test asserts that.

## Verified

- The nightly's shape, reproduced on newslab-118, which has only the 3.7 store
  workspace. A copied-out `examples/zephyr/c/talker` was built with
  `NROS_ZEPHYR_VERSION=4.4` (a line with no store workspace there) and
  `--zephyr-workspace` naming the 3.7 workspace:
  - with main's CLI, rc 1: `Error: configure: image
    `nros_zephyr_talker_c_bringup:zephyr_zenoh`` / `no Zephyr found`, the
    nightly's error exactly;
  - with this fix, rc 0: `west build` ran, and the build produced
    `zephyr.elf` and `zephyr.exe`.
- `configure_stamp::tests::output_flags_are_digest_inputs`: the flag reaches
  `OutputFlags` and leaves the digest unchanged.

Acceptance: the nightly's `zephyr copy-out check (4.4)` reaches `zephyr.elf`.

Sweep: `git grep -n 'zephyr_workspace: None' -- packages/cli/nros-cli-core/src`.
After the fix, the remaining sites are:

- `configure::build_args`, whose callers now set the field;
- `materialize` and `image_facts`, both `dry_run: true` and run by verbs
  that take no Zephyr workspace, so the handoff check never fires there;
- test fixtures in `build.rs`.
