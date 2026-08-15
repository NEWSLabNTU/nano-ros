---
id: 604
title: "Cold leaves after every pull: measure how many are genuinely invalidated versus merely re-stamped, before optimising either"
status: open
type: performance
area: build, testing
related: [issue-0509, issue-0466, issue-0442, issue-0445, phase-286, phase-353, phase-363]
---

## Why this exists

Issue 0509 is closed: its per-leaf-overhead claim was confirmed and then fixed
(0562 removed sync's restamping, phase-353 W2 removed `west-fixtures.sh`'s
unconditional wipe), and its storage direction was refuted by measurement
(iowait 0.25 % HDD vs 0.03 % NVMe, on a host that meanwhile doubled to 125 GB
RAM). Exactly one of its directions survived:

> **fewer COLD leaves** — a cold leaf costs ~28 s (512 s for 18, measured
> 2026-08-13), and the mtime treadmill is what makes leaves cold after every
> pull, rebase or `git stash`.

That is what this issue carries. It is filed as a MEASUREMENT, not as a defect,
because the premise it inherits may no longer hold.

## The premise may already be half-fixed

Two mechanisms landed since the treadmill was described, and both attack it:

* **Content-aware staleness** (#147 / phase-286 W2, moved into the shared module
  by phase-353 W2 so both arms use one spelling). A `.srcbaseline` sidecar
  records the binary's content hash plus each watched file's
  `(mtime, size, content_hash)`. A moved mtime with unchanged bytes refreshes the
  entry instead of reporting stale — which is precisely what `git pull --rebase`,
  `git stash push/pop` and a branch switch produce.
* **`codegen-fingerprint`** (phase-363). Compile-check and workspace signatures
  hash what the CLI *does*, cached by its binary hash, rather than the binary
  itself. A CLI rebuild that does not change codegen behaviour should therefore
  not invalidate anything.

If both work as designed, a pull should cost far fewer cold leaves than 0509
assumed, and the remaining cost is legitimate invalidation — a genuinely changed
input — which is not a bug at all.

## But the cost is still being paid, and unattributed

Observed on 2026-08-15 while getting a tier-2 run to a verdict, five separate
times:

| after | what went stale |
| --- | --- |
| `git rebase` | the in-tree CLI ("the checkout moved") |
| `just setup-cli` | 24 compile-check fixtures |
| `just native build-workspace-fixtures` | ten compile-checks, then the main set |
| a one-line `clang-format` of a header | every `platform_hdr_*` compile-check |

Each was individually plausible. Nobody has established which were **genuine**
(an input really changed), which were **artifacts** (an mtime moved, bytes did
not), and which were **over-broad** (the CLI binary changed but its codegen
fingerprint did not). Those three want different responses, and the aggregate
"the treadmill is expensive" hides all of them.

## What to measure

1. From a clean, fully-built tree, `git pull` (or `git stash push && pop` to
   simulate) and count leaves that read stale, per family.
2. Attribute each: genuine input change / mtime-only artifact / tool-fingerprint
   over-invalidation.
3. Only then optimise, and only the category that dominates.

`just fixture-staleness` already lists coordinates producing no runtime result,
and is the natural place to grow a `--why` that prints the attribution.

## Do not

Re-run the wall-clock A/B that 0509 warned about: this host produced 50–695 s for
provably identical work, so timing cannot answer any of the above. Count leaves
and name causes.

## Note on 0466

The mtime treadmill was 0466's territory, and 0466 is archived as resolved — its
subject was the ORDERED setup contract, which is genuinely fixed. The cold-leaf
cost outlived it and had nowhere to live; this is that home.
