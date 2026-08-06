# Phase 340 — Build-artifact reuse: compile each identity once

**Closes:** issue 0446. **Touches:** phase-334 (build-cache layout — this is the
*identity* question that layout question implies), phase-336 (the profile that
made `incremental` the default everywhere), RFC-0065 (`nros build` owns the
workspace build root). **Related:** issue 0400 (host/box target-dir split).

## Goal

Compile each distinct artifact **identity** once per build, instead of once per
directory. Today the tree does the latter, and the ratio is ~21:1.

## The measurement

`nros-core`, counted across 60 leaf `target*/nros-relwithdebinfo/deps` dirs:

```
total nros-core rlibs: 106
     45  3636184e65c044e3
     23  92233bfad5e3d350
     21  25e8c1176cfcdf63
     16  f221b2c956c26591
      1  9d786b2b3017abde
```

That hex is cargo's `-C metadata` hash — cargo's own judgement that two builds
are interchangeable. So "are these the same compilation?" is answered by
construction, not by inspection. Forty-five of them are.

The symptom is a `just build-test-fixtures` run with ~8 cargo frontends live,
**0–2 `rustc` processes** at any instant, and load ~12 on 32 cores. The machine
is not compiling; it is repeating.

## The disk story

Wall-clock is the symptom that gets noticed; disk is the one that accumulates.
The same repetition, measured as bytes on a working tree (2026-08-06):

```
668G  .
   402G  examples/
    56G  target/
    50G  packages/
    39G  build/
```

One leaf, `examples/native/rust/talker`, holds **7.4 GB across five target
dirs** — `target/` 2.1G, `target-zenoh/` 1.5G, `target-tls/` 1.5G,
`target-cyclonedds/` 1.2G, `target-xrce/` 1.1G — for one binary whose lower
half is identical in all five. That is R1 stated in bytes, and it is why
`examples/` alone is 402 GB. 327 `libnros_core-*.rlib` files exist under
`nros-relwithdebinfo/deps` right now.

Incremental state is a large and separable share of it:

| | dirs | size |
| --- | --- | --- |
| `nros-relwithdebinfo/incremental` (live) | 245 | **41 G** |
| `nros-fast-release/incremental` (dead profile name) | 79 | 20 G |
| `packages/cli/target/debug/incremental` | 1 | 4.1 G |
| all `incremental/` dirs | 858 | **70 G** |

Two things follow. First, W1 has a disk answer that does not depend on the
timing question at all: `incremental = true` costs 41 GB of live state in a tree
whose builds are overwhelmingly build-once-per-leaf. Second, 20 GB sits under
`nros-fast-release`, a profile name phase-336 renamed away — nothing writes
there any more, so it is pure museum, and its presence means these trees are
never reclaimed, only added to. Any reader taking a `du` measurement here must
separate live from orphaned before quoting a number.

This is not a theoretical budget. The volume holding the checkout was at **96 %
(36 GB free)** when these numbers were taken, with 115 GiB of that in the native
lane's target dirs alone — so "run the lane A/B four times" is itself gated on
reclaiming space first. A phase about redundant compilation is also, on this
host, a phase about whether the next sweep can run at all.

**Disk is an acceptance criterion below, not a side effect.** A change that
holds wall-clock flat while removing tens of GB of redundant state is a win this
phase should be able to record; one that trades disk for time needs the trade
stated rather than discovered later.

*Host note.* These figures come from a 20-core machine; the "load ~12 on 32
cores" observation above came from a different one. Absolute wall-clocks in this
document are therefore not comparable across sections — only within a section's
own alternating reps.

## Why the repetition happens

### R1 — isolation is per-DIRECTORY, incompatibility is per-IDENTITY

The per-RMW target dirs exist so one variant's artifacts cannot overwrite
another's. But the same identity appears across all of them:

```
 6  3636184e65c044e3 target-fixtures
 5  3636184e65c044e3 target-zenoh
 5  3636184e65c044e3 target-xrce
 5  3636184e65c044e3 target-cyclonedds
```

