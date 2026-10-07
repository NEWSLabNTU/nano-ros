---
id: 1666
title: "live-peer's board job reaches its run step and the Zephyr CI image has no `cargo nextest`"
status: open
type: bug
area: ci, zephyr
severity: medium
found: 2026-10-03
related: [1474, 1353, 1627]
---

## What happens

`live-peer regression` run **37096165733** (schedule 04:19Z, head `bcd44234a`),
job **111126657921** `rows whose board is NOT this runner` (container
`ghcr.io/newslabntu/nano-ros-zephyr-ci:humble-sdk0.17.4-r5`). The first failing
step is `Run the board cells with a recorded PASS`:

```
== qos_zephyr_ros2_interop_e2e ==
error: no such command: `nextest`
ERROR: nros-tests build/setup failed (nextest exit 101) — not a [SKIPPED] precondition.
```

The container's cargo has no `cargo-nextest`. Every cell this job runs goes
through `just _test-focused`, which calls `cargo nextest`, so the job cannot
produce a verdict for any board cell.

## Why it appears only now

Earlier nights this job died **before** the run step, in `Build the fixtures
those rows resolve` (issue 1627: a west configure counted ok on failure). This
is the first run here that got past the fixture build. The missing tool was
latent behind that red the whole time.

## What it is NOT

- Not disk. The same run's other job (`rows whose board IS this runner`,
  111126431759) did die on `No space left on device` writing the runner's own
  `_diag` log (issue 1353). That is a separate finding in the same run.
- Not a test failure: nextest never started.

## What would close it

- Fix it in the image, not on the host, as CLAUDE.md requires for a missing
  dependency on a self-hosted runner. Add `cargo-nextest` to
  the Zephyr CI image, at the same pinned version the gate image uses, or
  install it in a workflow step before `Run the board cells`.
- Acceptance: a `live-peer regression` run whose board job reports cell
  verdicts (`PASS`/`FAIL`/`[SKIPPED]`), not `no such command`.

## 2026-10-07 — the provisioning step installs nextest and then cannot see it

`live-peer regression` run **37570821619** (schedule 04:18Z, head `edbe96f92`),
job **112629242791** `rows whose board is NOT this runner`. The workflow's
`Provision cargo-nextest` step (added in `0f4839ec8`) now fails before the cells:

```
nros setup --tool cargo-nextest: prebuilt 0.9.143-nros1 (dist linux-x86_64) → /github/home/.nros/sdk/cargo-nextest/0.9.143-nros1
error: no such command: `nextest`
```

The install succeeds; the PATH does not follow. The step sources `activate.sh`
first and installs second, and `activate.sh` puts a store tool's `bin/` on PATH
only if that directory exists when it is sourced. Reproduced locally against an
empty `NROS_HOME`: after creating `sdk/cargo-nextest/0.9.143-nros1/bin/`, the
same shell still resolves another `cargo-nextest`, and only a second `source
./activate.sh` resolves the store copy.

Fixed alongside this entry: the step re-sources `activate.sh` after the install.
The acceptance above is unchanged: a board job that reports cell verdicts.
