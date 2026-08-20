---
id: 200
title: "fixture-build timing campaign — blocked on a big-disk CI runner (phase-226 validation residue)"
status: open
type: task
area: build
related: [phase-226, phase-340, phase-343]
---

## Summary

Phase 226 (fixture build orchestration audit) landed all its scheduler and
cache work, but three 226.F validation measurements could never run on the
maintainer host: a timed clean `just native build-fixtures` alone consumed
~52 GiB of per-RMW-variant cargo target dirs at 25 min (still incomplete,
host at 3.4T/3.6T) and was killed to protect the partition. See the
archived phase doc's 226.F section and `tmp/phase226/results.md`
(2026-06-13).

## UPDATE 2026-08-10 — re-derive the blocker before buying the runner

The ≥200 GiB requirement below is a consequence of the ~21:1 artifact
duplication, and three landed items have moved that number since it was written:

| landed | effect |
| --- | --- |
| phase-343 W1 (`7db7e72b5`) | 425 leaked sizes-probe dirs, **63.1 GiB** recovered, deduplicating 81:1 |
| phase-340 B3 (`a2e40b12f`, `bc7a286b3`) | `linux` **26796 MB → 5226 MB**; nuttx, freertos and both threadx platforms then joined the shared groups |
| phase-340 item 5 (in flight) | measured prize `linux` 46.1 GiB → ~7.0 GiB, −84.9 % |

Nobody has re-run this issue's arithmetic against that tree. **The first action
here is a re-derivation, not a procurement**: if the full matrix now fits the
maintainer host, the campaign is "run `just build-test-fixtures` twice and read
the joblog", which is what the Context section below already says it mostly is.

This makes the issue a validation item of the phase-340 / phase-334 program
rather than an independent blocked task — it should be run **after** item 5
lands, since item 5 changes both of the things being measured (bytes and
wall-clock) and a pre-item-5 number would be obsolete on arrival.

Also note the follow-up candidate at the end of this issue — "shared Corrosion
`--target-dir` per (triple, feature-set, profile) role group" — **is** phase-340
W2, whose mechanism was decided by measurement on 2026-08-08. It is no longer an
open question this issue needs to carry.

## What to measure (needs a runner with ≥200 GiB scratch)

1. Representative timings for direct platform fixture builds: native,
   qemu, zephyr, freertos, nuttx (clean + warm, `NROS_BUILD_JOBS=8`).
2. Representative `just build-test-fixtures` timing through BOTH the
   fifo-jobserver path (`build-all-jobserver.sh`) and the ordinary-make
   fallback path.
3. CPU utilization under `NROS_BUILD_JOBS=8` and a high-core default run
   (oversubscription vs idle-tail behavior of the make graph).

On a bounded-disk host the full matrix must be built per-platform with
prunes in between, which serializes exactly the wall-clock the campaign is
meant to characterize — hence the runner requirement, not a workaround.

## Context

- The make-driver joblog (`build/fixture-make-driver/…/fixtures.joblog` /
  `tmp/build-test-fixtures-latest`) already records per-leaf start/end/
  duration — the campaign is mostly "run it twice on big iron and read the
  joblog".
- Warm per-platform timings from Wave 13 (native 340 s, qemu 55 s,
  freertos 88 s, nuttx 22 s, zephyr-rs 7 s, zephyr-ccpp 22 s,
  `NROS_BUILD_JOBS=8`) are the only numbers on record; no clean-build or
  jobserver-vs-fallback comparison exists.
- Follow-up candidate identified by 226.E if numbers justify it: shared
  Corrosion `--target-dir` per (triple, feature-set, profile) role group —
  removes the ~200 structurally-non-cacheable staticlib recompiles per
  cell that sccache cannot cover.

## RESTATED 2026-08-16 against post-340/343 disk figures (phase-353 W3)

The 2026-08-10 update asked for a re-derivation before any procurement, and
made it conditional on phase-340 item 5. **That precondition is met**:
phase-340 is COMPLETE (2026-08-12, "every work item is closed"). So the
arithmetic is re-run here.

### The headline number from this tree is NOT the answer

The maintainer host's checkout currently measures **994 GB**:

| root | size |
| --- | --- |
| `examples/` | 563 G |
| `zephyr-workspace/` | 148 G |
| `target/` | 95 G |
| `build/` | 72 G |
| `packages/cli/target/` | 48 G |

Anyone sizing a runner from that would buy an order of magnitude too much. It is
an ACCUMULATED tree — a day of lane builds, wipes and rebuilds — and this issue's
own neighbours already warn about reading one as a cost: the artifact-identity
budget gate says "an accumulated tree can inflate it (issue 0499)", and issue
0446's re-measurement found the same trap.

### Measured: 120 GB of it is pre-340 residue that nothing rebuilds

**65 plain per-leaf `examples/**/target/` directories, 120 GB total**, with
mtimes spanning **2026-08-01 … 2026-08-06** — all before phase-340's completion
and untouched since. These are exactly the per-leaf dirs phase-340 P2 replaced
with coordinate-keyed group dirs (`target-*`, of which the tree has 49). Nothing
in the current build graph writes them; they are dead bytes that inflate every
estimate taken from this checkout.

That single line is the most useful output of this re-derivation: **a runner
sized from an accumulated dev tree is sized from residue.**

### The duplication factor the ≥200 GiB rested on has moved