Because `nros-core` does not depend on the RMW at all:

```console
$ grep -ciE "rmw|zenoh|xrce|cyclone" packages/core/nros-core/Cargo.toml
0
$ grep -A3 '^\[features\]' packages/core/nros-core/Cargo.toml
default = ["std"] ; std = [...] ; alloc = [...]
```

The RMW choice partitions the *upper* layers (`nros-node`, `nros-rmw-*`). The
lower layers are identical and get rebuilt once per variant for nothing. The
directory partition is coarser than the real one.

### R2 — every leaf is its own workspace

```console
root workspace:  f914a127d89299a3
leaf workspace:  317728cd7daaa57d     # examples/native/rust/talker
```

Same crate, same profile, same features — different identity, because the leaf
resolves through its own `Cargo.lock`. Leaves DO agree with each other (talker
and listener both carry `3636184e65c044e3`), which is what makes R1 fixable: the
duplicates are genuinely interchangeable among leaves.

### R3 — corrosion's explicit `--target` splits every crate again

```console
implicit host:      f914a127d89299a3
--target x86_64-unknown-linux-gnu:  888ced5467919627
```

Same triple, different identity. Corrosion always passes `--target=` (
`cargo rustc --lib --target=x86_64-unknown-linux-gnu …`); native leaves never
do. So the cmake-driven and cargo-driven builds of an identical crate can never
share, and nothing in the manifests makes that visible.

### R4 — `incremental = true` destroys byte-reproducibility

```console
CARGO_INCREMENTAL=1 -> differ
CARGO_INCREMENTAL=0 -> BYTE-IDENTICAL
```

With incremental on, two builds of the same identity produce the same
`-C metadata` hash and a byte-identical `lib.rmeta`, but codegen-unit members
differing by a per-session token
(`…2h1hivz5wi6wzpcc1ckgl7n8q.03iazng.rcgu.o` vs `….0802496.rcgu.o`). Any
content-addressed reuse — a shared dir, sccache, a CI cache restore — needs
byte-stability, so this forecloses the fix in R1/R2 even where identities match.

It also costs, on a fresh target dir (alternating runs, both cache-warm, so the
ordering effect is controlled):

| | run A | run B |
| --- | --- | --- |
| `CARGO_INCREMENTAL=1` | 38 s | 27 s |
| `CARGO_INCREMENTAL=0` | **23 s** | **17 s** |

~37 % slower, consistently, in both reps — and 649 MB vs 482 MB of target dir.
Incremental pays off when the SAME target dir is rebuilt after an edit. The
fixture lanes build each leaf once into a per-leaf dir, which is the case where
it is pure cost.

**Caveat on how this was measured.** The first A/B ran 1-then-0 and showed 0
winning; the reverse order showed 1 winning. In both, the *second* run was
faster — warm caches dominated. Only alternating repetitions isolate the factor.
Any future timing claim here needs the same treatment.

**Superseded in part by the W1 result below.** A re-measurement of the same leaf
reproduced the DISK figures exactly (649 MiB vs 482 MiB) but not the timing: it
found ~10 %, not ~37 %, and no difference at all without sccache. Since the two
runs agree on disk to the byte and disagree on time by 3×, the discrepancy is in
the timing method, not in the leaf or the setting — most likely that the `run A`
/ `run B` pair above is cold-then-warm within one arm rather than two
independent reps. Take the disk column from here and the timing from W1.

## The complete incompatibility set

Measured one factor at a time, fresh target dir, `CARGO_INCREMENTAL=0`:

| Factor | Changes identity? |
| --- | --- |
| Same build, different target dir | **no** |
| Profile (`relwithdebinfo` / `release` / `dev`) | yes |
| Feature set | yes |
| RUSTFLAGS | yes |
| Explicit `--target` vs implicit host | **yes, same triple** |
| Workspace root (leaf vs root) | yes |
| `incremental` | no (identity), but breaks byte-equality |

## Work items

