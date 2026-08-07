# Phase 340 — Build-artifact reuse: compile each identity once

**Status (2026-08-07).** IN PROGRESS, and **CONSOLIDATED with**
[phase-334](phase-334-build-cache-layout.md): the two are one program on two axes
and their items had begun to overlap (334 W3.a was this phase's W2, W3.b was its
W3). This phase owns WHAT gets compiled and how often; 334 owns WHERE the result
lives. The ordered plan for both is "Work order (both phases)" below — the
authoritative copy. Measurement-first, and the measurements
are the deliverable so far: W1 answered (`incremental` for the shared profile,
single-leaf and lane-scale results recorded), W5.a measured (the biggest
duplicate is INSIDE one invocation — the build-dep graph). W4 landed 2026-08-07:
the measured numbers are now a gate (`check-artifact-identity-budget`), so they
cannot regrow quietly while the rest is in flight. W2/W3 (collapse the R1
duplicates, the corrosion `--target` split) not yet landed.

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

Re-measured 2026-08-06 on `nros-core`, **features held constant**
(`--no-default-features --features alloc,std`), one factor varied per arm, each
into its own target dir. The value is `libnros_core-<hash>.rlib`:

| Factor | Changes identity? | hash |
| --- | --- | --- |
| *(baseline)* | — | `ef26eca89af52b10` |
| Same build, different target dir | **no** | `ef26eca89af52b10` |
| Workspace root (leaf vs root) | yes | `18ad8b827eebc4b1` |
| Explicit `--target` vs implicit host | **yes, same triple** | `2b8d1fd2adda0290` |
| RUSTFLAGS | yes | `a2b0dc427d43178a` |
| Profile name — `release` | yes | `4b39f882b3c08c23` |
| Profile name — `dev` | yes | `ec004e0c9f10d286` |
| `opt-level` | yes | `742760f17a80b8cc` |
| `debug-assertions` | yes | `118191be48cc117d` |
| `panic` | yes | `866fba4594ca08e4` |
| `codegen-units` | yes | `9ffc8f9e7768b1ce` |
| `lto` | yes | `1f752f577b8e31d5` |
| `incremental` | **yes** | `b8ac6e54ff73351f` |

**Two corrections to the earlier version of this table.**

1. **`incremental` DOES change identity.** The old row said "no (identity), but
   breaks byte-equality". Measured twice over: `CARGO_INCREMENTAL=1` yields
   `b8ac6e54ff73351f`, and so does the `nros-iterate` profile — two independent
   routes to "relwithdebinfo plus incremental" landing on the same hash, distinct
   from the baseline. This is also why W1's change forced a rebuild of 37 units:
   it altered the identity, not merely the on-disk bytes.

2. **"Profile" is not one factor — it is at least six.** `opt-level`,
   `debug-assertions`, `panic`, `codegen-units`, `lto` and `incremental` each
   change identity ON THEIR OWN. Two builds can therefore name the SAME profile
   and still not share, if any single setting is overridden.

That second point is not hypothetical here: `nros_cargo_profile_env()` injects
`CARGO_PROFILE_<NAME>_*` variables for corrosion targets, so the cmake path
constructs its profile from environment rather than inheriting the manifest's.
Any divergence in one injected setting silently splits the identity. A build
tree bears this out — `examples/workspaces/mixed` carries **ten distinct profile
hashes at a single `--profile nros-relwithdebinfo`**, which is what produced the
"unattributed ×2" recorded in W4: those two `nros-core` fingerprints agree on
rustc, features, target, path, rustflags, config, compile_kind and all three
deps, and differ ONLY in `profile`.

**Practical consequence for W2/W3.** Feature-set equality is necessary and
nowhere near sufficient. Any grouping key that claims two builds are
interchangeable must cover the workspace root, the `--target` spelling, RUSTFLAGS
and every individual profile setting — not the profile NAME, which is a label
over six independent axes.

## Work order (both phases)

**phase-334 and this phase are one program on two axes**, and their work items
had begun to overlap: 334's W3.a was this phase's W2, and its W3.b was this
phase's W3. Two spellings of one mechanism is the drift RFC-0070 R3 forbids for
paths, applied to work items. Resolved 2026-08-07:

* **phase-334 owns WHERE a cache lives and what it is called** (RFC-0070) —
  W2.b, W2.c.
* **this phase owns WHAT gets compiled and how often** — and absorbs 334's W3.a
  (→ W2), W3.b (→ W3) and W3.c (→ **W7**, new).

They meet at one point: a grouped build needs a derived path to write to. That
is why 334 W2.b comes before the grouping work here.