The requirement was a consequence of ~21:1 artifact duplication. Re-measured in
the same scope by issue 0446 (2026-08-15):

| | 2026-08-06 | 2026-08-15 |
| --- | --- | --- |
| `nros-core` rlibs | 106 | 707 |
| distinct `-C metadata` identities | 5 | 49 |
| duplication factor | **21.2x** | **14.4x** |

And the worst single population, `build/sizes-probe`, was narrowed by phase-353
W4: on one `lane=native`, **25 sub-keys / 7.2 G → 8 / 2.2 G**, with a second run
now adding zero new keys (it previously grew per run, which is how it had
reached 37 G).

### What is still NOT derivable without a runner

The campaign's three measurements are wall-clock and CPU-utilisation questions
(clean vs warm per platform, jobserver vs fallback, oversubscription), and they
need a CLEAN full-matrix build. This host cannot supply one:

* a clean `just native build-fixtures` alone consumed ~52 GiB and 25 min without
  completing (the original 2026-06-13 measurement, unchanged);
* the accumulated state above cannot be distinguished from matrix cost without
  wiping it, which is the same serialisation the runner requirement exists to
  avoid;
* and issue 0509 established that **wall-clock is not a usable instrument on
  this host at all** — seven no-op runs of one lane, provably identical work,
  took 50s…695s, a 14x spread set by page-cache state.

That last point sharpens the runner requirement rather than removing it: the
campaign needs a machine where timings mean something, not merely one with disk.

### Revised guidance for sizing a runner

Do **not** use 994 GB, and do not use the old ≥200 GiB either — both are derived
from trees that no longer describe the build.

The honest inputs are: 14.4x duplication (down from 21.2x), phase-340's group
dirs in force, and `build/sizes-probe` bounded rather than growing. Derive the
figure from a CLEAN build on the runner itself, measuring as it goes — which is
the campaign, not a precondition of it. Budget generously for scratch, then
measure; do not budget from a dev host's `du`.

**Recommendation:** delete the 120 GB of pre-340 residue on the maintainer host
(`examples/**/target/`, the 65 dirs above) before any future estimate is taken
from it. It is not this issue's to do — a stale-residue sweep is its own change,
and `check-example-leaf-target-dirs` currently passes with those dirs present,
which is worth checking separately.

## Re-derived 2026-08-21 — the residue is gone, and the 994 GB figure with it

This issue says "the first action here is a re-derivation, not a procurement".
Re-run on the maintainer host, five days after the 2026-08-16 pass.

### The recommendation is satisfied — by someone, not by this issue

> delete the 120 GB of pre-340 residue … (`examples/**/target/`, the 65 dirs)

```
$ find examples -maxdepth 5 -type d -name target   | wc -l   ->  0
$ find examples -maxdepth 5 -type d -name 'target-*' | wc -l ->  15
```

**Zero** plain per-leaf `target/` dirs remain; only the coordinate-keyed group
dirs phase-340 P2 introduced. The 120 GB is recovered.

### And the gate note is stale too

> `check-example-leaf-target-dirs` currently passes with those dirs present,
> which is worth checking separately

Checked, by creating one rather than by reading the script:
`mkdir examples/native/rust/talker/target` makes
`just check-example-leaf-target-dirs` FAIL, naming the class and prescribing
`rm -rf … then re-run a build. If one comes back, it is the second case and the
writer needs finding.` The gate covers the class now. Whether it was fixed since
2026-08-16 or those 65 dirs sat in leaves it exempts is not established here —
what is established is that the hole described above is closed.

### Current composition — 880 GB, and most of it is not matrix cost

| root | 2026-08-16 | 2026-08-21 |
| --- | --- | --- |
| `examples/` | 563 G | **344 G** |
| `zephyr-workspace/` | 148 G | **228 G** |
| `target/` | 95 G | **159 G** |
| `build/` | 72 G | **75 G** |
| `packages/cli/target/` | 48 G | **35 G** |
| total | 994 G | **880 G** |

The composition matters more than the total, and it argues the same way this
issue already does — do not size a runner from this tree:

* **`zephyr-workspace` is 228 GB and is not fixture-matrix cost at all.** It is
  one provisioned west workspace for the 3.7 line. Issue 0651 ran into the same
  number from the other side: a second workspace for the 4.4 line could not be
  provisioned here, and that is a PROVISIONING budget, separate from what a
  clean matrix build needs.
* `examples/` fell 563 → 344 G, consistent with the residue removal.
* `target/` ROSE 95 → 159 G. Not investigated here; flagged because it moves in
  the opposite direction to everything else and nobody has attributed it.

### Still blocked, and the reason is unchanged

The three measurements are wall-clock and CPU-utilisation questions needing a
CLEAN full-matrix build. Nothing above supplies one, and issue 0509's finding
stands: wall-clock is not a usable instrument on this host (seven no-op runs of
one lane, provably identical work, 50 s…695 s). The runner requirement is about
a machine where timings MEAN something, not merely one with disk.

### Operational note

The host sits at **98 % full, 27 GB free**. That is not comfort margin: on
2026-08-20 it reached zero mid-build and truncated four `Cargo.lock` files
(untracked leaf locks; every tracked lock verified intact afterwards). Whoever
runs the next big build here should reclaim first — `build/sizes-probe`,
`build/example-lint` and `build/metadata-probe` are derived caches that came
back to 87 GB once already.
