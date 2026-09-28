---
id: 1548
title: "The `setup-qemu-patched` composite action runs `just` unsourced and apt-installs indexed packages — rules its workflows obey"
status: open
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