### W1 — decide `incremental` for the shared profile

- [x] A/B `just build-test-fixtures lane=native` with `incremental` on/off in
      `[profile.nros-relwithdebinfo]`, alternating reps as above. Record BOTH
      wall-clock and the target-dir bytes each arm leaves behind.
- [x] If it holds at lane scale, drop `incremental = true` from that profile and
      give interactive work a separate profile that keeps it (the local-iteration
      case it actually serves). **Done 2026-08-06**: `incremental` dropped from
      all three copies of `nros-relwithdebinfo`; local iteration is the new
      `nros-iterate` preset, selected by `NROS_CARGO_PROFILE=nros-iterate`.

The profile is defined in **THREE** places, kept in agreement by two tests in
`packages/tooling/nros-cargo-profile/src/lib.rs`:

| copy | why it exists | gate |
| --- | --- | --- |
| `RELWITHDEBINFO` preset in `nros-cargo-profile` | cmake/bash/tests read it without parsing TOML | — (the SSoT) |
| `[profile.…]` in the root `Cargo.toml` | a bare `cargo build --profile` in this repo | `root_manifest_matches_this_table` |
| `[profile.…]` in the root `.cargo/config.toml` | a bare `cargo build --profile` in any LEAF, via cargo's config walk-up | `config_toml_matches_this_table` |

The third is the one that matters here and is the easiest to miss. Leaf
workspaces carry no `[profile.*]` block of their own; they resolve the name
through the config walk-up, so **editing the manifest alone changes nothing for
any leaf** — and the gates will fail the build before a half-applied edit can
be measured, which is the intended outcome.

For MEASUREMENT, do not edit any of the three. `CARGO_INCREMENTAL=0` in the
environment overrides the profile's `incremental` setting for every cargo in the
run, so both arms differ by exactly one variable and neither needs an `nros`
rebuild (which would stale every workspace fixture and confound the timing).
Edit the three copies only once the answer is known.

Verified that this override survives the cmake path, which is the one that could
have defeated it: `nros_cargo_profile_env()` injects
`CARGO_PROFILE_NROS_RELWITHDEBINFO_INCREMENTAL=true` explicitly for corrosion
targets, and `CARGO_INCREMENTAL=0` still wins — the `incremental/` directory is
created but stays empty (4096 B), and the target dir comes out at the
incremental-off size. Without this check the lane's "off" arm could have been
measuring the status quo under a different name.

**Acceptance:** the lane's wall-clock difference AND its disk difference are
measured and recorded, and whichever way it goes, the reason is written at the
profile — in all three spellings.

#### W1 result — single leaf (2026-08-06, 20-core host)

`examples/native/rust/talker`, 112 crates, `target/` removed before every run,
three alternating reps, sccache live. Steady state (reps 2–3, warm cache):

| arm | wall | `target/` | of which `incremental/` | sccache hits / misses |
| --- | --- | --- | --- | --- |
| profile `incremental = true` | 5.5 s | 649 MiB | 178 MiB | 98 / 1 |
| `CARGO_INCREMENTAL=0` | **5.0 s** | **482 MiB** | 1 MiB | 124 / 3 |

Cold-cache first rep: 8.8 s vs 5.0 s. Disk reproduced to within a few KB across
reps within each arm (674.01–674.03 MB incremental-on, 500.64 MB off) — this
factor is stable, unlike the timing.

**Incremental costs ~10 % wall-clock and ~35 % disk here.** The direction matches
the phase doc's premise; the magnitude does not — the doc's "~37 % slower" does
not reproduce at this scope, so treat that figure as scope-specific rather than
a property of the setting.

#### The sccache question, answered

The doc listed "incremental makes Rust compilations non-cacheable" as the
obvious hypothesis and the next experiment. It is **wrong**, and the truth is
sharper:

* `non_cacheable_compilations` was **0** in every arm. sccache caches
  incremental builds fine.
* What changes is the number of units it sees: 99 requests with incremental on
  versus 127 with it off. Fewer compilations reach the cache, so the warm-cache
  win is smaller — 98 hits versus 124.
