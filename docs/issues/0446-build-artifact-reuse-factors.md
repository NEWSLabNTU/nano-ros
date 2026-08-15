---
id: 446
title: "the same crate is compiled ~21x across leaf target dirs — what actually makes those builds incompatible"
status: open
type: perf
area: build
related: [issue-0400, phase-336, phase-334]
---

## The number

`nros-core`, counted across 60 leaf `target*/nros-relwithdebinfo/deps` dirs:

```
total nros-core rlibs: 106
     45  3636184e65c044e3
     23  92233bfad5e3d350
     21  25e8c1176cfcdf63
     16  f221b2c956c26591
      1  9d786b2b3017abde
```

**106 compilations, 5 distinct identities.** Forty-five of them are the same
compilation done forty-five times. That hex is cargo's `-C metadata` hash — its
own judgement that two builds are interchangeable — so this is not an estimate.

The same holds for every crate in the shared stack (`nros-node`, `nros-rmw-*`,
`nros-serdes`, …), which is what a `just build-test-fixtures` run spends most of
its wall-clock on: ~8 cargo frontends live, 0–2 `rustc` processes at any instant,
load ~12 on 32 cores.

## What makes two builds incompatible

Measured one factor at a time, everything else held constant, `CARGO_INCREMENTAL=0`
so the comparison is about identity rather than per-session noise. The value is
the `-C metadata` hash of `libnros_core-<hash>.rlib`:

| Factor | Variation | Hash | Blocks reuse? |
| --- | --- | --- | --- |
| *(control)* | same build, two different target dirs | `f914a127d89299a3` twice | **no — reuse is possible** |
| Profile | `nros-relwithdebinfo` → `release` | `129125bd817a6877` | yes |
| Profile | `nros-relwithdebinfo` → `dev` | `41759641665a431e` | yes |
| Features | default → `--no-default-features` | `128f964e277526f3` | yes |
| Target triple | implicit host → **explicit `--target x86_64-unknown-linux-gnu`** | `888ced5467919627` | **yes — same triple!** |
| RUSTFLAGS | none → `-C target-cpu=native` | `90d1e7dfec307209` | yes |

Two of these deserve attention:

**Explicit `--target` is not free.** Passing `--target <host-triple>` produces a
DIFFERENT artifact identity than not passing it, for the same triple. Corrosion
always passes `--target`; plain `cargo build` in a leaf does not. So a
cmake-driven build and a cargo-driven build of the identical crate can never
share, and that split is invisible in the manifests.

**`incremental` preserves identity but destroys byte-reproducibility.** The
control row above is byte-identical only with incremental OFF:

```console
CARGO_INCREMENTAL=1 -> differ
CARGO_INCREMENTAL=0 -> BYTE-IDENTICAL
```

With `incremental = true` the rlibs carry the same `-C metadata` hash and a
byte-identical `lib.rmeta`, but the codegen-unit members differ by a trailing
per-session token (`…2h1hivz5wi6wzpcc1ckgl7n8q.03iazng.rcgu.o` vs
`….0802496.rcgu.o`). Same partition, different build session.

That matters because `[profile.nros-relwithdebinfo]` sets `incremental = true`
and phase-336 made that profile the default for **everything** — previously only
the fixture lane used it while the cmake/corrosion path took `--release` (no
incremental). The setting predates phase-336; the blast radius does not.

## What this means for the DAG

The leaves are separate cargo workspaces with isolated target dirs, and that
isolation is deliberate — `target-zenoh` / `target-xrce` / `target-cyclonedds`
exist so one RMW variant's artifacts cannot overwrite another's. But isolation
was applied at the DIRECTORY level, while incompatibility actually lives at the
IDENTITY level, and those are not the same partition: 106 dirs-worth of work
collapses to 5 identities.

The critical path is therefore not "compile 106 crates" but "compile 5, then
link 106". Nothing in the current arrangement expresses that.

## Fix

