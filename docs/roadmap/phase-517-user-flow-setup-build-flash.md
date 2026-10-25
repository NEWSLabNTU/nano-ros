# Phase 517 — The user's flow: one setup, one build, the platform's own run

**Status (2026-10-25). Open; W5 in review (PR #2644), the rest not started.** Opened from a gap audit of the
user-facing book against the CLI source, plus a run of both book probes
(`just probe checkout` and `just probe installed`, both PASS at `9bfed390c0`).
W1 is docs-only and can start now. W2 is decided (maintainer, 2026-10-25): cut
a release, and the book's user front door becomes the installed `nros` — no
user clones nano-ros. W8 and W11 need a design note before code.

**Builds on:** [RFC-0062](../design/0062-unified-dependency-ssot.md) (one
dependency index), [RFC-0065](../design/0065-colcon-like-workspace-builder.md)
(`nros build` as the workspace builder),
[RFC-0098](../design/0098-generated-leaf-build-config.md) (generated leaf build
config), [RFC-0099](../design/0099-provisioning-is-planned-once-and-prefers-prebuilts.md)
(planned provisioning), [RFC-0093](../design/0093-board-naming-states-reach.md)
(board names). **Prior:**
[phase-452](phase-452-copy-out-front-door.md), which made the templates and the
scaffold build; this phase does the same for the steps around them.

---

## The experience this phase delivers

Five promises, each stated the way a user meets it:

0. **No nano-ros checkout.** A user installs `nros` (`scripts/install.sh`) and
   never clones this repository. Their project's own config files —
   `package.xml` for dependencies, `system.toml` for images and boards — are
   the whole input; `nros setup` and `nros build` provision and build from them.
   The clone-and-bootstrap path is for contributors and lives in the
   contributor chapters only.
1. **One `nros setup` installs what the workspace needs**, the way
   `rosdep install --from-paths src` does. Dependencies are declared in
   `package.xml`; the board comes from the image the workspace declares.
2. **`nros build` works like `colcon build`.** It discovers the packages,
   orders them, and builds the image for the board the user names. A user
   build file states no dependency that `package.xml` already states.
3. **Running and flashing use each platform's own tool** — `qemu-system-*`,
   `west build -t run` / `west flash`, openocd, NuttX's and PX4's own targets.
   No `nros run` or `nros flash` is invented. The book says, per platform, which
   command runs the image on QEMU and which flashes a real board, and
   `nros build` prints those commands with the real paths filled in.
4. **The book is what happens.** Every step a user is told to type works on a
   clean host, and a probe proves it for each platform chapter, not only for
   Linux.

## What the audit found (2026-10-25)

Method: 660 `nros …` invocations extracted from 43 in-scope chapters, each
subcommand and long flag checked against the clap definitions; relative links
and named paths checked with `git ls-files`; ESP32 chapters skipped (dormant,
issue 1525). Every quoted subcommand and flag exists. The gaps are flows,
vocabulary and staleness.

| promise | state | evidence |
| --- | --- | --- |
| 1 setup | **not met** | `nros setup --workspace` walks every `package.xml`, reads `<depend>` and the `[image.*] board` beside it, classifies names against `[prereq.*]` and the pinned rosdep snapshot — and only prints (`cmd/setup.rs:125`, "it does not install"). The book never mentions it. Installing takes three commands (`--system --sudo`, `<board> --rmw <rmw>` per board, plus ROS for `rmw_zenohd`). |
| 1 setup | **two board vocabularies** | an image names a board-descriptor alias (`qemu-mps2-an385` ×22, `freertos` ×14, `rtic-mps2-an385` ×9, `nuttx` ×9, `nuttx-riscv`, `qemu-cortex-a53`); setup keys are the index's 12 `[board.*]` names (`mps2-an385-baremetal`, …). `--workspace` prints "not an index board key" for each (`setup.rs:5243`). The descriptor already knows the link (`nros-board-mps2-an385`'s `names` + `cmake_module = "mps2-an385-baremetal"`); nothing reads it there. |
| 2 build | **mostly met** | discovery = `package.xml` ∪ cargo members, topological; `--packages-select` / `--packages-up-to` with colcon's meaning (`cmd/build.rs:95-105`); no root build file. Gaps: the unit named is an image id, not a board; Rust leaves hand-write `nros = {…features}`, `<pkg>_nros_selection = {path = "build/nros/selection"}` and message path deps that duplicate `<depend>`; C/C++ chapters teach three flows (raw `cmake -Dnano_ros_ROOT=…`, `nros build`, `nros init` + `cmake --preset`) with different artifact paths. |
| 3 run/flash | **uneven** | linux, ThreadX (linux sim + riscv64 QEMU), PX4 (SITL, `make … upload`) covered. FreeRTOS: QEMU only, and claims STM32F4/LPC55/MSP432 boards that have no crate. Bare-metal: QEMU only. Zephyr: `west build -t run` / `west flash` given without the `-d` dir `nros build` uses. NuttX: ARM QEMU only. ARM FVP: no user commands at all. Bare `qemu-system-arm` assumes the store's patched build is on PATH. |
| 0 no checkout | **not met** | the user path IS the checkout: `installation.md` puts `git clone` + `bootstrap.sh` first and says "**No release has been cut yet**" (`:102`) while `gh release list` shows `nros 0.6.0-nros3` (2026-10-15) and two earlier. 30 user chapters (`getting-started/`, `start-here/`, `user-guide/`, `platform-guides/`, `porting/`) name `git clone`, `bootstrap.sh`, `activate.sh` or `NROS_REPO_DIR`; 48 point at `examples/`; 8 tell the user to run `just`. The installed track is probed for `first-project.md` alone. |
| 4 book truth | **not met** | stale clone tag, retired board names, contradictory Zephyr and router statements, dead links, `cli.md` missing verbs — itemised in W1/W2. |

