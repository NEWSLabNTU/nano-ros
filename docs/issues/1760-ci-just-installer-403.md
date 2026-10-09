---
id: 1760
title: "CI installs `just` by piping `https://just.systems/install.sh`, with no retry, and one 403 failed the probe's installed track"
status: open
type: bug
area: [ci]
severity: low
found: 2026-10-09
related: [1439, 1304]
---

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

Three call sites pipe the installer with `-sSf` and no retry:

- `.github/workflows/probe.yml:71` (checkout track)
- `.github/workflows/probe.yml:98` (installed track)
- `.github/actions/setup-qemu-patched/action.yml:139`

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
