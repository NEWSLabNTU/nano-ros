---
id: 1755
title: "After the runner container restart, two tier-1 steps cannot create directories under `/home/runner/.cache`: Miri's sysroot and `nros::main!`'s model cache"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-08
related: [1685, 1387, 1457]
---

## Measured

`run-matrix` run **37685900447** on `main` (workflow_dispatch, 2026-10-07 20:57Z),
job **113036322535** `tier 1 (cells)`, runner `nano-ros-runner`. This was the
first tier-1 run on the self-hosted runner after its container was restarted.
Two failures in that run share one shape: a directory under `$HOME/.cache`
cannot be created.

**`nros::main!` model cache.** This is a real failure in
`nros-tests::native_main_macro_misuse::resolves_the_model_from_inputs_without_a_build_system`:

```
error: nros::main!: create /home/runner/.cache/nano-ros/models/70a3413b8d788475-demo_bringup: Permission denied (os error 13)
error[E0601]: `main` function not found in crate `demo_entry`
```

**Miri sysroot.** This happened in the `test-miri` step:

```
CARGO_PROFILE_DEV_OPT_LEVEL=0 cargo +nightly-2026-04-11 miri test -p nros-serdes -p nros-core -p nros-params
Preparing a sysroot for Miri (target: x86_64-unknown-linux-gnu)... fatal error: failed to build sysroot: failed to create target directory
error: recipe `test-miri` failed on line 3947 with exit code 1
```

Miri builds its sysroot under the user cache directory by default
(`~/.cache/miri`), the same parent directory as the model cache.

## What it is NOT

- **Not the whole `~/.cache`.** The same job used `/home/runner/.cache/sccache`
  (`30.3 GiB … self-capped`), so the directory exists and is usable at the top
  level.
- **Not a code change.** Neither path moved on `main`. Both failures arrive
  together on the first run after the container restart.

## Likely cause (not yet measured)

The restarted container reuses a persistent `/home/runner`, but its uid differs
from the one that created `~/.cache/nano-ros` and `~/.cache/miri` (or their
parents). The same restart left a build dir behind:
`build/zephyr-workspace-builds/3.7/build-cortex-m-c-talker-zenoh` still names
the old container's checkout `/home/runner/src/nano-ros`
(`check-zephyr-workspace-foreign-checkout`, same run, job 113013514754).

To confirm, run this on the runner:

```sh
ls -lnd /home/runner/.cache /home/runner/.cache/nano-ros /home/runner/.cache/miri
id
```

## Cause (measured 2026-10-11)

The engine creates a missing mount point, and every missing parent, as ROOT.
The image has no `/home/runner/.cache`, and `runner-container.sh --run` mounts
`nros-runner-sccache` at `/home/runner/.cache/sccache`, so every container
starts with a root-owned `~/.cache`. The sccache volume inside it is writable,
which is why the same job could use sccache. Reproduced against the local
`nano-ros-runner:local` image as uid 1001:

```
drwxr-xr-x 3    0    0 /home/runner/.cache
mkdir: cannot create directory '/home/runner/.cache/nano-ros': Permission denied
```

No uid change is involved; the "likely cause" above was wrong. The restart
mattered only because it was the first container built from an image without
the directory.

## Fix

- The generated Dockerfile creates every `-v` mount point under
  `/home/runner` before its `chown -R runner:runner`. A layer doing the same
  over the old image was measured: `~/.cache` and `~/.cache/sccache` come up
  owned by 1001, and `mkdir -p ~/.cache/nano-ros/models ~/.cache/miri`
  succeeds.
- `runner-doctor.sh` checks, for every runner whatever its labels, that the
  user cache dir can hold a new directory. Its self-test covers both directions
  (a read-only `.cache` fails; a writable one passes and is left empty).

It closes when the runner image is rebuilt
(`scripts/ci/runner-container.sh <labels> --build`, then `--run`) and the next
tier-1 run creates both directories.

## What would close it

- The next tier-1 run creates both directories. That means either fixing their
  ownership on the runner, or deleting them so they are recreated.
- The runner image or provisioning makes `$HOME/.cache` owned by the running
  uid on every start, so a restart cannot reintroduce this. The fix belongs in
  the image (CLAUDE.md: a missing dependency on a self-hosted runner is an
  IMAGE fix, never a host fix).