Probe findings (both tracks PASS, 2026-10-25):

* **installed track:** on the cyclonedds first project, `nros sync`'s metadata
  probe builds `zpico-sys`, which panics (`zenoh-pico source not provisioned …
  Run: nros setup --source zenoh-pico`, `nros-zpico-build/src/runner.rs:1731`)
  because the installed SDK carries no zenoh-pico. Sync degrades to "no producer
  for talker_pkg::talker", so the image keeps worst-case pool sizes. The user
  saw a Rust panic on the documented happy path and was told to provision an
  RMW their project does not use.
* **both tracks:** a first build prints 7–10 `REFUSED` lines and paragraphs
  citing issue numbers (0991, 1033, 0900) for a hello-world image that is
  correct. That is diagnosis written for maintainers, shown to a user at
  default verbosity.

## Work items

### W1 — book truth pass (docs only; S each)

Fix each, in place. Every line here was checked against source.

* **Board names.** `installation.md:120` ("e.g. `native`"),
  `start-here/setup-compared-to-ros2.md:70`, `reference/cli.md:95-98` (lists
  `native`/`posix`, which are refused, omits `linux`), `installation.md:415`
  (`stm32f4`, not a key). List the index keys; better, generate the list (see
  the gate below).
* **Router.** One sentence everywhere: zenoh's router is ROS's `rmw_zenohd`
  (RFC-0075); `nros setup` provides none; cyclonedds needs no daemon. Fix
  `bare-metal.md:18`, `integration-zephyr.md:171,184`,
  `setup-compared-to-ros2.md:36-41` (contradicts its own :145-147),
  `cli.md:58`.
* **Release state.** One statement: no release cut yet → build from the
  checkout. `first-project.md:7` assumes `install.sh`;
  `setup-compared-to-ros2.md:136-139` says "source-only";
  `installation.md:1-6` says "no `find_package`" while every example uses
  `find_package(nano_ros)`.
