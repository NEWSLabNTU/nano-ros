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
