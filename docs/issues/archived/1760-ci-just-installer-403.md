---
id: 1760
title: "CI installs `just` by piping `https://just.systems/install.sh`, with no retry, and one 403 failed the probe's installed track"
status: resolved
type: bug
area: [ci]
severity: low
found: 2026-10-09
related: [1439, 1304, phase-505]
resolved_in: "branch fix/phase-505-w2-net"
---

**Home: [phase-505](../../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## Measured

The `probe` run **37904203449** on `main` @ `ea333ea17` (schedule,
2026-10-09 08:19Z) ran two jobs on GitHub-hosted runners:

- `book probe — checkout track` passed.
- `book probe — installed track` (job **113733618515**) failed in the step
  `Install just (if absent)`. The step never reached the probe.

```
curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh \
  | bash -s -- --to "$HOME/.local/bin"
curl: (22) The requested URL returned error: 403
##[error]Process completed with exit code 1.
```

The 403 is on `install.sh` itself, not on a release download it fetches;
`install.sh` printed nothing. The checkout-track job fetched the same URL in
the same run and succeeded, and the previous day's probe (run 37749062211)
passed both tracks. So this is the third-party host refusing one hosted
runner, not a change in the repository.

## Where the pattern lives

Two call sites pipe the installer with `-sSf` and no retry (a third,
`.github/actions/setup-qemu-patched`, was deleted on 2026-10-14 with the
QEMU submodule it keyed on):

- `.github/workflows/probe.yml:71` (checkout track)
- `.github/workflows/probe.yml:98` (installed track)

`curl --retry` would not help as written: curl does not retry an HTTP 403.

## What it is NOT

- **Not issue 1439 or 1304.** Those are probe verdicts. This job produced no
  verdict, because it stopped before `just probe installed` ran.
- **Not a runner problem.** The job ran on a GitHub-hosted runner, not on
  `nano-ros-runner`.

## What would close it

One spelling for "get `just` onto a CI runner", at all three sites, that does
not depend on a single fetch from `just.systems` succeeding. Options:

1. Download a pinned release asset from `github.com/casey/just/releases`
   (authenticated with `GITHUB_TOKEN`, with `--retry` and `--retry-all-errors`).
2. Use the provisioned toolchain (`nros setup`) if it can supply `just`.

Acceptance: a probe run where both tracks reach their probe step. A run that
injects a failing first fetch should still install `just`.

## Resolution (2026-10-19)

Option 1, as one spelling at every site.

- `scripts/ci/install-just.sh --to <dir>` downloads the pinned release asset
  (`just` 1.57.0, the version ci-base already pinned) from
  `github.com/casey/just/releases`. It uses `curl --retry 5
  --retry-all-errors`, so a 403 or 5xx is retried too. It checks the asset's
  SHA-256 (pinned in the script for x86_64 and aarch64 musl) before extracting,
  and then checks `just --version`.
- Every site uses it: both `probe.yml` tracks, `ci/docker/ci-base/Dockerfile`
  (which piped the installer with `--tag`) and `ci/docker/zephyr-ros/Dockerfile`
  (which piped it unpinned). The zephyr image's tag moves to
  `humble-sdk0.17.4-r6`, with its schedule-only consumers (`nightly.yml`,
  `live-peer.yml`) moved in the same change. `check-ci-image-inputs` holds the
  new `COPY` to each image's `.dockerignore`, push paths, `changes` filter and
  content tag.
- `check-workflow-just-provisioning` now counts `scripts/ci/install-just.sh
  --to` as the installer. It REFUSES `just.systems/install.sh` anywhere in a
  tracked workflow, action or CI Dockerfile (comments excepted), and a
  self-test arm plants that spelling. `git grep just.systems` outside docs now
  finds only that gate.
- Not done: `GITHUB_TOKEN` authentication. A release-asset download is not
  API-rate-limited, and curl drops the header on the redirect to the asset
  host anyway.

Observation is what verifies this: a probe run where both tracks reach their
probe step.