* **Zephyr.** `integration-zephyr.md:187` says setup does not provide the
  Zephyr SDK (`[board.zephyr] packages = ["zephyr-sdk"]` does);
  `:191-194` say messages need `NROS_STD_MSGS_DIR` from ROS (they are vendored,
  `installation.md:202`); `:862-866` say `--nano-ros-path …/packages/core` (the
  flag takes the SDK root).
* **Stale notes and dead links.** `first-project.md:95-99` warns about issue
  1295 (resolved; link dead); `user-guide/configuration.md:398` links an
  archived phase-204; `user-guide/workflow-by-platform.md:161-163` describes a
  deletion that happened.
* **Templates.** `workspace-node-pkgs.md:132` shows
  `nros = {path = "../../../../../packages/api/nros"}`; the `nros new
  --workspace` template uses `version = "*"` plus a generated patch. Show the
  template.
* **"Path B … equivalent"** in the three first-node chapters (`cargo build
  --bin nros`) skips `nros-launch-resolve`, which `bootstrap.sh:281-283` says
  the workspace flow needs. Either make it equivalent or drop the word; it
  leaves the user chapters with the rest of the checkout material in W2.
* **`reference/cli.md`.** Add `sdk-root` (first-project uses it), `locate`,
  `store`, `toolchain`, `pin`, `update`; add `--system`, `--workspace`,
  `--check`, `--sudo` to setup and `--workspace` to `new`.

**Gate (the class, not the sites):** `check-book-board-names` — every board
word in a book code block or board list is an index `[board.*]` key or a
descriptor `names` alias, and no retired word (`native`, `posix` as a board)
appears outside `reference/retired-names.md`. Extend the existing relative-link
check to `book/src` if it does not already cover it.

**Done when** each bullet is fixed, the gate is on the fast line, and its
self-test fails on a planted `nros setup native`.

### W2 — cut a release; the book's front door is the installed CLI (decided, then L)

**Decision (maintainer, 2026-10-25):** cut a release, and move the book to `nros`
CLI provisioning. Users no longer clone nano-ros into or beside their project;
the installed SDK plus the project's config files are everything.

Today `installation.md:110` and `:526` clone `--branch nros-v0.6.0`, 663 commits
behind `main` (at that tag the host board was `native`, so the book's next step,
`nros setup linux`, fails there), and the probe never noticed because it
substitutes the branch under test for the tag. That whole class goes away when
no user step names a ref of this repository.

1. **Cut the release.** Dispatch `release-nros.yml` with a new version and
   `publish = true` (a maintainer act; the workflow creates the tag). Cut it
   after W5 lands — today a cyclonedds first project's sync panics in the
   installed SDK, and a release would ship that to every new user.
2. **`installation.md`:** the installed path (`install.sh`, `--version` to pin)
   is the only user path, first and unconditional. Delete the "no release has
   been cut" note. The clone, `bootstrap.sh`, `activate.sh` and "Path B"
   material moves to `internals/contributing.md` (or a new contributor setup
   chapter), together with W1's Path B bullet.
3. **Every user chapter** (`getting-started/`, `start-here/`, `user-guide/`,
   `platform-guides/`, `porting/`) stops assuming a checkout:
   * `examples/<…>` as a starting point → `nros new` (template) or a link to the
     example on GitHub as reading material, never a path to `cd` into;
   * `NROS_REPO_DIR` / `nano_ros_ROOT=<checkout>` → `nros sdk-root`, or nothing
     where `nros build` resolves it;
   * `just <recipe>` → the `nros` command it wraps, or out of the user book;
   * `source activate.sh` → `~/.nros/bin` on PATH, which `install.sh` prints.
   The two "(contributor)" chapters (`zephyr.md`, `nuttx.md`) either get a user
   counterpart or leave the getting-started section.
4. **The installed SDK carries what those chapters need.** Each platform chapter
   is replayed on the installed track (W10); whatever a chapter reaches that
   `stage-sdk-root.sh` does not stage, or `nros setup` does not provision, is a
   W2 defect, fixed in the staging or the index rather than by a checkout
   fallback.

