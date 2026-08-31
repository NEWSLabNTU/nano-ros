# Phase 400 — Rust dependency weight audit

**Status (2026-08-30). IN PROGRESS — W1 and W2a landed. W2's orchestration half
was ATTEMPTED AND REVERTED: its 43-crate estimate measured as 6, because
`nros-orchestration-ir` is needed by every arm of the macro and is itself what
pulls the heavy tail. The lever moves to splitting that crate; see W2. **W3 is
also RETRACTED**: its 20.6 s of bindgen belongs to upstream `zephyr-sys`, and the
four `*-sys` crates the plan named are workspace-EXCLUDED with zero consumers, so
they contribute nothing to any measured build. W4, the leaf-graph attribution, is
the only item left with a defensible premise — and should have been first. Waves
are ordered by measured value, not by discovery order.**

**Read the W2 table with W2a's caveat**: the 50.4 s pair assumes BOTH halves
ship, because the 27.4 s contested pool only frees when the last consumer of
those units goes. W2a alone frees the cbindgen-exclusive 8.1 s, not its share of
the pool. Opened because a Zephyr build profile showed cargo dominating the
wall clock. This phase is about what the image COMPILES, not about how the build
is scheduled — phase-371 (CLOSED) covered scheduling and is worth reading first
for its record of six retracted hypotheses.

## Method, and why it is stated first

Phase-371's lesson was that plausible build-performance conclusions are usually
wrong, so every number below names how it was taken.

* Crate sets come from `cargo tree --edges normal --prefix none | sed 's/ (\*)$//'
  | sort -u`, not from reading manifests. A manifest read misses transitive
  reachability, which is the whole question.
* Times are COLD (`rm -rf` the target dir), `--release`, and record **CPU as well
  as wall** (`/usr/bin/time`). On a 32-core host wall time hides cost: 48 extra
  crates that compile in parallel barely move the wall clock of ONE leaf while
  still competing for cores with every other leaf in a fixture sweep.
* sccache was verified OFF for these measurements (`RUSTC_WRAPPER` unset, 8
  lifetime compile requests). A cached measurement of dependency weight measures
  the cache.

**One method that did NOT work, recorded so nobody repeats it:** deleting deps
from a manifest and re-running `cargo tree` to see what disappears. `cargo tree`
ERRORS on the broken manifest and prints nothing, so the diff reports the entire
graph as removed. The valid form is a difference of SUBTREES —
`tree(nros-macros) − tree(syn) − tree(quote) − tree(proc-macro2) − tree(nros
without macros)`.

## The finding: `nros`'s `macros` feature triples the graph

Measured against the feature set every Zephyr Rust leaf uses
(`default-features = false, features = ["alloc", "rmw-cffi", "macros"]`):

| | crates | wall | CPU | max RSS |
| --- | --- | --- | --- | --- |
| `alloc,rmw-cffi` | **19** | 1.63 s | 4.1 s | 227 MB |
| `alloc,rmw-cffi,macros` | **67** | 5.26 s | 24.0 s | 364 MB |

The 48 added crates are a host-side toolchain: `serde` + `serde_derive` +
`serde_json` + `serde_yaml_ng`, `toml` + `toml_edit` + `toml_datetime` +
`toml_write` + `winnow`, `yaml-rust2` + `unsafe-libyaml`, `quick-xml` +
`encoding_rs` + `memchr`, `walkdir`, `eyre`, `thiserror` ×2 (both 1.x and 2.x),
`hashbrown` ×2, `indexmap`, `ahash`, `zerocopy`, and the three
`ros-launch-manifest` git crates.

They arrive through `nros-macros`, which is a proc-macro crate and therefore
compiles for the HOST on every leaf. Standalone examples are their own workspace
roots with their own target dirs (RFC-0026), so this is paid PER LEAF, not once.

### It is reachable, but not used by these images

`nros-macros`'s heavy deps are used by exactly two of its source files:

```
toml, nros-pkg-index, nros-orchestration-ir,
ros-launch-manifest-model            -> src/main_macro.rs
serde_json, nros-orchestration-ir    -> src/source_metadata_sidecars.rs
```

That is the LAUNCH ORCHESTRATION path — `nros::main!(launch = "bringup")`. The
Zephyr talker uses `force_link_backend!`, `zephyr_component_main!` and `node!`,
none of which touch it. Cargo compiles the whole proc-macro crate regardless of
which macro a leaf expands, so every non-orchestrating image pays for the
orchestrating one.

## W1 — landed: `nros-launch-parser` was declared and never referenced

`nros-macros` depended on `nros-launch-parser` with no `::` reference anywhere in
its `src/`. Confirmed independently by `cargo-machete`. Removed.

**Honest size of the win: one crate.** Everything `nros-launch-parser` brings
(`quick-xml`, `eyre`, `serde_json`, `walkdir`) is also reached through
`nros-pkg-index`, which the crate genuinely uses — 67 → 66. It is worth doing
because a dependency nobody references is a false statement about what the crate
needs, not because it is fast.

## W2 — orchestration half: ATTEMPTED 2026-08-30, REVERTED. The 43-crate figure was wrong.

The original plan, kept here because the correction is the finding: put
`main_macro.rs` and `source_metadata_sidecars.rs` (and their five deps) behind a
`launch` feature on `nros-macros`, off by default, forwarded by `nros`. Upper
bound quoted as **43 crates leave the graph**, measured by subtree difference.

**Implemented far enough to measure, and the number is 6, not 43.** With the
feature added and the four launch-only deps made optional:

```
feature OFF: 46 crates      feature ON: 52 crates
leaves: eyre, indenter, nros-pkg-index, quick-xml, same-file, walkdir
```

`zerocopy`, `yaml-rust2`, `hashlink`, `ahash`, `serde`, `toml`, `indexmap` — the
whole heavy tail the wave was built to shed — **all stay**.

**Why: `nros-orchestration-ir` cannot be gated with `main_macro.rs`.** It is not
launch-only. Every arm of the macro, including bare `main!()` and the board
resolution path shared with `node!`-only entries, reaches into it:

| item | where it is needed |
| --- | --- |
| `board_path_for` | board resolution, every arm |
| `FRAMEWORKS`, `framework_for_board_key`, `is_known_framework` | framework resolution, every arm |
| `executor_sizing::{LIFECYCLE_SERVICE_SLOTS, PARAM_SERVICE_SLOTS}` | executor sizing, every arm |

The compiler found this, not the plan: gating the crate produced
`unresolved import nros_orchestration_ir` at six sites outside the model branch.
The 43-crate subtree difference had silently assumed `nros-orchestration-ir`
leaves with the macro — and since that crate depends on all three
`ros-launch-manifest-*` crates itself, it is the one holding the tail. Gating
`ros-launch-manifest-model` at the `nros-macros` level is a no-op for the same
reason: it arrives through `nros-orchestration-ir` regardless.

This is the same failure mode this document already records once for the 31.9 %
figure: **a subtree difference is an upper bound on what COULD leave, never a
measurement of what DOES.** Both times the error was optimistic, and both times
only building it settled the question.

**The refactor was reverted rather than shipped.** Six light crates
(`walkdir`, `quick-xml`, `eyre`, ...) do not appear in the timings table at all,
and the cost to keep them out is a `launch` feature threaded through ~22 leaf
manifests plus the codegen emitter, a required-feature `compile_error!` path, a
`TierBake` boundary type to keep `ResolvedTierTable` out of the shared emit path,
13 `unused_mut` suppressions, and another round of leaf-lockfile churn. Against
the doc's own rule — *measure the pair; do not ship half and quote this table* —
that is not a trade worth making.

### Where the orchestration lever actually is: split `nros-orchestration-ir`

The measurement relocates the work rather than cancelling it.
**`nros-macros` needs a thin constant slice of `nros-orchestration-ir` and pays
for the whole model/tier/sched schema.** The slice is board paths, the framework
table, and the executor-sizing constants. The schema is what drags
`ros-launch-manifest-{types,sched,model}` -> `yaml-rust2` -> `hashlink` ->
`hashbrown` -> `ahash` -> `zerocopy` (6.9-7.3 CPU-s, the single largest unit in
the profile).

It is not a clean file-level cut, which is why it needs its own wave rather than
a follow-up commit. Of the crate's ten modules, six reference
`ros_launch_manifest` (`lib.rs`, `derive.rs`, `mapper_input.rs`,
`rtos_realizer.rs`, `cyclonedds_type_sizing.rs`, **and `executor_sizing.rs`**) and
four do not (`model_location.rs`, `qos_override.rs`, `sidecar_slots.rs`,
`wcet.rs`). Both `lib.rs` and `executor_sizing.rs` are on the every-arm path AND
touch the model types, so the split has to separate items, not files.

Sketch for whoever takes it: extract the board/framework/sizing constants into a
zero-dep crate (or a default-off `model` feature on `nros-orchestration-ir`,
with `nros-cli-core` enabling it), then re-run the `nros-macros` crate-delta
above. Only after that does gating `main_macro.rs` pay, and the two must be
measured together for the same reason W2a and this half must be.

