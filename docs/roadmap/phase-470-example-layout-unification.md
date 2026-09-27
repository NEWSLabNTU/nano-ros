# Phase 470 — example layout unification

**Status (2026-09-27). W1–W4 LANDED; W5–W7 open.** W5 is unblocked: W2 answered
the board question it waited on, and answered it differently than either this
phase or issue 1517 predicted (see W2). Gives `examples/` a named,
measured taxonomy; collapses the shapes that differ for no reason; and documents
the ones that differ for a reason. Implements no new RFC — it finishes
[RFC-0026](../design/0026-example-directory-layout.md) (standalone copy-out leaves)
and [RFC-0098](../design/0098-generated-leaf-build-config.md) D9 (entries are
GENERATED) across the trees that were never migrated to them.

**Prior:** phase-331 (workspace consolidation, the `ws-*` dirs), phase-383 W10.a
(`.colcon_workspace` as the tracked root marker), phase-445 W5 (generated
entries + package mode), RFC-0065 (the five-stage pipeline),
RFC-0034 D6 (the bare-metal allocator).

**Source:** a layout survey run 2026-09-27. Six issues came out of it — 1288
(extended), 1509, 1511, 1512, 1515, 1516 — plus three defects the survey found
while measuring: 1517, 1518, and the `aux_pkg` priority restatement already
fixed.

## What the survey measured

**Read the caveat first: the survey that opened this phase ran on a checkout 882
commits behind `main`.** That is the sole cause of W3's false premise — three of
the four template workspaces it reported as having no root marker had carried one
since phase-445 W5, sixteen days before the issue was filed. Every other claim
below was re-verified against `main` afterwards and held, but the lesson is the
one issues 0859–0862 already paid for: a measurement is only about the tree it
was taken on, and a survey is a measurement. Check out `main` before surveying,
and state which tree a finding came from.

An example's layout is decided by two questions, and only the second has ever
been written down.

**Who owns the link?** For C and C++ the answer is always cmake, because cmake
is the only C build. For Rust it varies by platform, and that variation is the
whole of what looked like "the Zephyr layout":

| | Rust leaf files | platforms |
| --- | --- | --- |
| cargo owns the link | `Cargo.toml`, `src/{lib,main}.rs` | native, mps2-an385-{baremetal,freertos}, esp32-c3-baremetal, qemu-armv7a-nuttx, threadx-linux |
| cmake owns the link | `CMakeLists.txt`, `src/{lib,app_main}.rs`; cargo emits a staticlib whose exported entry symbol cmake calls | `zephyr/rust/*`, `rv-virt-threadx/rust/*` |

**The two cmake-owned families do NOT share the exported symbol, and reading one
into the other is exactly the re-derivation this section exists to stop.**
Measured: `nros::zephyr_component_main!` emits
`#[unsafe(no_mangle)] pub extern "C" fn rust_main()`, the zephyr-lang-rust
convention `rust_cargo_application()` consumes; the ThreadX RV64 board's
`app_main!` emits `pub extern "C" fn app_main() -> !`. Same class, two
conventions, because the class is about WHO LINKS, not about one symbol name.
(An earlier draft of this table said both export `app_main()`.)

`rv-virt-threadx/rust/*` is the finding: it carries the shape people call "the
Zephyr shape" and has **no `prj.conf` at all**. Its cause is unrelated — ThreadX
riscv64 uses the C startup path and Cyclone needs C descriptors, so the link
goes through `nros_threadx_rv64_rust_app`. Zephyr is *this* class plus
`prj-<rmw>.conf`. And `threadx-linux/rust/*` is in the other class: same platform
family, different answer, correctly so.

It is a stronger finding than "one family looks like another": **both of its RMWs
build through cmake** — `builder = "cmake"` on its zenoh *and* its cyclonedds
`fixtures.toml` rows, since phase-369 W2 retired the cargo row and W3 deleted
`src/main.rs`. Its own `Cargo.toml` still says the `rlib` "keeps the existing
pure-cargo binary path intact"; phase-369 W3 falsified that comment, and it is
worth deleting the next time someone is in that file.

**Is it a leaf or a workspace?** A leaf is `<platform>/<lang>/<example>`
(RFC-0026). A workspace has `src/<pkg>/` and a bringup, and its entry is
generated (RFC-0098 D9) — except in 10 workspaces where it is hand-written, all
Zephyr (issue 1288).

## The taxonomy this phase lands