| # | item | why here |
| --- | --- | --- |
| 1 | **340 W4 follow-up** | a live RED in `check-fast` — the gate counts the size probe's nested dirs, so it fails on a tree that merely built more |
| 2 | **340 W5.b / W5.c** | near-deletion now that #464 removed the fallback the edge served; also removes the probe dirs inflating (1) |
| 3 | **340 W6 step 1** | Zephyr remap + `OUT_DIR` + `codegen-units` together — the largest measured population, additive, serialises nothing |
| 4 | **334 W2.b steps 2–4** | the derivation for source-relative cache data; the precondition for moving any path |
| 5 | **340 W2** | umbrella invocation per identity group (was also 334 W3.a) |
| 6 | **340 W3** | normalise the corrosion `--target` split (was also 334 W3.b) |
| 7 | **334 W2.c** | collapse `.gitignore`, once (4) has moved the paths |
| 8 | **340 W7** | re-measure both axes against phase-331's pair (was 334 W3.c) |

(1) and (2) are small and unblock reading the gate honestly. (3) is the biggest
win needing no restructuring. (4) unblocks every path move. (5) and (6) are the
restructuring proper and cannot precede the derivation. (7) is cleanup, (8) is
proof.

**Sequencing hazard.** (5) and (6) both change what a build writes and where.
Landing either before (4) means editing literals that (4) is about to derive —
the 236-literal problem, re-created. Landing them together means two path
conventions in flight at once, which is what makes a family's build, staleness
probe and test resolver disagree (#393). One at a time, each with its own
measurement.

### W7 — re-measure both axes (was phase-334 W3.c)

- [ ] Re-run the phase-331 W1/W5 measurement pair after (5) and (6), so the
      consolidation numbers stay comparable across the layout change.
- [ ] Record the identity-gate budget's new value in the same commit — a budget
      left above the truth is a gate that has stopped gating.

**Acceptance:** the numbers in "The measurement" and "The disk story" above are
restated for the post-change tree, and `check-artifact-identity-budget`'s
budgets are lowered to match.

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

**Rewritten after W1.** The original plan — "share ONE target dir per identity
group" — names the right grouping and the wrong mechanism, and it would make the
lane slower for a benefit sccache already delivers. Three findings force the
revision.

#### F1 — CPU duplication and DISK duplication are different problems

The phase opened by treating "106 rlibs, 5 identities" as 106 compilations. With
a warm sccache it is not: rep 2 of the lane A/B recorded **17846 hits against
222 misses**. Those 106 are 106 *materialisations* of a handful of actual
compiles. "The machine is not compiling; it is repeating" is a **cold-cache**
statement, and the lane is cold only on a fresh machine.

Disk is the part nothing dedupes. sccache caches the compile and each consumer
still writes its own copy: 327 `libnros_core-*.rlib` on disk, 402 GB under
`examples/`. So W2's target is **bytes, not CPU** — and that changes which
mechanism is correct, because the two have opposite contention profiles.

#### F2 — the mechanism already exists, and R1 is opted OUT of it

`scripts/build/fixtures-target-dir.sh` (phase-226.D) already groups rows by
platform, triple, profile, no-default flag, sorted features, sorted env and sync
mode — essentially this phase's identity tuple — and hands the group ONE
`--target-dir`. It is gated to a single platform:

```sh
export NROS_FIXTURE_SHARED_PLATFORMS="${NROS_FIXTURE_SHARED_PLATFORMS:-qemu-arm-baremetal}"
```

and it explicitly yields to rows that author their own dir:

> the manifest row did NOT author its own `--target-dir` (authored dirs such as
> `target-zenoh` / `target-safety` win and stay per-example).

Those authored dirs ARE the R1 population this phase measures. R1 is therefore
not an unbuilt feature; it is an **opt-out**, and W2 is mostly a question of
extending an existing, proven resolver rather than writing a new one.

#### F3 — sharing a dir across CONCURRENT cargo processes is the wrong shape

Cargo takes an **exclusive** lock on a target dir for the whole build. The
fixture fan-out is parallel (`run()` → `run_with_make`, unless
`NROS_JOBSERVER=1`), so pointing N concurrent rows at one shared dir converts N
parallel builds into N serial ones. Against a warm sccache — where the duplicate
work was already cheap — that is a **net loss**: it removes redundancy that cost
little and adds serialisation that costs a lot.

The alternative shape is the one to build: **one cargo invocation over N
packages**, not N invocations over one dir. A single invocation takes the lock
once, builds each identity once by construction, parallelises internally through
its own jobserver, and writes one copy to disk. That is the difference between
lock contention and inner parallelism, and it is the whole design question in
this work item.

**The constraint that bounds group size: feature unification.** Cargo unions
features across workspace members built in one invocation (resolver 2 only
avoids unification across build-dep / dev-dep / target boundaries). So a group
may contain only rows whose feature set is IDENTICAL. Grouping `rmw-zenoh` with
`rmw-xrce` rows would union them — and because nano-ros has **no
`compile_error!` on multiple RMW features** (only on multiple ROS editions),
that union does not fail loudly. It silently builds every backend into every
fixture and changes what the tests are exercising. A silent behaviour change is
a worse failure than a build error, so the group key must keep features exact.

**How many groups are actually worth sharing — measured 2026-08-06.** Over the
117 `linux` fixture rows there are 60 distinct variant signatures, and the
distribution is bimodal:

```
 37 rows   (default features)
 10 rows   --no-default-features --features rmw-zenoh
  8 rows   --no-default-features --features rmw-xrce
  5 rows   --no-default-features --features rmw-cyclonedds
  2 rows   --features link-tls
 ---
 62 rows in 5 signatures   |   55 rows in 55 singleton signatures
```

**55 of the 60 signatures are singletons** — they can never share with anything,
and sccache is their only dedup. **Five signatures cover 53 % of the rows.** So
W2.b's target is those five groups specifically, not "same-identity groups" in
general: five umbrella builds replace 62 separate cargo invocations, and the
long tail keeps today's shape.

#### Relationship to phase-334 — the questions were framed there first

**F1/F2/F3 above are not new questions.** Phase-334 W1 asked all three before
this phase existed: W1.d ("if cache-hit builds get within ~15 % of shared-dir
builds, PREFER separate dirs + sccache"), W1.a (measure the target-dir lock's
serialisation cost, "report [phase-226's] measured numbers first"), and W1.b
(the feature-unification hazard and the signature count). This phase re-derived
them without noticing, and what it contributed is the **measurements**, not the
framing.

The verdicts are recorded in phase-334's status block, which now owns the W1
answers; phase-334 keeps W1.c and all of W2 (layout and naming), which this
phase does not touch. Keep the two in sync rather than restating: **334 owns the
sharing verdict and the layout rule; 340 owns the identity measurements and the
W2/W3 implementation.**

#### Work items

- [ ] **W2.a** Extend the phase-226.D resolver so a manifest-authored
      `--target-dir` names a GROUP rather than opting the row out, for rows whose
      identity already matches. No new mechanism; widen the existing key.
- [ ] **W2.b** Convert the FIVE head signatures (above) from N parallel cargo
      invocations into ONE invocation each over a build-time-only umbrella
      workspace — 62 of 117 linux rows. Leaves keep their standalone manifests
      (RFC-0026's copy-out promise); the umbrella is generated for the fixture
      build and never committed. The 55 singleton signatures are out of scope
      by construction: they have nothing to share with.
- [ ] **W2.c** Measure W2.b against the status quo on disk AND wall-clock,
      alternating reps per the W1 method.

**Rejected design, recorded so it is not re-proposed:** N concurrent cargo
processes sharing one target dir. It serialises on cargo's exclusive flock while
sccache had already made the duplicate compiles cheap. Any future "just point
them at the same dir" proposal must first show a cold-cache scenario.

**Considered and not taken:** post-build hardlink dedup of identical artifacts.
W1 restored byte-reproducibility so this is now *possible*, but the volume is
**ext4** — no reflink, so it would be true hardlinks, and any tool that rewrites
an artifact in place instead of replacing it would corrupt every sibling. Revisit
only with a measured list of what writes into these trees.

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

**Direction, given W2's reframing.** Unifying means picking ONE spelling for
every host build, and the cheap-looking direction is the wrong one. Dropping
corrosion's `--target` is not a local edit — corrosion derives it from
`Rust_CARGO_TARGET` and the cmake integration is built around it. Adding
`--target` to the native leaves is mechanical but WIDE: with an explicit target
triple cargo moves artifacts from `target/<profile>/` to
`target/<triple>/<profile>/`, and every fixture resolver, staleness probe and
staging path that spells the former would have to move as one change. That is
the issue-0196 class — a path convention with many spellings — so it is a
class-wide sweep or nothing, never a per-site fix.

Note this split costs CPU as well as disk, and sccache does NOT paper over it:
a different `-C metadata` is a different cache key, so the cmake-driven and
cargo-driven builds of an identical crate miss each other's entries. Unlike R1,
this duplication is real compilation even with a warm cache — which makes W3 the
better CPU target and W2 the better disk target.

#### Measured 2026-08-06 — the split is total, and it is the FLAG not the driver

Same crate, same features (`std,rmw-zenoh`), same triple, one factor varied:

| build | `nros_core` `-C metadata` |
| --- | --- |
| implicit host (what a cargo leaf does) | `d43b191ebc13de43`, `ef26eca89af52b10` |
| `--target x86_64-unknown-linux-gnu` (what corrosion does) | `2b8d1fd2adda0290` |

Intersecting the identity SETS actually on disk:

```
corrosion trees  ∩  native cargo leaves      = {}          <- zero overlap
corrosion trees  ∩  explicit --target build  = {2b8d1fd2adda0290, 482da2a2e947742f}
corrosion trees  ∩  implicit host build      = {2b8d1fd2adda0290}
```

Three things follow.

1. **Feature-set equality is NOT sufficient for sharing.** The cmake-driven and
   cargo-driven halves of this repo share *nothing* — the intersection is empty,
   not small. Every crate in the shared stack is built at least twice.

2. **The cause is the `--target` flag, not cmake or corrosion.** A plain cargo
   build that passes the same explicit `--target` immediately shares two of
   corrosion's four identities. So this is normalisable by changing one flag's
   spelling, which is what W3 is.

3. **Part of the tree already normalised, by accident.** `2b8d1fd2adda0290`
   appears even in the IMPLICIT build — via the nested `nros-sizes-build` probe,
   which passes its own explicit `--target` regardless of the outer mode. The
   probe's sub-builds therefore already sit on corrosion's side of the split.
   That is a working demonstration that the fix produces sharing rather than a
   theory that it would.

**Acceptance:** either the two paths share an identity, or the reason they
cannot is written down where the next reader will find it.

### W4 — gate the property

#### The user-facing reproducer, measured 2026-08-06

The question a user actually asks: *my project needs a Rust leaf R and a C++
leaf C, both depending on a Rust dependency D, and I choose the feature set — is
D built once?*

`examples/workspaces/mixed` IS that project: Rust and C++ node packages in one
workspace over a shared `nros-core`. Counting `nros-core` in its build tree:

```
08130df18f473a4b  nano-ros_0b88c/x86_64-unknown-linux-gnu/nros-relwithdebinfo
2b8d1fd2adda0290  nano-ros_0b88c/x86_64-unknown-linux-gnu/nros-relwithdebinfo
482da2a2e947742f  nano-ros_0b88c/nros-relwithdebinfo
b2744896132993cc  nano-ros_0b88c/nros-relwithdebinfo
4a86c738c0e9ce80  nros_ws_runtime_14eac/x86_64-unknown-linux-gnu/nros-relwithdebinfo
623c410f90cfce6a  nros_ws_runtime_14eac/x86_64-unknown-linux-gnu/nros-relwithdebinfo
a46af0c36bd41dd5  nros_ws_runtime_14eac/nros-relwithdebinfo
cf7d853a4c3ed530  nros_ws_runtime_14eac/nros-relwithdebinfo
```

**D is built EIGHT times, with eight distinct identities — zero sharing**, in a
single workspace, at one user-chosen feature set. The split is three-way and
each axis is a different cause:

| axis | copies | cause |
| --- | --- | --- |
| two corrosion roots — `nano-ros_0b88c` (the C++ side) vs `nros_ws_runtime_14eac` (the Rust umbrella) | ×2 | **R2** — separate cargo invocations resolving through DIFFERENT workspace manifests. Corrosion keys its dir on `sha1(workspace_manifest_path)`, so they cannot even land in the same tree. |
| host vs `x86_64-unknown-linux-gnu` | ×2 | **R3** — the explicit `--target` split, here appearing WITHIN one invocation (build scripts/proc macros vs the library). |
| two identities per root+arch cell | ×2 | **unattributed.** Both fingerprints report identical `features = ["alloc","std"]`, so it is not feature-driven; `-C metadata` folds in dependency metadata, so the likeliest cause is the two staticlibs (`nros-c`, `nros-cpp`) resolving an intermediate crate differently. Not yet proven — do not quote a cause. |

Note what this rules out: the R/C split here is **not** the corrosion-vs-cargo
flag difference measured in W3, because BOTH sides go through corrosion with an
explicit `--target`. Same driver, same flag, same features — and still no
sharing, because they are different workspaces. **Fixing W3 alone would not make
D build once for this user.**

#### Work item — LANDED 2026-08-07

- [x] A check that fails when the same `-C metadata` identity is built into more
      than N target dirs in one lane, so this cannot silently regrow.
- [x] Use the mixed workspace as the gate's fixture: assert `nros-core` is built
      at most K times for one workspace at one feature set. It is the smallest
      honest reproducer of the whole phase, it already exists, and today it
      answers 8.

`scripts/check-artifact-identity-budget.sh`, wired into `check-fast` as
`check-artifact-identity-budget`. It reads `lib<crate>-<hash>.rlib` filenames
under `examples/workspaces/mixed/build-workspace-fixtures` — cargo's own
`-C metadata` judgement, so "same compilation?" is answered by construction —
and asserts three numbers, all measured 2026-08-07 on a full native-lane tree:

| budget | value | what it pins |
| --- | --- | --- |
| `nros_core` identities | 8 | the headline number this phase measured |
| any crate's identities | ≤9 | `nros_serdes` is the max: the 8 plus one more from the nested per-msg-package `nros-minsizerel` trees |
| copies of ONE identity | ≤5 | the five `src/*/nano_ros_cpp_ffi_*/target/` trees each write their own copy of the SAME hash — R1, verbatim |

The named budget pins the crate the phase measured; the two ceilings are the
class-wide arm, so regrowth in a crate nobody named still fails. **When W2/W3/W5
land, lower the numbers in the script** — a budget left above the truth is a
gate that has stopped gating.

Tier, and its cost: the gate is buildless (filenames only, no cargo/rustc/
resolution) but not source-free, since it needs a tree someone already built. On
the pristine per-push CI checkout it therefore SKIPS — loudly, naming the build
command. It is deliberately NOT in `build-test-fixtures`: a long-lived
incremental tree accumulates rlibs from earlier builds, so an over-count from
history alone is possible, and a gate that can red a BUILD on stale history gets
switched off. Failing a static check whose remedy is "wipe the tree and rebuild"
is survivable.

**Acceptance — met.** Tripwired on a filename-only replica of the real tree
(`find … -name 'lib*-*.rlib'` mirrored as empty files, which reproduces the real
verdict exactly): a 9th `nros_core` identity fails the named budget; a 10th
`nros_serdes` identity fails the ceiling; a 6th copy of one identity fails the
copies arm; and a tree holding rlibs but none for `nros_core` fails rather than
passing on a tree it did not understand. All four restored to green afterwards.

**What it cannot catch.**

- It counts **rlibs**. Proc-macro dylibs, build-script executables, staticlibs
  and binaries are invisible to it, so duplication that lands only in those
  forms is not counted.
- It reports a **number, not a cause**. It cannot say which of the axes grew,
  and a swap — one identity disappearing while a different one appears — keeps
  the count at 8 and passes.
- Only the **native mixed tree**. `build-workspace-fixtures-freertos`,
  `-threadx`, and the other three workspaces are unread; a regression confined
  to them is silent unless someone points `NROS_IDENTITY_BUDGET_TREE` at them.
- **Nothing at all** on a checkout that has not built the mixed workspace, which
  includes the per-push CI lane.
- It **over-counts on a long-lived incremental tree**: cargo never collects old
  rlibs, so identities from earlier builds accumulate on disk. That is the
  deliberate direction of the error — it can cry wolf, it cannot under-count —
  and the failure text says so, naming "wipe the tree and rebuild" before
  believing the number.

### W5 — the duplicate INSIDE one invocation: the build-dependency graph

Everything above is about duplication ACROSS invocations. Measured 2026-08-06,
the largest single source is inside ONE. `cargo --unit-graph` for `nros-c`
(`std,rmw-zenoh`, `nros-relwithdebinfo`):

```
181 units, 4 distinct profiles
  123 units  opt=0 panic=unwind debuginfo=0   91 lib + 27 custom-build + 5 proc-macro
   26 units  opt=2 panic=abort  debuginfo=1   25 lib + 1 staticlib   <- the product
   21 units  opt=0 panic=unwind lto=false     custom-build
   11 units  opt=2 panic=unwind lto=false     custom-build
```

**Only 26 of 181 units are the product.** Of the 14 product libraries, **12 are
ALSO built in the build-script graph** — `heapless`, `log`, `bitflags`,
`byteorder`, `cfg-if`, `hash32`, `atomic-waker`, `portable-atomic`,
`portable-atomic-util`, `stable_deref_trait`, `nros-rmw-cffi`, and `nros` itself.

**Features are NOT the cause here.** `nros-core` appears twice with an identical
feature set:

```
nros-core feats=['alloc','std']  panic=abort   opt=2     <- product
nros-core feats=['alloc','std']  panic=unwind  opt=0     <- build-dep graph
```

Same crate, same features, one invocation, two compilations. And sccache cannot
absorb it: different profile means different rustc arguments, so a different
cache key.

**It cannot be fixed by making the profile consistent.** `build-override` aligns
`opt-level`, `debuginfo`, `lto` and `codegen-units`, but cargo rejects the one
that matters:

```console
$ CARGO_PROFILE_NROS_RELWITHDEBINFO_BUILD_OVERRIDE_PANIC=abort cargo build …
error: `panic` may not be specified in a `build-override` profile
```

Build scripts and proc macros are always unwinding, by construction. **So while
the product profile carries `panic = "abort"`, every crate in both graphs is
compiled twice and no configuration prevents it.** Aligning the other four
settings buys nothing on its own, because `panic` alone still splits identity.

**The lever is the GRAPH, not the profile.** The whole runtime stack is in the
build-dependency graph because of one edge in `packages/api/nros-c/Cargo.toml`:

```toml
[build-dependencies]
# Phase 77.25: … force nros to compile before
# this build.rs runs so the size probe has a rlib to read.
nros = { version = "0.5.0", path = "../nros", default-features = false }
```

`nros` is a build-dependency **purely to force build ordering** — the size probe
locates its rlib with `find_dep_rlib`. That one ordering trick pulls
`nros → nros-node → nros-core → heapless, portable-atomic, log, …` into a graph
that then compiles all of it a second time at `opt=0 panic=unwind`.

- [x] **W5.a** ANSWERED 2026-08-06 — **the edge is dead weight for the DEFAULT
      probe path and load-bearing for the FALLBACK one.** It is a trade, not a
      free deletion. Detail below.

#### W5.a result

`find_dep_rlib` has two implementations and tries them in order:

* **`find_dep_rlib_isolated`** (default, first) — builds the crate in its own
  nested target dir. Needs no ordering from the outer cargo, so the build-dep
  buys it nothing.
* **`find_dep_rlib_filesystem`** (fallback, and what
  `NROS_SIZES_PROBE_MODE=filesystem` forces) — polls the OUTER target dir for the
  rlib with a 60 s timeout. The build-dep is what makes that deterministic
  instead of a race.

Removing the edge from `nros-c` and rebuilding:

| | units | product libs | build-graph libs | built twice |
| --- | --- | --- | --- | --- |
| with the edge | 181 | 14 | 68 | **12** |
| without | 165 | 14 | 64 | **8** |

The build succeeds, the isolated probe does NOT fall back (no
`cargo:warning=… isolated probe failed`), and the generated sizes are
**byte-identical** (`NROS_EXECUTOR_SIZE 89392`, …) to the with-edge tree. So
correctness is preserved on the default path, and removal buys 16 units and
drops `nros`, `nros-rmw-cffi`, `atomic-waker`, `portable-atomic` and
`portable-atomic-util` out of the duplicated set.

**But do not delete it outright.** `NROS_SIZES_PROBE_MODE=filesystem` still ran
green twice without the edge — which is NOT evidence of safety. That path is a
poll with a timeout, so it usually wins; the manifest comment records it losing
historically ("the probe ran against a missing rlib on clean" builds), and
`just verify-size-probe` exercises both modes as a parity gate. Two green runs
cannot refute a race the code was written to prevent.

The remaining 8-crate overlap (`heapless`, `log`, `bitflags`, `byteorder`,
`cfg-if`, `hash32`, `stable_deref_trait`) comes from the OTHER build-deps —
`nros-build-helpers`, `nros-zpico-build`, `nros-board-common` — so this edge was
never the whole story.
**Update 2026-08-07 — issue 0464 changed W5.b's cost.** The filesystem fallback
the edge existed to serve is GONE, and NuttX was verified building through the
isolated probe alone. So the edge now serves nothing on any exercised path: W5.b
is closer to a deletion than to the feature-gate below, and the feature-gate is
only needed if a future target reintroduces a fallback requirement. Re-measure
before choosing.

- [ ] **W5.b** Make the edge OPTIONAL rather than unconditional: an optional
      `[build-dependencies] nros` behind a feature (say `sizes-probe-filesystem`),
      off by default so the isolated path pays nothing, and enabled by the builds
      that actually need the fallback — the custom-target/`build-std` embedded
      configs the fallback exists for. Then `just verify-size-probe`'s filesystem
      arm enables it explicitly rather than relying on an edge every other build
      also pays for.
- [ ] **W5.c** Apply the same to `nros-cpp`, whose comment documents the
      identical pattern, and re-measure the overlap.
- [ ] **W5.d** Attack the residual 8: `nros-build-helpers` / `nros-zpico-build` /
      `nros-board-common` pull `heapless`, `log` and friends into the build graph.
      Establish whether those build-deps need the runtime crates at all.

**Acceptance:** the product/build-graph overlap drops from 12 of 14 with
`just verify-size-probe` still green in BOTH modes, or the reason it cannot is
recorded at the manifest edge that causes it.

### W4 follow-up — the budget counts W5's probe dirs, so it drifts with build history

`check-artifact-identity-budget` went RED on my tree today with
`nros_core has 9 distinct -C metadata identities … (budget 8, recorded
2026-08-07 by phase-340 W4)`, on a working tree whose only source change was
documentation. Counted:

```
52  libnros_core-*.rlib under examples/workspaces/mixed/build-workspace-fixtures
40  of them (76 %) inside .../build/nros-c-<hash>/out/sizes-probe-target-.../
 8  distinct identities once the sizes-probe dirs are excluded  ← the budget
```

The 9th identity comes entirely from the size probe's NESTED target dirs — one
per `nros-c` build-script instance, each rebuilding `nros_core` inside its own
`sizes-probe-target-rustc-…` tree. That is W5's duplicate-inside-one-invocation,
observed from the gate's side, and it is also what issue 0464 is about.

The consequence for the gate: **its count grows with how many times the tree has
been built, not with what the source says.** A budget recorded on one tree
red-lights another that merely built more, and `just check-fast` then fails for
a reason no diff explains — which is expensive precisely because the gate is in
the fast tier that every task runs first.

Options, in the spirit of the gate: exclude `*/out/sizes-probe-target-*/` from
the count (it measures a different phenomenon, already owned by W5), or count
identities per target-dir root rather than per tree. Excluding the probe dirs
restores the recorded 8 exactly, which suggests that is the population W4 meant.

### W6 — the ZEPHYR fixture lane, which no measurement above covers

Everything measured so far is the `linux` fixture rows (117 rows, 60 signatures)
plus the build-dep graph. The **Zephyr west lane is a separate population and a
bigger one**, and it is opted out of every mechanism this phase has considered.
Measured 2026-08-07 from the driver's own per-fixture records
(`build/zephyr-fixture-make-driver/status/<run>/*.status`, which carry
`start_epoch` / `end_epoch` / `duration_s`).

#### The lane is CPU-bound on duplicated Rust compiles, not on overhead

Two consecutive `just zephyr build-fixtures` runs:

| run | fixtures | wall | CPU-sum | median/fixture |
| --- | --- | --- | --- | --- |
| colder | 68 | 33.2 min | 871 min | 776 s |
| warmer | 68 | 19.4 min | 509 min | 441 s |

Recipe start → driver start is **~40 s**, so the fan-out is essentially the
whole wall time; there is no setup tax worth chasing. Concurrency is already
32-way for 58 % of the wall (a 26× compression of 509 CPU-minutes). The lane is
not under-parallelised — it is doing too much work.

A single fixture's ninja graph is 42 steps, and three of them are the cost:

```
[1/44] Building nros-c via Cargo
[2/44] Building nros-cpp via Cargo
[4/42] Building nros-rmw-zenoh-staticlib via Cargo
```

`nros_cargo_build()` sets `CARGO_TARGET_DIR` to
`${CMAKE_BINARY_DIR}/nros-rust` — **per Zephyr build dir**. So all 68 fixtures
compile those crates into 68 private target dirs. On disk:

```
415 G   zephyr-workspace/            (85 build dirs)
286 G     of which per-fixture cargo target dirs (45 `nros-rust` dirs)
1069    libnros_core-*.rlib copies
  36    libnros_c.a  /  36  libnros_cpp.a
```

For scale, the tree-wide count this phase opened with was 327
`libnros_core-*.rlib`; the Zephyr lane alone holds **1069**.

#### The duplicates are the same compilation, differing only by an embedded path

This is the part that decides the mechanism. Two same-RMW C fixtures
(`build-c-talker-zenoh`, `build-c-listener-zenoh`) have **identical Kconfig** —
`diff` over their 29 `CONFIG_NROS_*` knobs is empty — so their `libnros_c.a`
should be one compilation. The artifacts differ by **8 bytes**:

```
34796696  build-c-talker-zenoh/.../libnros_c.a
34796704  build-c-listener-zenoh/.../libnros_c.a
```

and each embeds its own build-dir path twice (`strings | grep -c` gives 2/0 and
0/2). They are byte-different only because the target dir lives inside the
Zephyr build dir and rustc bakes that absolute path in.

That matters beyond disk. sccache's Rust hashing includes the CONTENT of the
`--extern` rlibs a crate is compiled against. Once the bottom of the graph
differs by a path string, **every crate above it misses across fixtures** — the
cache can only hit for leaf crates with no externs. So the lane pays real
compiles 68 times for work that is identical modulo a string.

#### sccache is wired and sized — but the size is only honoured if `just` starts the server

`RUSTC_WRAPPER` is exported globally (`justfile:13`) and
`SCCACHE_CACHE_SIZE := "30G"` right below it, with phase-165.perf's reasoning
attached: the 10 GiB default "evicts mid-sweep once the ~150 standalone
example/fixture crates plus the Zephyr C objects land in the cache". So the lane
is neither missing the wrapper nor under-sized, and "raise the cache" is NOT an
available win. I first wrote that it was, having read `Max cache size 10 GiB`
from a server I had started myself outside `just`.

That mistake is the finding. As the justfile comment says, the variable is
"only read at sccache server start" — so whichever process starts the daemon
fixes the size for every later user. A server started by anything outside `just`
(a bare `sccache --show-stats`, an editor, rust-analyzer) silently gives the
whole sweep a 10 GiB cache, and nothing reports it. The sweep gets slower with
no signal, which is the same silent-degradation shape this repository keeps
finding elsewhere.

Cheap guard: have the Zephyr lane compare the running server's `Max cache size`
against `SCCACHE_CACHE_SIZE` and say so when they disagree (restarting the
daemon is a one-liner, but even just printing it converts an invisible 3×
capacity loss into a line of output).

#### The remap is PROVEN, not proposed — measured 2026-08-07

Built `nros-c` twice into two different target dirs and compared:

| build | result |
| --- | --- |
| plain, two target dirs | differ |
| `--remap-path-prefix=<dir>=/nros-target`, dev profile | differ — **379 bytes** of 24.5 MB |
| same remap, non-incremental profile | **IDENTICAL — 0 differing bytes** |

Two things fall out of the middle row. The residual 379 bytes were entirely
codegen-unit name suffixes (`nros_c.<hash>.1w7uncg.rcgu.o` vs `…10ahzya…`) —
independent confirmation of this phase's R4, that `incremental` destroys
byte-reproducibility, arrived at from the opposite direction. And the symbol
hashes were IDENTICAL across the two builds (`17h41f705d7eeb2beb8E` in both),
which answers a question the remap raises: each fixture must pass a DIFFERENT
`--remap-path-prefix` flag (its own dir on the left-hand side), and that flag
difference does **not** perturb `-C metadata`. So per-fixture remapping still
yields one shared identity.

W1 having already dropped `incremental = true` from the shared profiles
(2026-08-06 — it survives only in the interactive `nros-iterate`), the
non-incremental row is the configuration the fixture lanes are in TODAY.
**The embedded target-dir path is the only thing left between the Zephyr lane
and bit-identical artifacts.**

Repro:

```sh
RUSTFLAGS="--remap-path-prefix=$D/pA=/nros-target" cargo build -p nros-c --release --target-dir $D/pA
RUSTFLAGS="--remap-path-prefix=$D/pB=/nros-target" cargo build -p nros-c --release --target-dir $D/pB
cmp $D/pA/release/libnros_c.a $D/pB/release/libnros_c.a   # → equal
```

#### Tried on the lane: remap is NECESSARY but NOT SUFFICIENT

I implemented the remap in `nros_cargo_build.cmake` and rebuilt the two C
fixtures. The flag reaches the command (3 occurrences in `build.ninja`) and the
artifacts remain different — same size, **1539 differing bytes**, and the build
dir path is STILL embedded twice. Reverted rather than landed: a flag with no
measured effect is not worth a full-tree rebuild.

Two residuals explain the gap between this and the clean experiment above, and
both are worth knowing before anyone retries:

* **`env!("OUT_DIR")` is not a source path.** The surviving string is
  `…/nros-rust/…/build/nros-c-<hash>/out/…`, captured by nros-c's build script
  and baked in as a string literal. `--remap-path-prefix` rewrites paths in
  debug info and `file!()`; it does not rewrite the CONTENT of an `env!`
  literal. Any crate using the standard `include!(concat!(env!("OUT_DIR"), …))`
  pattern carries its absolute target dir into the artifact regardless.
* **`codegen-units = 16`.** The clean experiment used `--release`
  (`codegen-units = 1`) and reached 0 differing bytes; the fixture profile
  `nros-relwithdebinfo` uses 16, and the residual diff is again CGU name
  suffixes (`…0omp3nw.rcgu.o` vs `…1dda5ar.rcgu.o`). With 16 units the
  partitioning is not reproducible run to run. That is a real build-speed
  tradeoff, not an oversight — CGU=1 is slower to compile.

So cross-fixture artifact identity on this lane needs all three of: the remap,
an answer for `env!("OUT_DIR")` literals, and `codegen-units = 1` (or accepting
non-identity). Any one alone changes nothing measurable.

**Which matters for CPU, not just disk.** sccache hashes a crate's inputs
including the CONTENT of its `--extern` rlibs. CGU nondeterminism and OUT_DIR
literals both make the bottom of the graph differ, so misses cascade upward
whichever one is left unfixed. That is why partial application yields nothing:
the cascade only stops when the artifacts are actually identical.

#### Proposed order — cheapest and least risky first

1. **All three of remap + OUT_DIR + `codegen-units = 1`, together**, since the
   measurement above shows any subset changes nothing. That converts 68
   distinct-by-accident compilations into ONE cacheable identity, which fixes
   CPU (sccache hits cascade up the graph instead of dying at the first extern)
   AND makes the 1069 rlib copies dedupable. Verify with the two-fixture `cmp`.
   The `codegen-units` part needs its own A/B — it trades single-build speed for
   cross-build reuse, and on a 68-way fan-out that trade plausibly pays, but
   this phase's own W1 shows such intuitions need measuring.
2. **Report an sccache size/PID mismatch** on the lane (above). Not a speed-up
   in itself; it stops a 3× capacity loss from being invisible while measuring
   step 1.
3. **Only then consider sharing target dirs**, and note W2's F3 applies with
   full force here: the fan-out is 32-way, so pointing a group at one dir
   converts 32 parallel builds into serialised ones. Step 1 may make step 3
   unnecessary, which is the preferred outcome.

Steps 1 and 2 are additive — they neither serialise anything nor change what any
fixture builds — so they can land independently of W2's umbrella-invocation
design.

#### One caveat for whoever groups by identity here

The Zephyr lane's identity tuple must include the **resolved Kconfig knobs**,
not just profile/features/triple: `nros-node`'s `build.rs` bakes
`NROS_EXECUTOR_MAX_CBS` and friends into constants, so two fixtures with the same
cargo invocation and different `.config` are genuinely different compilations.
That divergence is newly REAL as of issue 0460's fix — before it, the Rust lane
silently compiled crate defaults regardless of Kconfig, so every image happened
to be identical for the wrong reason.


## Risks

**Shared target dirs serialise.** Cargo takes an exclusive lock per target dir,
so grouping leaves trades parallel-but-redundant work for serial-but-unique
work. That is a win only if the redundancy exceeds the serialisation; W2 must
measure rather than assume. The per-RMW isolation exists for a real reason
(issue 0400's host/box split is the same class) and must survive.

W1 sharpened this from a risk into a **design constraint**: with a warm sccache
the redundancy is already cheap, so the trade is a loss unless the sharing comes
from ONE cargo invocation rather than N invocations over one dir. See W2 F3.

**sccache's role — resolved for the `incremental` half, still open for the
rest.** The hypothesis recorded here was that `incremental` makes Rust
compilations non-cacheable, which would have made W1 a cache fix too. **It is
false**: `non_cacheable_compilations` was 0 in all four lane arms and in every
single-leaf arm, across ~30 000 compilations. What `incremental` actually does is
keep units away from the cache — 11514 sccache requests with it on versus 18780
with it off — so the cache serves less, without anything becoming uncacheable.

The 5310 non-cacheable calls in the original observation therefore have some
other cause, still unidentified, and the 68 % Rust hit rate is not explained by
this phase. **W2 must not be scoped as if fixing it were a known outcome.**
Finding what those calls are is worth its own investigation; the likely
candidates are the C/C++ compilations in the cmake fixtures rather than rustc at
all, since the overall rate (97 %) and the Rust rate (68 %) differ so widely.
