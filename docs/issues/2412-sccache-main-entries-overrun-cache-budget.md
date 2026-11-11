---
id: 2412
title: "Per-commit sccache entries on `main` hold most of the Actions cache budget, so LRU eviction picks what every other lane loses"
status: open
type: bug
severity: medium
area: [ci]
related: [phase-489, 2369]
found: 2026-11-10
---

**Home:** [phase-489](../roadmap/phase-489-ci-build-speedup.md)

## What

The repository's Actions cache held 10.67 GB of its 10 GB budget on 2026-11-10
(`gh api repos/NEWSLabNTU/nano-ros/actions/cache/usage`, 7 entries). By entry:

| key family | ref | entries | size |
| --- | --- | --- | --- |
| `sccache-Linux-check-<sha>` | `refs/heads/main` | 3 | 2021, 2022 and 2024 MiB |
| `nros-cli-Linux-<hash>` | `refs/heads/main` | 2 | 3455 and 212 MiB |
| `nros-cli-Linux-<hash>` | two pull-request refs | 2 | 212 MiB each |
| `nros-cli-bin-Linux-<hash>` | `refs/heads/main` | 1 | 15 MiB |
| `nros-bindings-v1-…` | `refs/heads/main` | 1 | under 1 MiB |

Over the limit, GitHub evicts the least recently used entries. So the budget's
contents were decided by eviction rather than by any lane's needs, and a new
entry, such as [issue 2369](archived/2369-sdk-release-504-ejects-merge-queue.md)'s
SDK-store cache, could be evicted before anything read it.

## Why

`warm-cache` (`gate.yml`, push to `main`) saves the sccache directory under
`sccache-Linux-check-${{ github.sha }}`. Cache keys are immutable, so a new key
is the only way to update the entry. Every push therefore added a ~2 GB entry,
and nothing removed the previous one. The CLI target dir works the same way,
keyed on a hash of the CLI sources.

Every reader restores through the `sccache-Linux-check-` and
`nros-cli-Linux-` prefixes, which take the NEWEST matching entry, so the older
entries were never read.

## Fix

`scripts/ci/prune-main-caches.py`, run by `warm-cache` right after its saves,
deletes every entry of a family on `refs/heads/main` except two: the one this
run wrote, and the family's newest. Two warm-cache runs can overlap, and an
older run that finishes last must not delete the newer run's entry. If the run
saved nothing, only the newest is kept, so the family is never emptied. Pull-request and merge-queue scopes are never touched. The
step is `continue-on-error`, so a failed prune cannot fail the warm, and the job
gains `actions: write`.

A dry run against the live cache on 2026-11-10 kept
`sccache-Linux-check-20d55a6…` and would have deleted the other two, freeing
4045 MiB. `--self-test` covers the selection: keep the saved key; keep both entries when
an older run finishes after a newer one; keep the newest when nothing was saved;
never delete a key outside the family.

## Acceptance

- After a push to `main`, `actions/caches?ref=refs/heads/main` holds at most
  two `sccache-Linux-check-` entries and two `nros-cli-Linux-` entries (one
  when no warm-cache runs overlapped).
- Cache usage stays under 10 GB, measured after the first prune.
