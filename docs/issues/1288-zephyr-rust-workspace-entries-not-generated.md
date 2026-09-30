---
id: 1288
title: "The 15 hand-written workspace entries are all Zephyr — `nros build` has no generator for a west application"
status: open
type: tech-debt
area: tooling, examples, zephyr
severity: medium
found: 2026-09-11
related: [1108, 1253, 1509, 1511, rfc-0065, rfc-0098, phase-445]
---

# What is left of RFC-0065 D4 on Zephyr

phase-445 W5 turned every hand-written CARGO workspace entry into a generated
one (`examples/workspaces/rust/src/esp32_entry` was the last) and moved every
workspace entry's deployment out of its manifest into the bringup image that
claims it (`leaf_system::for_entry`). These entries are still hand-written
packages, all Rust west applications:

| workspace | entry | image |
| --- | --- | --- |
| `rust` | `zephyr_entry` | `[image.zephyr]` |
| `rust` | `zephyr_entry_robot1` | `[image.zephyr_robot1]` |
| `realtime-rust` | `zephyr_entry` | `[image.zephyr]` |
| `features` | `zephyr_rust_{lifecycle,params,qos}_entry` | `[image.zephyr_rust_*]` |
| `safety` | `zephyr_rust_safety_entry` | `[image.zephyr_rust_safety]` |

(The C/C++ `zephyr_entry` / `fvp_entry` packages are the same shape and the
same gap.)

## Why they were not generated

`builder::entry` generates a CARGO entry (`build/<coord>/<id>_entry/`), and
for a `ZephyrStaticlib` board it already renders the `rustapp` staticlib. What
it cannot produce is the WEST APPLICATION around it — `CMakeLists.txt` with
`find_package(Zephyr)` and `rust_cargo_application()`, `prj.conf`,
`prj-<rmw>.conf`, `boards/*.overlay`, `sample.yaml`, `build.rs` with
`zephyr-build`. RFC-0065 D4 puts the overlays in the BRINGUP
(`boards/<board>/`) and D5 calls authored Kconfig "not derivable", and
`west_application_dir` (`cmd/build.rs`) says the same in code: "Inventing an
application here would be worse". So the entry is derivable and the application
that hosts it is not, and nothing yet splits the two.

## What W5 did to them

- Deployment facts moved to the bringup image (`locator` per image; board and
  RMW were already there). The manifests keep only the empty
  `[package.metadata.nros.entry]` marker. `nros::main!`, `nros sync`'s board
  projection, `nros ws board-facts` (the Zephyr lane's board facts, via
  `NanoRosBoardFacts.cmake`) and `nros build`'s entry classification all read
  it through `leaf_system::for_entry`.
- `examples/workspaces/rust`'s two images keep `board = "native_sim/native/64"`;
  the macro's board table gained that key (it is the zephyr descriptor's second
  name) so the image's board resolves as the hand-written `deploy = "zephyr"`
  did.
