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
`just check-cbindgen-headers`); the build scripts only compare and warn, so no
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
regen-c-headers` and `just check-cbindgen-headers` are unchanged. The
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

*Lanes run:* `just check-fast` (135/135 gates OK), `cargo clippy` on both
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
