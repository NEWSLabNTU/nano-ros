# Phase 470 — example layout unification

**Status (2026-09-27). All work items open.** Gives `examples/` a named,
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

An example's layout is decided by two questions, and only the second has ever
been written down.

**Who owns the link?** For C and C++ the answer is always cmake, because cmake
is the only C build. For Rust it varies by platform, and that variation is the
whole of what looked like "the Zephyr layout":

| | Rust leaf files | platforms |
| --- | --- | --- |
| cargo owns the link | `Cargo.toml`, `src/{lib,main}.rs` | native, mps2-an385-{baremetal,freertos}, esp32-c3-baremetal, qemu-armv7a-nuttx, threadx-linux |
| cmake owns the link | `CMakeLists.txt`, `src/{lib,app_main}.rs`; cargo emits a staticlib exporting `app_main()` | `zephyr/rust/*`, `rv-virt-threadx/rust/*` |

`rv-virt-threadx/rust/*` is the finding: it carries the shape people call "the
Zephyr shape" and has **no `prj.conf` at all**. Its cause is unrelated — ThreadX
riscv64 uses the C startup path and Cyclone needs C descriptors, so the link
goes through `nros_threadx_rv64_rust_app`. Zephyr is *this* class plus
`prj-<rmw>.conf`. And `threadx-linux/rust/*` is in the other class: same platform
family, different answer, correctly so.

**Is it a leaf or a workspace?** A leaf is `<platform>/<lang>/<example>`
(RFC-0026). A workspace has `src/<pkg>/` and a bringup, and its entry is
generated (RFC-0098 D9) — except in 10 workspaces where it is hand-written, all
Zephyr (issue 1288).

## The taxonomy this phase lands

| class | shape | today |
| --- | --- | --- |
| **1** | workspace, generated entry | 12 |
| **1z** | workspace, Zephyr image | 15 packages serving **16** image rows |
| **1b** | workspace with no bringup (RFC-0098 D9 as amended by phase-445 W5) | `templates/workspace-shadowing` |
| **3** | leaf, cargo owns the link | the majority |
| **4** | leaf, cmake owns the link (`+ prj*.conf` on Zephyr) | `zephyr/rust/*`, `rv-virt-threadx/rust/*` |
| **X** | foreign-build integration | `examples/px4/` (issue 1516) |

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

### W3 — four template workspaces declare no root

Issue 1515. A workspace root has two tracked spellings and
`detect_workspace_root` resolves them in order: `.colcon_workspace`, else a root
`Cargo.toml` with `[workspace]`. `c-and-cpp-mixed-workspace`,
`multi-node-workspace-cpp`, `pure-c-workspace` and `multi-package-workspace`
have **neither** — all four are C/C++-led, so the second rung cannot fire for
them even in principle. Three have a bringup.

**Measure before fixing.** The survey was static; no build was run. If a
consumer resolves these today the finding is a gate, and if it does not, the
failure text is what the gate should name. `multi-package-workspace` may
legitimately not be a workspace root at all, in which case the answer is a note,
not a marker.

**Acceptance:** a recorded build or consumer invocation per tree; each resolved
on that evidence; one gate over `examples/` whose predicate is the rule ("a tree
with a bringup declares a root by one of the two spellings"), not the four sites
— the issue-0196 rule.

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

**Blocked on:** W2 (a wrong board generates a wrong application). **Blocks:**
issue 1511 (no worked example of entry customisation — deferred behind this).

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

W1, W3, W4 are independent. W2 precedes W5. W6 is independent of all of them.
W7 lands last — after W5, its rule has one home.

## What this phase does not do

- Move anything under `examples/px4/` (issue 1516 — documentation only).
- Retire `templates/workspace-shadowing`'s bringup-less shape (class 1b is a
  decision, RFC-0098 D9 as amended).
- Write the entry-customisation tutorial (issue 1511, deferred behind W5).