Design note, still valid for whenever the gate lands: per the `std`-deletion rule
in CLAUDE.md, whose requirement it is decides the spelling. Launch orchestration
is a capability the CONSUMER picks, so the feature is REQUIRED (a
`compile_error!` naming `launch` when `main!(launch = ...)` is expanded without
it), not silently granted. That error path was written and worked; it is in the
reverted diff if it is wanted again.


### W2 re-measured 2026-08-30 on a CLEAN graph — the lever is 9 crates, and `zerocopy` is not in the build

Taking the remaining waves meant starting with W2's redirected lever (split
`nros-orchestration-ir`). The first measurement of it was WRONG, and finding out
why fixed a defect in the tool this phase tells people to trust.

**`just leaf-graph` had the stale-record defect.** A `.fingerprint/` directory
accumulates, so "every unit in the tree" is a historical record, not the current
build. On a freshly built Zephyr Rust talker leaf, two `nros-zpico-build` lib
units sat side by side:

```
nros-zpico-build-aeea…  mtime 08-29 23:02  feats=[]          deps: cbindgen, cc, …
nros-zpico-build-f75f…  mtime 08-30 22:21  feats=["default"] deps: cc, …
```

The tool reported `cbindgen <- nros_zpico_build` from the day-old one — for a
dependency **W2a had already removed**. Filtering stale records (175 of them in
that tree) drops the edge and confirms W2a is effective on this leaf, which the
contaminated graph had made look like a regression.

This is the same defect found in `shared-dir-churn` earlier the same day, in the
tool the "Use it before quoting any future number in this phase" line points at.
Fixed the same way, and with the same honesty about its limits: the exclusion is
REPORTED, not silent, because a live-but-fresh unit is not rewritten and an
incrementally built tree will legitimately have old ones. `--all-units` restores
the historical view. **Build the leaf, then measure it.**

**What the clean graph says.** Host side is **77 crates, not 129** — the extra 52
were stale records' crates.

```
--exclusive-to nros-orchestration-ir --exclusive-to ros-launch-manifest-model
  -> 9 crates drop: heapless, nros_core, nros_rmw, nros_serdes,
                    ros_launch_manifest_sched, ros_launch_manifest_types,
                    thiserror, thiserror_impl, yaml_rust2
```

**`zerocopy`, `ahash` and `hashlink` are ABSENT from the live build.** The W2
section above names `zerocopy` at 7.3 CPU-s as "the largest single unit freed" by
this wave and traces it through `ahash -> hashbrown -> hashlink -> yaml-rust2`.
That chain is not in this leaf any more: `hashbrown` survives but is required by
`indexmap`, and the other three are not built at all. `serde` and `toml` stay —
`zephyr_build`, `nros_board_common` and `nros_pkg_index` require them
independently of orchestration.

So the wave's headline justification is gone, and what remains is 9 crates whose
combined cost has not been measured. **The split is not being implemented on a
crate COUNT.** Nine crates including `nros_core`/`nros_rmw`/`nros_serdes` could be
seconds or tens of seconds; the phase's own rule is that a count bounds what
could leave and says nothing about what it costs. The next step is
`cargo build --timings` on this leaf attributing those 9, not a refactor.

*Status: W2 NOT implemented.* The design is sound and unchanged — a default-off
`model` feature on `nros-orchestration-ir` plus a `launch` gate on
`nros-macros`, landed together, since every schema-typed use in the macro crate
is confined to `main_macro.rs` (verified: `tier_from_model`,
`validate_tier_platform_applicability`, `ResolvedTierTable`,
`executor_sizing::count_*` all appear only there, and the four schema-free
modules `qos_override` / `sidecar_slots` / `model_location` / `wcet` are exactly
what the other arms use). What is missing is a reason, in seconds.

## W2 measured on the real build — `cargo build --timings`, cold

`--timings` injected into the Zephyr talker's `EXTRA_CARGO_ARGS`, the leaf's
`rust/target` (1.7 GB) DELETED, then `just zephyr build-one rust/talker zenoh`.
The HTML embeds `const UNIT_DATA`; per-unit durations parsed from it.

**234 units, 220 actually compiled, 13.5 s wall, 118.3 CPU-s.**

| chain (DISJOINT sets) | CPU | share |
| --- | --- | --- |
| `bindgen` chain, exclusive | 20.6 s | 17.4 % |
| `cbindgen` chain, exclusive | 8.1 s | 6.8 % |
| **nros orchestration crates** | **3.1 s** | **2.6 %** |
| **shared support crates** | **43.8 s** | **37.0 %** |
| target-side + everything else | 42.7 s | 36.1 % |

The "shared support" row is `serde`/`serde_core`/`serde_derive`, `toml` +
`toml_edit` + `winnow`, `serde_json`, `memchr`, `zerocopy`, `syn`, `thiserror`,
`indexmap`/`hashbrown`, `quick-xml`, `yaml-rust2`, `walkdir`, `eyre`. Heaviest:
`zerocopy` 6.9 s, `winnow` 3.7 s, `syn` 2.6+2.0 s, `memchr` 2.5 s.

**Gating the orchestration half removes 14.9 s exclusively (3.1 s of it the nros
crates themselves), not 37.7 s.** An earlier
version of this document claimed 31.9 %, and it was WRONG in a way worth
recording because the mistake is easy to repeat: the "removable" set was computed
from the `nros` package graph and then matched by NAME against the leaf's build,
so every shared crate got attributed to orchestration. The three groups
overlapped and were summed independently, which also double-counted them.
`cbindgen` parses `cbindgen.toml`, so it wants `serde` and `toml` whether or not
`nros-macros` does; `bindgen` wants `regex`/`memchr`/`prettyplease`. Cold, the
nros orchestration crates themselves are `nros-macros` 0.59 s, `nros-pkg-index`
0.06 s, `nros-orchestration-ir` 0.07 s.

The 43.8 s shared bucket is the real prize, and it is NOT claimable by any single
change: those crates leave the build only when EVERY requirer of them does.

**A discarded measurement, recorded so the number is not quoted from the log.**
The FIRST run of this build reported 45.3 s wall / 255.3 CPU-s and put
`nros-macros` second at 16.4 s. Two things were wrong with it: the leaf's
`rust/target` was already populated, so most third-party units read `0.00s`
(fresh, not compiled) and the orchestration share came out at a misleading 7.4 %;
and the per-unit times were inflated by CPU contention from other cargo work
running at the same time — cold and uncontended, `nros-macros` itself is 0.6 s.
Take the 118.3 s table, not the 255.3 s one.

## Work items, ordered by measured value

Sizes are exclusive savings from the cold Zephyr `rust/talker` profile below
(118.3 CPU-s total). "Exclusive" means: crates that leave the build when THIS
lever lands and nothing else changes.

| wave | lever | exclusive | share |
| --- | --- | --- | --- |
| W2 | gate orchestration **and** cbindgen, together | ~~**50.4 s**~~ see below | ~~42.6 %~~ |
| W3 | ~~`bindgen` -> committed output~~ RETRACTED | ~~20.6 s~~ 0 s | — |
| W4 | *landed* — `just leaf-graph`: ask the build, not the workspace | (enabling) | — |
| W1 | *landed* — unused dep removed | 1 crate | — |

**Numbering note.** W1 keeps its number because it has landed and is cited by
commit subject. The rest were renumbered into value order; an earlier revision of
this doc had the bindgen work as W4, the attribution as W3, and cbindgen as a
separate W5.

### W2 — gate the orchestration half AND move cbindgen, as ONE change

**Recorded as measured, then corrected — read the W2 section above before
quoting this.** The 27.4 s contested pool only frees if BOTH halves land, and
the orchestration half does not land as specified: gating `main_macro.rs`
removes 6 light crates, not the orchestration tail. W2a landed alone, so the
realised saving is the cbindgen-exclusive 8.1 s, not 50.4 s.

**50.4 s, 42.6 % — and only if both halves land.** This was the phase's main
finding and it is not visible from either half alone:

    orchestration-exclusive                    14.9 s   12.6 %
    cbindgen-exclusive                          8.1 s    6.8 %
    contested (serde, syn, toml, memchr,
      indexmap, thiserror, ...)                27.4 s   23.1 %

`cbindgen` parses `cbindgen.toml`, so it wants `serde` + `toml` + `syn`; the
orchestration path wants the same crates for launch files. **Do either one alone
and the 27.4 s contested pool stays** — a plausible-looking change that measures
as nearly nothing. Together they take 50.4 s of 118.3.

The orchestration cut is verified clean: `main_macro.rs` is 3798 of the crate's
4692 lines with `lib.rs:41` its only caller, and `source_metadata_sidecars.rs`
has exactly one caller (`main_macro.rs:884`). So
`#[cfg(feature = "launch")] mod main_macro;` plus five optional deps, forwarded
by `nros`. The feature is REQUIRED, not granted — a `compile_error!` naming
`launch` when `main!(launch = ...)` is expanded without it.

Largest single unit freed is `zerocopy` at 7.3 s, reached ONLY through
`ahash` -> `hashbrown` -> `hashlink` -> `yaml-rust2` -> `ros-launch-manifest-types`.
It is orchestration's transitive tail, not something the macro crate names.

