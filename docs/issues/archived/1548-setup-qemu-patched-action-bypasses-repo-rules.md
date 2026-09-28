---
id: 1548
title: "The `setup-qemu-patched` composite action runs `just` unsourced and apt-installs indexed packages — rules its workflows obey"
status: resolved
resolved: 2026-09-28
type: bug
area: [ci]
severity: low
found: 2026-09-28
related: [phase-472, ]
---

## What happens

`.github/actions/setup-qemu-patched/action.yml`:

- lines 136-139 run `just qemu setup-qemu` without sourcing `activate.sh`, which
  `check-workflow-repo-env` requires of every workflow;
- lines 106-115 apt-install `ninja-build`, `libglib2.0-dev` and `libpixman-1-dev`,
  which `nros-sdk-index.toml` already declares as `[prereq.ninja]`,
  `[prereq.libglib2-dev]` and `[prereq.libpixman-dev]` — what
  `check-workflow-indexed-apt` forbids.

Both gates read `.github/workflows/*.yml` and never composite actions, so the
action is outside both. Pasting its exact apt block into `run-matrix.yml` fails
the indexed-apt gate.

## Fix

Source `activate.sh`; install through the index. The loader fix in phase-472 W1
brings every composite action under both gates.

## Resolution

Fixed 2026-09-28 on branch `fix/1547-1548-zephyr-ci`.

- The action sources `./activate.sh` before `just qemu setup-qemu`, and
  resolves `ninja`, `libglib2-dev` and `libpixman-dev` through
  `scripts/sdk/prereq-packages.py` in the status-capturing
  `if ! pkgs="$(…)"` shape (issue 1466). It builds no CLI, so
  `nros setup --system` was not an option. Unindexed packages stay literal.
- `scripts/lib/workflow_commands.load_workflows(include_actions=True)` also
  returns every local composite action, with its steps under a pseudo-job.
  Both `check-workflow-repo-env` and `check-workflow-indexed-apt` use it, and
  the apt gate dropped its private loader. Rule 2 of the apt gate (status
  capture) now also reads `.github/actions/*/action.yml`.
  `check-workflow-just-provisioning` keeps the default: it already expands
  `uses: ./…` inside the job that provides the prerequisites.
- `check-workflow-repo-env`'s exemption was `"activate.sh" in run`, which a
  COMMENT satisfied. It now needs a real `source`/`.` command naming
  `activate.sh` (or the `./setup.bash` shim), before the first invocation.
  A ROS `setup.bash` does not count. Eight self-test cases were added.

**Mutation evidence** (each reverted afterwards): dropping the new `source`
line gives repo-env rc=1 naming the step; replacing it with the comment
`# not sourcing activate.sh here` also gives rc=1; the whole pre-fix action
gives indexed-apt rc=1 naming all three packages with their `[prereq.*]` keys,
and repo-env rc=1. All three workflow gates are green on the fixed tree.

**Not verified:** the action cannot be run locally and no workflow consumes it
yet, so CI is its first real run. `source ./activate.sh` was checked to
succeed under `bash -euo pipefail` with a minimal environment and no CLI built.
