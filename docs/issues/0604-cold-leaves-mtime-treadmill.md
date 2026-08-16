---
id: 604
title: "Cold leaves after every pull: measure how many are genuinely invalidated versus merely re-stamped, before optimising either"
status: open
type: performance
area: build, testing
related: [issue-0509, issue-0466, issue-0442, issue-0445, issue-0627, phase-286, phase-353, phase-363]
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

## MEASURED 2026-08-16 — the fingerprint is innocent; three rows still over-invalidate

This issue asked which of three causes the cascades are. Experiment (b), a
COMMENT-ONLY edit to `nros-cli-core/src/lib.rs` followed by `just setup-cli`, on
a tree whose compile-check fixtures had just been built:

```
fingerprint before: 02d3deaddbb42283ba1a36cf
fingerprint after:  02d3deaddbb42283ba1a36cf     <- UNCHANGED
stale fixtures:     4  ->  7
```

**`codegen-fingerprint` works as designed.** A behaviour-preserving CLI rebuild
does not move it, which is the whole point of caching it by binary hash
(phase-360). The four baseline-stale rows are the west ones, built by the west
lane rather than by `compile-check-fixtures.sh`, so they are unrelated.

**But three rows staled anyway**, and they have a shape:

| row | builder | staled? |
| --- | --- | --- |
| `board_agnostic_run_plan` | `cargo-build` | yes |
| `nav2_compat_smoke` | `cargo-build` | yes |
| `freertos_firmware` | `cross-build` | yes |
| `one_dep_component_pkg` | `cargo-check` | **no** |

So it is specific to the builders that actually BUILD. Two candidates ruled out
by measurement rather than reasoning:

* not the fingerprint — unchanged above;
* not the measured dep-info closure (phase-360 W4) — `dep-closure.py` over
  `board_agnostic_run_plan` reports **zero** entries under `packages/cli` or
  naming the `nros` binary.

Which leaves the source manifest: something under those rows' own directories
changed CONTENT during the rebuild. The likely writer is the `nros sync` /
codegen step that runs as part of building them, emitting a file whose bytes
carry the tool's identity. Issue 0562 made sync skip byte-identical writes, so
whatever moved is genuinely different bytes, not a restamp.

### Next step, precisely

Diff a row's directory across a behaviour-preserving CLI rebuild:

```sh
find packages/testing/nros-tests/fixtures/n_board_agnostic_run_plan -newer <marker> -type f
```

and read what changed. That names the file that carries the tool identity into
the signature, which is the last piece of the attribution this issue exists for.

### Scale check, so nobody over-reads this

3 rows of ~36 over-invalidate on a behaviour-preserving rebuild. The six
cascades that motivated this issue each followed a `git pull` or `rebase` that
brought REAL CLI changes, so most of that cost was legitimate invalidation, not
this. The remedy order should follow that: the treadmill is mostly correct
behaviour, and this is a narrow leak.

## ATTRIBUTED 2026-08-16 — the largest cause was upstream of every fixture

The table above stops at "three rows over-invalidate", which is a narrow leak.
Pulling on the first row of the earlier table instead — *`git rebase` → the
in-tree CLI ("the checkout moved")* — found a much larger one, and it is not a
fixture problem at all.

A commit touching only `packages/core/nros-node/src/executor/spin.rs` (issue
0589, a diagnostic sink swap) reported the CLI stale. The CLI does not compile
`nros-node`. Reconstructing `cli_source_dirs()`'s textual `path = "…"` walk and
diffing it against `cargo metadata`:

```
textual walk : 23 dirs outside packages/cli
cargo resolve:  8
```

**17 crates were watched that the CLI never compiles** — every platform port,
`nros-node`, `nros-log`, `nros-smoltcp`, `mps2-an385-pac`, `zpico-alloc`,
`nros-ghost-types`, three generated msg crates — all reached through ONE
`optional = true` edge the walk could not see. Those are among the
most-edited crates in the repo, so this fired constantly, and a stale CLI is the
cascade's source: it re-stales what keys on it, and `check-tier-preconditions`
puts it first for exactly that reason.

The same diff, from the other side, found **2 crates the CLI does compile and
the stamp was blind to** (`nros-core`, `nros-rmw`, reached by a
`workspace = true` dep with no `path =` on the line). That half is a correctness
bug, not a cost: `setup-cli` reported success without rebuilding.

Filed and fixed as issue 0627 — the closure now comes from `cargo metadata`
(`packages/cli/cli-source-dirs.txt`, gated by `check-cli-source-dirs`) rather
than a walk.

**In this issue's three-category framing this was category three,
over-invalidation, and it sat above the fixtures rather than among them.** Which
is why the per-row diff the section above prescribes had not found it: it was
looking inside the rows.

### Still open here

The 3-of-~36 residue in the section above. It is unaffected by 0627 — those rows
staled after `just setup-cli` with the codegen fingerprint UNCHANGED, and the
fixture signature's tool component is that fingerprint, not the CLI stamp. The
prescribed next step (diff a row's directory across a behaviour-preserving CLI
rebuild) still stands.