- **None of it was BUILT.** The west workspace's `nano-ros` module is a symlink
  to another checkout (issue 1253's shape), so a west build from the phase-445
  worktree compiles that checkout, not this one. The acceptance for this issue
  is a west build of each of them.

## Fix

Generate the application too: move each entry's `prj*.conf` / `boards/` into
`<bringup>/boards/<board>/` (D4's table), and have stage 4 emit the west
application shell (`CMakeLists.txt` + `sample.yaml` + `build.rs`) around the
generated staticlib entry under `build/<coord>/`, pointing `west build` there
with `APPLICATION_CONFIG_DIR` at the bringup's board dir (`builder::zephyr`
already resolves those overlays). Then delete the hand-written packages.

---

## 2026-09-27 — re-measured: it is FIFTEEN packages, not eight, and "Rust" was never the axis

A layout study re-enumerated the tree. Every number below was measured in this
worktree against the branch point named at the end of this section; where it
disagrees with what was reported to me, the measurement wins and the
disagreement is stated.

### The count in the old title was wrong on the day it was written

**There were never eight Rust entries. There are seven, and there were seven
when this issue was filed.** A Rust entry is one with a `Cargo.toml` and a
`src/lib.rs`:

```
$ git ls-files 'examples/workspaces/*/src/*entry*/Cargo.toml'
examples/workspaces/features/src/zephyr_rust_lifecycle_entry/Cargo.toml
examples/workspaces/features/src/zephyr_rust_params_entry/Cargo.toml
examples/workspaces/features/src/zephyr_rust_qos_entry/Cargo.toml
examples/workspaces/realtime-rust/src/zephyr_entry/Cargo.toml
examples/workspaces/rust/src/zephyr_entry/Cargo.toml
examples/workspaces/rust/src/zephyr_entry_robot1/Cargo.toml
examples/workspaces/safety/src/zephyr_rust_safety_entry/Cargo.toml
```

Seven. The same query against the tree of the commit that FILED this issue
(`docs(phase-445 W5): record generated workspace entries and package mode; file
1288, 1289`) returns the same seven paths — so nothing was added or removed
since, and there is no eighth hiding in `examples/templates/` (the only
template that is a west application, `zephyr-byo`, has no `src/` and is not a
colcon workspace at all). **The issue's own table below the title already listed
seven** (`rust` x2, `realtime-rust` x1, `features` x3, `safety` x1), and
disagreed with every prose statement above and below it: the title, the opening
sentence, the acceptance line and the fix all said eight, so the table was the
only thing in the file that had counted. (Issue 1518 dropped the number from
those three prose lines rather than correcting it; this section, which is ABOUT
the number, keeps it.) The one Rust entry that DID disappear that day is
`examples/workspaces/rust/src/esp32_entry`, deleted in phase-445 W5's own
commit because it became generated — it was never a Zephyr west application, so
it cannot have been the eighth either.

The wrong number propagated: **RFC-0098's phase-445 W5 amendment also said "the
eight Rust west entries"**, citing this issue — the second copy of a claim that
was never measured, reading as corroboration of the first. Filed as issue 1518
and fixed there, along with two carriers neither file knew about
(`docs/issues/1511-*.md` and `docs/roadmap/phase-445-*.md`); the sweep command
is recorded in that issue.

### All 15 hand-written entries are Zephyr west applications

`git ls-files 'examples/workspaces/*/src/*entry*'` finds 15 packages in 10
workspaces, 117 tracked files, 3 559 lines. **All 15 `CMakeLists.txt` call
`find_package(Zephyr)`** — 15 of 15, no exceptions — so the "Rust" framing in
the original text undercounts by more than half. The C/C++ eight are the same
gap in another language:

| workspace | entry | language | image row |
| --- | --- | --- | --- |
| `c` | `zephyr_entry` | C | `[image.zephyr]` |
| `cpp` | `zephyr_entry` | C++ | `[image.zephyr]` |
| `cpp` | `zephyr_cyclonedds_entry` | C++ | `[image.zephyr_cyclonedds]` |
| `derived-tiers-cpp` | `zephyr_entry` | C++ | `[image.zephyr]` |
| `features` | `zephyr_rust_{lifecycle,params,qos}_entry` | Rust | `[image.zephyr_rust_*]` |
| `mixed` | `zephyr_entry` | C + C++ + Rust | `[image.zephyr]` |
| `realtime-c` | `zephyr_entry` | C | `[image.zephyr]` in BOTH `demo_bringup` and `smp_bringup` |
| `realtime-cpp` | `zephyr_entry` | C++ | `[image.zephyr]` |
| `realtime-cpp` | `fvp_entry` | C++ | `[image.fvp]` |
| `realtime-rust` | `zephyr_entry` | Rust | `[image.zephyr]` |
| `rust` | `zephyr_entry` | Rust | `[image.zephyr]` |
| `rust` | `zephyr_entry_robot1` | Rust | `[image.zephyr_robot1]` |
| `safety` | `zephyr_rust_safety_entry` | Rust | `[image.zephyr_rust_safety]` |

15 packages, **16 Zephyr image rows** — the extra one is `realtime-c`, whose
single `zephyr_entry` serves `[image.zephyr]` in two different bringups and
picks between them at configure time with `if(CONFIG_SMP)`. That is not a
customisation the generator has to reproduce; it is two images, which the
bringups already are.

`realtime-cpp/src/fvp_entry` IS a Zephyr entry, confirming the study: it calls
`nano_ros_use_board(fvp-aemv8r-smp)` before `find_package(Zephyr)` and is built
by `west build` with no `-b`.

### What is uniform, what differs, and which of the differences is input

By content, the 3 559 lines are:

| kind | files | lines |
| --- | --- | --- |
| `prj*.conf` | 32 | 1 279 |
| `Cargo.toml` (Rust only) | 7 | 773 |
| `CMakeLists.txt` | 15 | 622 |
| `sample.yaml` | 10 | 223 |
| `package.xml` | 15 | 210 |
| `src/lib.rs` (Rust only) | 7 | 192 |
| `boards/*.conf` | 11 | 144 |
| `build.rs` (Rust only) | 7 | 70 |
| `.gitignore` | 13 | 46 |

**Rust.** All seven `build.rs` are byte-identical (one md5). All seven
`src/lib.rs` carry the identical two-line shell — `#![no_std]` and
`extern crate zephyr;` — above a single `nros::main!`. Nine of the eleven
`boards/*.conf` are byte-identical; the two that differ are
`derived-tiers-cpp`'s and `realtime-c`'s SMP one.

**A claim of mine and its correction were BOTH wrong about `nros::main!`.** The
first said the invocations are byte-identical; the correction said "some use
`model =`, some `launch =`". Measured:

```
$ grep -rn 'nros::main!' examples/workspaces/*/src/*entry*/src/lib.rs
```

**All seven use `launch =`. Not one uses `model =`.** The token `model =`
appears only inside stale doc comments in `realtime-rust/src/zephyr_entry` and
`rust/src/zephyr_entry`, both of which say "Same one-line `nros::main!(model =
...)` as the native sibling" three lines above a line reading
`nros::main!(launch = "demo_bringup");`. So the invocations differ in exactly
two ways, both of them generator INPUT and neither of them customisation:

- the launch target string — `"demo_bringup"`, or
  `"demo_bringup:rust_qos.launch.xml"` etc. — which every image row already
  declares as `launch =`;
- `zephyr_entry_robot1` alone spans three lines because it adds
  `args = [("host", "robot1")]`, which `[image.zephyr_robot1]` already declares
  as `args = { host = "robot1" }`.

**C/C++.** The study said these "differ from each other only in project name and
which node packages they `add_subdirectory`". That is four axes short. Measured,
they also differ in: `LANG c` (the two C entries); `PANIC platform` (four of the
eight); `mixed`'s `NROS_WS_RUST_NODE_DIRS` + `nano_ros_workspace_pkg_guard` stub,
set before `find_package(Zephyr)` so the Rust node joins the single runtime
umbrella; `realtime-c`'s `if(CONFIG_SMP)` bringup selection; and `fvp_entry`'s
`nano_ros_use_board` + `EXTRA_CONF_FILE` layering. Every one of those is a
DECLARATION the image can carry (`panic` already is a field, RFC-0065 D5.1), not
a piece of hand-written program — which strengthens the study's conclusion
rather than weakening it, but the generator has five shapes to cover, not two.

The Rust `CMakeLists.txt` are not uniform either: `rust/src/zephyr_entry` maps
three RMWs to cargo features and generates Cyclone C type descriptors inline,
while the other six are zenoh-only with a `FATAL_ERROR` otherwise. That map is
`[image.*] rmw`, one level up, already declared.

### The declarations the study wants in `system.toml` are already half there

This is the finding that most changes the size of the work. **Every one of the
16 Zephyr image rows already declares BOTH its board and its Kconfig overlay
list**, e.g.

```toml
[image.zephyr]
board = "native_sim/native/64"
entry = "zephyr_entry"
conf = ["prj-zenoh.conf"]
locator = "tcp/10.0.2.2:7430"
```

`ImageBlock::conf` is documented as "extra framework config fragments for THIS
image, in order… per-image, not per-board". So the image already names WHICH
fragments apply; what has no home in the bringup is their CONTENT.

And the D4 destination is already wired on the read side.
`builder::zephyr::resolve_in` searches each named fragment in this order:

1. `<bringup>/boards/<board with '/' replaced by '_'>/<name>`
2. `<application>/<name>`   (the rung issue 0892 added, which is why the
   fragments can sit in the entry package today)
3. `<bringup>/<name>`

and when rung 1's directory exists it is ALSO passed as
`-DAPPLICATION_CONFIG_DIR`, which is how Zephyr would then pick up a moved
`prj.conf` and `boards/*`. Measured: `git ls-files 'examples/**/*_bringup/boards/*'`
returns **nothing** — not one bringup in the tree has that directory yet. So
moving a workspace's fragments is a file move that needs no CLI change; the CLI
change is only the application shell that rung 2 currently exists to serve.

Only 6 of the 16 rows carry `entry = "<pkg>"`; the other 10 resolve their
application by board through `west_application_dir`'s deploy scan. Once the
application is generated there is nothing to resolve, and both paths — plus the
ambiguity error that forced `entry =` — go away.

### A defect the study surfaced without naming it: `[image.fvp]` declares the wrong board

```toml
[image.fvp]
board = "native_sim/native/64"
# Two entry packages target this board (`zephyr_entry`, `fvp_entry`) and both
# declare `DEPLOY zephyr`, so the application cannot be derived — see
# `[image.zephyr]` below, which is the other one.
entry = "fvp_entry"
conf = ["prj-cyclonedds.conf"]
```

The application it names builds for `fvp-aemv8r-smp`, not for
`native_sim/native/64` — the board is hard-coded inside
`fvp_entry/CMakeLists.txt` as `nano_ros_use_board(fvp-aemv8r-smp)` and the image
row never learns it. The comment's premise ("two entry packages target this
board") is therefore manufactured by the wrong value: they target two different
boards, and the ambiguity `entry =` exists to resolve would not arise if the row
said what the application does. Nothing catches the disagreement because nothing
builds this row through `nros build`: the only driver is
`just zephyr build-fvp-ws-entry`, which calls `west build` on the package
directly with no `-b` and no image. A board hidden in an application is exactly
the state this issue exists to end, and this row is the case where it is already
measurably false rather than merely undeclared.

### The one thing that is genuinely not yet declared anywhere

Across the seven Rust entry manifests, the node dependencies are plain
`default-features = false` path deps in every case but one:

```toml
rust_safety_listener_pkg = { path = "../rust_safety_listener_pkg", default-features = false, features = [
    "safety-e2e",
] }
```

A per-node FEATURE selection, made by the entry, that no `[[component]]` row
expresses. Everything else in those manifests is derivable from the bringup (the
node set matches the launch file — `zephyr_entry_robot1` depends on `talker_pkg`
alone, which is exactly what `args = { host = "robot1" }` over
`multihost.launch.xml` resolves to). This is 1509's invariance one level up: a
node package names no platform or RMW, but an entry still has to name a node's
capability features. The generator needs a declaration for it, or the `safety`
workspace is the one that cannot collapse.

### Closing condition

For each of the 10 workspaces:

- its `*_entry` package(s) are deleted — no `CMakeLists.txt`, `prj*.conf`,
  `boards/`, `sample.yaml`, `package.xml`, and for the Rust ones no
  `Cargo.toml` / `build.rs` / `src/lib.rs`;
- the Kconfig moves to `<bringup>/boards/<sanitized board>/`, where
  `resolve_in` rung 1 and `APPLICATION_CONFIG_DIR` already look, with the
  per-image fragment list staying in the `conf =` the image already carries;
- every remaining per-application fact becomes an image declaration: the board
  (including `[image.fvp]`'s real one), `panic`, `LANG`, the SMP split as two
  images rather than a configure-time `if`, and a home for the `safety-e2e`
  node feature;
- `nros build` generates the west application the way it generates the cmake
  root today, and the acceptance is a west BUILD of each image — not a
  generation diff. Issue 1253 still blocks that on any tree whose
  `zephyr-workspace` binds a foreign checkout, which is every agent worktree.

When they collapse, workspace shape is uniform across platforms: a workspace is
`src/` packages plus a bringup, on Zephyr exactly as on native, FreeRTOS, NuttX
and ThreadX.

`examples/templates/workspace-shadowing` is NOT in scope: it is a legitimately
bringup-less workspace, and the decision that says so is **RFC-0098 D9 as
amended by phase-445 W5** ("a workspace with no bringup has a build"), not
RFC-0065 D1 (which is the five-stage pipeline). It has no `system.toml`, so it
has no image to declare anything on, and it has no entry package either.

### The related issues, re-read

- **1253 — still open and still exactly the blocker this issue's acceptance
  needs.** Its "What still needs a human" section is unchanged: the runner's
  `zephyr-workspace` lives inside a second nano-ros checkout, west binds the
  `nano-ros` MODULE from that workspace's manifest, and tier 2 therefore
  certified another tree's module. What HAS landed since this issue was filed is
  that the condition now fails loudly —
  `scripts/check-zephyr-workspace-checkout.sh` asks the module question at the
  head of `check-tier-preconditions` and `NROS_SKIP_STALE_CHECK=1` does not
  silence that half. So "none of it was BUILT" above stays true, but the reason
  is now a refusal you can read rather than a silent wrong-tree build.
- **1108 — resolved and ARCHIVED** (`docs/issues/archived/1108-templates-materialize-dead-entry-pkgs.md`).
  It still says what this issue leans on it for (phase-383 W10 retired the
  hand-written entry shape across `examples/workspaces/**`, and
  `check-no-tracked-workspace-roots` keeps it out), so the citation is sound —
  but a reader following `related: [1108]` will not find it at the unarchived
  path.
- **1511** — no worked example of entry customisation. Deliberately deferred
  behind this migration: an escape hatch cannot be documented against 15
  hand-written applications that are the thing being escaped. **1288 landing is
  what unblocks 1511.**
- **1509** — node packages name no platform or RMW. The same invariance one
  level down; the `safety-e2e` feature above is where the two meet.

1509 and 1511 were being filed concurrently with this section and are carried in
this issue's `related:` rather than as prose pointers, which is where an id
whose file is still in another branch belongs (`check-prose-issue-refs` exempts
`related:` for exactly this, and would otherwise want a baseline row that goes
stale the moment those two land). If either id carries a different subject by
the time you read this, trust the file.

### Where this section disagrees with the study it came from

| studied | measured |
| --- | --- |
| "eight Rust entries" (title, and three prose lines in the body) | **seven**, and seven on the filing commit too |
| `nros::main!` invocations split between `model =` and `launch =` | **all seven use `launch =`**; `model =` survives only in two stale doc comments |
| C/C++ entries differ only in project name + `add_subdirectory` list | also in `LANG`, `PANIC`, `mixed`'s runtime-umbrella preamble, `realtime-c`'s `CONFIG_SMP` bringup switch, and `fvp_entry`'s board glue — five shapes |
| the bringup is "the natural home" for board + Kconfig | the image row **already declares both** (`board =`, `conf =`) for all 16 rows; only the fragment CONTENT has no home, and `resolve_in` already searches the destination |
| "22 workspaces total, 12 already generated, 10 hand-written" | the **10** is right. The total depends on what counts: 26 trees under `examples/` have a `src/` (16 in `workspaces/`, 10 in `templates/`); 22 is what you get counting only trees with any `system.toml`. Stated here so the next reader does not re-derive a third number. |
| `workspace-shadowing` is bringup-less per RFC-0065 D1 | bringup-less yes, but the decision is **RFC-0098 D9** as amended by phase-445 W5 |

Measured on branch point `fix(ci): verify-fvp-runtime is not a named platform
lane — FVP is license-gated`, 2026-09-27. `realtime-cpp/src/demo_bringup/system.toml`
was being edited concurrently by another session; its `[image.fvp]` and
`[image.zephyr]` rows are quoted as they stood at that branch point.

---

## 2026-09-28 — W5.a: the generator exists, and ONE image is migrated and BUILDS

phase-470 W5.a. Scope was deliberately "the generator plus one migrated image
that actually builds", not the sweep. **14 packages and 15 rows remain; that is
W5.b.** This issue stays OPEN.

### What was built

**`packages/cli/nros-cli-core/src/builder/west_app.rs`** (new) — the west
application shell, emitted beside the entry package `builder::entry` already
generated:

| file | content |
| --- | --- |
| `CMakeLists.txt` | `cmake_minimum_required` + `find_package(Zephyr)` + `project(<derived>)` + `rust_cargo_application()`. Four lines. It names **no board and no `prj.conf`** — those are `[image.<id>] board` and the bringup's, passed as `-b` / `-DAPPLICATION_CONFIG_DIR` / `-DEXTRA_CONF_FILE`. |
| `build.rs` | the byte-identical `export_kconfig_bool_options()` + `bake_nros_config()` pair all seven hand-written Rust entries carry. |

**The application directory IS the entry directory, and that is not a choice.**
`rust_cargo_application()` runs its cargo command with `WORKING_DIRECTORY
${CMAKE_CURRENT_SOURCE_DIR}` and passes no `--manifest-path`, so the manifest
cargo builds is whatever sits beside the `CMakeLists.txt` west was pointed at.
Both land in `build/<coord>/<id>_entry/`.

**`builder::entry` grew a `west: Option<WestApp>` field**, because Zephyr is the
one driver that hands cargo **no settings file** — the command line belongs to
zephyr-lang-rust and we never see it, so four facts that every other image
carries in `nros-cargo.toml` (RFC-0098 D1) have to reach cargo through the
MANIFEST instead: the `zephyr`/`zephyr-build` pair, `nros-zephyr-build`, the
RMW feature + backend dep, and `[patch.crates-io]`.

**The RMW feature is the part that would have shipped broken and silent.**
`nros::main!`'s Zephyr arm emits `#[cfg(feature = "rmw-zenoh")] { let _ =
::nros_rmw_zenoh::register(); }`, and on `target_os = "none"` nothing else
registers a backend (issue #129). The selection facade cannot supply it — a
`#[cfg(feature = ...)]` is evaluated on the entry crate and `::nros_rmw_zenoh::`
has to be a name in the entry crate's scope. Without both, the image compiles
cleanly and fails at run time with `Transport(ConnectionFailed)`. Which crate
that is comes from `[rmw.link] rlib_dep` in the backend's own `nros-rmw.toml` —
the table that already answers "is this backend a Rust crate?", and which says
`""` for cyclonedds and uorb (C/C++ libraries the Zephyr C port links, where the
macro's `#[cfg]` is then correctly OFF rather than missing).

**The `zephyr` board descriptor gained `[board.entry]`** — `crate_root_extra =
"extern crate zephyr;"` plus `crate_root_deps = ["zephyr", "log"]`. `log` is in
that list although no line of `crate_root_extra` names it, and that was MEASURED
rather than reasoned: the first generated entry failed with
`error[E0433]: cannot find 'log' in the crate root` pointing at the
`nros::main!` invocation, because the macro's Zephyr arm emits `::log::error!` /
`::log::info!` at five call sites. All seven hand-written Zephyr entries carry
`log = "0.4"` and none of them uses `log` in its own source.

### A live defect found on the way: `-b zephyr`

`west_build_board` fell through to the AUTHORED board string, which is right
only for an image that spells a Zephyr board — the thing `ImageBlock::board`
says never to author. **21 in-tree images author `board = "zephyr"`** (16 single
leaves under `examples/zephyr/`, plus `safety`, `features` x3 and
`realtime-rust`), and every one of them emitted `west build -b zephyr`, a board
Zephyr does not have. Measured with `--dry-run` before the fix, and again after.

Fixed at the DESCRIPTOR, not at 21 image rows: `[board.zephyr] west_board =
"native_sim/native/64"` on `packages/boards/zephyr/nros-board.toml`. Both of
that descriptor's `names` are the same board, so both must reach the same `-b`,
and the value is identical to what an image authoring the Zephyr id already got
— so no image that was already right changed. This is issue 1517's class one
door over, and `west_build_board`'s own doc-comment had called the second
`names` entry "smuggling" for exactly this reason.

The same `west_build_board` result is now what `builder::zephyr::resolve_in`
gets for the `<bringup>/boards/<board>/` directory, so `board = "zephyr"` and
`board = "native_sim/native/64"` reach the same Kconfig directory instead of two.

### The migrated image, and why that one

**`examples/workspaces/rust` `[image.zephyr]`.** The brief steered toward
`realtime-rust` (one Zephyr image in the workspace); measurement says otherwise
and this section states the disagreement:

- `realtime-rust`'s entry is the ONE of seven that names board-crate features —
  `nros-board-zephyr = { features = ["tiers", "zephyr-edf"] }`. Nothing derives
  those: `[board.*] board_features` is authored by no in-tree descriptor, and
  the facade omits the board dep entirely when it has no features to carry
  (`nros-board-zephyr` declares no `default`). So that image is blocked by a
  second undeclared fact, in the same class as `safety-e2e` below and NOT
  mentioned in the W5 brief.
- `rust`'s `[image.zephyr]` already authored the right board, its entry names
  `nros-board-zephyr` with no features (6 of 7 do), and its node set is the
  plainest (talker + listener).
- Its sibling `[image.zephyr_robot1]` is an ASSET, not a cost: it keeps
  `entry = "zephyr_entry_robot1"`, a package whose name is **not** `<id>_entry`,
  so the locate path and the generate path are exercised side by side in one
  workspace. That is the strongest available acceptance for "do not break
  `entry =`".

Its three-RMW `if(CONFIG_NROS_RMW_*)` ladder is not a loss: that shape exists
because one package served every backend and Kconfig was the only thing that
could choose. Per IMAGE the choice is already made, so the generated manifest
carries exactly one `[features] default`, and no `EXTRA_CARGO_ARGS` at all. Its
cyclonedds arm additionally called `nros_rmw_cyclonedds_generate_from_msg()` —
**W5.b inherits that**: a Zephyr cyclonedds image needs the generator to emit
that block, and no image in this workspace declares cyclonedds today.

### Where the Kconfig went

`src/zephyr_entry/{prj.conf,prj-zenoh.conf,prj-xrce.conf,prj-cyclonedds.conf}`
and `boards/native_sim_native_64.conf` moved verbatim to

```
src/demo_bringup/boards/native_sim_native_64/
  prj.conf  prj-zenoh.conf  prj-xrce.conf  prj-cyclonedds.conf
  boards/native_sim_native_64.conf
```

which is D4's destination and the rung `resolve_in` searches FIRST — the one
that `git ls-files 'examples/**/*_bringup/boards/*'` reported empty. The nested
`boards/` is Zephyr's own layout, not ours: once that directory is
`APPLICATION_CONFIG_DIR`, `configuration_files.cmake` discovers `prj.conf` there
and qualifies `<dir>/boards/<board>.conf` beneath it.

`zephyr_entry_robot1` now resolves its `conf` at rung 1 too, and its
`APPLICATION_CONFIG_DIR` is the shared directory. Safe because the two entries'
`prj.conf`, `prj-zenoh.conf` and `boards/*.conf` were **byte-identical** —
verified with `diff` before the move, and its built `.config` is byte-identical
to the migrated image's afterwards.

### ACCEPTANCE — it is a build, and it also RAN

```
$ nros build zephyr --workspace examples/workspaces/rust -- -d <dir>
nros build:   west application -> .../examples/workspaces/rust/build/zephyr-zenoh/zephyr_entry
nros build: demo_bringup:zephyr -> board native_sim/native/64 (platform zephyr), driver west
... [13/14] Running utility command for native_runner_executable
rc=0
```

- **The merged Kconfig is byte-identical** to the hand-written application's:
  `diff <baseline>/zephyr/.config <generated>/zephyr/.config` -> empty, over 2028
  lines. The baseline was built in this same worktree from the hand-written
  package before deleting it.
- **It runs.** Against an `rmw_zenohd` on `tcp/127.0.0.1:7433`:
  `nros: zephyr workspace entry up (2 nodes)` followed by
  `talker_pkg: talker publishing chatter seq=0..16`. That is the proof the
  backend `register()` reached the image — a missing feature would have been a
  clean compile and `Transport(ConnectionFailed)`.
- **`[image.zephyr_robot1]` still builds through the locate path**, `rc=0`, and
  its `.config` matches too.
- **The real fixture lane builds it too**, not just a hand-run `nros build`:
  `NROS_ZEPHYR_FIXTURE_FILTER=build-ws-rs-entry-zenoh just zephyr build-fixtures`
  -> `rc=0`, `zephyr-workspace/build-ws-rs-entry-zenoh/zephyr/zephyr.exe`
  produced, and `check-tier-priority-plan-image` judged the image it built
  (`transport [4, 4], pool [5, 14] — 8 pin(s)`). Its `.config` differs from the
  hand-written baseline's in exactly two lines: the row's own locator slot, and
  the spelling of the module path (`-DZEPHYR_EXTRA_MODULES` names the checkout
  directly where the west manifest names it through the `nano-ros` symlink) —
  the same tree either way, and no Kconfig VALUE differs.
- `just check fast`: 363 gates green.

Built in an agent worktree whose `zephyr-workspace/` is a `cp -al` of the main
checkout's non-`build-*` directories with `nano-ros` re-pointed at THIS
worktree, which is the documented way around issue 1253 (never a symlinked
`zephyr/`: Zephyr resolves the west topdir from `ZEPHYR_BASE`'s real path).

### What W5.b inherits

1. **The remaining 14 packages / 15 rows.** All eight C/C++ shapes are
   untouched — `LANG c`, `PANIC platform`, `mixed`'s `NROS_WS_RUST_NODE_DIRS` +
   `nano_ros_workspace_pkg_guard` stub, `realtime-c`'s `if(CONFIG_SMP)` bringup
   switch, `fvp_entry`'s `nano_ros_use_board` + `EXTRA_CONF_FILE`. `west_app.rs`
   is designed with them in view (the `WestApp` struct is manifest LINES plus a
   project name, so a C/C++ arm adds fields rather than a second emitter) and
   implements none of them.
2. **`just/zephyr-ci.just` still has FIVE guards keyed on an entry PACKAGE**
   (`features` x3, `safety`, `realtime-rust`). Two were corrected here and one
   of them was load-bearing in a way worth repeating: `if [ -d
   examples/workspaces/rust/src/zephyr_entry ]` decided
   `--include-workspace-entry` for **every** workspace-entry leaf in the zephyr
   lane, so deleting one migrated package would have dropped six leaves from the
   sweep with no message at all. Re-point each guard at the WORKSPACE before
   deleting its package.
3. **A migrated fixture row must drop `conf_files`.** It reaches west as
   `-DCONF_FILE=...`, which is a second spelling of `[image.<id>] conf` AND
   suppresses `APPLICATION_CONFIG_DIR` discovery outright; its relative names
   also resolve against `APPLICATION_SOURCE_DIR` (measured in
   `zephyr/cmake/modules/kconfig.cmake`: `WORKING_DIRECTORY
   ${APPLICATION_SOURCE_DIR}`), which a generated application does not hold.
4. **An existing test asserted the OLD `-b` behaviour and caught this**, which
   is worth knowing before W5.b moves another descriptor:
   `board_key_table.rs::a_zephyr_boards_west_b_comes_from_its_descriptor`
   floored its FALL-BACK arm at two names, and the `zephyr` descriptor was the
   only subject that arm had. Giving it a `west_board` emptied the arm rather
   than breaking the rule, so the arm moved to a synthetic out-of-tree
   descriptor (which needs a `package.xml` announcing the board — the loader
   refuses an unannounced descriptor by design) and the in-tree half now
   asserts `fell_back == 0` with a message saying what to do if that stops
   being true.
5. **`fixtures-manifest.py` now has ONE predicate for this**,
   `zephyr_application_is_generated`, read by both the validator and the
   `west-leaves` emitter. It mirrors `cmd::build`'s discriminator in the same
   order: `[image.<id>] entry` wins, else a `src/<id>_entry` carrying a build
   file suppresses generation, else generated. A generated row's existence
   contract MOVED rather than vanished — `_validate_generated_zephyr_application`
   requires `<bringup>/boards/<board>/prj.conf` and every fragment the image's
   `conf` names.
6. **Two genuinely undeclared facts**, both "a per-dep cargo feature the entry
   names", both blocking one workspace each:
   - `safety`: `rust_safety_listener_pkg = { ..., features = ["safety-e2e"] }` —
     a per-NODE feature no `[[component]]` row expresses.
   - `realtime-rust`: `nros-board-zephyr = { features = ["tiers", "zephyr-edf"] }`
     — a per-BOARD-CRATE feature no image row expresses.

   Options, decided by neither W5.a nor this issue:
   **(a)** `[[component]] features = [...]` for the node case and
   `[image.<id>] board_features = [...]` for the board case — two new
   declarations, each honest about what it is about;
   **(b)** DERIVE the board one — `tiers` is implied by the bringup declaring
   `[tiers.*]` and `zephyr-edf` by a `[tiers.*.zephyr] deadline`, which is a real
   derivation and not a guess, leaving only the node-feature case to declare;
   **(c)** those two workspaces keep a hand-written entry with the reason
   recorded, and 1288 closes at 13 of 15.
   (b) plus (a-for-nodes) is the shape that closes the issue completely; (b)
   alone halves the remaining problem and needs no new schema.
7. **A doc-comment sweep.** `west_application_dir`'s header still describes
   itself as the only way an application is found; it is now the fallback.
8. **An inherited property, stated so nobody reports it as a W5.a regression.**
   `nros sync` builds its facade candidate list from the ament scan plus
   `cargo_workspace_members(ws_root)`, and since RFC-0098 D9 there is no
   workspace root — so a GENERATED entry under `build/` is invisible to sync on
   every platform, and its facade is maintained by `generate_entry`'s heal,
   which fires only when the facade is MISSING. Migrating an image moves it from
   "sync maintains the facade" into that established class; it does not create
   the class. A `[system] ros_edition` change would therefore leave a generated
   entry's facade stale until it is deleted, for `native_entry` exactly as for
   `zephyr_entry`. Worth fixing once, for all roads, not inside 1288.

## 2026-09-30 — W5.b's emitted application is inside `build/`, and that is now a build failure

Nightly run **36672407797** (schedule, 05:13), job **109750040694**
(`tier 2 nightly (pairwise cover)`), step 5 `just build tier2-nightly`, on the
self-hosted `nano-ros-runner`. The zephyr fixture module fails, four times with
the same pair of lines:

```
== zephyr == FAILED (rc=2)
  CMake Error: The source directory
  "/home/runner/_work/nano-ros/nano-ros/examples/workspaces/rust/build/zephyr-zenoh/zephyr_entry"
  does not exist.
  ninja: error: rebuilding 'build.ninja': subcommand failed
```

That path is this issue's own W5.b output. `50f50be33` deleted the tracked
`examples/workspaces/rust/src/zephyr_entry/` and made `nros build` emit the west
application into `build/zephyr-zenoh/zephyr_entry/` instead; the workspace's
README and its `demo_bringup/system.toml` both name that directory. The run's
head `965504e38` has `50f50be33` as an ancestor and no `src/zephyr_entry`, so
this is the post-migration layout and not a stale checkout. (The commit dates
mislead here — the queue rebase-merges, so `50f50be33` carries an author date
later than the merge it precedes. `git merge-base --is-ancestor` is the only
thing that answers it.)

## What the failure actually says

`ninja: error: rebuilding 'build.ninja'` is a RE-CONFIGURE of a build directory
that was configured successfully at least once: cmake cached
`CMAKE_HOME_DIRECTORY` pointing at the emitted application, and by the time
ninja re-ran cmake the directory was gone. So the emitter did run — this is not
"the generator never fired".

The shape is the one the new layout creates: **the emitted application is both a
build OUTPUT and a cmake SOURCE directory, and it lives under the same `build/`
tree that build tooling creates, reuses and clears.** Anything that removes or
partially recreates `<ws>/build/` destroys the source tree of a build dir
configured against it, and the symptom surfaces one layer down as a cmake error
about a missing source directory rather than as a missing generated artifact.
A persistent workspace makes it reachable: the self-hosted runner keeps
`_work/` across runs, so a configured build dir from one run can meet a cleared
`build/` in the next.

## What this is NOT

- Not issue **1497** (the `zephyr_self_pkg` fixture leaves and their
  SystemModel). Different leaves, different path, and system generation is not
  where this one stops.
- Not issue **1366**. That is a gitignored `<ws>/Cargo.toml` naming a deleted
  cargo member, failing at manifest parse. This is a cmake source directory and
  a west build.
- Not `provision-zenohd` exiting 78, which appears above it in the same log and
  is the lane-skip protocol saying so in its own words (issue 1477).

## What would close this part

Either the emitted application moves out of `build/` to somewhere no build step
clears (a generated directory the tooling owns but does not treat as scratch),
or the west build dir is made to depend on the emitter such that a missing
emitted source re-emits instead of failing the reconfigure. The first is the
structural answer; the second leaves a source tree living inside a scratch
directory, which is the thing that failed.

Cross-referenced from issue 1158, which is what triage keys the tier-2 lane on.