**Gate:** `check-book-no-checkout` — no code block in a user chapter clones
this repository, runs `bootstrap.sh`, sources `activate.sh`, names
`NROS_REPO_DIR` or runs `just`; contributor chapters are exempt by section, not
by an allowlist of files. Its self-test fails on a planted `git clone
…/nano-ros.git` in `first-project.md`. The probe's checkout track then covers
contributor chapters only.

**Done when** a release newer than `0.6.0-nros3` is published from a `main`
that has W5, the gate is on the fast line, and `just probe installed` replays
every user chapter it can reach with no checkout in the container.

### W3 — one board vocabulary from image to setup (M)

An image's `board` resolves to its descriptor (`names` aliases); the
descriptor names its setup key. Add `setup_key = "<index board>"` to
`nros-board.toml` (or derive it from `cmake_module` where they coincide, and
refuse where they do not), and make `nros setup --workspace`,
`nros setup <board>` and `nros build`'s preflight all resolve through the same
function. `nros setup qemu-mps2-an385` then works and means
`mps2-an385-baremetal`.

**Gate:** every `[image.*] board` in a tracked `system.toml` resolves to an
index key (both directions: an index key no descriptor reaches is reported).

**Done (2026-10-25)** — `BoardCatalog::setup_board`, with the key DERIVED per
descriptor rather than a new `setup_key` field (the descriptor schema is
`deny_unknown_fields` and has several readers): its `cmake_module` when that is
an index key, else a `names` entry that is, else the key spelled like its
platform (`zephyr`). A word two descriptors claim with different keys
(`threadx`) is refused naming both. The gate is a unit test,
`every_image_board_reaches_an_index_key_and_every_key_is_reached`, which runs in
`check cli-tests` on every pull request; both directions hold today.

### W4 — `nros setup` installs the workspace (L)

`nros setup` with no board, inside a workspace (or `--workspace <dir>`),
becomes the rosdep-shaped front door:

1. Plan: the union of every image's board set (W3), each image's RMW, every
   `<depend>`/`<build_depend>`/`<exec_depend>` resolved through `[prereq.*]`
   and the rosdep snapshot, and the `--system` OS closure.
2. Print the plan (what is present, what will be fetched, which OS packages
   need root, per package manager).
3. Install: store items without root; OS packages only with `--sudo` (never
   silently), or print the exact command. `--check` keeps today's report-only
   behaviour; `--yes` skips the confirmation.
4. ROS-provided pieces (`rmw_zenohd` for a zenoh image) are planned like any
   other prereq: present → OK; absent → the apt package name for the host's
   distro and the ROS apt source it needs, installed only with `--sudo` when the
   source is configured.
5. A `<depend>` that is neither a workspace package nor a known key is an
   error naming the package.xml line, not a silent skip.

`nros build`'s preflight refusal (`check_declared_depends`, `build.rs:5396`)
then names `nros setup` (no args) instead of a board.

**Done (2026-10-25), with three items left:** plain `nros setup` at a workspace
root (a `package.xml` there or in a child of `src/`) is `nros setup --workspace
.`; it prints the report, provisions every image's `(board, rmw)` in one store
plan, reports ROS's `rmw_zenohd` for a zenoh image (found, or the apt names),
then the OS closure (installed with `--sudo`, else printed); `--check` stays
report-only. Measured in `examples/workspaces/c` with `--dry-run`. Left:
(1) the OS closure is the index's whole `[prereq.*]` set, not the workspace's
own keys; (2) an unknown `<depend>` is counted, not refused — telling a ROS
package from a typo needs the ament index; (3) no `--yes`, because nothing asks
for confirmation (`nros setup <board>` never did either).

**No checkout:** the plan is computed from the project's config files and the
installed SDK alone; nothing in it reads or asks for a nano-ros checkout.

**Done when** an installed-track probe runs `nros setup` once in a fresh container (no checkout) on a
workspace with a zenoh image and a FreeRTOS image, then `nros build --all`
succeeds with no further provisioning. The book's install and first-project
chapters, and `cli.md`, teach this command first.