**Phase 340** (`docs/roadmap/archived/phase-340-build-artifact-reuse.md`) carries the
work items, with the four repetition reasons (directory-vs-identity partition,
per-leaf workspaces, corrosion's explicit `--target`, and `incremental`) and the
measurements behind each.

## Directions, in order of confidence

1. **Verify the sharing is safe, then share by identity.** A shared
   `CARGO_TARGET_DIR` for leaves whose (profile, features, target-flag, RUSTFLAGS)
   tuple matches would collapse the duplication with no semantic change — cargo
   already proves equivalence via the metadata hash. Risk is concurrent access to
   one target dir, which cargo serialises with a lock; measure whether that
   serialisation costs more than the duplicate compiles it removes.
2. **Reconsider `incremental = true` in the shared default profile.** It buys
   fast local iteration on ONE tree and costs byte-reproducibility across trees,
   which is what any content-addressed cache (sccache, a shared dir, CI restore)
   needs. It may be the wrong default for a profile used by 60 leaf builds — but
   this needs an A/B on wall-clock before changing, not an assumption.
3. **Normalise the `--target` split.** If corrosion's explicit `--target` is
   load-bearing, nothing to do; if it is incidental, aligning it would merge two
   of the five identity classes.

## Not measured

sccache's interaction. Its overall hit rate is 97% but Rust-specific is 68%,
with 5310 non-cacheable calls recorded — I could not get consistent A/B numbers
for whether `incremental` is what makes Rust compilations non-cacheable, so that
hypothesis is UNVERIFIED and should be measured before it is acted on. It is the
obvious next experiment.

## Notes

Measured 2026-08-06 while investigating why `just build-test-fixtures` takes
minutes with an idle CPU. All commands above are reproducible; the factor table
came from `scratchpad/reuse-factors.sh` (one build per row, fresh target dir).

## Re-measured 2026-08-15 (phase-353 W1)

The 2026-08-06 census predates phase-340 (coordinate-keyed fixture target dirs),
phase-340 W3 (every cmake-emitted cargo command passes `--target`, host
included) and phase-343 W1 (the shared sizes-probe dir). Re-run in the ORIGINAL
scope — `libnros_core-*.rlib` under `*/nros-relwithdebinfo/deps/`:

| | 2026-08-06 | 2026-08-15 |
| --- | --- | --- |
| rlibs | 106 | **707** |
| distinct target dirs | 60 | **385** |
| distinct `-C metadata` identities | 5 | **49** |
| duplication factor | 21.2x | **14.4x** |

The ratio improved; the absolute waste grew, because the tree grew (60 → 385
dirs). Neither number alone is the story — **the duplication moved.**

### Where it is now

| population | rlibs | identities | factor |
| --- | --- | --- | --- |
| `build/sizes-probe` | 155 | **6** | **25.8x** |
| `examples/**` | 375 | 27 | 13.9x |
| `packages/**` | 96 | 13 | 7.4x |
| `build/cmake-fixtures` | 20 | 2 | 10.0x |
| `build/cargo-fixtures` | 31 | 13 | 2.4x |

The worst duplicator is now **the shared probe dir that phase-343 W1 created to
remove duplication**. Under its single `rustc-1.97.1` key sit **110 sub-key
directories holding 18 distinct nros-core identities**, and the top two are one
compilation done **70** and **69** times. It occupies **37 GB** (35 GB under
that one rustc key).

### Why — and it is deliberate, not a bug

`nros_sizes_build::probe_key` hashes `(target, sorted features, EVERY set
`NROS_*` knob + the matching `CONFIG_NROS_*` lines from Zephyr's `$DOTCONFIG`)`.
The knob half is deliberately broad, and its own comment says why: issue 0528
showed that sharing a probe dir across a knob difference is *order-dependent
corruption* — whichever leaf probes first writes the sizes, and a 4-CBS leaf
poisons a 16-CBS one into `EXECUTOR_OPAQUE_U64S too small`.

So the probe key is a hash of the REQUEST, while `-C metadata` is a hash of what
actually determines the ARTIFACT. Most `NROS_*` knobs change
`ExecutorInlineStorage` (in `nros-node`) without changing `nros-core`'s
compilation at all — so they split the directory without splitting the artifact.
This is the issue's own thesis, now applying hardest to the dedup mechanism:
**isolation at the DIRECTORY level, incompatibility at the IDENTITY level.**

### What this changes about the directions

* **Direction 3 (normalise the `--target` split) is DONE.** phase-340 W3 made
  every cmake-emitted cargo command pass `--target`, host included, with gate
  `check-cargo-target-spelling`. That merger has already happened.
* **Direction 1 (share by identity) is still the right idea, and the probe dir
  is now its best target** — 110 dirs collapsing to 18 is a bigger, more
  contained prize than the `examples/**` spread.
* **Any change here must preserve issue 0528's invariant.** A knob that CAN
  change a probed size must still split the key. The tractable question is
  whether the knob set can be narrowed to the knobs that actually reach the
  probed types, rather than every `NROS_*` in the environment — which is a
  correctness argument to be made per knob, not a blanket loosening.
* **Direction 2 (`incremental`) remains unmeasured** and is unaffected by this.

The issue's central question — *what actually makes those builds incompatible* —
now has a sharper answer for the largest population: **nothing does.** For 110
of the probe directories, cargo itself says the artifacts are interchangeable;
they are separate because the key is conservative on purpose.

## Narrowed 2026-08-15 (phase-353 W4)

The probe dir's over-keying is fixed, and the cause was not what the
re-measurement above guessed.

**Wrong first:** the section above blamed sizing knobs, and a follow-up blamed
absolute paths (`NROS_REPO_DIR`, `NROS_C_INCLUDE`, …). A denylist of exactly
those names, A/B'd on the same lane with the probe dir wiped both sides, changed
**nothing** — 25 sub-keys and 7.2 G either way. Those variables are CONSTANT
within a run, and a constant input cannot split anything. Both diagnoses were
inferred from what the knob list *contained* rather than from what *varied*.

**What actually splits it**, from a census the new `nros-probe-key-inputs.txt`
provenance records made possible: of 25 keys, all shared one target triple and
**19 shared the same feature set**, with 35 knobs varying inside that group and
not one of them a sizing knob:

```text
NROS_BUILD_LOG_DIR    .../logs/20260815-111859-1157807-9133   <- timestamp + pid
NROS_WS_RECORDS_FILE  .../ws-linux-20260815-112230-1214903-group-10.records
NROS_FIXTURE_ID       11 values
NROS_KIND_*           ~20 per-kind marker strings
NROS_BUILD_JOBS       24 vs 6
```

The timestamped ones differ on **every run**, so every fixture build minted
probe keys that could never be reused. That is the mechanism behind 110
directories / 37 GB where one lane creates 25.

**Result**, same lane, probe dir wiped both sides:

| | before | after |
| --- | --- | --- |
| probe sub-keys | 25 | **8** |
| disk | 7.2 G | **2.2 G** |

A second run of the same lane now creates **zero** new keys (8 → 8, 2.2 G) —
the growth, not just its size, is what stopped.

Issue 0528's invariant is preserved and tested: a knob that can change a probed
size still splits the key, unknown knobs still split by default, and
`NROS_BOARD_TOML` / `NROS_PLATFORMS_DIR` / `NROS_MODEL_DIR` / `NROS_HOME` can
never be excluded because each names a file whose CONTENT carries sizing knobs.

No wall-clock claim: issue 0562 established this host's lane timing is set by
page-cache state (a 14x spread on provably identical work).