**Classes 1 and 1z are properties of an IMAGE, not of a directory** — the first
draft of this table counted directories and the count was not reproducible.
`examples/workspaces/rust/` alone declares 17 `[image.*]` rows, 15 of them class
1 and two (`zephyr`, `zephyr_robot1`) class 1z. So a workspace is generally class
1 *and* 1z at once, and the membership below is stated as a predicate rather than
a number, for the reason W1 landed: a count is a fact every reader re-verifies
and every maintainer re-measures.

| class | shape | how to enumerate it |
| --- | --- | --- |
| **1** | workspace image with a generated entry | an `[image.*]` row with no `entry =` |
| **1z** | workspace image whose entry is a hand-written Zephyr west application | an `[image.*]` row whose `entry =` names a package calling `find_package(Zephyr)` — 16 such rows, served by 15 packages (issue 1288) |
| **1b** | workspace with no bringup (RFC-0098 D9 as amended by phase-445 W5) | `templates/workspace-shadowing` (no `system.toml` at all) and `templates/local-msg-package` (a `system.toml` beside a *package* — package mode). The amendment names both |
| **3** | leaf, cargo owns the link | a `<platform>/<lang>/<example>/Cargo.toml` with no `CMakeLists.txt` beside it |
| **4** | leaf, cmake owns the link (`+ prj*.conf` on Zephyr) | the same with a `CMakeLists.txt`: `zephyr/rust/*`, `rv-virt-threadx/rust/*` |
| **X** | foreign-build integration | `examples/px4/` (issue 1516) |

The 16 in class 1z is the one count kept, because W5 is sized by it and because
15-packages-for-16-rows is itself the finding (`realtime-c`'s single
`zephyr_entry` serves two bringups).

Two classes named in the survey's first draft do not survive, and saying why is
part of the deliverable:

- **"class 2", hand-written entries**, is not a class — it is class 1z before
  1288. A shape that exists only because a generator is missing should not be
  given a name that makes it look intentional.
- **"class 3r", Rust-only leaf families**, is not a class either. It looked like
  a platform port was missing; the measurement (issue 1512) says the bare-metal
  heap has existed since RFC-0034 D6 landed, `cmake/platform/nano-ros-baremetal.cmake`
  is complete, and the board cmake module's own header says it survives as the
  C/C++ seam — with zero live consumers. The C road was built and never driven.
  3r is feature wiring, and it dissolves into class 3.

## Work items

### W1 — RFC-0098's count, and 1288's unfollowable reference

Issue 1518. RFC-0098 line 242 says "the eight Rust west entries"; there are
seven, and there were seven when 1288 was filed. 1288's title has been corrected;
the RFC citing it has not, so the wrong number reads as corroboration. Also:
`related: [1108]` resolves to nothing — 1108 is archived.

Do the sweep, not the site: look for other carriers of the count, and prefer
dropping the number to correcting it. A sentence that says "the Rust west
entries" cannot drift.

**Acceptance:** RFC-0098 right or silent on the count; 1288 reaches 1108; the
sweep command recorded.

