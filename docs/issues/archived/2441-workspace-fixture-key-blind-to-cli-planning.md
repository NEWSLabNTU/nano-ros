---
id: 2441
title: "Workspace fixtures are keyed on the CLI's codegen fingerprint, so a change to what `nros build` / `nros configure` hand a build does not re-stale them"
status: resolved
type: tech-debt
severity: medium
area: [testing, fixtures, cli]
found: 2026-11-11
resolved: 2026-11-12
resolved_in: "workspace fixtures and the nros configure stamp key on a generated CLI planning-set fingerprint (option b), gated by check-cli-planning-set"
related: [issue-2439, issue-0196, issue-1018, issue-0604, phase-530]
---

> **Resolved (2026-11-12), option (b).** The ruling, verbatim: "Take (b) plus
> fixing the configure stamp." Workspace fixtures now key on the CLI's
> PLANNING fingerprint (`nros planning-stamp`), computed over a generated
> planning set, and the `nros configure` stamp records the same fingerprint,
> so a planning change re-runs configure. See "Resolution" below.

## What

Issue 2439's class, one lane over. `scripts/build/workspace-fixture-signature.sh`
names the CLI by `nros_codegen_fingerprint`: what its emitters produce for a
compiled-in corpus. A workspace fixture is built by `nros build`, so it is
also a function of the CLI's PLANNING: the seeds `nros configure` writes
(entity inventory, message bounds, sizing descriptor), the image statement,
the configure options. None of that is codegen output, so a CLI change to it
moves nothing the signature hashes, and the fixture is reused as fresh.

Issue 2439 fixed this for the six west compile-check rows by keying them on
the CLI SOURCE STAMP (`nros_cli_source_stamp`). That key was not applied here.

## Why not the same fix

`codegen-stamp.sh` and phase-318 W1 measured the cost of a wide CLI key on
this lane: a binary-keyed signature invalidated 40 fixtures / 35 build dirs,
a multi-hour ~100 GB rebuild, for a codegen change whose correct answer was
zero; 168 CLI binaries against 11 fingerprints. The source stamp is narrower
than a binary hash but still moves for a `doctor.rs` edit or a `play_launch`
pin bump.

## The decision

Which cost to pay:

