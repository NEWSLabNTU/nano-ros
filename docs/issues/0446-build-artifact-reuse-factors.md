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

**Phase 340** (`docs/roadmap/phase-340-build-artifact-reuse.md`) carries the
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
