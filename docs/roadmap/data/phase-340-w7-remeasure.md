# Phase-340 W7 re-measure — captured 2026-08-12

The third run of the phase-331 W1/W5 pair, after phase-340 changed what gets
compiled and how often. Compare against
[`phase-331-w1-baseline.md`](phase-331-w1-baseline.md) (2026-08-02, base
`82b82a6d6`) and [`phase-331-w5-remeasure.md`](phase-331-w5-remeasure.md)
(2026-08-03).

Same method, unchanged: `scripts/dev/measure-fixture-build.sh native` wipes every
build tree the manifest declares, rebuilds the CLI and launch resolver outside
the timed section, then times `just build-test-fixtures lane=native`.

## Wall clock

| | W1 (2026-08-02) | W5 (2026-08-03) | W7 run A | **W7 run B (steady)** |
| --- | --- | --- | --- | --- |
| wall | 7051 s (1 h 57 m) | 6794 s (1 h 53 m) | 919 s | **581 s (9 m 41 s)** |
| native stage | 5912 s | 5222 s | 548 s | **342 s** |
| fixtures built | 64 | 72 | 72 | **72** |
| errors | 0 | 0 | 0 | **0** |
| seconds per fixture (native stage) | 92.4 | 72.5 | 7.6 | **4.8** |

Against W5, at the same 72 fixtures: **11.7× on wall clock, 15.3× on the native
stage.**

Two runs are reported because the first one is not the steady state and saying
so is cheaper than defending a number I would have had to caveat anyway. Run A
followed a FAILED run (below) that left some group dirs half-populated; run B is
run A repeated immediately, and it is 37 % faster again. **Run B is the figure.**

### What that number does and does not mean

The comparison is honest about METHOD and misleading if read as a like-for-like
speedup, so both halves are stated.

*Comparable:* all three captures wipe exactly the manifest-declared workspace
trees and leave the cargo state alone. Same script since W5, same lane, same
host.

*Not comparable:* "leave the cargo state alone" now means something different.
In W1/W5 the warm cargo state was ~116 per-leaf `target*` dirs, so a workspace
rebuild recompiled the shared nano-ros crates once per leaf. After phase-340 B3
all 124 cargo rows build into `build/cargo-fixtures/<slug>`, so the warm state is
18 group dirs the rebuild genuinely reuses. **That IS the change being
measured** — but the figure is "cold workspaces against a warm SHARED cache"
where W1/W5's was "cold workspaces against a warm PER-LEAF cache". A tree with no
cargo cache at all would show a far smaller ratio, and this measurement does not
bound it.

## The measurement, restated

Phase-340 opened with `nros-core` counted across 60 leaf `target*/…/deps` dirs:
106 rlibs, 45 of them sharing one `-C metadata` hash. On the post-change tree,
counted the same way:

| | rlibs | distinct identities |
| --- | --- | --- |
| `build/cargo-fixtures/` (live, 18 group dirs) | 47 | 24 |
| `examples/**/target*` (leaf dirs, now residue) | 1892 | 60 |

The live side is what the fixture lane actually compiles: 47 `nros_core` rlibs
across 7 platforms and their variants, against 106 before. The 24 identities are
not duplication — a group per (platform, variant signature) is the design, and
each is a distinct compilation cargo would refuse to share.

The identity gate on a freshly rebuilt tree (`started_at` filter live, 245 of 245
rlibs counted):

```
nros_core 4/4 identities; worst crate 5/5; worst identity 5/5 copies
R3 axis (host vs explicit --target): identities 136/51, copies 166/79
```

**Every budget already equals the truth, so none is lowered.** W7's second bullet
asks for the new value to be recorded — this is it, and "no change" is the
answer, not an omission. The remaining `5 -> 3` is R2's, and phase-340 already
measured it as costing a corrosion ROOT rather than a path-spelling edit.

## The disk story, restated

Phase-340 opened with `examples/` at 402 GiB, one talker leaf holding 7.4 GiB
across five target dirs for one binary. On this tree:

| | | |
| --- | --- | --- |
| `examples/` | 402 GiB (2026-08-06) | **306 GiB** |
| live fixture cargo output (`build/cargo-fixtures/`) | — | **15 GiB** |

The five-dirs-per-leaf pattern is gone: the `target_dir` column that authored
those names was deleted (issue 0517), every cargo row shares a group dir, and
what remains under `examples/**/target*` is residue rather than live output.

**The dominant class has changed, and it is not what this phase was about.**
Classifying every `target*` dir under `examples/`:

| class | dirs | size |
| --- | --- | --- |
| **cmake metadata probe** | **108** | **82.4 GiB** |
| example leaves (phase-340 residue) | 356 | 28.9 GiB |

`nros metadata --build` gives every component a private cargo target dir inside
its own `build/nros-metadata/metadata-probe/<c>/`, holding a full host build of
that component and its dependency graph. 162 such trees hold 312 `libnros_core`
rlibs with **16 distinct identities** — phase-340's own thesis, on a build path
phase-340 never touched. Filed as issue 0522; not fixed here, because this phase
owns the fixture lane and is closing.

## The run that failed first, and why it counts

The first attempt exited 2 with 11 errors: the mixed workspace's runtime crate
could not compile `nros-pkg-index`, because `eyre::Context` is
`#[cfg(feature = "anyhow")]` from eyre 0.6.13 and that graph resolves fresh while
`packages/cli/Cargo.lock` pins 0.6.12. Every warm build and every `just check`
was green throughout.

Worth recording as a property of the measurement rather than an incident: a cold
build is the only thing in this repo that exercises the workspace-runtime graph
from scratch, so W1/W5/W7's method is also the only routine check on it. Fixed
(66 call sites to `WrapErr`) and gated by `check-eyre-context-alias` before the
timed run was repeated.

## Static counts

| | W1 | W5 | W7 |
| --- | --- | --- | --- |
| workspace directories | 35 | 15 | 14 |
| `[[workspace_fixture]]` rows | 86 | 93 | 93 |
| single-node `[[fixture]]` rows | 251 | 251 | 248 |
| tier1 coordinates | 10 | 10 | 10 |
| tier2 coordinates | 12 | 12 | 13 |