*Acceptance:* a Zephyr Rust leaf builds with the feature off; `main!(launch = ...)`
without it fails naming the feature; cold `--timings` on the same leaf shows
`zerocopy`, `yaml-rust2`, `serde`, `toml` and the `clap` stack ABSENT. Measure
the pair; do not ship half and quote this table.

### W2a — LANDED 2026-08-30. The cbindgen half was smaller and safer than first written

Issue 0452 already did the hard part. `nros-cbindgen-headers` is **the only
writer** of the committed headers (`just regen-c-headers`, `--check` via
`just check cbindgen-headers`); the build scripts only compare and warn, so no
build dirties the worktree. It is the Rust->C twin of `gen-abi-bindings.sh` +
`check-abi-bindings`. So the 8.1 s buys nothing at build time — it regenerates
files that are already committed and already gated.

cbindgen here scans IN-TREE crates only (`zpico-sys`, `nros-c`, `nros-cpp`), and
the output is message-type INDEPENDENT — verified, not assumed:
`grep -cE 'std_msgs|geometry_msgs|__msg__' zpico.h nros_cpp_ffi.h` returns 0 and
3, and the three are `per_msg_cap` / `max_msgs`, not type names. A user's own
message types never reach cbindgen; they are generated by
`nros_generate_interfaces()` / `nros generate-rust` from `.msg`, a separate
pipeline.

**The skip must be INPUT-AWARE, and this is the acceptance rule.** "Skip
regeneration" must mean *skip when the inputs are unchanged*, never *skip
unconditionally*. An unconditional skip is the museum-binary class this repo
keeps relearning (issues 0475, 0196, 0466): the build goes green against
generated code that no longer matches its source, and nothing points at it.

Two input sets, and they must not be conflated:

* **cbindgen headers** are a function of the in-tree Rust sources of
  `zpico-sys` / `nros-c` / `nros-cpp`. Skipping is safe only if a change to those
  still fails `check-cbindgen-headers`. It does today — that gate is what makes
  the build-time run redundant.
* **Message types are NOT part of that.** A changed `.msg` must still trigger
  codegen, through `nros_generate_interfaces()` and the CONFIGURE_DEPENDS edge
  (#182). Removing cbindgen from the build must not touch that path, and the
  acceptance below tests it explicitly rather than reasoning that they are
  separate.

*Acceptance:* (1) a cold leaf build compiles no `cbindgen`/`clap` unit; (2)
editing an in-tree source that feeds a committed header makes
`check-cbindgen-headers` FAIL; (3) editing a `.msg` in a leaf still regenerates
its message code and relinks the image — asserted by a test, because the whole
risk of this wave is that (3) silently stops happening.

#### What landed, and what each acceptance criterion actually showed

`cbindgen` is now `optional = true` in `nros-build-helpers` and
`nros-zpico-build`, behind a `cbindgen-drift-check` feature that defaults OFF.
`nros-cbindgen-headers` — the single writer — turns it on, so `just
regen-c-headers` and `just check cbindgen-headers` are unchanged. The
`rerun-if-changed` edges on the committed headers were deliberately left
UNCONDITIONAL: those describe the build's own inputs (the C stub includes the
header), and moving them behind the feature would have converted an opt-in
diagnostic into a missing dependency edge, which is issue 0475 one crate over.

**(1) — met, at the compile level, not just the graph level.** A cold `cargo
build -p nros-c` into a fresh target dir:

| | units | `cbindgen`/`clap` units | CPU (user+sys) |
| --- | --- | --- | --- |
| baseline | 113 | 6 | 24.9 s |
| W2a | 61 | 0 | 8.4 s |

The 52-unit drop is cbindgen plus its exclusive transitive stack (`clap` x3,
`anstream`/`anstyle` x4, `rustix`, `tempfile`, `heck`, `strsim`, `getrandom`,
`fastrand`, `toml_parser`, ...).

**Do not read 16.5 CPU-s as the Zephyr-build saving.** That standalone leaf had
nothing else pulling `syn`/`serde`/`toml`, so the whole stack vanished. In the
Zephyr profile those units are CONTESTED — other build-deps need them anyway —
which is exactly why the table above this section attributes only **8.1 s** to
cbindgen-exclusive. The honest claim is: 8.1 s on the profiled Zephyr leaf,
up to ~16 s on a leaf where cbindgen is the sole consumer of its stack.

**(2) — met, and tested rather than argued.** Appending a `#[repr(C)]` struct
and a `#[unsafe(no_mangle)]` fn to `nros-c/src/clock.rs`, with the feature OFF,
made the gate hard-fail:

```
[FAIL] these committed headers are STALE against their crate sources:
         .../nros-c/include/nros/nros_generated.h
```

Reverting restored `check-cbindgen-headers: OK (3 committed headers match)`.
The gate works precisely because the regenerator enables the feature for itself.

**(3) — NOT satisfied, and on inspection it does not apply to this wave.** The
criterion was written for the "committed output" shape, where a build SKIPS a
regeneration step. W2a introduces no skip: since issue 0452 the build already
never regenerated these headers, it only re-rendered them to COMPARE and print
a warning. What was removed is a redundant comparison, not a generation. The
`.msg` path (`nros_generate_interfaces()` / `nros generate-rust` and the #182
CONFIGURE_DEPENDS edge) is untouched — no file in the diff belongs to it.
Carry the criterion to W3, where committing bindgen output DOES create a real
skip and the test has to exist. It is not written yet.

#### The consequence nobody predicted: 18 leaf lockfiles

An optional-but-unactivated dependency drops out of a resolve, so every leaf
lock that carried cbindgen had to move — 18 files, **-5582 / +184 lines**, all
removals except the `[patch.unused]` bookkeeping. Moved with the sanctioned
`just lock-update "" "" <dir>`, never a bare `generate-lockfile`.

Two things came out of that:

* **The NuttX `libc` patch turned "unused".** `packages/boards/
  nros-board-nuttx-qemu/nros-nuttx-ffi/.cargo/config.toml` patches `libc` to the
  NuttX fork for `-Z build-std`; cbindgen's HOST stack was the only thing pulling
  `libc` into the main resolve, so removing it makes cargo warn the patch is
  unused. **This does not change the firmware.** Verified by building the leaf
  both ways to the same (unrelated, pre-existing) `APP_MAIN_CPP not set` failure
  and diffing unit sets: target-side (`armv7a-nuttx-eabihf`) units are
  IDENTICAL; only 20 host units disappear. The warning is a main-graph artifact,
  because build-std resolves separately.
* **The lock refresh also swept up staleness this branch had already caused.**
  Ten of the eighteen still listed `nros-launch-parser`, dropped from
  `nros-macros` by W1 earlier on this same branch without the leaf locks being
  moved. `--locked` tolerated the extra entries, so nothing was red — which is
  why it survived. It is fixed here, but it belonged to W1.

**`check-cbindgen-pin` now reads 1 tracked lock, down from 19**, and its header
comment says why. That is not lost coverage: the hazard it watched — a lockless
leaf resolving the caret to a different patch release — no longer exists,
because no leaf resolves cbindgen at all. Arms 1 and 2 and the vacuity guard are
unchanged.

*Lanes run:* `just check fast` (135/135 gates OK), `cargo clippy` on both
feature states, `cargo +nightly fmt --check`, `check-cbindgen-pin`,
`check-cbindgen-headers`, `check-leaf-lockfiles`, and a `--locked`
`cargo metadata` on all 18 updated leaves. NOT run: `ci-l1`, any fixture build,
any cross/QEMU lane.

### W3 — RETRACTED 2026-08-30. The 20.6 s of bindgen is not ours, and the crates named were not in the build.

The plan was: commit the output of the four `*-sys` driver crates
(`zephyr-posix-sys`, `nuttx-sys`, `freertos-lwip-sys`, `threadx-netx-sys`), the
way RFC-0054 already commits bindgen output for the ABI crates. **Three measured
facts kill it, and any one of them is sufficient.**

**1. The four crates are `exclude`d from the workspace.** They sit under
`exclude = [` in the root `Cargo.toml` (line 173), not `members = [` (which ends
at line 170) — deliberately, because each needs an external SDK env var
(`ZEPHYR_BUILD_DIR`, `FREERTOS_DIR`, `NUTTX_DIR`, `THREADX_DIR`).
`cargo metadata --no-deps` confirms: not members. So `cargo build --workspace`
and `just check` never build them, and they cost **zero** in any measured build.

**2. Nothing depends on them.** Repo-wide, across every tracked manifest, the only
mentions are their own `Cargo.toml`s and the exclude list. Zero consumers. They
are dead weight in a directory, not a lever.

**3. The leaf's bindgen belongs to upstream Zephyr.** The profiled leaf
(`examples/zephyr/rust/talker`) deps `zephyr = "0.1.0"`, which reaches
`zephyr-sys` in the west-managed module tree — and that is where `bindgen 0.72.1`
(with `experimental`) comes from. `zephyr-workspace/` is gitignored and
west-managed: not our code.

And its output is **not committable in principle**, independently of ownership.
`zephyr-sys/build.rs` reads `DOTCONFIG`, `INCLUDE_DIRS` and `INCLUDE_DEFINES` from
the specific Zephyr build tree — the bindings are a function of the IMAGE's
Kconfig, so two images with different `prj.conf` need different bindings. There is
no single committed artifact to produce. This is the opposite of RFC-0054's case,
where the headers are in-tree and fixed.