**DONE (2026-09-27).** The number was DROPPED at every carrier, not corrected.
The sweep found **five** live carriers, not the one this item assumed: RFC-0098
line 242, three prose lines inside 1288 itself (the opening sentence, the
acceptance and the fix — so the table 1288's own re-measurement credited as "the
only thing that had counted" disagreed with the whole file, not just the title),
`docs/issues/1511-*.md`, and `docs/roadmap/phase-445-*.md`. `related: [1108]`
needed no change: the bare id IS the convention (86 open issues cite an archived
id that way and `just issues --id 1108` resolves it), and 1288 already spells the
archived path in prose. Recorded in `docs/issues/archived/1518-*.md`.

### W2 — `[image.fvp]` declares a board its application contradicts

Issue 1517. The row says `board = "native_sim/native/64"`; `fvp_entry` hard-codes
`nano_ros_use_board(fvp-aemv8r-smp)` and is built by `west build` with no `-b`,
so the declared value has never had to be right. The comment justifying
`entry =` — "two entry packages target this board, so the application cannot be
derived" — is **manufactured by the wrong value**: they target different boards,
so with the row telling the truth there is nothing to disambiguate.

This blocks W5: generating an entry from a row that names the wrong board
generates an application for the wrong board.

**Acceptance:** the row names the board the application builds; the `entry =`
comment says something true, or `entry =` goes away because the derivation now
works — decided by running the derivation, not by the paragraph above.

**DONE (2026-09-27), and the paragraph above was wrong.** `board` is
`fvp-aemv8r-smp`, from the descriptor's `names`; the west spelling also resolves
and was ruled out by `ImageBlock::board`'s own doc-comment ("NEVER a framework's
own board string"). But **correcting the board does not make the application
derivable — the derivation goes from two candidates to ZERO.** The resolver
matches a package by the board its `DEPLOY` token resolves to, and a Zephyr
entry's `DEPLOY` names the PLATFORM (`zephyr`), because that is what
`NanoRosEntry.cmake`'s link gate compares against; the board the application
targets lives inside its own `CMakeLists.txt`, which the token scan never reads.
So `entry =` stays, with a comment that states the measured reason. Changing the
entry's `DEPLOY` to the board id would make it derivable and was rejected: the
link gate would stop matching and the image would link no nodes.

Two defects found on the way, **each of which would have made the correct value
worse than the wrong one** — which is the reason this item had to precede W5
rather than merely being tidy:

- `nros build` handed the authored board id straight to `west -b`, reading the
  OUTER `BoardDescriptor::west_board` that **no in-tree descriptor declares**. One
  rule now, `BoardDescriptor::west_build_board`.
- `check-deploy-board-resolves` FAILED on the correct value: it globbed
  `packages/boards/*/nros-board.toml` while the authority it speaks for descends,
  and the FVP descriptor is a level deeper. Issue 0196's shape, and an active
  obstacle — writing the right value turned the fast line red.

No gate for row-vs-application agreement: 10 `entry =` rows and exactly ONE
application declaring its own board, so the rule's population is one and W5
removes even that. The Zephyr `-b` projection was pinned by a test instead.

The manufactured ambiguity claim lived in **six** places, and after fixing them
the tree has **no measured example of the ambiguity arm at all** — so RFC-0085's
count carries a dated correction rather than a silent re-count. Follow-ups filed:
**1519** (23 rows write `board = "zephyr"` and reach `-b zephyr`, which west does
not know; 10 more work BECAUSE they author the forbidden framework string) and
**1520** (the prose class; 8 of 10 `entry =` rows still unclassified).

**What W5 inherits:** the row is trustworthy for the board IDENTITY, which is
what the generator needs. Two caveats — resolve the `-b`/`nano_ros_use_board`
argument through `BoardDescriptor::west_build_board`, never the authored string;
and `entry =` must stay named until the generator exists, because nothing in the
row reaches the derivation's `DEPLOY`-token matcher. Not verified: the FVP image
does not build here (Zephyr 3.7 workspace, `aarch64-zephyr-elf`, Arm FVP), so
"the row agrees with what its application builds" rests on the `-b` string and
the board crate's projected `NROS_BOARD_ZEPHYR_ID`, not on an image.

### W3 — four template workspaces declare no root — **LANDED, and the premise was false**

Issue 1515 (resolved, archived). The item as written said all four trees have
NEITHER tracked spelling. Measured: **three of them carry `.colcon_workspace`,
tracked, since phase-445 W5** — sixteen days before the survey — and the fourth
is measurably not a workspace root. One of the three could not have been missing
at all: `cargo-nano-ros`'s scaffold `include_str!`s
`multi-node-workspace-cpp/.colcon_workspace`, so its absence is a CLI compile
error.

So "measure before fixing" is what this item bought. Adding four files would have
added three no-ops and one false statement.

The measurement also corrected what the marker is FOR. `nros build` and
`nros sync` never ask `detect_workspace_root` for the tree — `--workspace`
defaults to the CWD and the generated entry is handed `NROS_WORKSPACE_ROOT` by
`builder::cargo_config` — so removing a marker leaves their output
byte-identical, on a C/C++ tree and on a Rust one. Rung 2 is load-bearing for a
BARE `cargo` on the generated entry, whose own `[workspace]` table stops rung 3
at the entry directory: without it, `nros::main!` reports
`pkg \`demo_bringup\` not found in workspace …/native_entry. Known pkgs: []`.
That is a contributor iterating with `cargo check`, rust-analyzer, and every
copied-out tree (where rung 4 has no `.git` to fall back to).

**Landed:** no marker added; `multi-package-workspace/README.md` states why it is
not a workspace root (no bringup, three independent single-package projects, and
it builds copied out with 3 artifacts); gate
`check-bringup-workspace-root` on the fast line, whose subject is every tracked
`system.toml` with no package manifest beside it — 33 bringups repo-wide, not the
four sites and not `examples/` alone — with the walk bounded strictly below the
repository root, because the repo's own `[workspace]` would otherwise make the
gate unfailable.

### W4 — the taxonomy, and PX4's exception

Issues 1516 and the table above. Write the class list where a survey will find
it: `examples/README.md` plus `examples/workspaces/README-layout.md`, which
already owns the naming rules for workspaces.

State PX4's exception with its cause — `EXTERNAL_MODULES_LOCATION` mandates
`src/modules/<name>/{CMakeLists.txt,Kconfig}`, so those trees are copy-INTO-PX4
sources with no `system.toml` to carry; and its sub-dir axis is the transport
case (in-firmware uORB C++ vs XRCE-DDS companion Rust), not the language, so the
missing `cpp/` companion and `rust/` firmware module cannot exist. No files under
`examples/px4/` move.

Name the link-ownership axis explicitly, and name `rv-virt-threadx/rust/*` as
class 4 — otherwise the next survey re-derives "class 4 = Zephyr" and
re-discovers the outlier.

**Acceptance:** a reader following the taxonomy reaches every tree under
`examples/` and is told which class it is and why, without reading an issue.

**DONE (2026-09-27).** Landed in `examples/README.md` (a new `## Layout classes`
section), `examples/workspaces/README-layout.md` (the workspace half, at the
head, ahead of the existing naming rules) and `examples/px4/README.md`. Every
class predicate is a **runnable command**, not a frozen count — W1's lesson,
applied without being asked for.

It also corrected the stale enumerations it had to walk past, which is the
argument for predicates over counts stated as evidence rather than as opinion:
`examples/README.md` listed 28 `ws-<topic>-<lang>` workspaces phase-331 W2/W4
had folded away (`realtime-cpp` five times over), and two remaining `<rmw>/`
paths where zero remain (the `aemv8r` pair left with the FVP code nothing ran,
issue 0537); `README-layout.md`'s coverage table named four generated
`[image.*]` rows as PACKAGES, sending readers after directories that cannot
exist, and missed four real gaps; `examples/px4/README.md`'s own "Cases" table
named neither of the two trees PX4 actually builds (phase-316 W3.1 moved the one
it named). All re-derived from the tree.

### W5 — generate the Zephyr workspace entries

Issue 1288, the large item. 15 hand-written packages serve 16 image rows
(`realtime-c`'s single `zephyr_entry` serves `[image.zephyr]` in both
`demo_bringup` and `smp_bringup`, chosen at configure time by `if(CONFIG_SMP)` —
two images, which the bringups already are). All 15 call `find_package(Zephyr)`,
`fvp_entry` included.

Smaller than it looks, in one respect: **the declaration home already exists and
is already wired.** All 16 Zephyr image rows carry `board =` and `conf =`;
`ImageBlock::conf` is documented as per-image; `builder::zephyr::resolve_in`
already searches `<bringup>/boards/<board with '/'→'_'>/<name>` first, then the
app, then the bringup, and passes rung 1 as `-DAPPLICATION_CONFIG_DIR`. And
`git ls-files 'examples/**/*_bringup/boards/*'` returns nothing — the destination
is wired and empty. Only the fragment CONTENT is homeless.

Larger than it looks, in another: the generator needs **five** shapes, not two.
Beyond project name and the `add_subdirectory` list, the C/C++ entries also vary
by `LANG c`, `PANIC platform` (4 of 8), `mixed`'s `NROS_WS_RUST_NODE_DIRS` plus a
`nano_ros_workspace_pkg_guard` stub before `find_package`, `realtime-c`'s
`if(CONFIG_SMP)` bringup switch, and `fvp_entry`'s `nano_ros_use_board` +
`EXTRA_CONF_FILE`. Each is still a declaration an image row can carry. The Rust
`CMakeLists.txt` are not uniform either: one maps three RMWs and generates
Cyclone descriptors, six are zenoh-only.

The Rust `lib.rs` files are nearly uniform — all seven use
`nros::main!(launch = …)`, **none** uses `model =` (the token survives only in
two stale doc comments sitting three lines above a `launch =` line). The varying
part is the launch target and `zephyr_entry_robot1`'s `args`, both already on the
image row, which is the argument for generating rather than against it. All seven
`build.rs` are byte-identical.

**One fact is genuinely undeclared and stops `safety` collapsing:**
`rust_safety_listener_pkg = { …, features = ["safety-e2e"] }` — a per-node cargo
feature no `[[component]]` row expresses. It is the only non-default node dep
across all seven manifests. Either `[[component]]` gains a features field or
`safety` keeps a hand-written entry with that reason recorded.

**UNBLOCKED (2026-09-27)** — W2 landed the board identity. Inherit its two
caveats: resolve the `-b`/`nano_ros_use_board` argument through
`BoardDescriptor::west_build_board`, never the authored string; and expect the
`DEPLOY`-token matcher to be no help, because a Zephyr entry's `DEPLOY` names the
platform, so the generator cannot lean on the existing derivation to find its own
application. **Blocks:** issue 1511 (no worked example of entry customisation —
deferred behind this).

**Acceptance:** each migrated workspace loses its `*_entry` package and gains
per-image Kconfig and board declarations; `nros build` generates the west
application the way it generates the cmake root today; the images build.
`templates/workspace-shadowing` is class 1b and out of scope.

### W6 — the bare-metal C/C++ arm

Issue 1512, which dissolves class 3r. Three parts:

1. **No `platform-bare-metal` on `nros-c`/`nros-cpp`** — measured:
   `cargo check -p nros-c --features platform-bare-metal` →
   "does not contain this feature". Three of the four template lines transcribe
   unchanged; the fourth does not, because `nros-platform/platform-X` has no
   single X on bare metal (three board-specific names where every other platform
   has one). Two shapes are on the table — per-board arms, or an arm that omits
   the line and leaves the concrete platform to the board crate, which is what
   the Rust road and `builder/entry.rs` already do. **Neither is picked**; pick
   one with a reason.
2. **Three spellings of one platform** — `bare-metal` (descriptors,
   `PlatformKind::kebab()`, the zenoh feature), `baremetal` (`cmake_deploy()`,
   `NANO_ROS_PLATFORM`), `esp32`. `nros_feature_set()`'s PLATFORM ladder has no
   bare-metal arm, so a call lands in the `elseif(_cross)` catch-all and asks for
   `platform-baremetal`: it fails hard rather than building wrong, but the two
   roads disagree.
3. **A board that has a heap declares it does not** —
   `nros-board-mps2-an385/nros-board.toml` says
   `[board.capabilities] heap = false` ("Pure bare-metal: no heap (static
   only)") while the same board runs a 128 KB `FreeListHeap` in `.bss`, installs
   the `#[global_allocator]`, and defines `malloc`/`free`/`realloc`/`calloc` by
   default. Consequence is purely C/C++, which is why nobody has hit it: no
   `NROS_PLATFORM_HAS_MALLOC`, so the issue-0038 compile gate rejects an
   `nros-cpp` heap-container TU on a board that has a heap. The other bare-metal
   board declares `heap = true`.

Part 3 is a per-board capability decision, not a build flag: declare the heap and
ship the full C surface, or keep `heap = false` and offer a permanently-subset
alloc-free C profile. Also: `examples/README.md`'s known-gaps row states this gap
with a cause ("assume a hosted RTOS for startup, heap, libc, RNG, and clock")
that is now stale in all five terms, so it aims the reader at a port that does
not need doing.

**Acceptance:** a C or C++ leaf builds under `examples/mps2-an385-baremetal/`;
one spelling of the platform; `examples/README.md`'s gaps row true.

### W7 — the node-package invariance gate

Issue 1509, filed and deferred. A node package documents what it DOES, not which
platform or RMW carries it. Code-only violations are already **0**; the raw-text
baseline is **10** after the `aux_pkg` and Class C/D relocations (was 12):
five `::setvbuf`, four in `Listener.c`/`QosListener.c`, one in
`rust_heartbeat_pkg` — where the platform names justify `#![no_std]` on the next
line and are correct.

This is the same invariance W5 enforces one level up, which is why it belongs in
this phase and not before it: the gate's rule is easier to state once entries are
generated, because then there is exactly one place a platform may be named.

**Acceptance:** per the issue.

## Order

W1, W3, W4 are independent and have LANDED. W2 preceded W5 and has landed, so
W5 is open and ready. W6 is independent of all of them. W7 lands last — after W5
its rule has one home.

## What this phase does not do

- Move anything under `examples/px4/` (issue 1516 — documentation only).
- Retire `templates/workspace-shadowing`'s bringup-less shape (class 1b is a
  decision, RFC-0098 D9 as amended).
- Write the entry-customisation tutorial (issue 1511, deferred behind W5).
