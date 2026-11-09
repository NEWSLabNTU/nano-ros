---
id: 2369
title: "A GitHub release 504 during `nros setup` fails template-copy-out and ejects merge-queue entries; the fetch retry gives up after ~15 s"
status: open
type: bug
severity: medium
area: [ci, cli, setup]
related: [2129, phase-489, phase-466]
found: 2026-11-09
---

**Home:** [phase-489](../roadmap/phase-489-ci-build-speedup.md)

## What

The merge-group-only `check / template-copy-out` jobs run `nros setup` and
download prebuilt SDK dists from `NEWSLabNTU/nano-ros-sdk` GitHub releases on
every merge group. Twice on 2026-11-09 those downloads got HTTP 504 on all
three attempts, the job failed, the aggregator `CI` failed, and the merge queue
ejected entries whose changes had nothing to do with it:

- run 42570508750 (06:06Z), `template-copy-out [zephyr]`: `xrce-agent`
  2.4.3-nros1 and `cyclonedds` 0.10.5-nros1, each
  `curl: (22) The requested URL returned error: 504` x3. Ejected #3317.
- run 42580213285 (07:40Z), `template-copy-out [1/3]`: `arm-none-eabi-gcc`
  13.2-nros5; `template-copy-out [zephyr]`: `cyclonedds` 0.10.5-nros1. Ejected
  #3321.

- probe.yml run 42584277829 (08:12Z), both tracks, step `Install just (if
  absent)`: `scripts/ci/install-just.sh` fetches the `just` release asset with
  a fixed `--retry 5 --retry-delay 5`, and got `curl: (22) ... returned error:
  504` six times, 5 s apart (25 s), so the lane stopped before its probe ran.

The same URLs returned 200 when probed by hand minutes later, so the outage
window is longer than the retry budget, not permanent.

## Why it fails

`run_fetch` in `packages/cli/nros-cli-core/src/orchestration/sdk_store.rs`
(issue 2129) retries a transient network failure `FETCH_ATTEMPTS = 3` times
with a doubling backoff starting at `FETCH_BACKOFF`, i.e. about 5 s + 10 s.
That covers a DNS blip, the case 2129 measured, and not a GitHub release-CDN
504 window, which lasts minutes. Every merge group pays the full download, so
each such window ejects every queued entry. The `queue-notify` comment already
says "almost certainly not your pull request", but the entry is still out and
someone must re-queue it.

## Fix

A server that answers 429 or 5xx is having an outage, which lasts minutes. A
network that fails (DNS, a reset, a timeout) is a blip, which lasts seconds.
The two now get different budgets:

- `run_fetch` (`sdk_store.rs`) classifies a failure as `Transient::Network`
  or `Transient::ServerOutage` (`transient_failure`). A network failure keeps
  issue 2129's 3 attempts. A server outage is retried until 5 minutes of
  waiting are spent (`SERVER_OUTAGE_BUDGET`), with doubling waits capped at
  60 s (`SERVER_OUTAGE_MAX_WAIT`): 5, 10, 20, 40, 60, 60, 60 s, so 255 s over 8
  attempts. A 404 or a missing ref still fails at once.
- The shell fetches of a GitHub release asset get the same bound:
  `scripts/ci/install-just.sh` (the probe run above),
  `scripts/probe/run-bootstrap-probe.sh` (the installed track's published
  release, which had no retry), and `docs.yml`'s mdbook-mermaid (no retry and
  no `-f`, so a 504 body went into `tar`). Each uses `--retry 8
  --retry-all-errors --retry-max-time 300`, curl's own 1 s doubling: 255 s of
  waiting. `--retry-max-time` alone does not bound it: against a local
  server that always answers 504, a 10 s limit stopped at 15 s, because curl
  starts a backoff that runs past the limit. So the retry count is the real
  bound.

Tested: `fetch_retry_tests` gains a 504 retried past the network's three
attempts until the budget is spent, a 504 window longer than the network
budget waited out, a 404 failing at once, and the classification itself.
With the old 3-attempt rule restored for a server outage, both 504 tests
fail.

Sweep: `git grep -nE -- '--retry( |-delay)|curl .*releases/download' -- scripts .github packages/cli`.
`scripts/sdk/measure-dist-floor.py` already retries 8 times with
`--retry-all-errors`. The rest are test fixtures or fetch no release.

## Follow-up: cache the store across merge groups (proposed, 2026-11-10)

The retry above covers an outage. A cache would avoid most of the fetches in
the first place. This section is measured, and the proposal was agreed with
the phase-489 lead before any workflow is written.

**What each merge group downloads.** Read from merge-group gate run
42639225486, all four `template-copy-out` shards (the `nros setup` lines in
each job log). Sizes are the release assets' `Content-Length`:

| shard | fetches | size |
| --- | --- | --- |
| `[zephyr]` | zephyr-sdk 0.16.8 (zephyrproject's own release) | 1378 MiB |
| `[zephyr]` | renode 1.17.0 | 60 MiB |
| `[zephyr]` | clang-format 17.0.6-nros1 | 1 MiB |
| `[1/3]`, `[2/3]`, `[3/3]` | arm-none-eabi-gcc 13.2-nros5, once per shard | 192 MiB x 3 |
| all four | corrosion 0.6.1-nros1, built from source (no prebuilt) | time, not bytes |
| (this issue's runs) | xrce-agent 2.4.3-nros1, cyclonedds 0.10.5-nros1 | 3 MiB, under 1 MiB |

That is about 2 GiB and four corrosion source builds per merge group.

**The budget.** The repository's Actions cache holds 8.6 of 10 GiB
(`gh api repos/NEWSLabNTU/nano-ros/actions/cache/usage`, 2026-11-09). It is
almost all `nros-cli-Linux-*` target dirs (about 3 GiB each) and
`sccache-Linux-check-*` (about 2 GiB each), and those are re-keyed on most
pushes.

**Proposal.** One shared entry of about 200 MiB, which every copy-out shard
restores and only a cache miss saves:

- Contents: the store dirs of the NEWSLabNTU-hosted dists the shards fetch
  (arm-none-eabi-gcc, xrce-agent, cyclonedds, clang-format) and the built
  corrosion. These are the rows whose fetch can 504, plus the one that costs a
  build.
- Key: `sdk-store-` plus a hash of exactly those `[tool.*]` rows of
  `nros-sdk-index.toml` (version, and `dist.linux-x86_64` url and sha256), so
  it moves only when one of those pins moves. Between pin bumps every merge
  group hits.
- Not cached: zephyr-sdk (1378 MiB, 14 % of the whole budget, from a
  different CDN). It stays a download, which this issue's retry now covers.

**Eviction.** GitHub evicts least-recently-used entries once the repository
passes 10 GiB. A 200 MiB entry read by every merge group stays recent, so it
outlives the 3 GiB CLI and 2 GiB sccache entries that churn around it. The
risk runs the other way: one more save of a 3 GiB `nros-cli` entry already
puts the budget over the limit, and evicts whatever was least recent. So the
same change should also bound the CLI entries. They should be saved from
`main` only, or carry a narrower restore prefix, so stale per-PR copies stop
filling the budget. Measure `cache/usage` before and after.

## Acceptance

A merge group whose release downloads 504 for a few minutes still passes
`template-copy-out`, or does not download at all.