**What the 20.6 s actually is:** the cost of COMPILING `bindgen` and its
`clang-sys` stack as a build-dependency of an upstream crate, in a cold
single-leaf build. Not the cost of running it, and not something a committed
artifact removes.

*If anything is to be done here it is caching, not committing* — the compile is
content-addressable, and phase-340's shared cargo group already amortises host
build-deps across leaves sharing a target dir, so the 20.6 s should be a
first-leaf cost rather than a per-leaf one. **Not re-measured**; whoever picks
this up should measure the SECOND leaf in a group before assuming either way.

*Separate, small, and real:* the four dead `*-sys` crates should be deleted or
given a consumer. They are excluded and unreferenced, so removing them saves no
build time — but a directory of crates nobody builds and nobody uses is a false
statement about what the project needs, which is the same reason W1 was worth
doing.

### W5 — the 118.3 s is PER-LEAF on Zephyr, because Zephyr is the one platform with no shared cargo group

The W3 retraction said the second-leaf cost was NOT re-measured and should be
checked before assuming the shared cargo group absorbs it. It was checked. It does
not — on Zephyr, which is the platform every number in this phase came from.

**Six platforms pass a shared cargo root. Zephyr passes none.**

```
NROS_SHARED_CARGO_ROOT / nros_fixture_target_dir_flag present in:
    freertos  native  nuttx  qemu-baremetal  threadx-linux  threadx-riscv64
zephyr.just, zephyr-ci.just:  0 occurrences
```

Instead, every west build dir carries its own cargo target dir: **191 of them
across 89 `zephyr-workspace/build-*` trees.**

**Measured with `just leaf-graph` over 12 of the 89:**

```
  build-c-action-client-cyclonedds     host= 86  target= 24
  build-c-action-client-xrce           host= 95  target= 25
  build-c-action-client-zenoh          host= 93  target= 32
  ...
  host crates: union=102, present in EVERY sampled build = 86
```

**86 host crates are recompiled from scratch in each build dir**, and the host
side outnumbers the target side roughly 3:1. That is what the 118.3 CPU-s profile
was measuring: not a first-leaf cost amortised across a sweep, but a cost paid
again per image. Host tooling was ~75 of those 118.3 s.

**Issue 0616 does NOT block the larger half — checked, not assumed.** A cargo
`--target-dir` serves exactly one workspace root, because `-C metadata` includes
the path SPELLING a crate was reached by. The 71 C/C++ Zephyr builds reach
`nros-c`/`nros-cpp` through the SAME spelling — identical fingerprint `path` hash
`7238329675919068069` in `build-c-action-client-zenoh` and
`build-cpp-listener-zenoh` — so a shared root would legitimately reuse units,
exactly as it does for nuttx and threadx today.

The 18 `build-rust-*` leaves are the case 0616 is about: each is its own workspace
root, so they need the per-leaf treatment rather than one shared dir. Do not
collapse those without re-reading 0616 — `nros-platform` holds the tree's one
`#[global_allocator]`, and two copies of it is the intermittent failure that issue
describes.

**CORRECTION 2026-08-30 — the first attempt at this wiring was a NO-OP, and was
reverted.** Passing `-DNROS_SHARED_CARGO_ROOT` through west does nothing on
Zephyr: `NROS_SHARED_CARGO_ROOT` is read by `nros_share_corrosion_cargo_dir` on
the CORROSION path, and Zephyr does not use Corrosion. `zephyr/CMakeLists.txt`
says so at line 197 — "`nros_c_cargo` — not a Corrosion target". Proven rather
than reasoned: after a full pristine C++ build with the wiring in place,
`build/corrosion-cargo/zephyr/` did not exist.

The lever is unchanged and still real (86 host crates recompiled per build dir);
only the mechanism was wrong. Zephyr needs the OTHER consumer of the shared-dir
helper — `nros_shared_cargo_dir()`, which the NuttX FFI driver already uses
because it too hand-rolls its cargo invocation and sets `CARGO_TARGET_DIR`
itself.

**But that is not a wiring change either, and this is why Zephyr was left out.**
Two blockers, both of them failure classes this repo has already paid for. Neither
is a reason to abandon the lever; both have to be designed for BEFORE any code.

**1. The generated headers live INSIDE the cargo target dir.**
`nros_cargo_build.cmake` writes `${CARGO_TARGET_DIR}/nros-c-generated/nros/
nros_config_generated.h` (and the cpp sibling), while `zephyr/CMakeLists.txt`
hardcodes the CONSUMER side as `${CMAKE_BINARY_DIR}/nros-rust/nros-c-generated`
(lines 272, 395-396). Redirect `CARGO_TARGET_DIR` without moving those and the
include path points at a directory the build no longer populates — which is
issue 0834's shape exactly: a mirror that no re-run repairs, and whose only
recorded escape is `rm -rf` on the west build dir. So the header location has to
be decoupled from the shared cargo dir first, or moved with it.

**2. The sharing KEY would have to include Kconfig, and today it does not.**
Those headers are a function of the IMAGE's Kconfig — `nros-zephyr-build` reads
`DOTCONFIG` and resolves `CONFIG_NROS_*` knobs (issue 0460). The Corrosion key is
`platform/rmw/board/caps/profile/target`; two Zephyr images agreeing on all six
but differing in `prj.conf` would share one directory and overwrite each other's
sizes header. That is the 0135/0460 ABI split — silent, and it is the class
CLAUDE.md already records for the cmake/cargo lanes disagreeing on
`MAX_QUERYABLES`. **Corrected below — for the SIZING knobs it is loud, not
silent: the issue-0360 stamp guard panics, and its stamp is keyed on them.**

*What would make this landable (superseded by "The roadmap" below, which sizes it
and names the two existing implementations to reuse):* extend the key with the resolved Kconfig knob set
(the same values `nros_zephyr_build::knob_usize` reads, so the key is a function
of what the header is a function of), and move the generated-header directory out
of `CARGO_TARGET_DIR` — or key its path the same way. Then measure two images
with `--timings` before quoting a saving.

*Status: NOT attempted.* The lane it needs was only unblocked by #0918 in the same
session, and the two blockers above were found by reading rather than by building.
Neither has been tested.

#### The roadmap — sized 2026-08-30. Both blockers have an in-tree precedent.

The two blockers above are real, but neither is novel: this repo has already
solved each of them once, in a neighbouring lane, and the solutions are tested.
W5 is assembling those two, not inventing anything.

**The KEY already exists, and it was written for exactly this failure.**
`probe_key()` in `nros-sizes-build` hashes rustc slug + target + SORTED features
+ `knob_identity()`, where `knob_identity()` is every `NROS_*` environment
variable PLUS every `CONFIG_NROS_*` line of `$DOTCONFIG`. Issue 0528 is the
reason the knobs are in there: two Zephyr leaves at the same (target, features)
disagreeing on `CONFIG_NROS_EXECUTOR_MAX_CBS` shared a probe dir, whichever
probed first wrote the sizes, and the 16-CBS leaf then compiled against a
constant sized for 4 and died on `EXECUTOR_OPAQUE_U64S too small`. It is pinned
by `zephyr_dotconfig_sizing_knob_splits_the_probe_key`. So "extend the key with
the resolved Kconfig knob set" is not a design to be invented — it is a function
to be reused.

**And a repo-root cargo dir shared across west build trees is already
load-bearing.** `build/sizes-probe/<rustc-slug>/<key>` has been the DEFAULT since
phase-343 I1, not an opt-in: 425 leaked private probe dirs, 63.1 GiB,
deduplicating 81:1. Every Zephyr west build already writes into it. What is
missing is not the mechanism or the key — it is that the MAIN cargo build never
got the same treatment.

**Sizing: measured across the 89 build trees, not estimated.** The open worry was
that putting Kconfig in the key would over-partition and give back the saving. It
does not.

```
zephyr-workspace/build-*                     89 trees (70 C/C++, 19 rust)
distinct NROS_RESOLVED_* knob sets            8
distinct FULL key (knobs + every --features
  line in build.ninja + triple), C/C++       14   sizes 12,10,10,10,6,6,5,3,2,2,1,1,1,1
```

**70 C/C++ build dirs collapse to 14** — 56 of them stop recompiling the ~86 host
crates and start reusing them. The conservative key (features included, though
cargo hashes those itself) costs only 14 groups against the 8 the knobs alone
would give, so there is no reason to narrow it.

The 19 rust trees are out of scope and must stay per-leaf: each is its own cargo
workspace root, which is what issue 0616 is about.

**Blocker 1 is one redirect site, not a sweep.** Consumers hardcode
`${CMAKE_BINARY_DIR}/nros-rust/nros-{c,cpp}-generated` in six files; that path is
correct and must not move. Only the WRITER's notion of the root has to change,
and it reads `CARGO_TARGET_DIR` in exactly two functions of
`nros-build-helpers/src/shared.rs` (`write_header_to_target_dir`,
`target_dir_path`). The same file already has the pattern for a per-leaf
destination given by the environment: `write_header_to_corrosion` writes to
`$CORROSION_BUILD_DIR`. So the change is one `generated_header_root()` helper
preferring a new variable, with Zephyr setting it to the path consumers already
use — which keeps every include path byte-identical. That is what keeps this out
of issue 0834: nothing moves on the consumer side at all.

