---
id: 2369
title: "A GitHub release 504 during `nros setup` fails template-copy-out and ejects merge-queue entries; the fetch retry gives up after ~15 s"
status: open
type: bug
severity: medium
area: [ci, cli, setup]
related: [2129, phase-489]
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

## Directions (not decided)

- Lengthen the retry budget for a 5xx specifically (minutes, not seconds),
  bounded so a real outage still fails.
- Cache the SDK store across merge-group jobs (`actions/cache` keyed on the
  index pins), so a merge group downloads nothing that the last one already had.
- Both: the cache removes most fetches, the budget covers the rest.

## Acceptance

A merge group whose release downloads 504 for a few minutes still passes
`template-copy-out`, or does not download at all.