- **Source stamp** (2439's key): correct, and pays a full workspace rebuild
  on most CLI edits.
- **A second fingerprint over the planning surface**: e.g. hash the
  `nros configure` outputs for a compiled-in corpus, as `codegen-fingerprint`
  does for emitters. Narrow and correct, but needs a corpus that exercises
  the planning paths, and is only as complete as that corpus.
- **Keep it**: accept that a planning regression can pass this lane on a
  reused build until something else rebuilds it.

## Resolution

The user's ruling (2026-11-12): "Take (b) plus fixing the configure stamp."

**The set is generated, not hand-listed.** `scripts/gen-cli-planning-set.py`
writes `packages/cli/cli-planning-set.txt`; `check-cli-planning-set` (fast
line, `[group("ci")]`) fails when the committed file disagrees with it and
names the direction of each line. It is the `cli-source-dirs.txt`
arrangement (issue 0604), chosen over deriving at stamp time for that file's
reason: the stamp lives in `source_stamp.rs`, which `build.rs` `include!`s,
so it can run neither `cargo metadata` nor a TOML parser, and the reach walk
below is no work to repeat on every `nros build`. The file has two parts:

- `+` lines: every in-repo crate the `nros` binary compiles, from the SAME
  cargo resolve `gen-cli-source-dirs.py` makes (`closure_package_dirs`, now
  shared), plus `packages/cli/Cargo.{toml,lock}` and the set file itself;
- `-` lines: each crate's `tests/`, `benches/`, `testdata/`, `examples/`, and
  the `nros-cli-core/src` files a reach walk from `lib.rs` does not reach.

The walk is computed: `crate::`/`self::`/`super::`/bare paths and `use`
trees (aliases and module imports bound, so `entry_codegen::x` resolves),
macro uses, `include!`, and impl blocks (an `impl T` in another file is an
edge INTO it from `T`'s file, since a method call reaches it by type). It
blanks comments, literals and `#[cfg(test)]` items first. `mod x;` is not an
edge. The one authored input is `LEAF_VERBS`, method (b)'s 13 leaf verbs
(`cmd/doctor.rs` and the rest): the walk drops a dispatcher's
(`lib.rs`, `cmd/mod.rs`, `cmd/dev.rs`) edge into a leaf, follows every other
edge, and FAILS naming the edge if any other file reaches a leaf. The file
lists exclusions rather than inclusions inside a crate, so a new source no
one regenerated for is INSIDE the set (an over-watch) until the gate asks.

At HEAD the walk reaches 163 of 185 `nros-cli-core/src` files (method (b)
measured 162). It differs from the hand-measured set where the measured walk
was file-granular or guessed: `cmd/prepare.rs` (used only by `nros flash`)
and `codegen/entry/emit_rust.rs` (the parity renderer, used only by tests)
are OUT; `cmd/setup/session.rs`, `orchestration/python_provider.rs` and
`orchestration/release_manifest.rs` are IN, reached from `cmd/setup.rs` and
`cmd/pin.rs`, which are not leaves. `cargo-nano-ros`'s `scaffold.rs` and
`workspace_scaffold.rs` stay IN: crates other than `nros-cli-core` are taken
whole, and `cmd/new.rs`, which calls them, is reached from `cmd/build.rs`
through `run_plan`. `examples/templates/**` is out (no crate reaches it).

**The fingerprint.** `source_stamp::planning_stamp` hashes every source under
a `+` path and no `-` path, with the source stamp's own entry encoding (one
shared `content_entries`/`fold_entries`, not a second loop): tracked content,
worktree edits, and untracked files under `packages/cli`. `build.rs` bakes it
as `NROS_CLI_PLANNING_STAMP`; `nros planning-stamp` (hidden) prints it for
the sources on disk.

**Fixtures.** `workspace-fixture-signature.sh` (v4) adds
`tool:nros-planning` through `nros_cli_planning_stamp`
(`scripts/build/codegen-fingerprint.sh`), beside the codegen fingerprint,
which stays: it still covers the emitter corpus under `rosidl-codegen/tests/`
and a toolchain change that moves no source. The ladder falls back to the
source stamp (wider) for an `nros` without the verb, never to "unchanged".
The test side (`current_workspace_fixture_signature`) runs the same script.

**Configure.** The stamp gains `cli_planning`; `currency` reports
`CLI planning changed: <old> -> <new>` when the running binary's baked value
differs, and a stamp without the field is stale once.

**The metadata sidecars, the configure stamp's other CLI key.** A configure
is also stale when a probe sidecar is (`metadata_refresh::probe_inputs_key`),
and that key named the CLI by its SOURCE stamp, so a `doctor.rs` edit
re-probed every sidecar and re-ran every configure anyway. The harness and the
probe driver are planning code, so the key now names the planning stamp too.
Measured on `examples/workspaces/rust` after `just setup-cli`:

| edit (then reverted) | source stamp | planning stamp | `workspace-rust-native` signature | `nros configure` |
|---|---|---|---|---|
| none (baseline) | `e6d379bff66873e6` | `0bc4c312367fb21b` | `18561726...` | `up to date (86 image(s))` |
| `const` appended to `cmd/doctor.rs` | `4019e9980288d5f4` | `0bc4c312367fb21b` | `18561726...` | `up to date (86 image(s))` |
| `const` appended to `cmd/configure.rs` | `172e5b000b9a8155` | `0600fc62e0d51ddc` | `889a73f8...` | `CLI planning changed: 0bc4c312367fb21b -> 0600fc62e0d51ddc`, 6 sidecars re-probed |

**One class fix on the way.** `is_cli_input` did not watch `pack.toml`
(entry packs, read by `nros-cli-core/build_entry_packs.rs`; emitter packs,
read by `rosidl-codegen/build.rs`), which build scripts compile in through
`concat!(env!("CARGO_MANIFEST_DIR"), ...)`, invisible to
`every_embedded_file_is_watched`. A pack edit left both stamps unmoved; it
now moves them.

## Measurements (2026-11-11)

Measurement only, taken before the ruling above.

### Method

**Window.** Every first-parent commit on `main` from 2026-10-28 00:00 to the
fetch at 2026-11-11 ~06:00 UTC: 1334 commits, about 14 days. Rates below
divide by 14.

**(a) source stamp.** A commit moves (a) when it changes a file the stamp
reads: `is_cli_input` in `packages/cli/nros-cli-core/src/source_stamp.rs`,
applied to `packages/cli` plus the `cli-source-dirs.txt` closure (both the
commit's and its parent's list), or the `play_launch` gitlink. 466 of the 1334
commits do.

**(b) planning set.** Defined here, file by file, as the code the fixture road
runs. The road is what `scripts/build/workspace-fixtures-build.sh` and the
cmake/west helpers invoke: `nros configure`, `nros build` (including
`--dry-run`'s stage 4), `nros dev codegen system`, the `dev codegen` /
`dev ws` / `dev profile` / `dev lock` / `dev board` / `dev sdk path` helpers
cmake calls, `nros locate`, the metadata probe, the entity census and the
sizing descriptor. The set is:

- every crate `cargo tree -p nros-cli -e normal,build` reaches (27 in-repo
  crates: the `rosidl-*` emitters, `nros-entry-lower`, `nros-lang`,
  `nros-pkg-index`, `nros-launch-parser`, `nros-msg-to-idl`, `cargo-nano-ros`,
  `nros-launcher`, `nros-extension`, `nros-orchestration-ir`, the core and
  tooling crates in the closure), their `Cargo.toml`, `build.rs` and templates,
  plus `packages/cli/Cargo.lock` and `cli-source-dirs.txt`;
- inside `nros-cli-core/src`, the files reachable from the road's verb modules
  (`cmd/{build,configure,ws,codegen,codegen_system,generate,locate,sdk_path,
  metadata,profile,board,entity_census,entity_inventory,sizing_descriptor,
  codegen_cyclonedds_descriptors,materialize,model_path,prepare,
  west_leaf_sizing,leaf_kconfig,leaf_settings,leaf_system,message_bound_knobs,
  image_facts}.rs`, the dispatchers `lib.rs`, `cmd/mod.rs`, `cmd/dev.rs`, and
  `stale_guard.rs`) by a module-reference walk (`crate::a::b`, `super::x`,
  sibling `x::`, `use crate::{..}`), with `codegen/entry/**` added whole.

It excludes:

- test-only code: any `tests/` or `benches/` dir (the `rosidl-codegen`
  fingerprint corpus is NOT excluded: it is `include_str!`d),
  `test_support.rs`, and the `#[cfg(test)]` modules `codegen/entry/golden.rs`,
  `parity.rs`, `emit/tests_{c,cpp,data_pack}.rs`;
- `nros-cli-core/src` files the walk does not reach from the road:
  `builder/dist.rs`, `cmd/{completions,config,doctor,explain,flash,graph,
  interface,monitor,probe_boot_rows,reserved,sdk_front,update,user}.rs`,
  `cmd/setup/session.rs`, `orchestration/{python_provider,release_manifest}.rs`;
- `packages/cli` crates the `nros` binary does not link: `nros-build`,
  `nros-launch-resolve` (keyed separately by `resolve-fingerprint.sh`),
  `colcon-cargo-ros2`, `build-support`, `interfaces`;
- scaffolding: `examples/templates/**` and `cargo-nano-ros/src/{scaffold,
  workspace_scaffold}.rs`, which only `nros new` emits.

The walk is file-granular, so it over-approximates: `cmd/build.rs` names
`run_plan`, which names `run_exec`, `bench` and the `new_*` verbs, and those
stay in. The result is that (b) is most of the CLI: 162 of the 185
`nros-cli-core/src` files. Two (b) columns are reported: **b-file** (the commit
touched a planning file) and **b** (it changed a planning file outside its
`#[cfg(test)] mod tests` block and outside comment-only lines, judged per diff
hunk). b is the narrowest key a source-level planning fingerprint could be;
an OUTPUT-level one (hash `nros configure` output for a corpus, as the issue
suggests) would be narrower still, and section "Configure output" below
estimates how much.

**(c) codegen fingerprint.** `rosidl_codegen::codegen_fingerprint()` (what
`nros codegen-fingerprint` prints) depends only on `rosidl-codegen` and its
dependency closure (`rosidl-parser`, `rosidl-resolve`, `rosidl-lower`,
`nros-lang`, `nros-serdes`, `nros-core`, the lock). For the 25 window commits
touching that closure, a scratch crate linking `rosidl-codegen` at the commit
and at its parent computed the fingerprint itself, so (c) below is exact, not
inferred from paths.

**Fixtures.** Row counts from `scripts/build/fixtures-manifest.py
list-workspaces`, narrowed with `--coords-from` the `lane-coords` output for
each lane: 388 rows in all (335 under `examples/workspaces/`), tier1 166
(152 linux, 7 zephyr, 7 threadx-linux), tier2 70, tier2-nightly 294. Every row
hashes the same CLI key, so under any option one key move restales every row
of the lane; no option narrows by row.

**Rebuild cost.** Measured in the ros2 box on one host, rows built one at a
time with `workspace-fixtures-build.sh <platform> --id <id>`, reading the
builder's own `row-time:` line (the per-workspace `nros configure` pre-pass is
outside it): four rows (linux rust, linux c, threadx-linux mixed, freertos
cpp), each (1) after a ~1 day catch-up from an older tree, (2) re-run with
nothing changed, (3) re-run after a CLI edit with no output effect (one
`const` appended to `cmd/doctor.rs`, `just setup-cli` rebuilt the binary in
85 s), (4) re-run after reverting it. The Zephyr row tried
(`workspace-c-zephyr-talker-xrce`) is built by the west lane, not this
builder, and was not timed. NuttX was skipped to keep off the shared kernel
tree (issue 0525).

### How often each key moves

| day | commits on main | (a) commits | b-file | b | (c) | (a) landing events | b events | (c) events |
|---|---|---|---|---|---|---|---|---|
| 2026-10-28 | 26 | 2 | 2 | 2 | 0 | 2 | 2 | 0 |
| 2026-10-29 | 62 | 16 | 16 | 11 | 0 | 15 | 10 | 0 |
| 2026-10-30 | 47 | 11 | 11 | 8 | 0 | 8 | 6 | 0 |
| 2026-10-31 | 79 | 22 | 21 | 16 | 0 | 16 | 13 | 0 |
| 2026-11-01 | 64 | 32 | 29 | 28 | 0 | 29 | 26 | 0 |
| 2026-11-02 | 42 | 17 | 16 | 16 | 0 | 17 | 16 | 0 |
| 2026-11-03 | 63 | 28 | 24 | 23 | 1 | 21 | 19 | 1 |
| 2026-11-04 | 119 | 42 | 38 | 37 | 0 | 20 | 17 | 0 |
| 2026-11-05 | 54 | 14 | 14 | 14 | 0 | 14 | 14 | 0 |
| 2026-11-06 | 171 | 52 | 45 | 36 | 1 | 36 | 28 | 1 |
| 2026-11-07 | 193 | 85 | 43 | 34 | 0 | 37 | 29 | 0 |
| 2026-11-08 | 1 | 1 | 1 | 1 | 0 | 1 | 1 | 0 |
| 2026-11-09 | 208 | 87 | 72 | 64 | 1 | 41 | 35 | 1 |
| 2026-11-10 | 171 | 49 | 40 | 33 | 0 | 34 | 27 | 0 |
| 2026-11-11 | 34 | 8 | 6 | 4 | 0 | 7 | 4 | 0 |
| **total** | 1334 | 466 | 378 | 327 | 3 | 298 | 247 | 3 |

A landing event is one committer timestamp: the queue rebase-merges a pull
request's commits with one timestamp, so an event is roughly one merged PR.
(a) moved on all 15 days, b on all 15, (c) on 3 (2026-11-03, 11-06, 11-09).
The three (c) moves are `fb7b343159a90f87cb5848ca6d2a00e4846ed6ac`,
`f57516b629d97860300b1001156001dc2438decb` and
`d71a15ecb76b5bba89135bc716805a64cd6ea9d7`; 22 other commits touched the
codegen crates without moving the fingerprint.

Of the 466 (a) commits, 139 (30 %) change no planning code (b says no): 70
touch only the leaf verbs listed above, 14 only tests, 3 only scaffolding, 1
only a crate outside the binary; the other 51 touch planning files only inside
`#[cfg(test)]` blocks or comments. Those 139 are (a)'s floor of false
restales. The ceiling is higher, see "Configure output": on 5 of the 15 days
the sampled configure output did not move at all although (a) and b did.

### Rebuild cost per restale

`row-time:` seconds per row, ros2 box, one row at a time:

| row | catch-up (~1 day of main) | no-op re-run | after a no-effect CLI edit | after revert |
|---|---|---|---|---|
| `workspace-rust-native` (linux, cargo) | 34 | 2 | 4 | 2 |
| `workspace-c-native` (linux, cmake) | 164 | 45 | 9 | 40 |
| `workspace-mixed-threadx-linux` | 161 | 51 | 51 | 52 |
| `workspace-cpp-mps3-an536-freertos` | 103 | 52 | 52 | 55 |

Wall clock per invocation, including the per-workspace `nros configure`
pre-pass, was 16 to 180 s. A no-effect CLI edit costs the same as a no-op
re-run: nothing recompiles, because the emitted files are byte-identical and
written only when changed. So a restale does not cause a rebuild of the image;
it causes a re-run of the builder pass, about 30 to 40 s per row here.

This matters for reading the cost. `workspace-fixtures-build.sh` never skips a
row whose `.inputsig` matches: it runs `nros build` for every row of the lane
every time, and the stamp is read only by the test side
(`require_prebuilt_workspace_binary`), which refuses a fixture whose signature
moved. So the key decides when a test run DEMANDS a builder pass, not what the
pass compiles. A lane that always builds before it tests (CI's `just build
tier1` / `tier2`) pays the pass on every run under every option; the key costs
extra only where fixtures are reused without a build in between (a developer's
`just test` after a pull, a test re-run against a kept fixture tree).

### Configure output

Sample: `examples/workspaces/rust` (cargo road) and `examples/workspaces/c`
(cmake road), which hold 167 of the 388 rows, every image in them. One scratch
worktree per role: CLI binaries built at the last CLI-touching commit of each
day (16 points, plus 6 more to split three days, below), each with its own
`nros-launch-resolve` for its `play_launch` pin; an evaluation worktree checked
out at the EARLIER point of each pair. For each pair, both binaries ran on the
same tree: wipe `build/`, `generated/` and `src/*/metadata/`, then `nros
configure --no-provider-index` (its older name `sync` before the 2026-11-04 rename) and
`nros build --dry-run --all` (stages 1 to 4: models, seeds, sizing
descriptors, `nros-cargo.toml`, generated cmake roots, entries, message
crates). Every text file written was diffed after rewriting the tree, binary
and resolver paths. A noise run (one binary twice) differed only inside the
metadata probe's own cmake build dir, which is excluded. Sidecar
`inputs_digest` lines carry the CLI stamp itself and are ignored.

On 2026-11-01, 11-03 and 11-06 the later binary refused the earlier tree's
board descriptors (a descriptor schema change landed with the CLI change).
Those days were split at the migrating commit
(`6b702792827cada1c48eba18dd41ed7e3cfecf0f`,
`1cfa392bb63a43215435cc7ebbd66976f93a8948`,
`8cf3ef68219e954c603645bc97f00722b0af319f`) and each half measured; the
migrating commit itself is code plus data and is not separable this way.

Configure output that moved, by day, with the commit that introduced the
changed text (found with `git log -S` on it, not by bisection):

| day | what changed in the output | commit(s) | kind | b | (c) |
|---|---|---|---|---|---|
| 10-28, 10-29, 10-30 | nothing | - | - | - | - |
| 10-31 | Zephyr entries gain a generated `prj.conf` | `237a866ada7c21508a993f180b2126769d07360a`, `13ad648f78fd12f3d3e0c36aac3688ba0f961a46` | build input | yes | no |
| 11-01 | model `resolver.version` (play_launch pin) only | pin bumps | resolver | yes | no |
| 11-02 | model `resolver.version` only | pin bump | resolver | yes | no |
| 11-03 | model schema 2 to 3, pin; message-crate emitter stamp | pin bump; `fb7b343159a90f87cb5848ca6d2a00e4846ed6ac` | resolver; codegen | yes | yes |
| 11-04 | model schema 3 to 4; sizing refusal text says `arrival_curve`; new configure `stamp.toml` files | `4c51ec760cf3ea6ee268569d8a140df9317294d7`; `8e87acb5ed73a0dbc5970fd19212769a0ee352bd`, `a64f9065395db4bcbc26de410bb2f5ca23ab768f`, `21800f60a4b4158c862694ab4666deeb8be82196` | resolver; text; bookkeeping | yes | no |
| 11-05 | model schema 4 to 5; seed comments say "sched task"; one entity digest | `a0669c42197fdceb7f30d3b00e1a8dad7e45e248` | resolver; text | yes | no |
| 11-06 | entry `Cargo.toml` features (`ros-humble`, board features) and the selection crate folded into the entry; `NROS_ZENOH_MAX_LARGE_SUBSCRIBERS = "0"` in `nros-cargo.toml`; sizing text | `b273012e57d56a39e35a4e9b499841e7e03c39e2`; `6089cbe3cc43d1911bffdda4f588c23e58ab2c4e` | build input | yes | no (one (c) move this day, `f57516b629d97860300b1001156001dc2438decb`, is the emitter stamp only) |
| 11-07 | every cmake root renamed to name the bringup; sizing refusal text | `fd6f9594d38547bea1dbf50fbb632d36eea892fc`; `bc1c8cfe56df243853746c9ae31e677e1b55a784` | build input; text | yes | no |
| 11-08 | nothing | - | - | - | - |
| 11-09 | new per-coordinate seeds (`resolved.cmake`, `resolved-message-bounds.cmake`, `resolved-entity-inventory.json`, `coordinate.toml`); Zephyr Rust entry calls `nros_app_register_backends`; stamp format 3; generated comments name the current verbs | `1f493fd18f9e1ded73f8e2dc4c8a4ea4bad1d7d9`, `3267957768cc4bdcdb7ca8afdaf1ffc0b3d36a90`; `0224a4628d1f09e9ca1637209e06c137c4136efa`, `0a52fb84729d72ef7971a2e3f46cf1a80d3f9e6e`; `2f08c930aa2567a46e2f7981ba604b2fe3546bb2`; `d71a15ecb76b5bba89135bc716805a64cd6ea9d7` | build input; text | yes | only `d71a15ecb76b5bba89135bc716805a64cd6ea9d7` |
| 11-10 | `nros-cargo.toml` gains `[profile.nros-inspect]` | `ff1b56a27b62eebdb8db39cc0e28c1c365aa5be3` | build input (a profile no default build selects) | yes | no |
| 11-11 (to 06:00) | nothing | - | - | - | - |

"Build input" means a file the image build reads changed; "text" means a
comment or a refusal reason; "resolver" rows are covered today by the
signature's `tool:resolve` term, not by (c). Counting commits whose change
reaches a build input: 10 named above on 5 days, none moved (c). Every
commit that moved the sampled output moved b. One of them also moved the
fixture RECORD itself (`fd6f9594d38547bea1dbf50fbb632d36eea892fc` rewrote
`examples/fixtures.toml`'s build dirs), which re-keys those rows under every
option.

### Summary

Rates over 14 days. Rows per lane: tier1 166, tier2 70. "Pass" is one builder
pass over every row of the lane at the measured ~35 s per row, serial: tier1
about 97 min, tier2 about 41 min (the builder runs independent units in
parallel, so wall clock is lower; CPU is not).

| | (a) source stamp | (b) planning set (b) | (c) codegen fingerprint (today) |
|---|---|---|---|
| key moves, commits per day | 33.3 | 23.4 (b-file 27.0) | 0.21 |
| key moves, landing events per day | 21.3 | 17.6 | 0.21 |
| days the key moved, of 15 | 15 | 15 | 3 |
| rows restaled per event, tier1 / tier2 | 166 / 70 | 166 / 70 | 166 / 70 |
| row restales per day, tier1 / tier2 (per event) | 3533 / 1490 | 2922 / 1232 | 36 / 15 |
| pass minutes per day if every event is followed by a test run on kept fixtures, tier1 / tier2 | ~2070 / ~870 | ~1710 / ~720 | ~20 / ~9 |
| pass minutes per day if fixtures are refreshed once a day, tier1 / tier2 | ~97 / ~41 | ~97 / ~41 | ~19 / ~8 |
| extra minutes in a lane that always builds before testing (CI tier1/tier2) | 0 | 0 | 0 |
| sampled configure-output changes to a build input it would miss | 0 | 0 | 10 commits, 5 days |
| false restales (moved, sampled output did not) | 139 of 466 commits by code, and all of 5 quiet days | 5 quiet days | none observed |

### What a stale fixture risks under (c), and what a false restale costs under (a)

Under (c) a test reuses an image whose configure inputs the current CLI would
write differently, which is the issue's own case: the seeds `nros configure`
writes (entity inventory, message bounds, sizing descriptor), the image
statement, the configure options. The window shows the shape is not
hypothetical: 11-06 delivered a new knob value
(`NROS_ZENOH_MAX_LARGE_SUBSCRIBERS = "0"`) and new entry features, 11-09
changed what a Zephyr Rust entry registers at boot and added the seeds the
cmake road reads. A kept fixture built before those commits would have passed
as fresh while carrying the old image.

Under (a) a false restale costs one builder pass per lane per event (table
above) and nothing more, since the pass compiles nothing new. Under (a) and
(b) alike every CLI-touching landing marks all 166 tier1 rows stale, so a
developer reusing fixtures across pulls re-runs the lane on essentially every
pull.

One more finding, independent of the option: `nros configure` (since
2026-11-04) skips when its stamp is current, and the stamp
(`configure_stamp.rs`) keys on the project's inputs, the environment, the
resolver pin and the codegen VERSION constant, not on the CLI's planning code.
So in a kept build tree a planning-only CLI change does not re-run configure
even when the builder pass runs; its outputs (models, seeds, sidecars) stay as
the older CLI wrote them until an input changes or someone passes `--force`.
`nros build`'s own stage 4 (entry, sizing descriptor, `nros-cargo.toml`) is
rewritten on every build. Changing the fixture key alone would therefore make
the test demand a pass that, for configure outputs, may not refresh what the
key says moved. Not measured beyond reading the stamp code and observing one
`nros configure` after the CLI edit above.

### Caveats

- Sample, not census: two workspaces (167 of 388 rows, cargo and cmake roads,
  linux, threadx-linux, freertos, nuttx and zephyr images as dry-run stage 4
  writes them). Changes that only show in other workspaces (`cpp`, `mixed`,
  `features`, the realtime ones) or only in stage 5 (cmake/west configure of
  the generated root, the cargo build itself) are not counted, so the (c)
  misses are a lower bound. Daily granularity: a change made and reverted
  within one day is invisible.
- Attribution by `git log -S` names the commit that introduced the text, not
  a bisected first-bad commit; where two commits are listed, either may own
  the change.
- The three migrating commits (descriptor schema plus CLI) were not measured
  as a pair; their own configure effect is unknown.
- The (b) set is file-granular and comes from a regex module walk, not the
  compiler; it over-includes (verbs reached through `cmd/build.rs` ->
  `run_plan`), and could miss a module reached only through a macro or a
  re-export the walk does not parse. The b column's test/comment filter is
  per diff hunk and treats a `#[cfg(test)] mod tests` block as the end of a
  file's live code.
- Rebuild cost is four rows on one host under a light load, one run each; the
  no-op and edit columns differ by up to 36 s on one row, which is noise of
  the same size as the effect. Zephyr (west lane) and NuttX rows were not
  timed. Minutes per day assume every row costs the measured mean.
- Lane row counts came from the prebuilt `lane-coords` of 2026-11-10 in this
  worktree (tier2: 11 coordinates; the CI run of 2026-11-11 reported 12).
- The (a) column replays today's `is_cli_input` over the whole window. One
  rule changed inside it: `1a178df9d902df67a9c812a110007bc44ad44075`
  (2026-10-29) added the template-tree clauses, so before it a non-`.rs`
  template edit did not move the real stamp; it counts here.

### Per-commit table

All 466 commits in the window that move (a). b: yes (planning code moved
outside tests and comments), file-only (only test or comment lines of a
planning file), no. Subjects are as committed, with non-ASCII replaced and one retired CLI
spelling reworded (the `cli-old-spellings` gate).

<details>
<summary>466 rows</summary>

| date | commit | subject | areas touched | (a) | b | (c) |
|---|---|---|---|---|---|---|
| 2026-10-28 | 2fd961d1b1dd65d764761e667f62495d85a4a160 | fix(#2085): the CMake runtime umbrella carries the central patch | cli-core/lib | yes | yes | no |
| 2026-10-28 | 39e844300b5c32af52082c016ee7e668f1176c30 | fix(#2129,#2130): store fetches retry a network failure; live-peer releases compile intermediates | cli-core/orchestration | yes | yes | no |
| 2026-10-29 | 018e6197da6e700d9f8498f5bfdbc530b09adf0f | feat(phase-517 W4): workspace setup asks for its own OS keys and refuses an unknown <depend> | cli-core/cmd | yes | yes | no |
| 2026-10-29 | 94e6fde0502dedf461c70a7d164c4bbbc5abeb41 | feat(issue 2135): report received_timestamp on zenoh and XRCE takes | packages/core/nros-core | yes | file-only | no |
| 2026-10-29 | 3819c97b812f639696fcec426134f14ae202b966 | fix(#2124): refuse every '[[domain]]' no bridge opens | cli-core/orchestration | yes | yes | no |
| 2026-10-29 | 3bb7c43cc4fa7e4b8ea6f98cae5e44dad0c0dc64 | fix(#2123): C/C++ multi-session specs carry each session's RMW properties | cli-core/cmd, cli-core/codegen, cli-core/lib +2 | yes | yes | no |
| 2026-10-29 | 43e5baa2f07551c2c5992f8ba845db54baca6781 | fix(issue 2137): nros build takes the census a cross image is checked against | cli-core/cmd | yes | yes | no |
| 2026-10-29 | 1b2579b2fee5703bd0dd6d6d5115cf9b27743d24 | fix(issue 1782): the mps2-an385-zephyr board package owns its Kconfig | cli-core/cmd | yes | yes | no |
| 2026-10-29 | dae1384ed21d0fb4271c29b0f335cf8118411df5 | check(phase-515 W0.d): a code comment may not quote a symbol that exists nowhere | packages/tooling/nros-build-paths, packages/tooling/nros-cargo-profile, packages/tooling/nros-platform-config +2 | yes | file-only | no |
| 2026-10-29 | f04353d36553e2e973c1f1b4c4015452cd6bdcb2 | check(phase-515 W0.d): sweep packages/boards for names that exist nowhere | packages/boards/nros-board-common | yes | file-only | no |
| 2026-10-29 | 360238aee94b8381996eed703f99d3052819fe09 | check(phase-515 W0.d): sweep packages/cli for names that exist nowhere | cli-core/builder, cli-core/cmd, cli-core/codegen +17 | yes | file-only | no |
| 2026-10-29 | 971d48e312d4639bf84b19ddb2e614c556682f6d | check(phase-515 W0.d): sweep packages/core for names that exist nowhere | packages/core/nros-core, packages/core/nros-orchestration-ir, packages/core/nros-rmw +1 | yes | file-only | no |
| 2026-10-29 | 1a178df9d902df67a9c812a110007bc44ad44075 | feat(phase-517 W12): a real template per node role behind 'nros new --template' | cli-core/cmd, cli-core/lib, cli-core/source_stamp +2 | yes | yes | no |
| 2026-10-29 | 32dd231e95e2b715681ba34f7cd7cb71ec99f01e | refactor(phase-517 W14): one QEMU ladder crate for the CLI and the harness | cli-core/cmd, cli-core/orchestration, packages/cli/Cargo.lock +3 | yes | yes | no |
| 2026-10-29 | ba24060b0a6d343db8f2ca676f322013b060ae21 | feat(phase-517 W13): a Rust node off Linux names the workspace commands | cli-core/cmd, cli-core/orchestration, packages/cli/cargo-nano-ros | yes | yes | no |
| 2026-10-29 | ef9b79a9ce50ff16cfc49971685f34245112b8b5 | feat(issue 2136): take_response_with_info - a service client's rmw_service_info_t | packages/core/nros-rmw | yes | yes | no |
| 2026-10-29 | a9f60d5845a8198da26ffcf57ed58c3ca59824cb | feat(phase-517 W17): ship nros-build-profile, probe the other language roads, fix the nightly board channel | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-10-29 | ce6b42b35cc0b60362d70e2164956faebb7c774d | fix(phase-517 W17): the Zephyr Rust road builds - fetch zephyr-lang-rust, enable NET_DRIVERS for NSOS | cli-core/cmd | yes | yes | no |
| 2026-10-30 | e4357d4d9f164a7df14760ca11235ff02ae572b6 | docs(phase-515 W0.d): rewrite prose around struck names (cli, rmw, small areas) | cli-core/builder, cli-core/cmd, cli-core/codegen +9 | yes | file-only | no |
| 2026-10-30 | 1c81f56b75cf3bb3da1ae71d84af053323413310 | docs(phase-515 W0.d): rewrite prose around struck names (core, testing, api) | packages/core/nros-core, packages/core/nros-orchestration-ir, packages/core/nros-rmw | yes | file-only | no |
| 2026-10-30 | d71d1a5b3a3d7e93d5610d5d495116b8b0330575 | fix(issues 2154/2156/2134): withdraw C++ TickCtx, drop dead wake/fd helpers, retire Embassy framework | cli-core/cmd, cli-core/codegen, cli-core/orchestration +3 | yes | yes | no |
| 2026-10-30 | f1634a37698f4ca282946511dc4e2f112d348bcc | fix(issue 2152): a scaffolded Zephyr entry gets its selection package | cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-10-30 | d0a346670ba4e1a301bf93ebc6ceea53b84e1c77 | fix(issue 2151): provision and preflight a Rust image on Zephyr | cli-core/builder, cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-10-30 | 93b35e6ce2d104bd13ef6e0e99bf481f1ec7012c | fix(issue 2151): read the board catalog from the SDK root, not the index's directory | cli-core/cmd | yes | file-only | no |
| 2026-10-30 | b275e885b31145283afbbe15aeadef78a71aa12d | fix(issue 2151): a Rust Zephyr image needs rustup's clippy too | cli-core/builder, cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-10-30 | 34baef9a6682ee382650e652323cdd347daa8a70 | fix(issue 2149): the cargo road configures the NuttX kernel before a Rust image | cli-core/cmd | yes | yes | no |
| 2026-10-30 | 9a8e18a5cf69a140daaa691d48823479af71a5fa | fix(issue 2157): native_sim NSOS is one snippet the board names | cli-core/builder, cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-10-30 | d1a40e70a84719b5a8f47b28528d53be10663a72 | fix(issue 2153): refuse the plan's unapplied build-tuning keys by name | cli-core/orchestration | yes | yes | no |
| 2026-10-30 | 400c5faa0c91f4fe8f959d2434906ee171fb98b8 | fix(#2058): cyclonedds-sys names the pinned make, so its nested build joins a fifo jobserver | packages/tooling/nros-build-paths | yes | yes | no |
| 2026-10-31 | d39bccb3ee885703f9b9a50aa3cd13bd3e5eb04d | fix(issue 2170): the ThreadX dgram pair takes its machine line from the ladder | packages/tooling/nros-qemu-ladder | yes | yes | no |
| 2026-10-31 | cb3cdaf30d7f3cbe0ae6e1b22ecd876cd63c548b | fix(issue 2169): the printed run command must equal the line its chapter boots | cli-core/cmd | yes | file-only | no |
| 2026-10-31 | 5a0f7d5a4939e6328378751b769a8720b50a2e63 | fix(issue 2168): an inherited dispatch marker no longer stops another nros from following the pin | packages/cli/nros-cli-core, packages/cli/nros-launcher | yes | yes | no |
| 2026-10-31 | d361877f83266a9f8c118ea3cfdfb4597e19baaa | fix(issues 2147, 2160): nros build's west road names its Zephyr toolchain; recipes see the checkout's real path | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-10-31 | bb9331cc36877e368e5fe0e8ee27327857afc688 | feat(phase-520 W3): 'nros::setup!' defers a 'dispatch = "deferred"' component and hands the host the drain | packages/cli/nros-pkg-index | yes | yes | no |
| 2026-10-31 | 609f20af994ce784e1c91d780a029ce8ee369ffe | refactor(issue 2161): retire the per-node dispatch-slot registry nothing filled or drained | packages/cli/nros-build | yes | no | no |
| 2026-10-31 | 237a866ada7c21508a993f180b2126769d07360a | fix(#2179): a board package's Zephyr facts apply only to its own west board | cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-10-31 | d5e53924d2f9e18dcfae56d25acc1bcec0b6a327 | fix(issue 2180): image-facts says why an existing workspace answered nothing | cli-core/cmd | yes | yes | no |
| 2026-10-31 | 93b59bc3c5bb7ed9d979b91cfd44ccc216f847a6 | fix(issue 2180): a plan query never refuses over the build toolchain | cli-core/cmd | yes | yes | no |
| 2026-10-31 | 37c2c645c6ee07eb97a1cb7bfd44eaa79bd654c4 | feat(issue 2178): replay the NuttX rv-virt Rust workspace image | cli-core/cmd | yes | file-only | no |
| 2026-10-31 | 37d6466751889d24415ca62c7daceb062ca228f1 | fix(#2183): a root only its outer build resolves is re-seeded, not realigned crate by crate | cli-core/orchestration | yes | yes | no |
| 2026-10-31 | 52f4c8cc560b32c0d3f7a1955236a90c7b39451a | fix(#2184): the enclosing checkout's index outranks the store cache, and an unreadable cache is refetched | cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-10-31 | 13ad648f78fd12f3d3e0c36aac3688ba0f961a46 | fix(issue 2177): a scaffolded Zephyr Rust entry for mps2-an385-zephyr builds and boots | cli-core/builder, cli-core/cmd, packages/cli/nros-cli-core +1 | yes | yes | no |
| 2026-10-31 | ae6de4d16dc694c141197eee4773998ecf26bb99 | style(issue 2177): rustfmt the board table row | packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-10-31 | e3b059c0928726ddd03a067e4750ac96d7090a5d | test(issue 2177): the three new Zephyr board keys join issue 1435's refused set | cli-core/codegen | yes | file-only | no |
| 2026-10-31 | ffaf7940f2126ccdf681c71c14320e5e0ac7278c | docs(phase-418): the closure's content, which the previous commit left unstaged | cli-core/orchestration | yes | yes | no |
| 2026-10-31 | fb2b52b857f6d3eefb184c322a7ccee06150caef | feat(phase-522 W1): the board registry states runs, cadence and breadth beside the tiers | cli-core/cmd | yes | yes | no |
| 2026-10-31 | 8c098d5d4e648720200a3861331a704021c18c18 | feat(phase-522 W2): one generator writes the board-support views from the registry | cli-core/cmd | yes | file-only | no |
| 2026-10-31 | 18263ff1b9a31e8791c9c7d150f68d922571486c | feat(phase-522 W3): check-board-registry is the one board-support gate | cli-core/cmd | yes | file-only | no |
| 2026-10-31 | 50a2bc3f37334adb9bec6f02d8dd82529a0c349b | feat(phase-522 W4): the book reads the board registry and the tier numbers retire | cli-core/cmd | yes | yes | no |
| 2026-10-31 | 2e3b854a822c30c3a65e75b9febf8b06f3fa5b8b | feat(phase-522 W5): nros board and nros doctor --report read the board registry | cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-10-31 | 0d0cfaab3ddedc2455cdf37b85c65af496eddc6b | fix(issue 2187): Zephyr 4.4 builds the mps2 rows and the confs 3.7 took through aliases | cli-core/orchestration | yes | yes | no |
| 2026-11-01 | 89c24caf5f5a575be634af03eedb195acc652d33 | feat(phase-521 W3/W4): the readiness slot and the POSIX handle park | packages/core/nros-rmw | yes | yes | no |
| 2026-11-01 | 91914b3be679f605443c88974e95f65a40a033df | fix(issue 2189): a minimal Zephyr image configures on 4.4, and a Rust entry gets the Rust API | cli-core/cmd | yes | yes | no |
| 2026-11-01 | 50767620e5f8faa9b86c9b260c82c6afa49d85c5 | feat(phase-522): the rendered book applies the board staleness rule; phase-522 archives | cli-core/cmd | yes | no | no |
| 2026-11-01 | 6e607dadf756783880d98d7f7dd0e73411010bd6 | chore(deps): ros-launch-manifest v0.3.0, play_launch 442faaa3 | play_launch pin; cli-core/cmd, cli-core/orchestration, packages/cli/Cargo.lock +3 | yes | yes | no |
| 2026-11-01 | 6458cccfa4596a9af6e8d5761af581269102d363 | feat(phase-523 W3/W4): the industrial extras and the adoption-probe briefs | cli-core/cmd, cli-core/orchestration, packages/cli/nros-cli | yes | yes | no |
| 2026-11-01 | 03f844e063b65806351f416cc285d479cdf6f8b6 | feat(#2173): a route monitor per hazard rung (play_launch phase 96 W4.2) | cli-core/codegen, packages/cli/nros-entry-lower, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-01 | 676bc974f5d3e060e12dfcb799f9c28d6e5a8d0a | feat(#2173): judge the rung actually running, timed from its starts_at | cli-core/codegen, packages/cli/nros-entry-lower, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-01 | 86782e6639f5097fcdab5f5852b40c48127cc8ae | fix(cmake): don't derive MAX_LARGE_SUBSCRIBERS from a type count (2201) | cli-core/message_bound_knobs | yes | yes | no |
| 2026-11-01 | 3d90c236b26d7e65932e2f15431822c22bd0b7ef | feat(phase-524 W3, RFC-0115): Nucleo-F767ZI board package and registry row | cli-core/cmd, cli-core/codegen, packages/cli/nros-entry-lower +1 | yes | yes | no |
| 2026-11-01 | 65b0955bfd87e88db06b71f96d524a7925fe55f5 | fix(zephyr): refuse a board rust_target zephyr-lang-rust does not pick (#2204) | cli-core/cmd | yes | yes | no |
| 2026-11-01 | 3a8b2446f208fd2059a8de2f08778855f6650d94 | fix(cli): name every resolver rung when nros-launch-resolve is missing (#2205) | cli-core/cmd, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-01 | b46ba47d6da3f7aaae77140776888cb2b66dfad4 | fix(board-facts): resolve the leaf's own image, name the right one (#2199) | cli-core/cmd | yes | yes | no |
| 2026-11-01 | 595ee795131f201e2ba729caf32c798e9fa9bbfe | feat(#2174): warn on 'stale_after' where the backend carries no request source timestamp | cli-core/cmd, cli-core/codegen, cli-core/orchestration +1 | yes | yes | no |
| 2026-11-01 | 624c75888cf98b26aec8b10360efd6033f08f9bd | fix(issue 2200): read nros CLI output line by line, never as a cmake list | cli-core/cmd | yes | yes | no |
| 2026-11-01 | 41891e96e3a5dbfab66a5599abfbca8d9ea49077 | feat(phase-519 W3a): the XRCE agent paces a subscription that declares an ingress rate | packages/core/nros-rmw | yes | yes | no |
| 2026-11-01 | 6b702792827cada1c48eba18dd41ed7e3cfecf0f | feat(phase-528 W2): [board.run] / [board.console] in the board descriptor; the registry's emulator is derived from it | cli-core/builder, cli-core/cmd, cli-core/orchestration +2 | yes | yes | no |
| 2026-11-01 | 4428a45cdabe3c3747f9b0e4096f2f138fb91aa8 | feat(phase-519 W3b): a declared ingress reaches the XRCE agent's pace | cli-core/contract_join, cli-core/entity_inventory, cli-core/sizing_descriptor +1 | yes | yes | no |
| 2026-11-01 | 73f66accd1091710e69e863227c9e6a67a5fbeb2 | fix(issues 2175/2167): materialize rebases path deps; build cleans an entry whose cargo root changed | cli-core/builder, cli-core/cmd, cli-core/orchestration +1 | yes | yes | no |
| 2026-11-01 | 04f08024b25024967f18291f41887e117f219b90 | test(cli): board_new_scaffold runs the nros binary cargo built for it | packages/cli/nros-cli, packages/cli/nros-cli-core | yes | no | no |
| 2026-11-01 | 3f0cb8764f53c54cbbdd83379445fb2d63d32d6e | test(cli): use CARGO_BIN_EXE_nros in the moved board_new_scaffold | packages/cli/nros-cli | yes | no | no |
| 2026-11-01 | 20fd5e54e616034c09f47341a7aa24fa1a982f26 | fix(cli): the cargo-leaf contract join keeps the declared ingress | cli-core/contract_join, cli-core/entity_inventory | yes | yes | no |
| 2026-11-01 | 839d8835b0a52dcd67f8422712dcda901fbbbbc3 | feat(phase-529 W1, schema): [board.flash], [image.<id>.flash/console], prepare steps and command templates | cli-core/orchestration, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-01 | d1c63c3021353dff5297526e3c597ff504097932 | feat(phase-526 W1): the 'nros dev' tier, with silent aliases | cli-core/cmd, cli-core/lib, packages/cli/nros-cli | yes | yes | no |
| 2026-11-01 | 0b0226c8d7a2583551b752dc3597772994d64d72 | feat(phase-526 W1): 'nros help --aliases [--json]' prints the alias table | cli-core/cmd, packages/cli/nros-cli | yes | yes | no |
| 2026-11-01 | b40cd0ec4980642991e2010a78ca148065970d81 | chore(deps): ros-launch-manifest v0.3.1, play_launch 0.22.0 (2c66272d) | play_launch pin; packages/cli/Cargo.lock, packages/cli/nros-cli-core, packages/cli/nros-launch-resolve +1 | yes | yes | no |
| 2026-11-01 | 19f31360172cf13e864b2cac9cdb6427dae72cf3 | feat(phase-527 W3): packages/runners workspace and the nros-extension SDK crate | cli-core/lib, packages/cli/Cargo.lock, packages/cli/cli-source-dirs.txt +2 | yes | yes | no |
| 2026-11-01 | 1092c40478c758c7b16b902ef6a71ecc52c48328 | feat(phase-521 W6a): entries park until their next deadline | cli-core/cmd, examples/templates/multi-node-workspace, packages/cli/cargo-nano-ros | yes | yes | no |
| 2026-11-01 | 70f33d6e745a4fbac27357a354c98afb0840af9e | feat(phase-526 W2): the user tier regrouped, with noting aliases | cli-core/cmd, cli-core/lib, packages/cli/nros-cli | yes | yes | no |
| 2026-11-01 | 16982105761ea15b5bd7ecbcb91960ec6fb920a7 | feat(phase-524 W3, RFC-0115): Pico W board package and registry row | cli-core/cmd, cli-core/codegen, packages/cli/nros-entry-lower +1 | yes | yes | no |
| 2026-11-01 | 06d8f1c6ae81ae391988a3480e4ab771f3bdc6f8 | feat(phase-528 W1): the RunPlan resolver, 'nros build''s next step from the plan, 'nros run --dry-run --why' | cli-core/cmd, cli-core/lib, cli-core/orchestration +3 | yes | yes | no |
| 2026-11-01 | d98e57e2b8db6dd3992cadc6d792c80992dabbd4 | feat(phase-526 W5): help text says what a verb does; history moves to // comments | cli-core/cmd, packages/cli/nros-cli | yes | file-only | no |
| 2026-11-01 | c25eca840ed8199d9ce9a9b67993d83d49e87424 | feat(phase-524 W4a): twin probe tracks renode-nucleo and wokwi-pico | cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-11-02 | 0387d5e4ff678308158242691c22770b4d558284 | feat(phase-529 W1): execute a RunPlan's prepare steps | cli-core/cmd | yes | yes | no |
| 2026-11-02 | f563e489396695b5ef158743416b65209f86683e | feat(phase-529 W4): 'nros bench scan/list' and RFC-0118 D6 port resolution | cli-core/cmd, cli-core/lib, packages/cli/nros-cli | yes | yes | no |
| 2026-11-02 | 7111f230e1e64a868656176ed2b1d3e80f36321a | fix(issues 2192, 2189): west fixtures per Zephyr line; a sibling image reaches Kconfig or is refused | cli-core/cmd | yes | yes | no |
| 2026-11-02 | 943b39fc2ef9db04cf3eb38d0d2ea0d33326c72f | feat(phase-525 W2): the single-package front door, 'nros new --board' | cli-core/cmd, packages/cli/cargo-nano-ros, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-02 | eb17daa5ea1fefbd21d4df91a9d7a7f5264fb554 | ci(phase-524): the Wokwi twin leaves CI; it is user-run | cli-core/cmd | yes | no | no |
| 2026-11-02 | 36cb44b00bf7b303b080a97a4b699001e6260134 | feat(phase-527 W1): the runner and command provider kinds | cli-core/builtin_verbs, cli-core/lib, packages/cli/Cargo.lock +3 | yes | yes | no |
| 2026-11-02 | 6f74319a3f3d61bd729645856f87ec8cca3f80f4 | feat(issue 2194): a board's [knobs] names any registry knob, and every road carries it | cli-core/cmd, cli-core/orchestration, packages/tooling/nros-platform-config | yes | yes | no |
| 2026-11-02 | 26cb5cf4948ba1fc1184acd953f33b2dc6340fb6 | fix(#2214): a value-fault hazard is not observed through the silence row | packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-02 | 714095472dd1000b648aa4d45bcb0a693c931d26 | feat(phase-525 W1): the user surface names no internal id | cli-core/builder, cli-core/cmd, packages/cli/cargo-nano-ros +2 | yes | yes | no |
| 2026-11-02 | dc244aafd4c8fe992770bc4e2d1148b127770730 | chore(deps): ros-launch-manifest v0.3.2, play_launch 0.23.0 (ea18e049) | play_launch pin; packages/cli/Cargo.lock, packages/cli/nros-cli-core, packages/cli/nros-launch-resolve +1 | yes | yes | no |
| 2026-11-02 | ea032575ba0aecc22af736f5d1a0f27bd3ed5884 | fix(phase-527 W3): RunPlan prepare steps carry 'run' and a typed 'wait_for' | cli-core/cmd, packages/cli/nros-cli-core, packages/runners/nros-extension | yes | yes | no |
| 2026-11-02 | a3e1ecbd879c75f20543dd55a4b19289f810f44b | fix(issue 2216): the entry-root clean no longer stops a fresh fixture build | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-02 | ba0622ab4f9a8116e7d7a47bf632d733070fe697 | feat(phase-526 W3): colcon's flag names on build, list, sync and new | cli-core/builder, cli-core/cmd, packages/cli/nros-cli +1 | yes | yes | no |
| 2026-11-02 | ba6fbe1366fa7c771b5b0d99623d48f4f9f6c627 | feat(phase-526 W4): check-cli-old-spellings, and the tree on the new spellings | cli-core/cmd, examples/templates/multi-node-workspace, packages/cli/cargo-nano-ros | yes | yes | no |
| 2026-11-02 | d4279de7c2af36bcff78b1298e31dd1ab70c86ac | feat(phase-527 W2): the host build role in nros build | cli-core/builder, cli-core/cmd, packages/cli/cargo-nano-ros +1 | yes | yes | no |
| 2026-11-02 | 272980e134a4776597789c0a412128a2d7c70448 | feat(RFC-0117 D4): '--generated-base' on the configure verb (then still named sync) names the generated-crate directory | cli-core/cmd, cli-core/leaf_payload_classes | yes | yes | no |
| 2026-11-02 | c394daceade3806553982d00e97a094e68a7a512 | fix(issue 2211): check a Zephyr Rust image's triple against the board '-b' names | cli-core/cmd | yes | yes | no |
| 2026-11-03 | 1cfa392bb63a43215435cc7ebbd66976f93a8948 | feat(phase-521 W6)!: retire the ESP32-C3 bare-metal board (RFC-0113 D5) | cli-core/builder, cli-core/cmd, cli-core/codegen +7 | yes | yes | no |
| 2026-11-03 | 442aaf2379be3081e765e257c521f7d9cc23385a | feat(phase-521 W7): the Zephyr idle-wake witness, and the first RTOS XRCE row | cli-core/cmd | yes | no | no |
| 2026-11-03 | 766cb74a4d189c2c3561505e54cd29855322b20c | feat(phase-521 W8a): 'px4' joins FRAMEWORKS, with the rank-ordered work-queue table | cli-core/cmd, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-03 | 55a467ea518a0f7accd4d9bf587cd107b1e901b5 | fix(issue 2207): a zenoh client open follows a stated connect budget, not a test-sized retry loop | cli-core/cmd, packages/tooling/nros-platform-config, packages/tooling/nros-zephyr-build | yes | yes | no |
| 2026-11-03 | 443b86a3c3caeae3d09649f53274046196c6d786 | feat(phase-521 W8b): the generated PX4 entry, one work item per tier | cli-core/codegen | yes | yes | no |
| 2026-11-03 | 2652092a3faab647090801d89ca944dbc867309f | feat(phase-528 W1): the emulator NIC and a console command reach the RunPlan | cli-core/cmd, packages/cli/nros-cli-core, packages/runners/nros-extension | yes | yes | no |
| 2026-11-03 | 052707ed35dac94f8cd216e52767f22acf20f465 | feat(phase-521 W8d): the PX4 examples on the generated entry; uORB states Notify | cli-core/cmd, cli-core/codegen | yes | yes | no |
| 2026-11-03 | 30ea9fdf714a0894aadb27efe05f120e973e8c51 | feat(phase-521 W8e): PX4 SITL runs in CI; the uORB cell is Gated, not carved out | cli-core/cmd | yes | no | no |
| 2026-11-03 | 1fb544b9b5d5fbf4d6d990983235aa4288aa3b4b | feat(phase-528 W3): runner packages native, qemu, fvp, west, command + check-board-runner-resolves | cli-core/cmd, packages/cli/nros-cli-core, packages/runners/nros-extension | yes | yes | no |
| 2026-11-03 | b81f762df11fa2348ff550dab6e330685d3bb93c | feat(phase-521 W8e/f): D2 measured under PX4's work queues; PX4 admitted to Tier 1 | cli-core/codegen | yes | yes | no |
| 2026-11-03 | 8fdb89bd4a1def0cf013b05bc03555b4b5d7ed53 | feat(phase-528 W1): how a board runs has one source, the descriptor | cli-core/cmd | yes | yes | no |
| 2026-11-03 | 33ba61ad461b176e43db6938a7afdb816fd3688c | fix(issue 2229): resolve Zephyr board facts from NROS_IMAGE_SYSTEM_DIR | cli-core/cmd | yes | yes | no |
| 2026-11-03 | fb7b343159a974008944bc852bda0480aecf6fe6 | fix(#2215): a C++ subscription feeds its topic's age row from the stamp | packages/cli/rosidl-codegen | yes | yes | yes |
| 2026-11-03 | 79c3a00a2dbd1bb86780220808c3e0d638afe837 | chore(deps): ros-launch-manifest v0.3.3, the standard names | packages/cli/Cargo.lock, packages/cli/nros-cli-core, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-03 | 30bcf1c18aa0fbf6187260410434157e24668a5d | feat(phase-528 W4): nros run executes its RunPlan | cli-core/cmd, cli-core/lib, cli-core/target_session +3 | yes | yes | no |
| 2026-11-03 | 7f00ef2a78dd03d06aaba9299a7482128f455d46 | chore(phase-528 W4): record the session's three unsafe sites | cli-core/cmd | yes | file-only | no |
| 2026-11-03 | 792f6db1d8fe26f4746e28512bfd2963594e5053 | docs(phase-528 W4): the book and the CLI tests know nros run runs | packages/cli/nros-cli | yes | no | no |
| 2026-11-03 | 5d8775ce1d25872e16d777a7bc86183ca21397d0 | feat(phase-528 W4): console commands, the qemu serial pty, and --prefix on the host runner | cli-core/cmd | yes | yes | no |
| 2026-11-03 | ec1c588ad67ed58ce256db3482f43b8f36923047 | fix(phase-528 W4): the run builds what the plan named; a serial guest waits for its router | cli-core/cmd | yes | yes | no |
| 2026-11-03 | 479278c8435762f37cda7e2337be2cc8519ac057 | chore(phase-528 W4): format, and issue 2231's links both ways | cli-core/cmd | yes | yes | no |
| 2026-11-03 | 29fe1674e7f32121ee3baa19b01932204c642b56 | feat(phase-529 W2): the serial console runner and 'west flash'; espflash deferred | cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-03 | 3ae24e8063ad17e5f5be42b27336b861c46f94a1 | chore(deps): ros-launch-manifest v0.3.4, play_launch 0.24.0 (3a813cfe) | play_launch pin; packages/cli/Cargo.lock, packages/cli/nros-cli-core, packages/cli/nros-launch-resolve +1 | yes | yes | no |
| 2026-11-03 | bb522ac619ce601d905d2db2ee6111ae27e289fb | fix(issue 2227): NuttX build scripts watch the config header they read, not the shared tree's | packages/boards/nros-board-common, packages/tooling/nros-build-paths | yes | yes | no |
| 2026-11-03 | 01b8066de9d4b1a2a67652d404b66c4583383366 | feat(phase-526): --parallel-workers defaults to the CPU count, under one jobserver | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-03 | c701598c15b024311d47b37e960cbaf4f6e572a8 | feat(RFC-0117 D4): 'nros build -j N' is the job knob; '--parallel-workers' is refused | cli-core/builder, cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-03 | f0091554ce4f1fe038bd7a5e8029515bac198a0d | fix(issue 2224): board-facts refuses a named image it cannot resolve instead of panicking | cli-core/cmd | yes | yes | no |
| 2026-11-03 | 3ca9f1b54a5eb8478b18ca14d8db7a98272a47eb | feat(phase-527 W2b): declared workspace overrides for provider packages | cli-core/cmd, cli-core/lib, cli-core/orchestration +3 | yes | yes | no |
| 2026-11-03 | 13358e690b0d6ef4d74ed18271ea60356adb7774 | test(phase-527 W2b): declare the host-role fixture's 'native' override | packages/cli/nros-cli-core | yes | no | no |
| 2026-11-04 | bf40155e946afa777d43895e69b3f3879a5f4461 | chore(deps): ros-launch-manifest v0.3.5, play_launch 0.24.1 (3b863cea) | play_launch pin; packages/cli/Cargo.lock, packages/cli/nros-cli-core, packages/cli/nros-launch-resolve +1 | yes | yes | no |
| 2026-11-04 | b1eb25b1a8ed689ebc1953aa0e25b9d097e5a3c4 | docs(phase-529, phase-533): move the bench-only items to their own phase | cli-core/cmd | yes | file-only | no |
| 2026-11-04 | 9e278c2ca7b40ed08a37ab6410f8346d631d7e55 | feat(phase-529 W3): nros flash and nros monitor | cli-core/cmd, cli-core/lib, packages/cli/nros-cli +1 | yes | yes | no |
| 2026-11-04 | 68b3eec9d7895cf947c9d71a48763fa1db0d0575 | chore(phase-529 W3): select_target, and the monitor test's pty in the census | cli-core/cmd | yes | yes | no |
| 2026-11-04 | 9d00c1e5c5dd01ffe40f365d9921d4f04931a795 | feat(phase-528 W4): nros run resolves its runner under the override policy | cli-core/cmd | yes | yes | no |
| 2026-11-04 | 09fe37368cbbf3ae0a8f432616412614c78aa96e | feat(phase-524 W4c): a PlatformId and a Renode smoke cell for the Nucleo-F767ZI | cli-core/cmd | yes | yes | no |
| 2026-11-04 | 5ca2426de999a4b5de9b06f0e01dffa01b04f46a | feat(phase-527 W2c): source roots are peers; overrides live in nros.toml | cli-core/cmd, cli-core/lib, cli-core/orchestration +3 | yes | yes | no |
| 2026-11-04 | ec2a8c5bac669dad18e9d5cf34307d93ce6257d3 | feat(phase-527 W2c): the leaf system.toml fallback is one function and says where to move | cli-core/provider_overrides, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-04 | 40eb449b2198408c601de88f88b7f50bca687312 | test(phase-527 W2c): the host-role fixture declares its override in nros.toml | packages/cli/nros-cli-core | yes | no | no |
| 2026-11-04 | 8b9cf7c499e349fe3283529d09574e772cec4dc9 | feat(phase-525 W4): the first chapter runs a board over serial, timed | cli-core/cmd | yes | yes | no |
| 2026-11-04 | 9682c7830a893a606ea5c47168ae3e4be5b75554 | chore(deps): play_launch 0.24.2 | play_launch pin | yes | yes | no |
| 2026-11-04 | 08eaa7c5bcd7c4be7bf2a5a274273e7b2e60455c | feat(phase-527 W2d): --base-paths names the source roots, on every verb | cli-core/builder, cli-core/cmd, packages/cli/cargo-nano-ros +1 | yes | yes | no |
| 2026-11-04 | b19321815377e1522411f3a84ea373146243d7f8 | fix(issue 2219): nros build picks the jobserver form for the make on PATH, whatever drives the build | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-04 | fd2660c12b1f9d4f13fee860a6c71ae47538b1e8 | feat(cli): 'nros locate tool <name>' answers an SDK tool's directory | cli-core/cmd, packages/cli/nros-cli | yes | yes | no |
| 2026-11-04 | f285e038084995cd8f155eb94483ab0f8960c773 | chore(deps): ros-launch-manifest v0.3.8, play_launch 0.25.0 (phase 534 W1, W2, W4) | play_launch pin; packages/cli/Cargo.lock, packages/cli/nros-cli-core, packages/cli/nros-launch-resolve +1 | yes | yes | no |
| 2026-11-04 | 4530138b56a0e46486cb5cf228d8fb8cc7aaf397 | refactor(cli): one shared exclusive file lock (phase-530 W1) | cli-core/file_lock, cli-core/lib, cli-core/orchestration | yes | yes | no |
| 2026-11-04 | 8e87acb5ed73a0dbc5970fd19212769a0ee352bd | feat(cli): the configure stamp: input digest, recorded env (phase-530 W1) | cli-core/configure_stamp, cli-core/lib, cli-core/orchestration | yes | yes | no |
| 2026-11-04 | 92d9e7828c1661301c8b8498efe7e4d2cf076218 | fix(cli): configure stamp review fixes (phase-530 W1) | cli-core/configure_stamp, cli-core/orchestration | yes | yes | no |
| 2026-11-04 | a64f9065395db4bcbc26de410bb2f5ca23ab768f | feat(cli): nros configure, with sync as its alias (phase-530 W1, RFC-0120 D2) | cli-core/builder, cli-core/cmd, cli-core/configure_stamp +6 | yes | yes | no |
| 2026-11-04 | 52ea268ca34ebdbb057c9bcbf506210c11ea7815 | fix(cli): configure review fixes: leaf image switch, lock seed in C1 (phase-530 W1) | cli-core/cmd, cli-core/configure_stamp, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-04 | fcee31155c05754038b469232f16b391de4a004c | fix(cli): configure --offline reaches the lock prune; selection writer moves the marker (phase-530 W1) | cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-04 | 21800f60a4b4158c862694ab4666deeb8be82196 | feat(cli): nros build configures implicitly, like cargo (phase-530 W1, RFC-0120 D3) | cli-core/builder, cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-04 | 64e3575999940e3ab8eef573cd8851c82f1fdb60 | fix(cli): configure stamp review fixes: skip stamps, dry-run, ament, remedies (phase-530 W1) | cli-core/abi_guard, cli-core/cmd, cli-core/configure_stamp +7 | yes | yes | no |
| 2026-11-04 | 4c5b52d7d3add12b5c8503b3fd2d0aef3cbaff71 | fix(cli): a skip stamp re-checks its image's preflight (phase-530 W1, Ruling R19) | cli-core/cmd, cli-core/configure_stamp | yes | yes | no |
| 2026-11-04 | fb7a00ec7e4b3e79233bb11990b664961caecb66 | chore: sweep nros sync -> nros configure (phase-530 W1, check-cli-old-spellings --fix) | examples/templates/multi-node-workspace, examples/templates/multi-node-workspace-cpp | yes | no | no |
| 2026-11-04 | 9356ffb72f00a4f5a197c8c58771fa0d4cc9d2a4 | fix(cli): configure's census child no longer deadlocks on its lock (phase-530 W1, Ruling R20) | cli-core/cmd | yes | yes | no |
| 2026-11-04 | 188ce6c585963b588ca83497b878bb7af9b577fe | fix(cli): configure final review fixes (phase-530 W1, I1 I2 R22 R25) | cli-core/cmd, cli-core/configure_stamp, cli-core/file_lock +2 | yes | yes | no |
| 2026-11-04 | 17d689378b2ff44150bf09ae7227883d54b93304 | fix(cli): configure stamp: one search path, whole probe key, recorded env (phase-530 W1) | cli-core/cmd, cli-core/configure_stamp, cli-core/orchestration +1 | yes | yes | no |
| 2026-11-04 | 53581ee7a92993daf2d96fdaf40f318c5fc686ef | fix(phase-530 W5): a reader digests the search path configure sets | cli-core/configure_stamp | yes | yes | no |
| 2026-11-04 | 8ca3f926ada322ed6bbbe223275320e3a21eb599 | refactor(cli): one derivation of configure's default search path (phase-530 W1) | cli-core/cmd, cli-core/configure_stamp | yes | yes | no |
| 2026-11-04 | 82bbcbfcb52f45bd889124aa38ef22bdeba440c0 | fix(cli): configure round 2: dry runs write nothing, search path is an argument (phase-530 W1, R43 R44 R45) | cli-core/cmd, cli-core/configure_stamp, cli-core/contract_join +3 | yes | yes | no |
| 2026-11-04 | 24ca2c4fc557e8c1b595586a6fff82adc5cb8e1d | fix(cli): R43 follow-ups: a dry run names its step; tests pin the new rule (phase-530 W1) | cli-core/cmd, cli-core/configure_stamp, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-04 | be61d439606ac1c53f0d35845a0568abc1f44765 | fix(cli): configure meets main's run, Args and docs (phase-530 W1 rebase) | cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-04 | 86201196b41afa2d5f83579239afee6214743674 | fix(cli): follow main's source-root API after the rebase (phase-530 W1) | cli-core/cmd, cli-core/configure_stamp | yes | yes | no |
| 2026-11-04 | 04e98704c4f973e7b6c4a7cdf7129f9a674303aa | fix(#2253): a generated entry honours 'panic = "platform"' | cli-core/builder, cli-core/orchestration | yes | yes | no |
| 2026-11-04 | fa452e9e1e6693dd74fe44c839e1cacc8bb59e09 | feat(phase-527 W2e): a duplicate inside one source root is refused too | cli-core/builder | yes | yes | no |
| 2026-11-04 | 4c51ec760cf3ea6ee268569d8a140df9317294d7 | feat(phase-534 W3)!: rlm v0.3.9 (model schema 4); nano-ros takes the standard names | play_launch pin; cli-core/cmd, cli-core/codegen, cli-core/contract_join +11 | yes | yes | no |
| 2026-11-04 | e4800f0161e27550f05a87e14e0b6aef96489e9d | feat(phase-524 W4b): Nucleo-F767ZI runs in its Renode twin through 'nros run' | cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-11-04 | 41b94a1b52455a9c8c167cec39edf54e66ba82bf | feat(phase-527 W4): dispatch an unknown verb to its command package | cli-core/builder, cli-core/builtin_verbs, cli-core/cmd +2 | yes | yes | no |
| 2026-11-04 | 838346e779e3130e23a4045a15a6e7319574f6ff | feat(phase-535): the C++ workspace packages run the ROS 2 tutorial programs | packages/cli/nros-cli-core | yes | no | no |
| 2026-11-04 | cdecce55b5093ba15eec1d29737e78cf19399e55 | feat(phase-527 W6): the release underlay - prebuilt runner packages in share/nros/packages | cli-core/builder, cli-core/cmd, cli-core/orchestration +3 | yes | yes | no |
| 2026-11-04 | 89112a349ec2bc126e6d5614ed6328bd067d60ef | test(cli): list shipping contracts from the index, not the disk (issue 1565 class) | packages/cli/nros-cli-core | yes | no | no |
| 2026-11-05 | 35494e2a4a989f0d9f29423f354e2cac15be7736 | docs(phase-532): two books by audience; the user book teaches the workspace | cli-core/cmd, cli-core/orchestration, packages/cli/cargo-nano-ros +3 | yes | yes | no |
| 2026-11-05 | aa23fc52178f349e9a6271a9a361f0e1153bd7c1 | feat(phase-531 W2): system.toml only in a bringup - two ratchets and a sync warning | cli-core/cmd | yes | yes | no |
| 2026-11-05 | 548ee74edd49d1c267f172ea7fa5b052cee8b07c | feat(sched): system.toml writes tasks, rlm v0.3.10 (phase 536 W1) | play_launch pin; cli-core/builder, cli-core/cmd, cli-core/codegen +8 | yes | yes | no |
| 2026-11-05 | e4091ec23aace488629a86d01daab4a4fd62dbf6 | feat(phase-527 W5): nros router and nros px4 become command packages | cli-core/builder, cli-core/cmd, cli-core/lib +2 | yes | yes | no |
| 2026-11-05 | d1e5cca62015340799e9209d2c73bd8b183181b6 | feat(sched): model schema 5, tiers refused, rlm v0.3.11 (phase 536 W3) | play_launch pin; cli-core/builder, cli-core/cmd, cli-core/codegen +7 | yes | yes | no |
| 2026-11-05 | 66b9272b5ab2eefdd037977d6a014cd28988f60e | fix(issue 2280): the metadata probe skips a Zephyr west application instead of configuring it for the host | cli-core/builder, cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-11-05 | 2a968b951b72989e4cab8265a4b3016ea02ec7bc | chore(deps): rlm v0.3.12, play_launch 0.29.0 (phase 536, rlm R+2) | play_launch pin; packages/cli/Cargo.lock, packages/cli/nros-cli-core, packages/cli/nros-launch-resolve +1 | yes | yes | no |
| 2026-11-05 | 017706c54139b44300360b6a3dd6a35ae1bb7a32 | docs(book): move RTIC to Internals; drop it from the user book | cli-core/cmd, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-05 | 76abc1bbb82f7375ed226e323b7a27853a5f5a15 | feat(phase-527 W7): runner and command scaffolds, and their checks | cli-core/builder, cli-core/cmd, cli-core/lib +4 | yes | yes | no |
| 2026-11-05 | f890d2c3f0ea76864bd03082d8b3d68d0f518c50 | fix(issue 2281): a Zephyr C/C++ image stops relinking on every nros build / nros run | cli-core/builder, cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-05 | 5a0856da51fc1a71c4aaddd59e3239f1c22643ee | fix(#2288): preflight asks rustup from the workspace it builds | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-05 | 6e1f411889d81a0d4363794611b8683ecbfd0200 | docs(phase-536 W0): the tier rename table; task record -> callback record | cli-core/cmd, cli-core/codegen, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-05 | b719d169edd95da680924ed6458335caa746a08e | feat(phase-531 W3/W4): nros new writes a workspace; nros run <node> | cli-core/builder, cli-core/cmd, cli-core/model_gate +5 | yes | yes | no |
| 2026-11-05 | a0669c42197fdceb7f30d3b00e1a8dad7e45e248 | feat(phase-536 W2)!: a scheduling tier is a sched task in nano-ros's own names | cli-core/builder, cli-core/cmd, cli-core/codegen +9 | yes | yes | no |
| 2026-11-06 | 405348d51be637e61342a32f12f0ada3a5b9950e | fix(issue 2282): nros new's next steps start the image the way the board says | cli-core/cmd | yes | yes | no |
| 2026-11-06 | b273012e57d56a39e35a4e9b499841e7e03c39e2 | feat(phase-531 W5): the generated entry states its own selection | cli-core/builder, cli-core/cmd, cli-core/orchestration +1 | yes | yes | no |
| 2026-11-06 | 05bf8151fdbd394be3f45c999d6b497be801dc8d | fix(phase-528 W5): nros run writes the console's last bytes before it exits | cli-core/cmd | yes | yes | no |
| 2026-11-06 | 80f746c223da817bc43c5bff12de749730963cf7 | perf(phase-528 W5): a runner lookup walks only the in-tree runner packages | cli-core/cmd, packages/cli/cargo-nano-ros | yes | yes | no |
| 2026-11-06 | eeeb279a6e1a07ccd1eb42905189625a0c533981 | perf(phase-528 W5): a session's teardown answers an empty group in one syscall | cli-core/target_session | yes | yes | no |
| 2026-11-06 | f0713402a898fa380f5efca64e327cb171d934b7 | fix(issue 2260): scaffold the sourced ROS 2 edition; nros run warns on a mismatch | cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-06 | 4f8aaa58f7d8bd721907e6fd165c3c1be7f7cc80 | fix(issue 2254): a new nros build jobserver retires the orphaned FIFO dirs | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-06 | f57516b629d97860300b1001156001dc2438decb | docs(phase-536 W4)!: say sched task in records, messages and docs; archive phase 536 | cli-core/builder, cli-core/cmd, cli-core/codegen +14 | yes | yes | yes |
| 2026-11-06 | 7e8a1121d9e1b3f96e881d42b0dfe3b4a701a674 | feat(phase-535 W3 pilot): freertos example pairs as workspace images | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-06 | a75a6682ae2db9d79cb0471ea0180e9329234266 | test(phase-535 W3 pilot): the selection-package reach floor follows retired leaves | cli-core/cmd | yes | file-only | no |
| 2026-11-06 | 2718d95a4dbd606fa54ae7b9463293284b6568dd | fix(phase-531 W4): a workspace's C/C++ node package names no DEPLOY | cli-core/builder, cli-core/cmd, packages/cli/cargo-nano-ros | yes | yes | no |
| 2026-11-06 | 306ab92280cc39009d101b789a4cac346e89285d | feat(probe): keep awaits only where a CLI fix is still in flight | cli-core/cmd | yes | no | no |
| 2026-11-06 | 1ca8b3cc0ad886e2064c5447f90d40208ac1f00f | fix(cli): a package-only leaf's configure stamp sees a deleted generated/ crate | cli-core/configure_stamp | yes | yes | no |
| 2026-11-06 | 97f06ea817afd4ed0f62cc283ae3a01c9bb17a9e | fix(issue 2257): a released nros reads the SDK index it ships with | cli-core/cmd | yes | yes | no |
| 2026-11-06 | 12be94da53611e350315121fe301b7cf44364af4 | docs(book, phase-537 W1): move the Nucleo-F767ZI page to starters/ | cli-core/cmd | yes | file-only | no |
| 2026-11-06 | e93b9a1dc43c7986165ed2a4726a42a558eb707b | docs(book, phase-537 W1): move the Tutorial pages | cli-core/cmd, packages/cli/cargo-nano-ros | yes | file-only | no |
| 2026-11-06 | b804542c1cdb94b57d1eb146548e032f313337ae | docs(book, phase-537 W1): move the User guide pages into their task groups | cli-core/cmd, packages/cli/cargo-nano-ros, packages/core/nros-rmw | yes | yes | no |
| 2026-11-06 | 6b3fbf9c000920e7e280dc34d4d36f0d7f2689bc | docs(book, phase-537 W1): move the Background pages | packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-06 | 9cd445279dc62a12f09b9f83d56175afad42b5c6 | docs(book, phase-537 W1): move implementer pages and Arm FVP to Internals | cli-core/cmd, packages/core/nros-rmw | yes | yes | no |
| 2026-11-06 | c178637b71a9d9f337e5dcfb9bd5ad5a6f40a993 | docs(boards): point the offline probe's note at the moved page | cli-core/cmd | yes | no | no |
| 2026-11-06 | 3d12406d1ac5a6f1c7f0c438fdd09f9161921b56 | style(cli): rustfmt the scaffold test's moved book paths | packages/cli/cargo-nano-ros | yes | no | no |
| 2026-11-06 | a5fc15ebbd6317f44856d66871318c85a92022e2 | style(cli): rustfmt the other two moved book paths | cli-core/cmd | yes | yes | no |
| 2026-11-06 | 0676fd2cf3ca450306f728d282f025f6f8adef5d | fix(issue 2296): nros configure's stamp records the resolver pin | cli-core/cmd, cli-core/configure_stamp | yes | yes | no |
| 2026-11-06 | edc0fb85014f24e410d02314f7daf87896cba9e8 | feat(probe): drop the bug-1 awaits; the C scaffold names its DEPLOY | cli-core/cmd | yes | no | no |
| 2026-11-06 | 2feb7c5bcb317555ee8758060294a119e78bd64b | feat(issue 2197): 'nros setup --offline-dirs' names what an air-gapped copy needs | cli-core/cmd | yes | yes | no |
| 2026-11-06 | 39543aafea489043da6237a42d726be0ea97b1a1 | fix(zephyr): set up a C-only west workspace, and keep a relative workspace path | cli-core/cmd | yes | yes | no |
| 2026-11-06 | a7fe82012beb0a3fbb1f6f94013f0f873f8bfa23 | feat(phase-528 W5): QEMU cells launch through 'nros run --plan' | packages/tooling/nros-qemu-ladder | yes | yes | no |
| 2026-11-06 | 8cf3ef68219e954c603645bc97f00722b0af319f | chore(issue 2283)!: Wokwi is no twin; the Pico W has none | cli-core/cmd, cli-core/orchestration, packages/cli/nros-cli-core +3 | yes | yes | no |
| 2026-11-06 | 2ab9e9ba065b109db5caffd3f74ab280549c8602 | test(issue 2283): the scaffold reach check asks only of a board a leaf names | cli-core/cmd | yes | file-only | no |
| 2026-11-06 | ba2a85058c6d8bed330716ff810ea9a65a2078f8 | docs(book): rewrite The execution model | packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-06 | 6089cbe3cc43d1911bffdda4f588c23e58ab2c4e | fix(#2298): a workspace image sizes its take buffer and payload classes | cli-core/cmd, cli-core/leaf_payload_classes, cli-core/leaf_take_buffer | yes | yes | no |
| 2026-11-06 | baba1ea9115153a04ceed4599476a1de4f903d06 | docs(book): generate the CLI reference from the CLI's own help | cli-core/cmd, packages/cli/nros-cli | yes | yes | no |
| 2026-11-06 | 091bc5c71dd0a8b0f20ada7f7f77678ddb10f2cc | fix(cli): name nros configure's --nano-ros-root value DIR | cli-core/cmd | yes | yes | no |
| 2026-11-06 | f6957c899c49a0f7943236863843d53283b9b9ef | feat(phase-535 W4): linux example pairs as workspace images, all three RMWs | packages/cli/nros-cli-core | yes | no | no |
| 2026-11-06 | 043570afd58f15bdd64046381ae7bc22937e897e | fix(zephyr): a board that IS the network turns the network stack on | cli-core/cmd | yes | file-only | no |
| 2026-11-06 | 0ede1760c2c3fd3e2bc40644be3ccbfe0c7fa02b | fix(issue 2270): a scaffolded talker builds quietly | cli-core/builder, cli-core/cmd, cli-core/codegen +12 | yes | yes | no |
| 2026-11-06 | 3b1f5639d2b294c365987ddefeb792cb65ce334f | docs(book): merge the board pages into Add a board | cli-core/cmd, packages/core/nros-orchestration-ir | yes | file-only | no |
| 2026-11-06 | 57c28913af4581ca4f30ee9945553b0523c1978a | fix(issue 2293): nros config explain finds the workspace's platform packages | cli-core/cmd, packages/cli/nros-cli, packages/tooling/nros-platform-config | yes | yes | no |
| 2026-11-06 | 8aaa6fe4c19f85b9437d9f6aa8461e4e2b8ec5e1 | style(cli): rustfmt the issue-2293 test's imports | packages/cli/nros-cli | yes | no | no |
| 2026-11-06 | e56958f343f032ce08f2f9003f3a50054e154817 | fix(issue 2290): each Zephyr board's package states its own link | cli-core/cmd | yes | file-only | no |
| 2026-11-06 | f091f01d904482a4a0d323d705d0d09b63aeb179 | fix(issue 2297): a workspace configure stamp sees a deleted generated/ crate | cli-core/cmd, cli-core/configure_stamp | yes | yes | no |
| 2026-11-06 | fb48b572ad060f8737ca373d819261fe731a8c9c | fix(issue 2279): the unsourced-ROS refusal names the verb the reader ran | cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-06 | 0a684715bc38bb6fc64a769637e1c42a6ca2c84f | fix(issue 2311): a flash dry run prints its plan when the port does not resolve | cli-core/cmd | yes | no | no |
| 2026-11-06 | 2af1ee8515ef1887f926a19bf3b68965558b0b7d | feat(phase-535 W4): threadx-linux example pairs as workspace images | cli-core/cmd | yes | file-only | no |
| 2026-11-06 | 94e3cbf872848b3a6b753e86c90c08df01ccb049 | fix(issue 2291): nros board new --out-of-tree needs no checkout | cli-core/cmd, packages/cli/nros-cli | yes | yes | no |
| 2026-11-06 | 74677a911530795fc10b616582fd7284dde17947 | fix(issue 2292): nros new board --for-platform calls the one board scaffolder | cli-core/cmd, packages/cli/nros-cli | yes | yes | no |
| 2026-11-06 | a2b05c14757f7dcbe1e62779e9e99b544ab5f16a | fix(issue 2292): an out-of-tree board's next steps name no just recipe | cli-core/cmd, packages/cli/nros-cli | yes | yes | no |
| 2026-11-06 | aea7e1cad0b878273bf2c2b36a829e37d2137deb | fix(issue 2185): a west workspace you made gets the patch set, and an unpatched one is refused | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-06 | 6fa911082d3f3af3aa95c5d6a85ceeece4bf004f | fix(census): an action declared by its 'actions:' wiring is declared | cli-core/entity_census | yes | yes | no |
| 2026-11-06 | 9c4f11fa7690368980b99b2aeb6ec3ceeea7df35 | docs(issues): file 2304, 2305 and 2312 from the phase-537 W5 book session and renode replay | cli-core/entity_census | yes | file-only | no |
| 2026-11-06 | 71d39cb53559c1d6e712461fb19cc7cc3e9d7d5f | feat(phase-531 W6): nros dev migrate-bringup, the RFC-0121 D8 one-shot | cli-core/cmd | yes | yes | no |
| 2026-11-06 | 0de03a994ef0f4938fe12c6fdf7133f6d026eb3b | fix(build): a message package beside a Rust node is no cmake node | cli-core/builder, cli-core/cmd, cli-core/orchestration +1 | yes | yes | no |
| 2026-11-07 | c01f64b809c20b53f73b423c17dd2a2a4f373075 | refactor(cli): retire the manifest [deploy.*] table and the self-bringup it made (phase-531 W7 step 1) | cli-core/cmd, cli-core/orchestration, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-07 | cf88308afbb543628e23af42b5f575a9af0a7051 | feat(phase-535 W4): zephyr native_sim example pairs as workspace images | cli-core/entity_census | yes | yes | no |
| 2026-11-07 | 16bc1646f9922d37b69631f0ffe2abed35125529 | feat(issue 2278): the CMake road refuses the bundled interfaces, as configure does | packages/cli/cargo-nano-ros | yes | yes | no |
| 2026-11-07 | 474a9a6750065d3df0a32f4ab26d263fb908a113 | feat(issue 2294): one user-tier 'nros check <package>' for runner, command and board packages | cli-core/cmd, packages/cli/nros-cli, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-07 | 315abd4e146a69a2a792cf78ec96a780a5a5b90b | feat(registry): PX4 SITL gets a board-support row, Boots manual | cli-core/cmd | yes | no | no |
| 2026-11-07 | 663ac49d940e247c292a5133da634804078157b2 | feat(probe): a 'px4' installed-track probe track replays targets/px4.md | cli-core/cmd | yes | no | no |
| 2026-11-07 | c46db4a32c38322f280fb77b61b8e65ec0558ba1 | feat(probe): the px4 track awaits issue 2289 | cli-core/cmd | yes | no | no |
| 2026-11-07 | 9da2183cbc5755b9f067aacba5a4440bc25a0cff | feat(probe): the px4 track drops its issue-2289 awaits | cli-core/cmd | yes | no | no |
| 2026-11-07 | 080697bd7ef3bc2f2e56a8751da978337a016753 | fix(registry): the px4 row and track follow the PX4 page to guide/platforms/ | cli-core/cmd | yes | no | no |
| 2026-11-07 | 5e565e738086f1020e4d3e413b5882dedd813a34 | feat(registry): an external row states its flavour; PX4 reads Boots, manual (phase-542 W0) | cli-core/cmd | yes | no | no |
| 2026-11-07 | fd6f9594d38547bea1dbf50fbb632d36eea892fc | fix(issue 2287): the cmake build dir names the bringup | cli-core/cmd, examples/templates/multi-node-workspace-cpp, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-07 | 72a1c217f4ce9fba505eb97c31a97207d6f679a9 | fix(issue 2312): the Zephyr C road's nightly is read, installed and checked | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-07 | 9c5113d0106d106052f2d903708b093120aad07b | fix(issue 2317): a release trusts its probe sidecars; the talker contract is complete | cli-core/cmd | yes | yes | no |
| 2026-11-07 | 564ba458408025a9d522ebf9c0b78881b620b3a0 | feat(phase-545 W1): resolve every CLI rmw name through the provider scan | cli-core/builder, cli-core/cmd, cli-core/codegen +2 | yes | yes | no |
| 2026-11-07 | 0f21b8ff418a590fb35acb20e1415f63ba9bb3c4 | docs(phase-545 W1): strike the deleted rmw helpers as history | packages/cli/cargo-nano-ros | yes | file-only | no |
| 2026-11-07 | fbf8542cfe3ff38f6075647f5db9d682ef468de9 | docs(book): put the Nucleo-F767ZI starter on the starter template | cli-core/cmd | yes | no | no |
| 2026-11-07 | bf3275515369bf0042c9c9c4c109209a28bf994a | docs(book): add the mps2-an385 bare-metal starter | cli-core/cmd | yes | no | no |
| 2026-11-07 | 73a1669fe2293fe36797217d91de4ab4d3bcdd04 | docs(book): add the mps2-an385 FreeRTOS starter | cli-core/cmd | yes | no | no |
| 2026-11-07 | 09e45116f05e02d25177f7e0bdd7d73cbeed629f | docs(book): add the mps2-an385 Zephyr starter | cli-core/cmd | yes | no | no |
| 2026-11-07 | ced037abb37f8042b82bf760d195ff9b8622301a | docs(book): add the NuttX in QEMU (Arm) starter | cli-core/cmd | yes | no | no |
| 2026-11-07 | 08e685f461857c1549994d6454f29cebf9ed17d3 | docs(book): add the ThreadX in QEMU (RISC-V 64) starter | cli-core/cmd | yes | no | no |
| 2026-11-07 | edde4d8ae602788f96a51fa54dc427e404276911 | docs(book): add the Linux host starter | cli-core/cmd | yes | no | no |
| 2026-11-07 | 6ba9f330686e3823e2cc8ed4b9ca6df4183830ec | docs(book): add the Zephyr native_sim starter | cli-core/cmd | yes | no | no |
| 2026-11-07 | ada807e7b1a559843d666c7d500c3fd610154567 | docs(book): add the FreeRTOS POSIX starter | cli-core/cmd | yes | no | no |
| 2026-11-07 | ff953a966e14cff4eea24ce9ae323361d00c7d19 | test(cli): read the boot lines and the Rust module pin from the starters | cli-core/cmd | yes | file-only | no |
| 2026-11-07 | f994353b65c94799a64bbccf735bb7073d93f163 | style(cli): rustfmt the printed-run-command test's twin skip | cli-core/cmd | yes | file-only | no |
| 2026-11-07 | 11a7bdd17844314bfd920ad548da7ec22916cf90 | feat(probe): the three new starter tracks name what they wait on | cli-core/cmd | yes | no | no |
| 2026-11-07 | 4321232c393c2670661e13eac0c8e90284bd6564 | docs(book): Tutorial 1, run a talker on an emulated board | cli-core/cmd | yes | no | no |
| 2026-11-07 | 3ae2ba1bd4365649072ff6d9d632cfc2bf73283f | docs(book): Tutorial 2, what you just built | cli-core/cmd | yes | no | no |
| 2026-11-07 | 22d4319f9d17111959be1425e079b141a0fa3eea | docs(book): Tutorial 3, add a second node, in three languages | cli-core/cmd | yes | no | no |
| 2026-11-07 | 0a9db00705561cb84e4e57e120e3f4fcc3883832 | docs(book): Tutorial 4, pick and configure the RMW | cli-core/cmd | yes | no | no |
| 2026-11-07 | 88320119dec8ff9b4fdebdbbb4f126fe1d59b82c | docs(book): Tutorial 5, your own message | cli-core/cmd | yes | no | no |
| 2026-11-07 | c9f2ef0045a18514e5a76eebe7d36ce8486c7079 | docs(book): Tutorial 6, services and actions | cli-core/cmd | yes | no | no |
| 2026-11-07 | 5cac4e20c22e9166f81c44ce72264e6994ba1172 | docs(book): Tutorial 7, parameters and launch files | cli-core/cmd | yes | no | no |
| 2026-11-07 | f987338fc55b53c87ecabfeff3736cf051ec5895 | docs(book): Tutorial 8, tell the build what your system creates | cli-core/cmd | yes | no | no |
| 2026-11-07 | ddf76dd79d70c00724db00ee7d4f262777868cec | docs(book): Tutorial 9, same project, another target | cli-core/cmd | yes | no | no |
| 2026-11-07 | 5c3c028eafea7ee915252f9f7f82a1f65fda94a5 | docs(book): Tutorial 10, talk to ROS 2 | cli-core/cmd | yes | no | no |
| 2026-11-07 | 22ba10d94cbb94ec4fbfcd01f4703fe47e6d72ed | feat(probe): Tutorial tracks for the C and C++ tabs | cli-core/cmd | yes | no | no |
| 2026-11-07 | e018d4e4dfcc1c05ea521867a666f10b44ecad98 | docs(book): Platforms, one page per platform, grouped by status | cli-core/cmd | yes | no | no |
| 2026-11-07 | f156301ced59117f766a345c384b33c34e70f0cf | feat(probe): tracks for the platform pages; the west app page in tabs | cli-core/cmd | yes | no | no |
| 2026-11-07 | 843bcd83aa7776d60c3ec1af960f2d09f0b4261e | feat(probe): bench tracks for the starters' flash steps | cli-core/cmd | yes | no | no |
| 2026-11-07 | ea2591c06656d9a951d1f0ee004493e63863164e | docs(book): follow main under the starters - awaits and sched tasks | cli-core/cmd | yes | no | no |
| 2026-11-07 | 9af6630ed2121adfeeac16967caee6bec14183a2 | fix(registry): the C starter tracks keep their starter chapters; bug 1 is fixed | cli-core/cmd | yes | no | no |
| 2026-11-07 | aa40a21ed168c6f27344ba09c2c4c0b48cab2854 | docs(book): the Pico W leaves the user book, following main's issue 2283 | cli-core/cmd | yes | no | no |
| 2026-11-07 | 61d8e499784c406c14a848e5040a8aae23a906ad | docs(book): carry phase-523's book stops onto the reorganized pages | cli-core/cmd | yes | no | no |
| 2026-11-07 | 343a47572c39c4e04b3efbf5c360df9fdd2c4642 | fix(cli): the starters test drops its runnerless-twin skip | cli-core/cmd | yes | file-only | no |
| 2026-11-07 | 28f665b1a62f560893fb91b10d03de39630cb339 | fix(cli): the starters test stops naming the Pico W starter | cli-core/cmd | yes | file-only | no |
| 2026-11-07 | d476aa3a457c24b70b24eb56d5b014af1f0be8d4 | test(cli): the job-overlap test waits for the overlap instead of sleeping | cli-core/cmd | yes | yes | no |
| 2026-11-07 | 3b34e42998c2f8adf6f81285be67144170c1740d | refactor(cli): retire nros plan's synthesised one-node model (phase-531 W7 step 2) | cli-core/cmd, cli-core/orchestration, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-07 | 287321ceab6878fe7263d59ba206361898fb2581 | feat(phase-545 W2): link a backend the board crate does not name | cli-core/builder, cli-core/orchestration, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-07 | bbb9b19bc1726628a3c41921ae1878b71ae1821d | fix(issue 2341): the FreeRTOS POSIX board states its port for cargo | cli-core/cmd | yes | file-only | no |
| 2026-11-07 | c3015e3685af9850487211db64b95ef885f04f5f | feat(phase-535): threadx-linux C++ Cyclone service and action as images | cli-core/cmd | yes | no | no |
| 2026-11-07 | d7b0e6932f17116e93995cd9663f427aaa84856a | fix(issue 2343): a build retires the pre-2287 cmake root of its configuration | cli-core/builder, cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-07 | 50b443bd3f63d8be6d4d53af7522a1a47751b71e | fix(cli): the legacy-root messages name no issue id | cli-core/cmd | yes | yes | no |
| 2026-11-07 | 0f0d00b574a68f6cd546ca83ec08f5e961eb269d | fix(issue 2330): add the ROS 2 apt source in Install, at a pinned version | cli-core/cmd | yes | no | no |
| 2026-11-07 | 8f67a95429ea51c7001cba155223e4acf54f207a | docs(book): FreeRTOS POSIX starts on Cyclone DDS until zenoh builds there | cli-core/cmd | yes | no | no |
| 2026-11-07 | 8e0d9fc44497960a7a847c55a7e5ee1d21f7a47c | fix(issue 2323): carry ROS 2 and the base packages in the offline .deb recipe | cli-core/cmd | yes | no | no |
| 2026-11-07 | a2410ebd9877f687549ef0f4aedf71a3df39d56d | docs(book): the FreeRTOS POSIX starter shows no zenoh step that fails today | cli-core/cmd | yes | no | no |
| 2026-11-07 | 1d3d1857ef3ad5b4ae15a933d39b5be6b68bf4ec | feat(RFC-0121 D9): '[image.<id>] node_features' turns on a Rust node's cargo features | cli-core/builder, cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-11-07 | ee619fc09b16cb6b42355605891f0b691d85be95 | fix(issue 0476): every test stub goes through write_executable_stub; gate it | cli-core/cmd, packages/cli/nros-cli | yes | file-only | no |
| 2026-11-07 | fd0a7f306c774fc4819fd42d2f640a506deaa232 | fix(issue 2320): a workspace syncs its messages for its bringups' edition | cli-core/cmd | yes | yes | no |
| 2026-11-07 | 59b8b5e514d8bb256b2bdfddd39bfdca761907f0 | feat(phase-523 W3): release provenance, policy and memory evidence (issues 2321, 2325, 2326, 2328, 2329) | cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-11-07 | 5e0c35e5b1d92e2e86ef2c78b73107baa23982e5 | probe: drop the message-package awaits from Tutorial 5-10 | cli-core/cmd | yes | no | no |
| 2026-11-07 | 53dd4f71fba7d04148c7c6afb3b17444b8a49e15 | docs(issue 2350): nros run --dry-run refuses the stale model; Tutorial 9-10 await it | cli-core/cmd | yes | no | no |
| 2026-11-07 | fe1cb8ebc46a5c085488ae125a83de4a5062714b | feat(phase-535 W5): threadx-riscv64 example pairs as workspace images | cli-core/orchestration | yes | yes | no |
| 2026-11-07 | 933dd5da2b2e0a5c1de709c74415f0c5c88da6ed | fix(#2314): a west-built Rust entry is told its workspace on the configure line | cli-core/builder, cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-07 | edbc047932b28eacb99f708242c00f8b4f2740ed | feat(phase-546, issue 2342): zenoh on freertos-posix through a host-socket FreeRTOS arm | cli-core/cmd, packages/tooling/nros-platform-config | yes | yes | no |
| 2026-11-07 | 4e6f01eb76985a42ba52248b0b5b1c665bdb6fa7 | chore(phase-546): regenerate the board-support views on current main | cli-core/cmd | yes | no | no |
| 2026-11-07 | d4839c2a1155decd039df222d08a6f7b551fc461 | fix(phase-548 W3): the NuttX cross compiler is the store pin, with a C++ GCC floor | packages/boards/nros-board-common | yes | yes | no |
| 2026-11-07 | 27beff735835f03c5b1592b9bb5c38748f91cc3d | fix(phase-548 W3): the module doc's two links name their path | packages/boards/nros-board-common | yes | file-only | no |
| 2026-11-07 | 3f49b54a6d4bce7afa45421e5964a14420ca1585 | feat(phase-548 W1): Rust on the FreeRTOS POSIX board (cyclonedds) | cli-core/builder, cli-core/cmd, cli-core/codegen +2 | yes | yes | no |
| 2026-11-07 | de94abdc041114b99001394178aaf9000de1a4a0 | fix(phase-548 W2): a generated Zephyr C++ application states C++17 | cli-core/builder, cli-core/codegen | yes | yes | no |
| 2026-11-07 | 649c9d0ea255c2c2581433500fee2be0250e6934 | feat(phase-548 W6): 'nros build' reconfigures a build dir whose cross compiler changed | cli-core/cmd | yes | yes | no |
| 2026-11-07 | 4acd030b5bc278886bec80c2190d98fc18e69438 | fix(issue 2345): one cmake spelling of a platform kind; bare metal is 'baremetal' | cli-core/builder, cli-core/orchestration | yes | yes | no |
| 2026-11-07 | 9fba92da4df71e6979466f15ab887a9bf93d1f40 | fix(phase-548 W1): a host-stack board dials its router on loopback | packages/boards/nros-board-common | yes | yes | no |
| 2026-11-07 | bc1c8cfe56df243853746c9ae31e677e1b55a784 | fix(issue 2348): a Rust declaration states its buffered row, so a Cyclone listener is not sized at its type's bound | cli-core/sizing_descriptor | yes | yes | no |
| 2026-11-07 | 0043b1f1622f88d9a7eca6a293daf220551b6fb7 | fix(cli): nros version prints the release version (issue 2334) | cli-core/cmd | yes | yes | no |
| 2026-11-07 | 3740071f1f2680934c513f346e15e411266e5bfa | fix(cli): name NROS_OFFLINE=1 on a DNS failure (issue 2324) | cli-core/orchestration | yes | yes | no |
| 2026-11-07 | 84ed04d0eef35a580d950ffd9f0f2590239703ca | fix(cli): setup outside a project locks in the store (issue 2333) | cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-11-07 | 8d0a5e3ab8b7fbd01f543a6bb3be4e9ac39adfe9 | fix(cli): a clean build keeps its detail behind -v (issue 2332) | cli-core/build_notes, cli-core/cmd, cli-core/leaf_entity_env +6 | yes | yes | no |
| 2026-11-07 | c671d0ef967f66d9adb19c812287342022c2286a | test(phase-548 W7): the Nucleo's runtime needs name thumbv7em-none-eabi | cli-core/builder, cli-core/cmd | yes | file-only | no |
| 2026-11-07 | 5f6861b5306149eed7742e40a48de6c9b1f32497 | fix(phase-531 W6): a generated Rust west application builds a Cyclone image | cli-core/builder, cli-core/cmd, cli-core/leaf_payload_classes | yes | yes | no |
| 2026-11-07 | 29ebce927fd7fdfc6dba78d5c8f11e8bea2a92ef | fix(issue 2350): a dry run reads a model its configure would re-resolve | cli-core/cmd, cli-core/model_gate, packages/cli/nros-cli | yes | yes | no |
| 2026-11-07 | bddf7d9bc0d178ad8318222036705049f7b9afa3 | fix(phase-548 W5): setup outside a checkout reads its own SDK root's index | cli-core/cmd | yes | yes | no |
| 2026-11-07 | 9c52ae9e349955fef06501963d61ebc63cf0629c | feat(run): an emulator image's router default is derived from its board, by one rule | cli-core/cmd, packages/boards/nros-board-common | yes | yes | no |
| 2026-11-08 | 6e35469bdecb5061ad495ecf0bf122a8029d0998 | fix(phase-531 W6): migrate-bringup marks the workspace and places a west prj.conf | cli-core/cmd | yes | yes | no |
| 2026-11-09 | b2850bdecf23ba6e9aa0a91420f00612b61b90ea | fix(issue 2344): 'nros run''s router launcher uses the same sourced test | cli-core/cmd | yes | yes | no |
| 2026-11-09 | 750022ba54a13599722f65c984cb735e258a2f02 | fix(phase-548 W10): 'nros run --expect' lets the matching line finish | cli-core/target_session | yes | yes | no |
| 2026-11-09 | 1735241b95d18e839ffe44ce24b499ed9d711b03 | fix(issue 2361): a stale probe sidecar stays a configure input | cli-core/configure_stamp, cli-core/contract_join, cli-core/orchestration | yes | yes | no |
| 2026-11-09 | 29e329219888501360293ce4f7f1d9f5eb33e463 | fix(phase-548 W9): rust-lld and llvm-ar come from the pinned toolchain, and setup provisions the pin | cli-core/cmd | yes | yes | no |
| 2026-11-09 | 979636e6c8b2f04ba215ebc3457cec733aab923c | fix(issue 2315): 'nros run --router auto' reuses a zenoh router already on the locator | cli-core/cmd | yes | yes | no |
| 2026-11-09 | a2d317cb536132c7ff104edb88326f2cb078c714 | test(issue 2315): the router-port tests never claim a released port is free | cli-core/cmd | yes | yes | no |
| 2026-11-09 | 8246ad41aa40dfa58430be41e946d5d13452cce7 | fix(issue 2358): nros setup board prints a west entry that fetches zephyr-lang-rust | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-09 | e45ba738511ac82789d8f695c305cebe30bb278f | test(issue 2358): the third-tree Zephyr fixture carries zephyr-lang-rust | packages/cli/nros-cli-core | yes | no | no |
| 2026-11-09 | c2fc899e67fae48c3c195c9a093a3c7131a7dc15 | fix(issue 2308): a Zephyr source tree without .west/ is not a west workspace | cli-core/cmd | yes | yes | no |
| 2026-11-09 | a0565954933d06a3c59e7211315a1fb32322c5d2 | fix(phase-544 W1, issue 2172): the harness reads each QEMU board's boot from [board.run.qemu] | cli-core/cmd, cli-core/orchestration, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-09 | f36df7e36d2575a0b9c6f6f00c341b8a83d1e17e | feat(phase-530 W2): 'entities = "defaults"' parses, the census opt-out | cli-core/orchestration, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-09 | 6ae5a2e3a6f86f6b93f60b807e945a0a6a09449a | feat(phase-530 W2): census by default, one pass, no 'nros build' spawn | cli-core/builder, cli-core/cmd, cli-core/leaf_census +3 | yes | yes | no |
| 2026-11-09 | 2b3e972e8a88919a14c6b6b7c4d2dcdcf478a6e5 | fix(phase-530 W2): 'nros build' names a Zephyr leaf's image to the hook | cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-09 | 7015cf20f3249d5d0dede395ddfca13020fb55a6 | fix(gates): W2's comment names and a test closure the language gate reads | cli-core/builder, cli-core/cmd | yes | file-only | no |
| 2026-11-09 | bca55910b17039fc797752c33b489aa3897afc4b | feat(phase-530 W2 R29): '[system] entities = "defaults"' | cli-core/cmd, cli-core/orchestration, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-09 | a95ef9adfb7092a35ffc663501c5221e6337117c | fix(phase-530 W2 R29, M1/M5/M6): component-less leaves, the selected image | cli-core/cmd, cli-core/leaf_census, cli-core/leaf_entity_env +1 | yes | yes | no |
| 2026-11-09 | 89e3993c3c12ced0f03e87f10641674cb811cc5a | fix(phase-530 W2 R29): a program is a call, and the nano-ros tree is not one | cli-core/cmd, cli-core/leaf_census | yes | yes | no |
| 2026-11-09 | 051f53758c633ab63f6fd6ab8ecd4a63922b20c1 | fix(phase-530 W2): the host census build drops the cross configure's CC/CXX, after rebase onto main | cli-core/cmd | yes | yes | no |
| 2026-11-09 | a9dc35a5e13b5889521836e582169bef3977f9a9 | fix(phase-530 W2): the census toolchain check and 'take' speak main's APIs, after rebase onto main | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-09 | d8d25d8eb1728ac54e6566b502ebf133a571278c | fix(issues 2353, 2301): 'nros run' on Zephyr finds its west tree and its router from the build | cli-core/cmd | yes | yes | no |
| 2026-11-09 | d1a54debfe0bab41763e63e73a946903b6f3a325 | fix(issue 2307, phase-544 W3): a SIGKILLed nros run takes its image down | cli-core/target_session | yes | yes | no |
| 2026-11-09 | 0c38baea95437649ad2550f4e3ce6a6230f6264f | feat(contracts): rlm v0.3.13, the safety model's keys (play_launch phase 102 M4) | play_launch pin; cli-core/codegen, packages/cli/Cargo.lock, packages/cli/nros-cli-core +3 | yes | yes | no |
| 2026-11-09 | 41679cafb3c5c264fba2c0facda9ae5d5444c7dd | fix(#2339): the metadata probe takes a C++ component's shape from the verb that declared it | cli-core/orchestration | yes | yes | no |
| 2026-11-09 | d6f80d76e6f2cea8d6e69180f02769a140d4c6c8 | fix(#2347): 'nros new' teaches 'nros configure', and the old-spelling gate reads packages/ | packages/cli/cargo-nano-ros | yes | yes | no |
| 2026-11-09 | e7958c76b14fbd7d62e148453b947f4ac53298c1 | fix(#2370): copied-out projects build on the pinned toolchain on the cargo and west roads | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-09 | 6c2bb2f71adc8d6f7c0df52c573264eb9b81ab5a | fix(#2370): the census host-toolchain check passes the toolchain argument main's caller lacked | cli-core/builder | yes | yes | no |
| 2026-11-09 | a9061b0f1c9d150e28d8fa929c67abb82311b6f1 | fix(issue 2365): zenoh task stacks no longer inherit the main stack; main stack per image | cli-core/cmd, packages/cli/cargo-nano-ros | yes | yes | no |
| 2026-11-09 | 1f493fd18f9e1ded73f8e2dc4c8a4ea4bad1d7d9 | feat(phase-530 W3): resolve the Rust west entry and the shared cmake configure | cli-core/builder, cli-core/cmd, cli-core/configure_stamp +1 | yes | yes | no |
| 2026-11-09 | 38c880939c1cf254c706f6e402b5ad743abf63b5 | fix(phase-530 W3): the sizing decision file names no internal id | cli-core/cmd | yes | yes | no |
| 2026-11-09 | 96cd8eb6d4c807e6fe04bbfec342b9beab56e6a8 | fix(phase-530 W3): review round 1 (R27, R28 and four minors) | cli-core/cmd, cli-core/configure_stamp, cli-core/message_bound_knobs | yes | yes | no |
| 2026-11-09 | 3267957768cc4bdcdb7ca8afdaf1ffc0b3d36a90 | fix(phase-530 W3): review round 2 (R35, union coverage, SharedConfigure) | cli-core/cmd, cli-core/configure_stamp, cli-core/sizing_descriptor | yes | yes | no |
| 2026-11-09 | 0a52fb84729d72ef7971a2e3f46cf1a80d3f9e6e | fix(#2354): a generated Zephyr image on Cyclone DDS configures and registers | cli-core/cmd | yes | yes | no |
| 2026-11-09 | b5d2596d307fd497a99bd0f2b33063528d711eee | feat(phase-535 W5): nuttx arm example pairs as workspace images | cli-core/cmd, packages/cli/nros-cli-core | yes | file-only | no |
| 2026-11-09 | c11598d995eae8f6311d0da1d155b4688da0235d | test(phase-544 W2, issue 2273): the violation channel's QEMU leg | cli-core/cmd | yes | no | no |
| 2026-11-09 | 8ba21fd02d3cf34f0de4b9aeffb227a665ad9993 | fix(phase-544 W3, issue 2307): the verb waits for its runner's process chain | cli-core/cmd, cli-core/target_session, packages/runners/nros-extension | yes | yes | no |
| 2026-11-09 | 0a56f3d71f33eb0cd98827783a3256714682bffe | feat(phase-530 W4): configure locates an image's answer by identity | cli-core/builder, cli-core/cmd, cli-core/configure_stamp +2 | yes | yes | no |
| 2026-11-09 | 3097c28ef1667dc55219a03fc222723a0f79723c | fix(phase-530 W4): locate's configured mode before its tests; fmt | cli-core/cmd | yes | yes | no |
| 2026-11-09 | 70ee8b46cbdb11c8be1f0280695810459c54d1da | fix(phase-530 W4): a census's raw recorder document is not a census | cli-core/configure_stamp | yes | yes | no |
| 2026-11-09 | 76da02c89ab3c4f523ee5383269010b9a10929aa | fix(phase-530 W4): the fast line's gates over the hook and its test | cli-core/message_bound_knobs | yes | file-only | no |
| 2026-11-09 | b207ea669fa0fb0b8c4d9d25c028c49afc087888 | fix(phase-530 W4): seed only from a current stamp; walk up only for a generated app | cli-core/cmd, cli-core/configure_stamp | yes | yes | no |
| 2026-11-09 | f28021947d0580426a9175500585bc29ff5aed20 | fix(phase-530 W4): review minors; the leaf seed's edition is the host's | cli-core/cmd, cli-core/message_bound_knobs | yes | yes | no |
| 2026-11-09 | b2928ac7c5c40d4d9914bb1de2eab4a58f78416b | fix(phase-530 W4): the Zephyr hook applies the image's ROS edition | cli-core/cmd, cli-core/message_bound_knobs | yes | file-only | no |
| 2026-11-09 | aa9ec31842abf4b3b507c7bdff626a563b72b284 | fix(phase-530 W4): a stamp from an older configure is stale once | cli-core/cmd, cli-core/configure_stamp | yes | yes | no |
| 2026-11-09 | 69c5bcf04cd66e220f2df4a65b73ec0f81be58e2 | fix(issue 2322): an image is the same bytes from any project directory | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-09 | cdd6564ecf2b792d3dd0fee760db81441c87806a | fix(issue 2369): a release server's 5xx outage gets minutes, not 15 s | cli-core/orchestration | yes | yes | no |
| 2026-11-09 | 354fef120cc64b3eadbc06d847f150969e1da742 | feat(phase-530 W5): the CMake hooks call 'nros configure' | cli-core/builder | yes | yes | no |
| 2026-11-09 | 783538288899466cc474e630f5de81ff7083e7a8 | fix(phase-530 W5): the resolve composes a cmake image over its registrations | cli-core/builder, cli-core/cmd, cli-core/resolve | yes | yes | no |
| 2026-11-09 | 92d07e7aab4b2bdf4a0855ebc4de072f403bbf06 | fix(phase-530 W5): the seed's package scope is the cmake root's SUBDIRS | cli-core/builder, cli-core/cmd, cli-core/resolve | yes | yes | no |
| 2026-11-09 | 459be2a73f42479afac1fe2ac366e7fe92283153 | feat(phase-530 W5): configure's digested files are host configure deps | cli-core/cmd, cli-core/configure_stamp | yes | yes | no |
| 2026-11-09 | 9e5e27a51c327578672c3ec70fc3ff33fcfe3592 | fix(phase-530 W5): a stamp without its recorded inputs is stale once | cli-core/configure_stamp | yes | yes | no |
| 2026-11-09 | 7fcfe828456a11206df55a297d7f60b2ea416f37 | fix(issue 2378): one census taker per leaf at a time | cli-core/cmd | yes | yes | no |
| 2026-11-09 | 62ec5db63ecc7ac527731680ea311aa34ce9cce3 | fix(issue 2374): a Zephyr build dir re-runs its config discovery on every configure | cli-core/builder | yes | file-only | no |
| 2026-11-09 | cd02d858bfec2e3e6d53b9739864024f7d93d84c | fix(phase-545 W3): nros build finds a workspace RMW provider, and the image selects it | cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-11-09 | d9e2fc13012f3a48a1b33489853743135e5fedbf | test(phase-545 W3): an out-of-tree RMW backend builds and runs through nros build | packages/cli/nros-cli-core | yes | no | no |
| 2026-11-09 | b0302052457953c11e579d1ff0874c7d525b1d70 | test(phase-545 W3): hide the step-1 scratch copy from checkout-wide provider scans | packages/cli/nros-cli-core | yes | no | no |
| 2026-11-09 | 1a7d61024af45b7049c07dd143e5f2930551ef0b | feat(issue 2379): the Zephyr QEMU boards state [board.run.qemu], held to Zephyr's run line | cli-core/cmd, packages/core/nros-orchestration-ir, packages/tooling/nros-qemu-ladder | yes | yes | no |
| 2026-11-09 | 5ab4317f8560188de31d41e56ed96fe00cc768d9 | fix(issue 2299): a host tool's cargo runs outside the workspace config | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-09 | 77292ed16dbf87fb01bd55137ac1acf2a81fd55a | test(issue 2299): a package toolchain that disagrees with the pin wins | cli-core/builder | yes | yes | no |
| 2026-11-09 | e56647ee055304c58f1f303258a8a8cbc890751a | revert: issue 2322's '_ws' link breaks in-tree workspace builds | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-09 | 2abdabcf3393584414bcfb29b07e903ae22b16e9 | fix(issue 2381): nros run reads the RMW's declared host peer, not its name | cli-core/cmd, packages/cli/cargo-nano-ros, packages/runners/nros-extension | yes | yes | no |
| 2026-11-09 | d71a15ecb76b5bba89135bc716805a64cd6ea9d7 | chore(issue 2347): the tree spells every nros verb the current way, baseline 340 files -> 0 | cli-core/abi_guard, cli-core/builder, cli-core/cmd +18 | yes | yes | yes |
| 2026-11-09 | 1d6a3094c7e8bc6a0465b5486bbcdd7594a9cfbb | fix(issue 2386): a cross image's census works on a never-built workspace | cli-core/cmd | yes | yes | no |
| 2026-11-09 | d7ec8043a2feb33aa699275326ca4e92f0a6deb6 | docs(phase-545 W4): custom-rmw is an out-of-tree package, replayed by a probe track | cli-core/cmd | yes | no | no |
| 2026-11-09 | be9752e762bec3c5b938435caf3dfa75e5342dfe | perf(issue 2202): Zephyr kernel heap sized to what Zephyr itself uses | packages/cli/cargo-nano-ros | yes | no | no |
| 2026-11-09 | 1d26d86c912b506af66e7dbd7014cf67c49c3559 | feat(phase-530 W6): a census digest, and a stamp that records what it said | cli-core/cmd, cli-core/configure_stamp | yes | yes | no |
| 2026-11-09 | 34ccef8a83ea047f4ac535372fb8f799da3ae0bd | fix(phase-530 W6): the fast line's gates over the census step | cli-core/cmd | yes | file-only | no |
| 2026-11-09 | 9620e1bb4be102a6620d8fcb49711548c254d60e | fix(phase-530 W6): take --build-step / --fail-if-changed; digest the flag | cli-core/cmd | yes | yes | no |
| 2026-11-09 | 3f62d4d5a541b6805ed5c202e92cae91cc25b200 | feat(phase-530 W6): a workspace census re-taken in a build, no re-plan | cli-core/cmd | yes | yes | no |
| 2026-11-09 | 61163bb95f5a259549c9153181b6412294b22f46 | fix(#2238): an incremental metadata probe keeps every member's row | cli-core/orchestration | yes | yes | no |
| 2026-11-09 | 2f08c930aa2567a46e2f7981ba604b2fe3546bb2 | fix(phase-530 W6): a stamp without its recorded censuses is stale once | cli-core/configure_stamp | yes | yes | no |
| 2026-11-09 | dc4011927520c87f9848e6d3dbf3a584f16c33e0 | fix(phase-530 W6): the issue-2378 lock test passes build_step | cli-core/cmd | yes | file-only | no |
| 2026-11-09 | 34c2626e50d66f22ff8c40165f37537452fea37f | docs(starters): Rust, C and C++ tabs on every starter, one probe track each | cli-core/cmd | yes | no | no |
| 2026-11-09 | 0bb2f84cf66dfd7a5223555a7bddf9d39e9895f9 | probe: the Tutorial's C and C++ tracks await phase-531 W6 batch 2 | cli-core/cmd | yes | no | no |
| 2026-11-09 | e2a51be34351ea4560b35c2481a2a599816233c3 | docs(starters): expect the whole publish line; phase-548 W9 and W10 | cli-core/cmd | yes | no | no |
| 2026-11-09 | f69800f794865d07d2a44755d93edce263d03401 | docs(freertos-posix): zenoh on the board, probed in three tabs | cli-core/cmd | yes | no | no |
| 2026-11-09 | f54e0ee2b5faf1a4e2329d157f999f2e853993f1 | probe: recipes for the W8 tracks; chained platform tracks name their language | cli-core/cmd | yes | no | no |
| 2026-11-09 | 04ff48ce2a94d08936d89aac71a7e3466aa96567 | probe: Rust on FreeRTOS POSIX over Cyclone DDS replays green; the zenoh half awaits W1 | cli-core/cmd | yes | no | no |
| 2026-11-09 | 59e0189c6ac3019f8da9fbaead4ef51ee8ded367 | docs(starters): bare metal stays Rust only; C and C++ readers go to FreeRTOS | cli-core/cmd | yes | no | no |
| 2026-11-09 | ac29dd2fb292a32aef4ea21108b0c2efc505fe12 | docs(phase-548 W8): Rust over zenoh on FreeRTOS POSIX is green; drop its awaits | cli-core/cmd | yes | no | no |
| 2026-11-09 | 274fa85bdf83beb7d1a963e2b6be1a4370a3a6cf | docs(phase-548 W8): drop awaits on the tracks the replay turned green | cli-core/cmd | yes | no | no |
| 2026-11-09 | 5028793dc6c5fd0eb5e89fe3506c47ad8cf1cdbd | chore(deps): rlm v0.3.14 and play_launch 8bbd64df, in lockstep | play_launch pin; packages/cli/Cargo.lock, packages/cli/nros-cli-core, packages/cli/nros-launch-resolve +1 | yes | yes | no |
| 2026-11-09 | 0224a4628d1f09e9ca1637209e06c137c4136efa | fix(phase-531 W6): a generated Zephyr Rust entry registers a backend with no rlib | cli-core/orchestration | yes | yes | no |
| 2026-11-09 | edd4a6550a85ca4303caed6b50abf923fb477f42 | feat(phase-542 W3): user-facing text never tells a user to run a dev verb | cli-core/cmd, cli-core/entity_census, examples/templates/multi-node-workspace +1 | yes | yes | no |
| 2026-11-09 | 5dc2cc62805d3a8845f88e00cffbc935a276ff5f | feat(phase-530 W7): sizing-descriptor --from-leaf --entity-cmake | cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-09 | f11fdfc77d0a09ebad719c47d14d84a7f5d483fa | feat(phase-530 W7): delete the fixed point (RFC-0120 D5) | cli-core/entity_inventory, cli-core/resolve | yes | file-only | no |
| 2026-11-09 | 05f998cbaecbd16e862daf8e4c3b9d3691a2e980 | fix(phase-530 W7): review round 1 -- west no-op, one default helper, gates | cli-core/cmd | yes | yes | no |
| 2026-11-09 | 86ab962b362d90d3eeadf11dd8acd476780f6c7f | fix(phase-530 W7): review round 2 -- configured means built, honest remedies | cli-core/cmd | yes | yes | no |
| 2026-11-10 | 02eb7288686396526f7d71c54a28edddbb0aba18 | feat(phase-535 W6): linux serial XRCE runs the plain images | cli-core/cmd | yes | file-only | no |
| 2026-11-10 | c31b485e1aaca47fd64103827b5cc1bab4fc1a8f | probe(tutorial-10): the track replays green; drop its awaits | cli-core/cmd | yes | no | no |
| 2026-11-10 | 27e26cecfb7fc0cac5ff1329245f6321c810499c | fix(issue 2399): a mixed cmake image's seed registers its Rust component | cli-core/cmd, cli-core/resolve | yes | yes | no |
| 2026-11-10 | 50e1b4d76674935eb3f55a37d84963aaf7840800 | fix(issue 2396): the census build step takes nothing for a model with no contract | cli-core/cmd | yes | yes | no |
| 2026-11-10 | 5f6b2cd21a4d762a4396242cd02c4170e4a5e5b3 | fix(issue 2404): the message-bound seed's interface closure sees the workspace | cli-core/cmd, cli-core/message_bound_knobs, packages/cli/cargo-nano-ros | yes | yes | no |
| 2026-11-10 | 1238f6abb8f2836dc2dc800367c296c88c395dca | feat(phase-542 W5): the PX4 SITL uORB cell is Runtime, in its own lane | cli-core/cmd | yes | no | no |
| 2026-11-10 | 0cc09c1a3dce85756b1627bafb5c8fe7ce1e1c66 | fix(issue 2405): a hand-written C/C++ west entry gets its message-bound seed | cli-core/cmd | yes | yes | no |
| 2026-11-10 | 892839d73e35e940051406bf430232a66590f736 | docs(phase-548 W8): Zephyr Rust starters gain missing west steps | cli-core/cmd | yes | no | no |
| 2026-11-10 | 22c568698e9e21608c17825e1eec2305354a8073 | feat(issue 2372): the Zephyr RMW choice is the descriptor's module backend | cli-core/cmd, packages/cli/cargo-nano-ros | yes | yes | no |
| 2026-11-10 | c925c161d10a2a4737d20fbeefc32693f3c8af3e | test(issue 2372): the out-of-tree RMW image builds and runs on Zephyr | packages/cli/nros-cli-core | yes | no | no |
| 2026-11-10 | 722aba921d10bc7e255d23340d575d2f3e50b0cf | feat(phase-535 W6): bare-metal serial and XRCE as workspace images | cli-core/cmd | yes | file-only | no |
| 2026-11-10 | b1e8fb56d36e51c39d06fa2c7557ed4ed75196bd | fix(issue 2393): every backend dependency line shares one feature derivation | cli-core/builder, cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-11-10 | 9f2b11d4f20029835d69e0aafbecffc11b1215ab | fix(issue 2376): 'nros new' refuses a C or C++ node on a board with no C/C++ entry | cli-core/cmd | yes | yes | no |
| 2026-11-10 | 9beda94c321861472846ec24424ae9afdaf90d5c | docs(issue 2376): the Tutorial's C and C++ tabs run on mps2-an385-freertos | cli-core/cmd | yes | no | no |
| 2026-11-10 | 49476bf101b8f4bfb5e38a8bae72478295130da2 | docs(issue 2269): Lab: QoS is checked twice | cli-core/cmd | yes | no | no |
| 2026-11-10 | 2383e29e80cea03cf93f633016ebee82fb0838b2 | fix(issue 2376): 'nros new --to' adds the node's contract row | cli-core/cmd | yes | yes | no |
| 2026-11-10 | ff1b56a27b62eebdb8db39cc0e28c1c365aa5be3 | fix(#2407): every road asks cargo for the staticlib only, so the profile's LTO applies | packages/tooling/nros-cargo-profile | yes | yes | no |
| 2026-11-10 | 578b5ac5dce9ba5e7843ffa207ffa80bf1fb616a | fix(phase-535 W6): the workspace road resolves board facts per image | cli-core/cmd | yes | yes | no |
| 2026-11-10 | 9e483626d542167f2cc8c26d3953741cb4110e53 | feat(phase-535 W6): a generated RTIC entry depends on rtic and its device crate | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-10 | 2953b993aac95001ba05b8f0f350fd948fad2068 | feat(phase-535 W6): bare-metal RTIC leaves as workspace images | cli-core/cmd, packages/cli/nros-cli-core | yes | file-only | no |
| 2026-11-10 | 11c71773d0545e3085f951e0f568d42c594017e6 | fix(issue 2376): 'nros new --to' writes every role's contract row | cli-core/cmd | yes | yes | no |
| 2026-11-10 | e6a3cb5feeeb013b38f95ea0bfac71ccdb8e6339 | feat(issue 2394): user-tier homes for the census, source metadata and the bringup move | cli-core/cmd, cli-core/entity_census, packages/cli/cargo-nano-ros +1 | yes | yes | no |
| 2026-11-10 | f5d0713b68676b3fa621e3c5410f201eb53ece85 | fix(issue 2394): the stale-census line marks its image as a placeholder | cli-core/cmd | yes | yes | no |
| 2026-11-10 | f6cf57d394bf4acc940bd12a39c25e74decd5774 | fix(issue 2241): pick the jobserver form from every make the build runs, cached ones included | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-10 | 29bbc628ec07b89861eae4fa9f530beaba11563c | fix(issue 2418): a leaf whose program registers nothing states entities = [] | cli-core/cmd | yes | yes | no |
| 2026-11-10 | 82f18f4a1cff0f73338e838b81c6a7f353c3f7e5 | feat(phase-531 W6, issue 2409): refuse a workspace image of another ROS edition; the linux Rust talker is a workspace | cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-10 | 3ceff465d8fd62372d6088295ec1b80175a164df | fix(issue 1371): a node with no callback groups is a verdict in the plan | cli-core/cmd, cli-core/orchestration, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-10 | 8665bcb61928154daded2e273105c0785678da49 | refactor(phase-531 W7): migrate the entry-poc test bin into a one-package workspace | cli-core/builder, cli-core/cmd | yes | yes | no |
| 2026-11-10 | 7584ea3cc42c5f99a844fccc544ab90513ae955a | test(cli): the selection-package leaf floor drops to 73 with the entry-poc bin | cli-core/cmd | yes | file-only | no |
| 2026-11-10 | 23e67ee590a92c2ecd7adbfce669d5d8f1ece8c6 | feat(phase-535 W6): linux TLS and peer-mode pairs as workspace images | cli-core/cmd | yes | no | no |
| 2026-11-10 | 9b41a9da3050b3f16937ea868d43faf753768ba1 | fix(phase-535 W6): keep link-tls on the leaves; peer mode alone moves | cli-core/cmd | yes | no | no |
| 2026-11-10 | 7a85070f8c91ae62983339d8fb666b6867e33c1d | fix(cli): a bringup's component row declares its workspace node package to the metadata probe | cli-core/orchestration | yes | yes | no |
| 2026-11-10 | 2c4bd9f5a41f18a8a8a6df022b952eebb85d6fcf | fix(cli): report a bringup component row no declaration covers, and gate the tree on it | cli-core/orchestration | yes | yes | no |
| 2026-11-10 | ab74b4885079a4c737137b7bf877c72cb904ac7c | fix(cli): the undeclared-component report says what to do, without an issue id | cli-core/orchestration | yes | yes | no |
| 2026-11-10 | b480d26f8226acb6960b39d5a05cd37f55a1e5bb | feat(RFC-0071 D19): one owned entry builds for several images (issue 2406) | cli-core/cmd, cli-core/orchestration, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-10 | 1c4f851e7e42d90b13fae6889872d86d0661fde6 | fix(RFC-0071 D19): an entry's optional backend is re-declared only for its own image; tests follow the cargo road | cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-11-10 | b5fb685377ba759895a37d66188d3ad6b1cff041 | fix(issue 2304): the cmake road generates the runtime's own interface types | packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-10 | 1817d12136af3fbb5cc0a7410d731dd8f1219376 | test(issue 2304): the C parameters example on Cyclone has a verdict | cli-core/cmd | yes | no | no |
| 2026-11-10 | 84fd55cd3af21434cfa64bb99a782140001912d0 | feat(run): '--expect' repeats, and every pattern must match | cli-core/cmd, cli-core/target_session | yes | yes | no |
| 2026-11-10 | b98ba8dd50dca5738958e33228136cb62d961989 | fix(phase-550 W2, W12): the NuttX make road names its kernel tree; dead and stale pieces go | cli-core/orchestration | yes | file-only | no |
| 2026-11-10 | fec825b92aef041722c6625b786066c5d25efbf5 | feat(phase-535 W6): threadx-riscv64 C/C++ pubsub as workspace images | cli-core/cmd | yes | yes | no |
| 2026-11-10 | 31f1cfd59c830ff0630758bd84c79941508a5875 | fix(cli): migrate-bringup keeps the service families a leaf manifest turned on, and drops a path-less default bin | cli-core/cmd | yes | yes | no |
| 2026-11-10 | 739dea6e5c4b612ea18cf03e4ca2c7f3dfeafff2 | refactor(phase-531 W7): migrate the NuttX and rv-virt-threadx param-store bins into one-package workspaces | cli-core/cmd | yes | file-only | no |
| 2026-11-10 | 60f46e4c5e558f5eab5742ed8a5ab20d0268d352 | fix(cli): a bringup must declare the service families its node packages compile | cli-core/cmd, cli-core/orchestration | yes | yes | no |
| 2026-11-10 | 6ae052645208406e769f19c2aed47233e78ccb07 | fix(phase-531 W6): a migrated leaf's image-settings include is dropped | cli-core/cmd | yes | yes | no |
| 2026-11-10 | cb799bc93b9e407659824b84d96cf79de0dfa7f1 | fix(issue 2431): the census build step applies '[census] on_missing' | cli-core/cmd | yes | yes | no |
| 2026-11-10 | 16bb810c2eb24da1ca097aa93b49005baa97547c | fix(cli): a component class is a Rust path; migrate-bringup writes the crate ident, not a dashed package name | cli-core/cmd, packages/core/nros-orchestration-ir | yes | yes | no |
| 2026-11-10 | 850b6f7700dd00c3e3eab2b1e8ba2b5cb17dd27c | test(cli): the system.toml dispatch fixture names its class by the crate identifier | cli-core/cmd | yes | file-only | no |
| 2026-11-10 | 183c5b90f0dfbdc932334f6ec8fdc7473b88f712 | cli: build a multi-image owned entry on the CMake road (RFC-0071 D19) | cli-core/cmd | yes | yes | no |
| 2026-11-11 | 48926d5a6f85ca627a6b465c4c54d353d6c98e50 | refactor(phase-535 W6): linux Rust listener, service and action as one-node workspaces | cli-core/cmd, packages/cli/nros-cli, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-11 | ae3172d80b38b7d62be78a7f3f5cfc76da3a33b5 | docs(issue 2269): Lab: two sched tasks keep a promise | cli-core/cmd | yes | no | no |
| 2026-11-11 | bf7128d81e126a2e8d5937e563b5d3b19680498a | fix(issue 2318): 'just generate-bindings' runs C1 alone via 'nros dev bindings' | cli-core/cmd, packages/cli/nros-cli-core | yes | yes | no |
| 2026-11-11 | cbd799c254fde20776bf2d8b00556912d245315a | refactor(phase-535 W6): linux Rust demos as owned-entry one-package workspaces | cli-core/cmd | yes | file-only | no |
| 2026-11-11 | eb248aec9f11414ed5c0578436c885d6a66fc470 | fix(issue 2435): re-probe a fresh sidecar with no probe build; cover 2405 | cli-core/orchestration | yes | yes | no |
| 2026-11-11 | 55e4af3be10ad34afa42d957dcc5a022caef7096 | fix(issue 2436): nros build states a hand-written west entry as its image | cli-core/cmd | yes | yes | no |
| 2026-11-11 | 104034d9223f5b0f986b6e1edf1cc255ee7a55e7 | cli: the selection-package reach floor follows the three migrated Rust talkers (64 -> 61) | cli-core/cmd | yes | file-only | no |
| 2026-11-11 | 7bf09caffb86c5ea07c6640094920eceeecf9615 | cli tests: the node-declaring example floor follows the three migrated Rust talkers (53 -> 50) | packages/cli/nros-cli-core | yes | no | no |

</details>