**The other unhashed output is NuttX's solved problem.** `LIB_PATH` is an
unhashed uplift (`libnros_c.a`) exactly like `nros-nuttx-ffi`, and
`nros-nuttx.cmake` already evicts it from the shared dir with cargo's own
`--artifact-dir` plus an explicit depfile copy, for the stated reason that a
shared depfile hands every other leaf the wrong rebuild triggers. Zephyr needs
the same two lines.

**Correction to blocker 2's framing: the failure is LOUD, not silent — for the
sizing knobs.** `write_header_if_absent_or_verify` (issue 0360) `panic!`s when
two builds disagree about a generated header, and its stamp derives from the
sizes probe, which is keyed on `knob_identity()`. A `MAX_CBS` divergence
therefore aborts the build rather than corrupting an image. This is NOT
established for the `ZPICO_*` transport knobs, which reach the C shim's
`-D` defines rather than `EXECUTOR_SIZE`; that gap is what acceptance 3 below
has to close. The earlier "silent 0135/0460 split" reading was pessimistic and
is corrected here.

##### Stages

**W5.a — evict the unhashed outputs. No sharing yet.** Add
`generated_header_root()` (new var, falling back to `CARGO_TARGET_DIR`, then
`cargo_target_dir()`); have Zephyr set it to `${CMAKE_BINARY_DIR}/nros-rust`;
move `LIB_PATH` to `--artifact-dir` as `nros-nuttx.cmake` does. *Acceptance: with
sharing OFF, a from-pristine C and C++ leaf produce the same files at the same
paths as today.* This stage is a deliberate no-op on layout, which is exactly why
it can be verified before any sharing exists to confound it.

**W5.b — the key, with ONE implementation.** cmake already has the material:
`NROS_RESOLVED_KNOBS` plus `NROS_RESOLVED_<knob>`, cached at configure. Pass those
into `nros_shared_cargo_dir(KEY ...)`. But note the asymmetry that makes a second
implementation dangerous: `knob_identity()` reads `$DOTCONFIG` DIRECTLY, so it
sees `CONFIG_NROS_*` knobs cmake never resolved, and a cmake-side key derived
only from `NROS_RESOLVED_KNOBS` is therefore COARSER than the probe's. Either
derive the cmake key from the same `.config` scan, or gate the two views against
each other. Two spellings of a sizing key is issue 0135 with a different noun.

**W5.c — wire and measure.** `nros_build_dir "$NROS_KIND_CORROSION_CARGO" zephyr`
into `zephyr.just` / `zephyr-ci.just`, consumed through `nros_shared_cargo_dir()`
— NOT `nros_share_corrosion_cargo_dir()`, which this section already proved is a
no-op on a lane that uses no Corrosion.

##### Acceptance

1. Two images from the SAME cluster: the second build compiles zero host units of
   the shared set (`just leaf-graph`, W4 — use it rather than reasoning).
2. Two images from DIFFERENT clusters, differing in
   `CONFIG_NROS_EXECUTOR_MAX_CBS`, build correctly in EITHER order and survive
   repeated alternation. This is issue 0528's reproduction lifted from the probe
   dir to the main build dir, and it belongs in the tree as a test.
3. A `ZPICO_*`-only difference either splits the key or is shown to reach no
   artifact in the shared dir. Do not assume the 0360 stamp covers it — it is
   derived from the sizes probe, and these knobs are not.
4. From pristine, two images leave `<build>/nros-rust/nros-{c,cpp}-generated/nros/`
   populated with no `.stamp` lacking its `.h` — the 0834 survey, run as a check
   rather than trusted.

##### Stop conditions

* **Any `rm -rf` needed to converge means the design is wrong**, not that the tree
  is dirty. That is 0834's signature and the reason it has an exemption instead of
  a fix.
* **Thrash.** Alternating between two cluster members rebuilds whatever is
  downstream of the knobs. Measure the order the SWEEP actually builds in, not a
  synthetic A/B pair — if the sweep interleaves clusters, sharing can cost more
  than it saves, and that is a result worth having rather than a reason to stop.

**No saving may be quoted before W5.c.** This phase has retracted three estimates
for skipping the measurement, and the sizing above is a count of build
directories, not of seconds.

*Original next step, still the sizing that must come first:* wire the C/C++ Zephyr lane to
`nros_build_dir "$NROS_KIND_CORROSION_CARGO" zephyr`, the same call the six other
platforms already make, then re-measure a two-image build with `--timings` before
quoting a saving. **This phase has retracted three estimates for skipping exactly
that step.**

### W5.a — LANDED 2026-08-30. The generated-header location is one decision again.