* Separately, and much more sharply: **sccache 0.8.2 refuses to run at all when
  the `CARGO_INCREMENTAL` environment variable is set to 1.** It aborts during
  the `rustc -vV` probe with `sccache: increment compilation is prohibited` and
  the build exits 101 having compiled nothing.

That last point is a trap for anyone measuring this. `CARGO_INCREMENTAL=1` and a
profile's `incremental = true` are **different inputs**: cargo does not set the
env var for a profile setting, it passes `-C incremental=<dir>` on the rustc
command line. So the env var is not a louder spelling of the profile setting,
and an A/B that uses it for the "on" arm measures a build that never happened.
The first attempt at this measurement did exactly that and had to be redone.

A second trap: `command -v sccache` finds nothing unless `activate.sh` has been
sourced, so a harness that sets `RUSTC_WRAPPER` from it silently runs every arm
uncached. The `sccache` columns above are only meaningful because the harness
sources `activate.sh` first — which itself needs `set +u`, since ROS's
`setup.bash` reads `$AMENT_TRACE_SETUP_FILES` unguarded.

#### W1 result — lane scale (`build-test-fixtures lane=native`)

Four runs, alternating, every native-lane target dir removed before each:

| rep | arm | wall | target dirs | sccache hits / misses |
| --- | --- | --- | --- | --- |
| 1 | profile (on) | 1882 s | 43643 MiB | 9626 / 1888 |
| 1 | off | 1927 s | 31600 MiB | 13692 / 5088 |
| 2 | profile (on) | 1366 s | 43643 MiB | 10874 / 76 |
| 2 | off | **1199 s** | **31600 MiB** | 17846 / 222 |

**Disk: −12043 MiB, −27.6 %, and identical to the MiB across both reps.** This is
the solid result. It scales past the single leaf and it does not depend on cache
state, machine load, or run order.

**Wall-clock: +2.4 % in rep 1, −12.2 % in rep 2.** The sign flips, which is
precisely the pattern the doc warns about — but here the cause is visible in the
miss counts rather than left as "warm caches dominated":

* Rep 1 is cache-POPULATING. The `off` arm sends more units to sccache (18780
  requests vs 11514) so it pays a bigger one-time population cost: 5088 misses
  vs 1888. It comes out 45 s slower.
* Rep 2 is cache-WARM, and the same property now pays: 17846 hits vs 10874,
  misses collapsed to 222 and 76. It comes out 167 s faster.

So the two reps do not disagree; they measure two different regimes. The
steady-state regime is rep 2, and its −12.2 % matches the single leaf's −10 % in
both sign and magnitude. **The honest reading is that dropping `incremental`
wins ~10–12 % once the compile cache is warm, loses ~2 % on the very first cold
run, and saves ~28 % disk unconditionally.**

`non_cacheable_compilations` was 0 in all four arms, across ~30 000 compilations
— the refutation above holds at lane scale, not just on one leaf.

**A flaw in this harness, stated so the numbers are read correctly.** The two
disk metrics have different scopes: the target figure is `-maxdepth 4 -name
'target*'`, the incremental figure is an unlimited-depth `-name incremental`. The
latter therefore counts `incremental/` dirs under `build/` trees the former never
visits, which is why it reads LARGER than the total it appears to be a share of
(46589 vs 43643 MiB). Both arms were measured identically so the delta is sound,
but the incremental column is not a subset of the target column and must not be
quoted as one. Fix the scopes before re-running.

#### Two hazards the new profile ran into, now gated

Adding the opt-in profile was not a one-line mirror of the old settings. Both of
these were found by shipping the obvious version first and watching it fail:

1. **A chained `inherits` does not survive env injection.** The natural spelling
   is `inherits = "nros-relwithdebinfo"`, but [`env`] emits ONE profile's
   settings, so outside this checkout — where the `.cargo/config.toml` walk-up
   does not reach — cargo fails `profile 'nros-relwithdebinfo' is not defined`.
   Presets must bottom out in a cargo builtin and repeat the settings.
   `presets_all_inherit_a_builtin` *claimed* to enforce this and did not: it
   checked only that some `inherits` key was present. Now it checks the value.