### W5 — the installed SDK's first build is clean (M)

* The metadata probe must not build an RMW the image does not select
  (cyclonedds image → no `zpico-sys`). If it legitimately needs zenoh-pico,
  the installed SDK ships it or `nros setup` provisions it as part of W4's plan.
  A probe build failure is a warning with one actionable line, never a Rust
  panic dump.
* **Default output.** A correct build prints stage lines and the artifact
  path. `REFUSED` provenance notes, "over-provisioned" hints and issue-number
  prose go to `-v`/`--explain` (they stay in `nros-plan.json`). A warning shown
  at default verbosity must say what the user should do.

**Done when** `just probe installed`'s log for the first project contains no
panic, no `REFUSED`, and no issue numbers, and the image derives its pools (no
"no producer" line).

**Progress (2026-10-25, PR #2644).** The probe compiles the backend the
workspace's images select, so the cyclonedds first project builds no
`zpico-sys` and both components are probed ("2 rebuilt"). The false
`-DNROS_RMW=zenoh is OVERRIDDEN` warning on every re-configure is gone (the
generated root caches the image's RMW). Each clean-build note is one line; the
reasoning prints under the new `nros build -v` or `cmake --log-level=VERBOSE`
through `nros_message_detail()`, and `check-cmake-user-output-refs` ratchets the
63 default-level cmake messages that still cite an issue, RFC or phase. Measured
with `just probe installed`: no panic, no `REFUSED`, no "no producer", no issue
number in cmake output.

**Remaining — the pools are still not derived.** The probe's sidecars only
FLOOR a launch-file contract (`metadata_refresh::contract_inventory`); with no
`<bringup>/launch/*.contract.yaml` — and the template ships none — every
component's declaration stays `Absent`, the entity inventory refuses, and the
image keeps its configured `NROS_EXECUTOR_MAX_CBS`. Correct, not tight. Closing
it needs a sizing decision first (RFC-0100): may the fresh probe sidecars of a
workspace stand as a component's declaration when no contract names it, and
with what freshness rule? Then the template either relies on that or ships a
contract.

### W6 — build by board, and print the platform's next command (M)

* `nros build --board <b>` builds the image(s) whose board resolves to `<b>`
  (W3's resolver); with one match it is the same as naming the image, with
  several it lists them. Image ids stay the primary unit (RFC-0065).
* After a successful build, `nros build` prints the platform-native follow-up
  commands with real paths, read from the board descriptor (it already carries
  `runner` for cargo boards and `runner = "armfvp"` for FVP):
  `qemu-system-arm … -kernel <path>`, `west build -d <dir> -t run`,
  `west flash -d <dir>`, the NuttX/PX4 target. It runs nothing.
* QEMU invocations name the store's patched build (`nros locate
  qemu-system-arm`), never assume PATH.

**Done when** each tier-1/2 board's build prints a command that, pasted, runs
the image; a test asserts the printed command for one cargo, one west and one
cmake board.

**Done (2026-10-25)** — `nros build --board <b>` (`cmd::build_next`): an image
matches when its authored board equals `<b>` (any case), both resolve to one
descriptor, or `<b>` is the index key its descriptor provisions through (W3's
`setup_key`). One match is planned exactly as `nros build <image>`; several or
none is refused naming the qualified ids (and each image's board); with image
arguments or `--all` it is refused. After a successful build (not `--dry-run`,
not `--check`) each image prints its artifact and next command from ONE
function, `build_next::next_steps(plan, descriptor, qemu)`, which runs nothing.
The artifact location is a plan fact (`ResolvedBuild::output`), recorded where
the handoff is assembled: cargo's target dir (the caller's `--target-dir` /
`CARGO_TARGET_DIR` when named), triple, profile and the manifest's one bin;
cmake's `-B` dir plus the generated entry (or a leaf's own
`nano_ros_add_executable` name); west's `-d`. Per class: host boards print the
binary; a descriptor `runner` (mps2-an385, its FreeRTOS twin, mps3-an536) is
printed with QEMU from the ladder; NuttX (arm `virt`, rv-virt) and ThreadX
riscv64 print the test harness's QEMU line; west prints `-t run` / `west flash`
from what the build's own `build.ninja` (`run_<emu>`) and `runners.yaml`
(`flash-runner`) say; FVP prints `-t run` plus the configure-time model note;
anything else (S32Z270) names its book chapter. QEMU comes from
`orchestration::qemu_bin` — the CLI twin of `nros_qemu_bin` /
`nros_tests::qemu::qemu_system_path`, without the `$PATH` rung — also exposed
as `nros locate tool qemu-system-<arch>`; with nothing provisioned the printed
command asks `nros locate` at paste time and a note says to run
`nros setup --tool qemu`. Tests: `build_next` unit tests (one cargo, west, FVP,
cmake host + cross, NuttX/ThreadX/S32Z270, `--board` one/many/none, a leaf) and
three plan-level tests in `build_verb_pipeline.rs`.

**Left:** no printed command has been pasted on every tier-1/2 board yet (the
done-when's "runs the image" half is verified only for the shapes the tests
pin); the three QEMU invocations derived for NuttX/ThreadX riscv64 restate the
harness rather than share code with it (the harness is a test crate); the
`nros-tests` QEMU ladder and this one are two Rust spellings of one ladder;
no `-v` flag exists to document.

### W7 — one build flow per language in the book (M)

* **Zephyr:** `nros build` is primary everywhere; `nros new entry` is the
  escape hatch; raw `west build` is the contributor path. Fix
  `user-guide/deployment.md:108-126` (says entries are "not yet generated",
  citing resolved issue 1288), `integration-zephyr.md:334-380`,
  `user-guide/build-artifacts.md:154-173` (`./build/zephyr/zephyr.exe`
  contradicts its own path; follow-up `west` commands need `-d`, build.rs:4609).
* **C/C++:** `first-node-c.md`, `first-node-cpp.md`,
  `workflow-by-platform.md:174-191`, `freertos.md:146-156`,
  `your-own-msg-package.md:182` lead with `nros build`; raw `cmake
  -Dnano_ros_ROOT=…` with a hand-picked toolchain file becomes the documented
  escape hatch ("Build as a CMake subdirectory"), and the artifact path quoted
  is the one `nros build` writes.

### W8 — no dependency boilerplate in Rust build files (L; design first)

Today a Rust node or entry hand-writes `nros = {version = "*", features =
[…]}`, `<pkg>_nros_selection = {path = "build/nros/selection"}`, and a path dep
per message package that repeats `<depend>`. Adding a `<depend>` is two edits.

Design note first (an RFC-0098 amendment): which of these `nros sync` derives
from `package.xml` + the image, and how it reaches cargo without editing a
tracked `Cargo.toml` — candidates are injecting `[dependencies]` through the
generated `nros-cargo.toml` (cargo cannot add deps through `--config`, so this
needs measuring) versus a generated `Cargo.toml` fragment the user's manifest
`include`s versus a generated wrapper crate. C/C++ is the reference: a
`<depend>` already reaches `nros_generate_interfaces()` with no second edit.

**Done when** the RFC amendment is accepted and a Rust node package in the
scaffold carries no message path dep and no selection dep, and adding a
`<depend>` is one edit.

### W9 — flash and run, per platform, honestly (M)

For each board in `packages/boards/board-support.toml` at tier 1–3, the
platform chapter has a **Run on QEMU / simulator** block and, where a real
board is supported, a **Flash a real board** block, each in the platform's own
convention. Concretely:

* **FreeRTOS (mps2-an385, tier 2):** QEMU via the store's `qemu-system-arm`.
  Delete the STM32F4/LPC55/MSP432 claim (`freertos.md:251-253`; no such board
  crates); point to the porting guide for a new board instead.
* **Bare-metal (mps2-an385):** QEMU, and the porting-guide pointer for probe-rs
  / openocd on a user's own board.
* **Zephyr:** native_sim and QEMU with `west build -d <dir> -t run`; real boards
  with `west flash -d <dir>` for the tier-3 hardware boards; fix
  `qemu_cortex_a9` being called "a real board".
* **NuttX:** both QEMU targets (ARM `-M virt` and RISC-V `rv-virt`), booting the
  image `nros build` produced. Verify `integration-nuttx.md:174`'s
  `-DBOARD=… -DCONFIG=…` against NuttX's own CMake (`-DBOARD_CONFIG=`); fix if
  wrong.
* **ThreadX:** linux sim and riscv64 QEMU (already present); state that no real
  board is supported.
* **S32Z270 (tier 3):** the vendor's flash flow, or an explicit "build-only;
  flash with your S32 tooling" note.
* **PX4:** verify `px4.md:58-100` (configures one tree with
  `EXTERNAL_MODULES_LOCATION`, then `make px4_fmu-v5_default` builds another).
* **ARM FVP:** add `nros build <bringup>:fvp` then the `armfvp` runner
  invocation (license-gated; say so).

**Gate:** `check-book-board-run-sections` — every tier 1–3 board has a run
block in the chapter its registry row names, and a board flagged as hardware
has a flash block.

### W10 — every platform chapter is probed (L)

The probe proves the Linux checkout and installed tracks. Extend the
INSTALLED track (W2: user chapters have no checkout track) to the QEMU
chapters: FreeRTOS, Zephyr native_sim, NuttX (both), ThreadX riscv64,
bare-metal — each replaying its chapter's `probe=NN` blocks in a clean
container and asserting the documented output. These are opt-in `just probe
<platform>` recipes plus a weekly scheduled lane (no runner is started on a
developer host). The SUSPECTED items in W9 are settled by these runs, not by
reading.

**Done when** each platform chapter has a probe track, the weekly lane runs
them, and a planted wrong command in one chapter fails its track.

### W11 — the project states its nros version (design first, M)

With no checkout, the SDK version is the one thing a project's config files do
not yet say, so two users of one project can build it with different SDKs and
nothing tells them. `install.sh --version` pins it per host, not per project.
Design note first (an RFC-0097 amendment: which file holds it, and what
"compatible" means given only `codegen` promises anything): a workspace states
the `nros` release it builds with, the way `rust-toolchain.toml` states a
channel, and `nros setup` installs that version into the store when it is
absent, while `nros build` refuses (naming `nros setup`) when the running CLI
does not satisfy it.

**Done when** the design is accepted, `nros new` writes the field, and a probe
on a project pinned to an older release installs and builds with it.

## Order and dependencies

* W1 now (docs). W5 before W2's release cut; W2's book move after the release
  exists, chapter by chapter as W10 proves each on the installed track.
* W11 after W2; its design note can start any time.
* W3 → W4 (setup needs one vocabulary) → W6's `--board` (same resolver).
* W5 independent; small enough to land beside W1.
* W7 and W9 are docs; W9's real-board blocks wait on W6's printed commands so
  the book and the CLI say the same thing.
* W8 starts with its design note; no code before it is accepted.
* W10 last per platform, after that platform's W7/W9 text is final.

## Non-goals

* An `nros run` / `nros flash` verb. Promise 3 is the platform's own tool.
* ESP32 (dormant, issue 1525).
* New board support. W9 documents the boards that exist and removes claims
  about boards that do not.
* Replacing rosdep for ROS's own packages. W4 installs what nano-ros images
  need, using rosdep's key names (RFC-0062), and does not consult rosdep.

## Acceptance for the phase

A new user on a clean Ubuntu host follows the book with no nano-ros checkout:
`install.sh`, `nros new`, one `nros setup`, `nros build` for a named board, then the command
`nros build` printed — and the image runs, for Linux and for each QEMU
platform, with the probe for that chapter green and no step outside the book.