Seven consumers spelled the header directory as the literal
`${CMAKE_BINARY_DIR}/nros-rust` — three `zephyr_include_directories`, four
`OBJECT_DEPENDS` file edges — while `nros_cargo_build()` decided it independently
in another file. They agreed only by coincidence, which is what made blocker 1
dangerous: move the cargo dir and the include path keeps pointing at a directory
nothing populates (issue 0834's shape, whose only recorded escape is `rm -rf`).

`nros_resolve_cargo_dirs()` now resolves it ONCE and caches it, on the same
terms and for the same reason as `nros_resolve_knobs()` beside it, and consumers
ask through `_nros_generated_header_dir()` (cmake/NanoRosCodegenCore.cmake, next
to `_nros_is_zephyr` — the same "promote the second idiom to the single
definition" that issue 0282 needed). The fallback keeps every non-Zephyr caller
on the path it had.

**It resolves TWO directories, not one, and that is the substantive part.**

```
NROS_GENERATED_HEADER_DIR    per-image, ALWAYS — content is a function of this
                             image's Kconfig and of nros-{c,cpp}'s features
NROS_ROOT_CARGO_TARGET_DIR   shareable — the ~86 host crates W5 is about
```

Equal today, so this changes nothing; separate so that W5.c can move one without
moving the other. Collapsing them would have let a wiring change decide a design
it never argued for — and the first draft of W5.a did exactly that before the
fork below was noticed.

*Verified, not asserted:* `build-c-listener-zenoh` and `build-cpp-listener-zenoh`
both build to `zephyr.elf`; both caches show the new variables resolving to the
old literal; the `-I...nros-{c,cpp}-generated` flags in `build.ninja` are
unchanged. `check-fast` 138/138.

### W5 blocker 3 — `DOTCONFIG` (RETRACTED — read the retraction below before acting on this)

Found by reading the fingerprints of a real build tree rather than the sources.
`scripts/check-path-env-fingerprints.py` (issue 0491) forbids fingerprinting a
PATH-valued env var as a STRING, and exempts `DOTCONFIG` with this reason:

> "per-zephyr-build-dir; zephyr leaves share no cargo group"

**That is precisely the invariant W5 removes.** The same file records what
happened last time an exemption of that shape went stale: `CORROSION_BUILD_DIR`
held one on the premise that every cmake build dir owns its own cargo target dir,
issue 0805 made leaves SHARE, and while the exemption stood every leaf
invalidated the previous leaf's build script — 459 s of cargo on one platform's
warm rebuild, against 6.7 s once fixed. W5 is the same change one lane over.

It is not hypothetical here. In `build-c-listener-zenoh`,
`rerun-if-env-changed=DOTCONFIG` appears in the build-script output of `nros`,
`nros-node`, `nros-params` and `nros-rmw-cffi`. Its value is a per-tree path, so
in a shared dir those scripts re-run — and their dependents rebuild — on every
alternation between images.

**Exporting more knobs does not close it.** All 26 `NROS_RESOLVED_KNOBS` already
reach the C lane's cargo command, and `knob_usize` returns before touching
`$DOTCONFIG` when the env carries the value. These crates fall through anyway,
for knobs cmake never resolves — `NROS_EXECUTOR_ARENA_SIZE`, `NROS_MAX_ARRAY_LEN`,
`NROS_RMW_MAX_NODES`, the `NROS_RUNTIME_*` family. Chasing that list is
whack-a-mole with no gate behind it.

**Dropping the directive is worse, and this is the trap.** The obvious reading of
0491's doctrine — "watch the CONTENT" — is already half-done: `dotconfig_usize`
emits `rerun-if-changed=<path>` too. But that path is recorded from the run that
happened. Build tree A first, then tree B against the same shared dir, and cargo
checks *A's* `.config`, finds it unchanged, declares the script fresh, and B
compiles A's values. Silent, and in the direction that ships. The env fingerprint
is load-bearing for CORRECTNESS; it cannot simply be deleted to stop the churn.

*The fix that satisfies both:* fingerprint the content under a SPELLING-INDEPENDENT
name. cmake computes a digest of the `CONFIG_NROS_*` lines and passes
`NROS_KCONFIG_DIGEST`; `dotconfig_usize` fingerprints that instead of `DOTCONFIG`,
and keeps `rerun-if-changed=<path>` so editing `.config` in a single tree still
re-runs. Equal for every tree in a cluster, different across clusters — which is
0491's own rule ("what a build script depends on is the CONTENT"), applied to a
variable whose content happens to live in a file.

### W5 blocker 3 — RETRACTED 2026-08-30. `DOTCONFIG` is not fingerprinted on the lane that shares.

The section above filed `DOTCONFIG` as a blocker on the reading that Zephyr build
scripts fingerprint it and its value is a per-tree path. **Measured, it is not a
blocker, and the reasoning was the same mistake this phase has now made four
times: computed from the source, not from a build.**

```
DOTCONFIG in run-build-script fingerprints, all zephyr build trees:
  C/C++ lane (nros-rust/)   654 records, 41 trees   ALL <unset>
  rust  lane (rust/)        526 records, 18 trees   ALL set
```

The split is issue 0460's design working. The C lane bakes all 26 knobs into its
`cmake -E env` command, so `knob_usize` returns at the env check and the
`$DOTCONFIG` fallback is never reached — hence `val: null` in every record. The
Rust lane cannot get env from cmake (zephyr-lang-rust builds its own command), so
it reads the file. **W5 shares the C/C++ lane only** — the Rust leaves are
separate workspace roots and share nothing (issue 0616). On the lane that shares,
the value is a constant, so it contributes no churn.

**Generalised, because "is `DOTCONFIG` a problem" was the wrong question.** The
question is whether the SAME unit records a different value in two trees that
would share a directory:

```
clusters examined 8   shared units 99
ENV divergence: 0
```

Zero, in every cluster. So the env half of the fingerprint namespace is already
consistent, which is the precondition for sharing rather than an obstacle to it.

Three fixes were designed for this non-problem and are all dropped: a
`NROS_KCONFIG_DIGEST` content digest, a per-cluster `.config` projection, and a
sweep to export every knob cmake does not currently resolve.

**What survives is the file half, and it is the more serious one.** 15 units
record a DIFFERENT watched-path set across trees of one cluster. Examined:
`nros-c-344671de436426d7` records 132 paths in nine trees and 21 in a tenth, the
21 a strict SUBSET, all ten built within seven minutes — so not artifact age,
which was checked before this was written down.

That matters because cargo decides freshness from the RECORDED list; it cannot
know the new one without running the script. A shared dir holding the 21-path
record leaves the other 111 in-repo sources unwatched for every member of the
cluster. Sharing does not create the under-watch — that tree is already
under-watched today — it promotes one tree's defect to twelve.

*Not diagnosed.* Why the script emits two different lists is unknown, and it is a
lead with a reproduction rather than a root cause.

**Upstream has nothing coming.** Cargo's `-Zchecksum-freshness` replaces mtime
with content hashing but explicitly excludes build-script inputs — "Files
ingested by build scripts will continue to use mtimes, even when
checksum-freshness is enabled" (rust-lang/cargo#14136). So the general hazard is
not waiting on a cargo release, and nobody should plan around one.

**Two follow-ups this leaves:**

* `scripts/check-path-env-fingerprints.py` exempts `DOTCONFIG` with the reason
  "per-zephyr-build-dir; zephyr leaves share no cargo group". Under W5 the
  CONCLUSION stays right and the REASON goes stale. That file requires an
  exemption to state the invariant it rests on — precisely because
  `CORROSION_BUILD_DIR` held one whose premise issue 0805 falsified — so it needs
  rewriting to the true invariant: uniformly unset on the lane that shares, set
  only on the lane that does not. That also arms the tripwire, because anyone who
  later passes `DOTCONFIG` on the C lane converts it into churn.
* W5.c acceptance gains an item: watched-path sets must be stable per unit across
  a cluster before that cluster is collapsed.

### W5 tooling — `just shared-dir-churn`, so the acceptance item is runnable

`scripts/nros-shared-dir-churn.py` reads the build-script fingerprints that
builds ALREADY WROTE and reports, for units common to two or more trees, both
divergences: env value (churn) and watched-path set (correctness). It is the W4
move applied to W5 — ask the build, not the source — and it exists because the
blocker it was written to investigate turned out not to be there.

`--self-test` encodes four cases, including the one that would make the tool cry
wolf on every build: two DIFFERENT units inside ONE tree recording different env
values is NORMAL (feature variants) and must never be reported. Only the same
unit, across trees, counts.

On four real cyclonedds trees it reports what the manual measurement found:
0 env divergences, 1 path divergence, `[21, 132] paths, smallest is a SUBSET`.



### W5.b — LANDED 2026-08-30. The key is ready; the LANE is refused, loudly.

Two mechanical pieces, and one result that was worth more than either.

**`nros_shared_cargo_dir()` moved to `cmake/NanoRosSharedCargoDir.cmake`,
unchanged.** It lived in `NanoRosCorrosion.cmake` because its first two consumers
did. The Zephyr C/C++ lane is the third and uses no Corrosion — that module is
never included there — so the helper was simply unreachable from the platform
with 89 unshared cargo directories. Including the Corrosion module to reach it
would drag Corrosion provisioning into a lane with no use for it, and a second
normalise-and-hash is what the helper's own doc block forbids.

**The key is computed by `_nros_root_cargo_dir()`, per PACKAGE.** Not alongside
the header dir, and not cached once: its key contains this package's features,
and `nros_cargo_build()` is called once per package. The first draft did cache it
once and had to name a variable that does not exist — caught by writing it out,
not by building.

    triple, profile   the artifact's target and optimisation
    features          keys nros-c and nros-cpp apart; their archives differ
    knobs             every NROS_RESOLVED_*, because they reach compiled code
                      through the build-script environment (issue 0528)

Features are in the key rather than left to cargo's hashing BECAUSE of the
uplift: with them in, the eleven unhashed artifacts that collide are
byte-identical by construction, so the collision is harmless. That is how this
lane avoids `--artifact-dir`, which needs `-Z unstable-options` — and this lane
forces nightly only for `armv7a|thumbv|riscv32`, so native_sim is on STABLE and
the NuttX eviction mechanism is unavailable here. Checked, not assumed.

The 0616 duplicate-root guard moved with it. It hashes `CARGO_TARGET_DIR`, and
deferring the resolution past it left it hashing an EMPTY string — registering
ownership under an empty key and silently protecting nothing.

#### The header blocker is now DEMONSTRATED, not predicted — and by accident

Enabling sharing on this lane produces a broken build. That was the prediction;
this is the observation, and it arrived unplanned. An earlier reverted experiment
had left `-DNROS_SHARED_CARGO_ROOT` in the CMake cache of ONE build dir. It was
inert while Zephyr never read the variable. W5.b makes Zephyr read it, so a stale
cache entry silently switched sharing ON for that dir, and the C++ leaf failed:

```
ninja: error: 'nros-rust/nros-c-generated/nros/nros_generated.h',
needed by 'CMakeFiles/listener_lib.dir/src/Listener.cpp.obj',
missing and no known rule to make it
```

Exactly the predicted mechanism: the build script writes the headers under
`$CARGO_TARGET_DIR`, so a shared dir means image B takes a cargo cache hit, the
script never re-runs, and B's per-image header dir — where W5.a correctly points
the consumers — is never populated.

Worth naming the second lesson: **W5.b converts a previously inert stale cache
value into an active behaviour change.** A `-D` that no code reads is not a
no-op forever; it is a latent input waiting for a reader.

*So the opt-in is REFUSED with a FATAL_ERROR* naming the failure and the escape
hatch (`-DNROS_ZEPHYR_SHARED_CARGO_UNSAFE_OK=ON` for whoever develops W5.c).
A fatal error rather than a silent fall back to per-image dirs: a caller who
passed the flag asked for sharing, and quietly not sharing is how a measurement
gets attributed to a build that never shared anything.

*Verified:* C and C++ Zephyr leaves both build to `zephyr.elf`; `check fast`
139/139; the stale cache entry cleared with `cmake -U` and a re-configure, never
`rm -rf` (the build converged, so the antipattern rule applies).

*Status: W5.c NOT attempted.* One problem remains and it is the whole of it —
getting the generated headers to each image while the dependency mass is shared.
Three candidate routes, none free, all recorded above and in the design-fork
section.

### W5 open items — closed 2026-08-30, and the path-divergence finding is RETRACTED

**1. The `DOTCONFIG` exemption reason — rewritten.**
`scripts/check-path-env-fingerprints.py` exempted it as "per-zephyr-build-dir;
zephyr leaves share no cargo group". The second clause is what W5 sets out to
falsify, so the entry was a queued repeat of the `CORROSION_BUILD_DIR` story two
rows above it. Replaced with the invariant that was actually measured and that
survives W5: unset on the C/C++ lane (the one that shares), set only on the Rust
lane (which does not, issue 0616). Stated as a TRIPWIRE — forwarding `DOTCONFIG`
on the C lane to close a knob gap would make it a per-build-dir path inside one
shared namespace, which is what the gate exists to prevent.

**2. The 132-vs-21 path divergence — RETRACTED. It was contaminated evidence.**

The finding was that 15 units record different watched-path sets across trees of
one cluster, `nros-c-344671de436426d7` at 132 paths in nine trees and 21 in a
tenth. Chasing it produced one real defect and one retraction, in that order.

*What is real:* `emit_probe_watches` looked for the depfile ONLY at
`rlib.with_extension("d")`, and cargo does not put it there for a hashed `deps/`
artifact. Measured in this repo's shared probe store: **182 uplifted rlibs, 182
depfiles, 269 `deps/` rlibs with none.** Cargo's `compiler-artifact` event can
name either spelling. When it named the `deps/` one the lookup missed — and the
function then did `let Ok(..) = read else { return }`, emitting ZERO watches.
Silently. That is the defect issue 0563 filed and this function was written to
fix, reintroduced by its own error handling, and the doc comment above it still
claimed "every source that went into the measurement is watched".

Fixed: resolve both spellings (`probe_depfile`), and PANIC rather than return
when neither exists — the contract cannot be met, and the consequence of pretending
otherwise surfaces two crates away as `EXECUTOR_OPAQUE_U64S too small`. Pinned by
a unit test covering both layouts and the absent case.

*What is retracted:* the claim that this explains the 21-path record. **It was
never reproduced.** Two rebuilds of the tree holding it — one after touching
`nros-c/build.rs` to force the script — left the record byte-identical at its
08-16 timestamp, because that unit is not in the tree's current configuration and
nothing rebuilds it. A `.fingerprint/` directory ACCUMULATES: records were found
spanning 08-15 to 08-30 in one tree. Comparing trees built two weeks apart
compares the repo's history, not a property of sharing.

Re-measured on trees built from the same source state: **40 shared units, 0 env
divergences, 0 path divergences.** The same comparison across build eras reports
the old findings and now says why.

So the depfile fix stands on its own evidence — the measured layout, the code
path, and the test — and NOT on the symptom that led to it. Whether any live
build reaches the `deps/` spelling is still unverified; if none does, the fix is
hardening plus the loud failure, which is the part worth having either way.

**3. `just shared-dir-churn` had two defects of its own, both found by using it.**

* It compared STALE ORPHAN units. First attempted fix — filter by
  `invoked.timestamp` — was WRONG and measurement said so: that file marks build
  scripts that RE-RAN, not units that participated (9 of 64 in a freshly built
  tree, **zero overlap** with the units holding records). A live-but-fresh unit is
  indistinguishable from an orphan by mtime, so any timestamp rule either drops
  live units or keeps orphans. The tool now does not guess: it reports the age
  spread and refuses to certify a comparison whose trees were built more than six
  hours apart.
* It printed **OK on zero compared units** — a vacuous pass, the shape
  `check-no-vacuous-tests` exists to forbid, in a tool whose entire output is a
  safety claim. Comparing trees with no unit in common now exits INCONCLUSIVE,
  and the self-test pins it.

*The methodology this settles, and it is the actual deliverable:* **build the
cluster, then measure it.** A fingerprint directory is a historical record, not a
statement about the present, and this phase has now spent two separate
investigations on that distinction — issues 0859-0862 first, this second.

### W5.c — use cargo's OUT_DIR, not a side channel in its target dir

The three routes weighed earlier (fingerprint the destination / copy from the
shared dir / include the shared dir) all preserve the same premise: that the
headers live at `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/`, a path INSIDE
cargo's tree that cargo does not manage. It works because nothing cleans it.

That premise is the blocker. Share the target dir and image B takes a cache hit,
the build script never re-runs, and the directory the consumers were pointed at
is never written. Every route above is a way to route AROUND that; none removes
it.

`$OUT_DIR` removes it. It is where cargo says build-script output goes, it is
per-unit and **hashed by cargo** — so two feature sets cannot collide without us
keying anything — and its path is reported on the STABLE JSON stream:

    {"reason":"build-script-executed", …, "out_dir": "…/build/nros-c-<hash>/out"}

Both properties were measured rather than assumed:

* cargo emits `build-script-executed` with `out_dir` on a FULLY CACHED run —
  13 events with nothing to rebuild. That is exactly the case the side channel
  cannot serve.
* the same crate under different features lands in different `OUT_DIR`s
  (`nros-c-f03b86696704b69c` for default, `nros-c-ff8a58bc63dfe8d4` for
  `rmw-cffi,platform-posix,std,ros-humble`), with the header present at
  `nros/nros_config_generated.h`.

*Landed so far:* the headers are emitted to `$OUT_DIR` as well as the side
channel (`write_header_to_out_dir`). Additive, so nothing changes for existing
consumers, and it establishes the supported location.

*The cmake half — LANDED.* `scripts/build/cargo-out-dir-headers.py` RUNS cargo
(rather than being piped from it: `add_custom_target(COMMAND …)` has no shell, so
there is no pipe to hang a filter on), reads `out_dir` off the JSON stream, and
copies the generated tree into the per-image dir the consumers already include.
One process tree, one exit code, stderr untouched —
`--message-format=json-render-diagnostics` keeps diagnostics human on stderr and
leaves stdout pure JSON.

The BYPRODUCTS moved with it: they now name `${NROS_GENERATED_HEADER_DIR}/…`
instead of `${CARGO_TARGET_DIR}/…`. That is the whole point — under a shared
target dir the old spelling named a file this image would never write.

**A bug caught by design review rather than by building:** `nros-cpp`'s build
script emits BOTH its own header and the c-format companion, so a per-package
destination would have filed `nros_config_generated.h` under
`nros-cpp-generated/`. The fix is that both sides keep the full relative path —
`write_header_to_out_dir` preserves the `nros-{c,cpp}-generated/` segment and the
placer copies the tree verbatim — so each header lands where its consumers look
whichever package produced it. Flattening would have let include ORDER pick
between two different headers, which is issue 0360.

*Verified:* `build-one c/listener zenoh` and `cpp/listener zenoh` both build to
`zephyr.elf`, both report the placement, and the header is freshly written into
the per-image dir. `just check fast` 145/145. Self-test covers six cases
including the c-vs-cpp separation and the unchanged-header mtime.

*Still to do before the side channel can be deleted:* the target-dir write in
`write_header_to_target_dir` is still there, and non-Zephyr lanes (Corrosion,
NuttX) still read it. Removing it means giving those lanes the same placer, which
is a separate change with its own verification.

**One hypothesis tested and REFUTED before scoping:** that
`compiler-artifact.filenames` reports a hashed `deps/` path for the staticlib,
which would have let cmake link an unhashed-collision-free artifact, dropped
features from the key and collapsed 70 dirs to 14 instead of 28. It does not —
for `crate_types: [staticlib, cdylib, lib]` cargo reports only the uplifted
`libnros_c.{a,so,rlib}`. Features stay in the key.

**Exposure register: issue 0945.** The campaign depends on five things nobody has
promised to keep working — the Corrosion path formula the 0805 symlink redirects,
`--artifact-dir`'s unstable flag, cargo's private `.fingerprint` format that
`just leaf-graph` / `just shared-dir-churn` parse, this side channel, and the
undocumented depfile location. Read it before extending any of them.

### W5 design fork — D2, resolved by the per-PACKAGE call

Two coherent designs exist and only one survives contact with `nros_cargo_build()`.

**D1 — the headers follow the shared dir.** Then the key must contain everything
the headers depend on, features included. It fails: `nros_cargo_build()` is called
per PACKAGE, and `nros-c` and `nros-cpp` are built with different feature sets, so
a features-keyed dir gives them DIFFERENT directories — while `nros-cpp`'s build
script also writes `nros-c`'s headers (deliberately, for CPP-only images). The
include path then has no single answer.

**D2 — the headers stay per-image; the dependency mass is shared.** Key on
(triple, profile, knobs); evict the per-image outputs. Feature variants then
coexist safely because cargo hashes features into `deps/` — which is already
observable: `build-c-listener-zenoh` holds NINE `nros-c` build-script units side
by side today, in one directory, without incident.

**Only the UNHASHED outputs collide — and there are ELEVEN, not the two first
written here.** Enumerated from a real tree
(`build-c-listener-zenoh/nros-rust/x86_64-unknown-linux-gnu/nros-relwithdebinfo/`)
rather than recalled:

    libnros_c.a  libnros_c.rlib  libnros_c.so  libnros_c.d
    libnros_cpp.a  libnros_cpp.rlib  libnros_cpp.d
    libnros_rmw_zenoh_staticlib.a  libnros_rmw_zenoh_staticlib.d
    nros-c-generated/   nros-cpp-generated/

THREE packages uplift, not one, and each uplifts several shapes.
`.cargo-lock`, `.cargo-build-lock` and `.cargo-artifact-lock` sit beside them and
are cargo's own, not ours to separate. Everything else in the directory
(`deps/`, `build/`, `incremental/`, `.fingerprint/`, `.rustc_info.json`) carries
a cargo hash and is identical-or-distinct by construction.

**The rule, stated so the design can be checked against it: share only what is
provably identical across the builds sharing it; separate everything else.** An
eviction list assembled by recall is how one of the eleven gets missed, and a
missed one is the wrong artifact linked into an image — which then gets "fixed"
with `rm -rf`, destroying the evidence and teaching the next person that the tree
is untrustworthy. The enumeration is the deliverable, not the list of things
someone happened to notice.

Note precisely what `--artifact-dir` buys and what it does not: cargo still
uplifts into the profile dir, so the contended copy remains there — what changes
is that the CONSUMER reads a per-image artifact dir, so nothing reads the
contended one. `nros-nuttx.cmake` also copies the depfile explicitly, because
that is per-image too and `--artifact-dir` does not carry it.

D2 is the NuttX pattern with one extra eviction. That is the design W5.c should
build.

*One more thing W5.c has to do first:* `nros_shared_cargo_dir()` is NOT reachable
from the Zephyr path — `NanoRosCorrosion.cmake` is never included there. It must
be factored into its own module rather than pulling in the Corrosion one, which
is what that helper's own comment already asks for ("A second copy of the
normalise-and-hash rule is how the two would drift apart").

*Status: W5.a landed and verified. W5.b and W5.c NOT attempted.* Blocker 3 was
found while designing W5.c and has not been fixed; the digest above is a design,
not a measurement.

### Duplicate compiles (feature / version variants) — measured, and it is ~2 %

Asked whether crates get built repeatedly because of feature or version variants.
Measured on a cold `packages/cli` release build (202 units, 111.8 CPU-s), using
`--timings` for cost and `just leaf-graph` for the edges:

| duplicate kind | crates | redundant CPU | share |
| --- | --- | --- | --- |
| VERSION variants, actually compiled | 3 (`hashbrown`, `thiserror`, `thiserror-impl`) | 1.2 s | 1.1 % |
| same version compiled as a lib twice | 5 | 2.27 s | 2.0 % |

**The lock overstates the version problem by 10x.** The root `Cargo.lock` lists 35
multi-version packages, which reads as alarming; only 3 are ever compiled. The
rest are `windows-*` (9 crates, ~25 versions) and embedded crates that never build
on a Linux host. Reading the lock alone is the same workspace-vs-actual-build trap
this phase hit three times — W4 exists for exactly this.

**The five same-version duplicates are NOT a feature choice anyone made, and are
not fixable by configuration.** `cc`, `find-msvc-tools`, `shlex`, `unicode-ident`
and `memchr` are each needed by BOTH the build-dependency/proc-macro graph and the
normal dependency graph, and cargo compiles those as separate units. Three
hypotheses were tested against a real build and all three were REFUTED — the
duplicate set was byte-identical (same 5 crates) in every case:

| tried | result | total CPU |
| --- | --- | --- |
| as shipped (`panic=abort`, `lto=fat`, `codegen-units=1`) | 5 dups, 2.27 s | 111.8 s |
| `panic=unwind`, `lto=false`, `codegen-units=16` | 5 dups, 2.69 s | 134.7 s |
| `build-override` matched to the release profile | 5 dups, 3.28 s | 137.8 s |

Two of the three "fixes" made the build 20 % SLOWER. The split is cargo's
build-graph separation, which no profile setting collapses.

**`memchr`'s feature difference is a symptom, not the cause — and "fixing" it is a
provable no-op.** Its two units differ in features
(`['alloc','default','std']` vs `['alloc','std']`) only because resolver v2
resolves features independently for the two graphs. The units are already
distinct by graph position, so aligning features cannot merge them. It is also
not actionable: `just leaf-graph` reports its requirers as `object`, `quick_xml`
and `serde_json` — all external, none ours.

*Conclusion: not a lever.* ~2 %, inherent to cargo, and every intervention tried
costs more than it saves. Recorded so the question is not re-opened from the
lockfile's 35 rows.

### The pattern this phase keeps hitting, three times now

W2's orchestration half, and now W3, failed the same way the 31.9 % figure did:
**the removable set was computed from names and subtrees, then assumed to be what
the leaf's resolved graph contains.** Every time, building it showed otherwise —
and every time the error was optimistic:

| estimate | claimed | measured | why it was wrong |
| --- | --- | --- | --- |
| orchestration share | 31.9 % | 12.6 % | overlapping groups summed independently |
| W2 orchestration gate | 43 crates | 6 crates | `nros-orchestration-ir` needed by every arm |
| W3 bindgen | 20.6 s | 0 s from our crates | crates excluded + unreferenced; bindgen is upstream's |

**W4 — attribute the contested pool inside the LEAF's graph — should have been
first, and is now the only remaining item with a defensible premise.** Its whole
job is to answer "what does this build actually resolve?" from the build itself
rather than from the workspace. Everything above is what guessing costs.

### W4 — LANDED 2026-08-30. `just leaf-graph` — ask the build, not the workspace.

Everything above used reverse dependencies from the WORKSPACE (`cargo tree -i`),
not from the leaf, because the leaf does not resolve standalone (`zephyr-build`
comes from the west environment). Three estimates were built on that and all
three were wrong. This wave removes the excuse.

`scripts/nros-leaf-graph.py` (`just leaf-graph <target-dir>`) reads
`<target-dir>/**/.fingerprint/<crate>-<hash>/*.json` — the files cargo writes for
every unit it actually built, each carrying a `deps` array naming its dependency
units. That is the edge set of the build that RAN: no re-resolution, no
assumption about features or platform, and no need for the leaf to resolve
standalone. `--exclusive-to X` then computes by FIXPOINT what would actually
leave if X went, which is the calculation the failed estimates approximated by
eye.

Host and target sides are reported separately, keyed on whether a target-triple
component appears in the path rather than on directory depth. A cross build has
two graphs in one target dir, and conflating them is how a host-only tool gets
counted against firmware.

**Validated against the method it replaces, including a real disagreement.** On
`packages/cli/target`, the tool reported zerocopy's direct requirers as
`ahash, half, ppv_lite86`; `cargo tree -i zerocopy -e normal,build` reported only
`ahash`. The tool was right — adding `dev` to cargo's edge filter reproduces the
tool's answer exactly:

```
cargo tree -i zerocopy -e normal,build      ->  ahash
cargo tree -i zerocopy -e normal,build,dev  ->  ahash, half, ppv-lite86
just leaf-graph packages/cli/target         ->  ahash, half, ppv_lite86
```

That mismatch is the phase in miniature: `cargo tree` answered a NARROWER
question than the one being asked, silently, and the answer looked authoritative.
The build had compiled the dev-dependency units; the query had excluded them.

`--self-test` encodes the W2 failure as a regression case: in a graph where both
`macros` and `cbindgen` require `serde`, dropping `macros` must NOT drop `serde`,
and dropping both MUST. Two bugs were found and fixed by testing rather than
reasoning — side detection assumed a top-level `target/`, and a `**` glob walked
hundreds of thousands of files (0.11 s after bounding it).

*Acceptance (met):* a crate -> requirers table taken from the build's own
artifacts, cross-checked against `cargo tree -i` on a graph where both can be
asked the same question.

**Use it before quoting any future number in this phase.**

### W1 — landed

`nros-launch-parser` removed from `nros-macros` — declared, referenced nowhere.
One crate (67 -> 66): everything it brought is also reached through
`nros-pkg-index`, which the crate genuinely uses.

## Directions, in measured order

1. **Gate the orchestration half of `nros-macros`** — see W2, and note it must
   ship WITH the cbindgen move or the contested pool stays put.
2. **`bindgen` at build time — 18.4 %.** The repo ALREADY has the alternative and
   proved it: RFC-0054 commits bindgen output for the ABI crates
   (`nros-{rmw,platform,board}-cffi/src/generated.rs`) and gates staleness with
   `check-abi-bindings`. Four `*-sys` driver crates still generate at build time
   (`zephyr-posix-sys`, `nuttx-sys`, `freertos-lwip-sys`, `threadx-netx-sys`).
   **Not a straight copy of that pattern:** these bind the USER's RTOS headers
   via `ZEPHYR_BUILD_DIR` and friends, not in-tree ones, so committed output
   would assert which SDK generated it. The allowlists are small (a handful of
   socket types), which makes it tempting to hand-mirror the structs — do NOT:
   that is issue 0160's hazard, where a mirror-only TU passes a shorter struct
   and the tail field is garbage. If this is taken, it should be commit + a
   regenerate-and-diff gate per supported SDK, mirroring `check-abi-bindings`.
3. **`cbindgen` — 6.8 %**, same shape one size down (`nros-zpico-build`,
   `nros-build-helpers`), and it drags the whole `clap` CLI stack into a build
   dependency.

## Not yet examined

* Whether the same profile holds for the C/C++ Zephyr leaves, for NuttX/FreeRTOS,
  or for a workspace (non-standalone) build where the phase-340 shared cargo
  group amortises host deps across leaves. Everything above is ONE leaf,
  `rust/talker`, zenoh, `native_sim`.
* `heapless` appearing TWICE at 5.1 s and 4.4 s — two feature-distinct units of
  the same version, 9.5 s combined, larger than `bindgen` itself. Not traced.
* `getrandom` at 3.0 s on an image that should not need OS entropy.
* `thiserror` appearing at BOTH 1.x and 2.x, and `hashbrown` at 0.14 and 0.17 —
  duplicate major versions compile twice. Not yet traced to their requirers.
* `cargo-machete` across the repo flags 467 rows, but it is largely UNUSABLE
  here: the top entries (`nros-rmw-zenoh` ×59, `nros-platform` ×50,
  `nros-platform-cffi` ×23) are FORCE-LINK deps, present so rustc's staticlib DCE
  does not drop their `#[no_mangle]` exports. Machete cannot see `extern crate`
  force-links. Its output needs per-row triage against that pattern before any of
  it is acted on; W1 is the one row confirmed by reading the source.