2. **The obvious NAME is ambiguous.** `nros-relwithdebinfo-incremental`
   uppercases to `CARGO_PROFILE_NROS_RELWITHDEBINFO_INCREMENTAL`, which is also
   `nros-relwithdebinfo` + its `incremental` key. Cargo resolves it as the
   latter and dies with `could not load config key
   profile.nros-relwithdebinfo`. Hence `nros-iterate`. The new
   `preset_names_cannot_collide_with_another_preset_key` compares each preset
   name against every preset crossed with **every cargo profile key**, not just
   the declared ones — the first version of that test compared declared keys
   only and happily passed the very name that had just failed, because
   `incremental` had by then been removed from the parent.

Both gates were tripwired: each fails on its own defect with the other reverted.

#### What the idle time actually is

Sampled every 15 s across all four runs (554 samples): **mean 2.8 `rustc`, 5.4
`cargo`, load 5.7 on 20 cores**, and **22.9 % of samples had ZERO `rustc`
running**. That corroborates the "0–2 rustc" symptom this phase is built on.

It also qualifies it. Part of that zero-rustc time is the serial `nros sync` /
`generate-rust` codegen prologue, which compiles nothing and which W2's
deduplication would not touch — the first ~3.5 minutes of every run were spent
there before the first `== native ==` banner. Before crediting W2 with the whole
gap, the prologue should be timed separately; the joblog records per-PLATFORM
stages only, so it cannot currently answer this.

### W2 — collapse the R1 duplicates

- [ ] Group leaves by identity tuple (profile, features, target-flag, RUSTFLAGS,
      workspace) and share ONE target dir per group, instead of one per leaf.
      `nros_sizes_probe_dir` in `scripts/build/cargo.sh` is the precedent — same
      shape, already proven for the size probe.
- [ ] Measure the cargo target-dir lock contention this introduces. It must be
      cheaper than the duplicate compiles it removes; if it is not, the answer is
      a content-addressed cache instead, which needs W1 first.

**Acceptance:** `nros-core` rlib count drops from 106 toward the identity count,
the native lane's wall-clock does not regress, and the target-dir bytes for the
grouped leaves drop by roughly the share the duplicates accounted for. The disk
figure is the one that proves the duplicates are GONE rather than merely
counted differently — an rlib count can fall because the probe changed.

### W3 — the corrosion `--target` split

- [ ] Establish whether corrosion's explicit `--target` is load-bearing for the
      host-native case or incidental. Corrosion sets it from
      `Rust_CARGO_TARGET`; for a host build that may be redundant.
- [ ] If incidental, align it so cmake-driven and cargo-driven host builds share
      one identity. If load-bearing, record WHY at the call site so the split
      stops looking like an accident.

**Acceptance:** either the two paths share an identity, or the reason they
cannot is written down where the next reader will find it.

### W4 — gate the property

- [ ] A check that fails when the same `-C metadata` identity is built into more
      than N target dirs in one lane, so this cannot silently regrow.

**Acceptance:** the gate catches a deliberately reintroduced duplicate.

## Risks

**Shared target dirs serialise.** Cargo takes an exclusive lock per target dir,
so grouping leaves trades parallel-but-redundant work for serial-but-unique
work. That is a win only if the redundancy exceeds the serialisation; W2 must
measure rather than assume. The per-RMW isolation exists for a real reason
(issue 0400's host/box split is the same class) and must survive.

**sccache's role is unverified.** Overall hit rate is 97 %, Rust-specific 68 %,
with 5310 non-cacheable calls. The obvious hypothesis is that `incremental`
makes Rust compilations non-cacheable, which would make W1 also a cache fix —
but repeated A/B probes contradicted each other, so it is written down as the
next experiment, not as a premise for any of the work above.
