# Phase 517 — The user's flow: one setup, one build, the platform's own run

**Status (2026-10-31). Complete, archived. W1–W17 are done; each work item records what landed. Release `0.6.0-nros5` is cut and pinned. Every platform chapter is replayed on the installed track, both language roads included, by the 13 `probe-platforms.yml` tracks (11 platform tracks plus `workspace` and `pinned`); NuttX riscv Rust got its track later (issue 2178). Three loose ends left the phase as issues: [2168](../../issues/archived/2168-probe-installs-published-release.md) (the pinned track with a newer CLI installed), [2169](../../issues/archived/2169-printed-run-commands-per-board.md) (the printed run command run per board) and [2170](../../issues/archived/2170-harness-hand-spelled-qemu-launchers.md) (the harness's sibling-pair QEMU launchers). Three more gaps were filed 2026-10-31: [2176](../../issues/2176-unprobed-fvp-sitl-flash-lines.md) (FVP, SITL and flash lines run by no probe), [2177](../../issues/archived/2177-mps2-an385-zephyr-network-unmeasured.md) (the `mps2-an385-zephyr` image's network) and [2178](../../issues/archived/2178-nuttx-riscv-rust-no-probe-track.md) (NuttX rv-virt Rust has no track). The rest of the **Left** lists either stays recorded where it lives (the dormant `idf.py` text, issue 1525) or was found already fixed on main (2026-10-31: the C/C++ scaffolds write a full board name, `--platform zephyr --lang c` writes a Zephyr application, a C package's next step is `nros build`).** Opened from a gap audit of the
user-facing book against the CLI source, plus a run of both book probes
(`just probe checkout` and `just probe installed`, both PASS at `9bfed390c0`).
W1 is docs-only and can start now. W2 is decided (maintainer, 2026-10-25): cut
a release, and the book's user front door becomes the installed `nros` — no
user clones nano-ros. W8 and W11 need a design note before code.

**Builds on:** [RFC-0062](../../design/0062-unified-dependency-ssot.md) (one
dependency index), [RFC-0065](../../design/0065-colcon-like-workspace-builder.md)
(`nros build` as the workspace builder),
[RFC-0098](../../design/0098-generated-leaf-build-config.md) (generated leaf build
config), [RFC-0099](../../design/0099-provisioning-is-planned-once-and-prefers-prebuilts.md)
(planned provisioning), [RFC-0093](../../design/0093-board-naming-states-reach.md)
(board names). **Prior:**
[phase-452](../phase-452-copy-out-front-door.md), which made the templates and the
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

**Done (2026-10-25, confirmed 2026-10-29).** `82d50ea0ee` fixed every bullet
and added `check-book-board-names`. It is on the fast line
(`check-gate-lists.py --list fast` names it), and its self-test fails on a
planted `nros setup native`, `posix`, an unknown key, `board = "native"` and
`--platform native`. `check-book-links` already covered `book/src`. Re-checked
2026-10-29: every cited site reads correctly, `cli.md` documents `sdk-root`,
`locate`, `store`, `toolchain`, `pin` and `update`, and no first-node chapter
says "Path B … equivalent".

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

**Done (2026-10-25)** — steps 1–3 and the gate. Release `0.6.0-nros4` is
published (tag `nros-0.6.0-nros4`) and `[tool.nros]` pins it.
`installation.md` has one user path — `install.sh` (`--version` to pin, `nros
pin` for the project) — and no "no release" note; the clone, `bootstrap.sh`,
`activate.sh`, "Path B", `just setup` and Docker material moved to
`internals/contributing.md` § Development Setup, which now holds the probe's
`probe=20 track=checkout` block (cloning `main`, never a tag). The two
"(contributor)" chapters left getting-started: they are
`internals/{zephyr,nuttx}-in-tree.md` under "Per-Platform Contributor Lanes",
and the user material `zephyr.md` held (§ Configuring an image) moved into
`integration-zephyr.md`. Every chapter under `getting-started/`, `start-here/`,
`user-guide/`, `platform-guides/` and `porting/` (ESP32 pages excepted, issue
1525) now scaffolds with `nros new` instead of `cd examples/…`, links examples
on GitHub as reading material, names the SDK root through `nros sdk-root` (or
nothing, where `nros build` resolves it), puts `~/.nros/bin` on PATH instead
of sourcing `activate.sh`, and runs no `just`. The Zephyr chapters name the
module as `ZEPHYR_EXTRA_MODULES="$(nros sdk-root)"` (what `nros build`'s west
handoff passes) instead of a `west.yml` project pinned at `nros-v0.6.0`;
`vendored-tree.md` is now "pin and mirror a release" (`NROS_INSTALL_URL`,
`NROS_INDEX_URL` / `--index` / `NROS_OFFLINE`, cached `NROS_HOME`). Staging:
`scripts/stage-sdk-root.sh` now ships `integrations/` (276 KB — the PX4 module
template and the NuttX `apps/external` shell the chapters copy from
`$(nros sdk-root)`). Probe: the checkout and zenoh tracks read
`installation.md` (prereqs) + `contributing.md` (clone/bootstrap) + the
first-node chapter, whose talker is now `nros new`-scaffolded
(`verify-zenoh-interop.sh` runs `my_talker`). Gate:
`check-book-no-checkout` (`scripts/check-book-no-checkout.py`, fast line via
`just/check/docs.just`) — no fenced block in a user chapter clones this
repository, runs `bootstrap.sh`, sources `activate.sh`, names `NROS_REPO_DIR`,
runs `just` or `cd`s into `examples/`; user chapters are decided by directory,
parked pages by SUMMARY's "(…, dormant)"; blockquoted fences are read too. Its
self-test fails on a planted `git clone …/nano-ros.git` in `first-project.md`.

**Left** (each an installed-track gap found while rewriting; none got a
checkout fallback — the chapter says what is missing instead):

* **Not replayed.** No chapter beyond `first-project.md` has run on the
  installed track yet (W10). The Linux single-package scaffolds (`nros new
  --platform linux --lang {rust,c,cpp}`) build with the in-tree CLI; the
  FreeRTOS / ThreadX / NuttX / bare-metal / Zephyr flows were checked to
  scaffold and to plan (`nros build <image> --dry-run`), not built or booted.
  → **Replayed (W10, 2026-10-26):** bare-metal, FreeRTOS, ThreadX (both
  flavours) and Zephyr native_sim run green on the installed track; NuttX
  (both boards) too, since issue 2064 was fixed.
* **`integrations/` reaches users only from the next release.** `0.6.0-nros4`
  predates the staging change, so `$(nros sdk-root)/integrations/…`
  (`integration-px4.md`, `px4.md`, `integration-nuttx.md`) is empty there; the
  chapters say so and link GitHub for that release.
* **`nros new --platform {freertos,threadx,nuttx,baremetal} --lang c|cpp`
  writes a board alias the C/C++ road cannot resolve.** The scaffold says
  `board = "freertos"` (resp. `threadx`, `nuttx`, `baremetal`); `nros build`
  then fails at configure with `no board overlay at
  …/nano-ros-board-.cmake` (measured for FreeRTOS; with `board =
  "mps2-an385-freertos"` the configure gets past it). The chapters tell the
  reader to set the full board name; the fix belongs with W3's one vocabulary.
  The same split shows the other way: `[image.*] board =
  "mps2-an385-baremetal"` (the setup key) is refused by the image road, which
  knows `qemu-mps2-an385` / `baremetal`. The alias the image road knows,
  `qemu-mps2-an385`, itself names the emulator (RFC-0093 R3): issue 2063.
* **No single-package Rust scaffold off Linux / bare-metal.** `nros new
  --platform {freertos,nuttx,threadx,zephyr} --lang rust` refuses (issue 0333
  follow-up); the chapters start Rust as a workspace plus `nros new --image
  <id> --board <board>`. Zephyr Rust has no scaffold at all — the chapter
  points at the GitHub example.
* **`nros new --use-case listener|service|action` still writes a talker**
  ("template diversification: TODO"), and the C/C++ talker template publishes
  once and exits where the chapters' examples publish on a timer. The
  first-node C/C++ chapters show the scaffold's real output and link the full
  example's `main.c`/`main.cpp` to paste over it; `ros2-interop.md` uses the
  workspace scaffold's `listener_pkg` for "the other direction".
* **Workspace-located index sources.** `nuttx-kernel`, `nuttx-apps`,
  `nuttx-libc`, `px4-autopilot`, `px4-rs`, `cyclonedds-src` and
  `tonbandgeraet` have no `location = "store"`: their `dest` is a checkout
  path, and `nros sdk-path --source` refuses them. What `nros setup
  qemu-armv7a-nuttx` / `nros setup --source px4-rs` do with no checkout is
  unmeasured. `px4.md` now has the reader clone PX4 themselves.
  → **Measured (W10):** with no checkout, `nros setup qemu-armv7a-nuttx` /
  `rv-virt-nuttx` provision `nuttx-kernel`, `nuttx-apps` and `nuttx-libc` into
  the installed SDK root's own `third-party/nuttx/` (`[provisioned]`), and the
  kernel then builds there. `px4-rs` / `cyclonedds-src` / `tonbandgeraet` stay
  unmeasured (no W10 chapter reaches them).
* **QEMU is not on PATH.** `nros setup` provisions `qemu-system-*` into the
  store but `~/.nros/bin` does not front it; the chapters run
  `"$(nros sdk-path qemu)/bin/qemu-system-arm"`. → Measured working (W10): the
  bare-metal, FreeRTOS and ThreadX riscv64 tracks boot through exactly that
  line.
* **`nros board list` needs a checkout.** `find_workspace_root` walks up for
  `packages/boards/` and never asks the SDK-root ladder, so outside a checkout
  it fails unless given `--workspace "$(nros sdk-root)"`. The book no longer
  sends users to it.
* **`nros-build-profile` is not shipped as a binary.** `build-profiling.md`
  installs it with `cargo install --locked --path "$(nros
  sdk-root)/packages/testing/nros-build-profile"` (measured against a staged
  SDK root).
* **Zephyr `west nros`** (`scripts/west-commands.yml`) has no installed-track
  spelling: a `west.yml` `self: west-commands:` needs a literal path, and the
  SDK root is versioned. `board-crate-import.md` dropped the line.

**Follow-up (2026-10-26) — `nros new` for C/C++ on Zephyr, and every
scaffold's "Next steps".**

* **`nros new --platform zephyr --lang c|cpp` writes a Zephyr application.**
  It wrote the host shape (`nano_ros_add_executable` over a `main()`, plus a
  QEMU-slirp `nros.toml`), whose configure refused with `nros_feature_set:
  unknown PLATFORM 'zephyr'`. It now writes what `integration-zephyr.md`
  § Project layout shows: `find_package(Zephyr)` first, then
  `find_package(nano_ros)` / `find_package(std_msgs)`, a TYPED component
  (`src/Talker.c` with `RCLC_C_COMPONENT`, or `src/Talker.{hpp,cpp}`)
  registered with `nros_components_register_node`, a `prj.conf` with
  `CONFIG_NROS=y` and no `CONFIG_NROS_*` knob, and the component in
  `system.toml`'s `[[component]]`; no `nros.toml` (the network is Kconfig).
  Test: `zephyr_c_and_cpp_scaffold_a_stock_zephyr_app` (its CMake lines are
  asserted present in the book too). Measured against Zephyr 3.7: both
  scaffolds BUILD (`zephyr.exe`) with the book's own command.
* **The book's plain `west build` now configures.** Measuring that showed
  `west build … -- -DZEPHYR_EXTRA_MODULES="$(nros sdk-root)"` failed for EVERY
  app, the in-tree `examples/zephyr/c/talker` included: "Could not find a
  package configuration file provided by nano_ros". Issues 1304/1998 had each
  passed `-Dnano_ros_ROOT` on their own road's line (cmake, `nros build`'s west
  handoff); the module now sets `nano_ros_DIR` to its own root
  (`zephyr/CMakeLists.txt`, beside `NROS_REPO_DIR`), so the root is answered
  where it is known and the book's command needs nothing more.
* **"Next steps" is the book's flow per language and platform.** Every
  scaffold printed `nros sync` + `cargo build  # or: cmake --build build / west
  build` (and `eval "$(nros ws env)"`). One function,
  `scaffold::next_steps(name, lang, platform)`: Rust `nros sync` then `nros
  build`; C/C++ `nros build` alone (bindings are generated by CMake); Zephyr
  C/C++ the module-naming `west build`, with `NROS_ZEPHYR_WORKSPACE=… nros
  build` as the hand-off. Test: `next_steps_follow_the_books_flow_per_language_and_platform`
  (every language × offered platform; no `cargo build` anywhere).

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

**Follow-up (2026-10-25), two installed-track gaps the W2 book rewrite
measured.** Recorded here because W2's "Left" list is not on `main` yet.

* **The scaffold's board word now resolves on every road that builds it.**
  `nros new --platform <p> --lang c|cpp` wrote the PLATFORM word as the image
  board (`freertos`, `nuttx`, `baremetal`, `threadx`). The catalog took the
  first three as aliases, but the cmake road decided "this board IS its
  platform" LEXICALLY (`board == deploy token`, `NanoRosPackageXml.cmake`),
  emptied the board, and configured against `nano-ros-board-.cmake`;
  `threadx` named two boards. Now `nros ws leaf-system` decides it over the
  RESOLVED values (`cmake_board_provider`: empty only when the board's module is
  its platform — `linux`, `zephyr`) and refuses an ambiguous board as
  `nros build` does; `nros new` writes the platform's default board from the
  catalog (`BoardCatalog::default_board_for_platform` →
  `BoardDescriptor::canonical_name`: `mps2-an385-freertos`,
  `qemu-armv7a-nuttx`, `mps2-an385-baremetal`, `linux`, `zephyr`), refuses
  `threadx` naming `rv-virt-threadx` and `threadx-linux`, and takes `--board`
  for a C/C++ project (resolved like an image board, and required to be a board
  of that platform). Test: `every_offered_platform_scaffolds_a_board_every_road_builds`
  (every offered platform × C/C++, through the image road, the cmake road and
  the setup join — plan level).
* **Every setup key is an image board word.** `mps2-an385-baremetal` was the one
  index key no descriptor `names` (it was only the board's `cmake_module`), so
  `nros setup` and `nros build --board` took it and an image naming it was
  refused. The catalog now answers to a board's `cmake_module` (its setup key,
  `setup_key` rule 1), so the join runs both ways for any board; the key is a
  `BOARD_PATHS` entry for the Rust road, and `nros::main!`'s bare-metal entry
  shape is derived from the key's board type instead of a second list of its
  spellings. Test: `setup_keys_are_image_board_words` (every index key: catalog
  round trip, image road, cmake overlay, entry tables). Not done as a new
  `names` entry: that made `check-board-name-reach` reach this descriptor for
  the first time, and it then reports its existing alias `qemu-mps2-an385`
  (names the emulator, RFC-0093) — a real violation the gate cannot see today
  because none of the descriptor's `names` is an overlay board. Filed as issue
  2063 (no rename yet).
* **`nros board list|info|cmake-vars|net` and `nros setup board` work outside a
  checkout.** They found the catalog only by a walk-up for `packages/boards/`;
  they now ask the SDK-root ladder (`--workspace`, `$NROS_WORKSPACE_ROOT`, a
  checkout, then the binary's own SDK root). Two siblings read SDK data the same
  way and moved too: `nros config`'s platforms tree and the executor-capacity
  check's platform config (which fell back to the default silently). `nros
  board new` and `nros doctor` keep the checkout walk-up: they write into, or
  run `just` in, a checkout.

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

**Done (2026-10-25); the three items left then are closed below:** plain `nros setup` at a workspace
root (a `package.xml` there or in a child of `src/`) is `nros setup --workspace
.`; it prints the report, provisions every image's `(board, rmw)` in one store
plan, reports ROS's `rmw_zenohd` for a zenoh image (found, or the apt names),
then the OS closure (installed with `--sudo`, else printed); `--check` stays
report-only. Measured in `examples/workspaces/c` with `--dry-run`. Left:
(1) the OS closure is the index's whole `[prereq.*]` set, not the workspace's
own keys; (2) an unknown `<depend>` is counted, not refused — telling a ROS
package from a typo needs the ament index; (3) no `--yes`, because nothing asks
for confirmation (`nros setup <board>` never did either).

**The three items, closed (2026-10-29):**

1. The OS closure is the workspace's own keys. `run_workspace_scan` returns
   the `[prereq.*]` keys the workspace names: its build types' host tools, and
   its `<depend>`s whose role is `package` or `buildtool`. `run_system` asks
   only for those, prints them (`nros setup: OS packages the workspace names:
   clang, cmake`), and its `--sudo` re-run line is `nros setup --workspace
   <dir> --sudo`. Measured with `--dry-run`: `examples/workspaces/c` asks for
   `clang, cmake`, `examples/workspaces/rust` for `cargo, clang, cmake`, where
   the index's whole set is every board's toolchain too. Test:
   `a_workspace_asks_only_for_the_os_keys_it_names`.
2. An unknown `<depend>` is refused, before anything is fetched and in
   `--check` too. `nros setup` runs `nros build`'s own ladder
   (`check_declared_depends`, phase-398 W3), so the two verbs cannot disagree
   about a name. The ament rung is the one that tells a ROS package from a
   typo; the ladder already had it. Measured: a scaffolded workspace with
   `<depend>std_msgz</depend>` stops with `1 <depend> name(s) resolve to
   nothing: std_msgz — declared by ./src/talker_pkg/package.xml`, and the
   same workspace without it passes `--check` with and without
   `AMENT_PREFIX_PATH`.
3. No `--yes`: closed, not built. `nros setup` never asks for confirmation:
   store installs need no root and run, and OS packages are printed unless
   `--sudo` is given. A flag that skips a prompt nobody shows would be a
   no-op.

`installation.md` gained "Provision a whole workspace", and `cli.md`'s
`--workspace` row no longer says it installs nothing.

**Done (2026-10-29): the probe.** `just probe workspace` replays
installation.md's "Provision a whole workspace" (`probe=30..50
variant=workspace`) in a clean `ros:humble` container on the installed
release:
1. a Rust zenoh workspace plus `[image.freertos]` (`nros new --image`);
2. one bare `nros setup`;
3. `nros sync && nros build --all`.

`verify-installed-workspace.sh` then asserts that both ELFs exist, that `nros
setup --check` finds nothing missing, and that no checkout appeared on the
host. GREEN on probe-platforms run 40793777845: setup reported `rmw_zenohd` and
asked only for `cargo`, and the build wrote `linux_entry` and
`freertos_entry`. The weekly lane carries it as a matrix entry, and the other
eight tracks were green in the same run.

`first-project.md` teaches the command first. A probed `nros setup` step
follows the scaffold, and the page no longer asks for `nros setup linux --rmw
cyclonedds`. GREEN on probe run 40795130689, both tracks: the installed track's
bare setup printed `linux — all packages already present` and asked for
`clang, cmake`.

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

**Decision and evidence (2026-10-26) — first run BLOCKED, issue 2076.** The maintainer
accepted RFC-0100 Amendment 3 with its evidence plan as a precondition. The
amendment lets fresh probe sidecars stand as a contract-less image's
declaration, with `basis = "probe"`. It was implemented on a branch,
`feat/517-w5-probe-sizing`, which was not pushed, and measured against
`origin/main` 37b950503e.

*First project* (`nros new my_robot --workspace`, C++ talker + listener,
CycloneDDS, no contract):

| | entity counts | executor arena | run (6 s) |
| --- | --- | --- | --- |
| default | refused: "the launch tree resolved, and it declares neither wiring nor parameters" | 74,240 B | 12 `Received:` |
| probe-derived | DERIVED from 2 components: 3 entities, `max_cbs 2`, `max_nodes 2`, `basis = "probe"` | 14,488 B | 12 `Received:` |

*Every contract-less model image* (`nros build --dry-run --all` over every
in-tree colcon workspace): 112 images go from REFUSED to derived. On
`multi-package-workspace` and `image_config_ws` not every launched node has a
fresh sidecar, so they stay refused (all or nothing).

*Linux lane at runtime* (`just build linux` + `just test linux`, both trees):

| tree | passed | lane-deselected | real failures |
| --- | --- | --- | --- |
| default | 3097 | 186 | 0 |
| probe-derived | 3093 | 186 | 4 |

The 4 failures come from 2 images, and each reproduces when run alone:

| image (fixture row) | derived | default | derived result |
| --- | --- | --- | --- |
| `bridge-cyclonedds` linux (`workspace-rust-native-bridge`) | `max_cbs 1`, `nodes 1` | crate defaults; passes | boot: `OpenSession("NodeTableFull")`; `declarative_bridge_zenoh_to_cyclonedds` ×2 and `rmw_coordinate_truth` fail |
| `bridge-xrce` linux (`workspace-rust-native-bridge-xrce`) | `max_cbs 1`, `nodes 1` | crate defaults; passes | no node registration; `declarative_bridge_zenoh_to_xrce` fails |

The other 65 linux images that moved from refused to derived passed in the lane
above. These include `rmw_coordinate_truth`, which boots every generated
linux entry and checks node registration. Among them: rust, c and mixed
`linux`/service/action/robot rows; `features` ×22; `safety` ×6; `managed`;
`realtime-{c,cpp,rust}`; `sizing` (`max_cbs 6`); `launch`; cpp robot1/2.

*Why the bridge fails.* A `[[bridge]]` is not a component. The runtime creates
its nodes and endpoints from `system.toml`, so no probe sees them. That is a
blocker under the plan's rule: an image that fails derived and passes default.
It is not lazy registration. The static search found no `create_*` call reached
from a callback in any model-road component, so no contract row was added.

*Also found.* The cmake configure keys metadata rows by `exec` and probe rows by
node name. A renamed node (`realtime-c`, `realtime-cpp`) therefore refused at
configure while the resolve phase derived. The branch covers this.

*Not run.* No embedded platform ran. The linux blocker stopped the plan first.

**Landed (2026-10-26), after issue 2076.** The maintainer chose to count a
bridge's nodes and endpoints in the image's inventory. The rows come from the
bringup's generated `nros-bridge.toml`, the file the runtime reads, and the
bridged backends reach the descriptor's `backend_count`; the details are in
issue 2076. The evidence was then re-run against `origin/main` 9f2d7b4bb3. For
each lane, both trees built the scope's fixtures and ran the whole suite
narrowed to its coordinates, and every test's result was compared:

| lane | images moved to derived | default (pass / fail) | derived (pass / fail) | status diffs |
| --- | --- | --- | --- | --- |
| linux | 66 (+ 11 cmake configures), both bridges included | 3107 / 0 real | 3108 / 0 real | none |
| FreeRTOS | 12 | 2837 / 26 | 2840 / 24 | 2 base reds passed derived; none the other way |
| ThreadX linux | 4 | 2829 / 23 | 2830 / 23 | none |
| ThreadX riscv64 | 1 | 2822 / 23 | 2823 / 23 | none |
| Zephyr native_sim | 5 C/C++ west entries | 2840 / 60 | 2841 / 60 | none |
| NuttX | 8 | 2819 / 30 | 2820 / 30 | none |

The remaining reds are red in both trees, for host preconditions (Docker;
`qemu-br`, which needs `sudo`) or for Zephyr C++ images that already fail to
link at the base (a duplicate `__popcountdi2`). Not run: FVP, s32z270 and ESP32
(dormant). The derived bridge-cyclonedds image states `max_nodes 3`,
`max_cbs 3` and `backend_count 2`, and its bridge tests pass.

*First project, final:* DERIVED from 2 components: 3 entities, `max_cbs 2`,
`node_count 2`, `basis = "probe"`. The executor arena is 14,488 B against
74,240 B by default, and the run prints 12 `Received:` in 6 s. W5's last open
item is closed: the template relies on the probe and ships no contract.

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
`nros-tests` QEMU ladder and this one are two Rust spellings of one ladder.
(`nros build -v`, landed with W5, is now in `reference/cli.md`.)

**Follow-up (2026-10-26) — a printed QEMU is one the store builds, or the
host's, said so.** The rv-virt NuttX board printed `qemu-system-riscv32`
through the store ladder, but the store QEMU is configured
`--target-list=arm-softmmu,riscv64-softmmu` (`[tool.qemu.source]`, held equal
to the prebuilt's `build-qemu.sh` by `check-qemu-configure`), so no rung could
ever answer and the note sent the user to `nros setup --tool qemu`, which does
not provide it. Adding `riscv32-softmmu` was priced and not taken: it is a new
`qemu-*-nros8` cut of three dists in `nano-ros-sdk` (a maintainer dispatch, new
sha256 rows), for a target that needs none of the fork's patches — the harness
already boots rv-virt on a stock `qemu-system-riscv32` (apt
`qemu-system-misc`; QEMU 9.2.2 here). Instead the ladder reads the store's
target list from the index (`qemu_bin::store_targets`) and a target outside it
is `QemuBin::Host`: the command names it bare and the note says to install it
from the distro (or set `QEMU_SYSTEM_<ARCH>`); `nros locate tool
qemu-system-riscv32` answers from `$PATH` for such a target only. Tests:
`every_printed_qemu_is_the_stores_or_named_for_host_install` (every non-Zephyr
board descriptor; mutation-checked — it fails when the `Host` rung is
removed) and `a_target_the_store_does_not_build_is_the_hosts`. The same sweep
found `next_steps`' chapter pointers for NuttX and Zephyr naming
`getting-started/{nuttx,zephyr}.md`, which W2 moved to `internals/`; they name
`integration-{nuttx,zephyr}.md` now, and `every_chapter_a_note_names_exists`
holds every pointer to a file.

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

**Done (2026-10-25)** — every listed chapter leads with `nros build`, quotes
the artifact path the plan records (`build_next::BuildOutput`: cargo leaf
`build/<image>/target/[<triple>/]<profile>/<bin>`, cargo workspace
`build/<platform>-<rmw>/<image>_entry/target/…`, cmake leaf
`build/<image>/cmake/<nano_ros_add_executable name>`, west
`build/<platform>-<rmw>/<bringup>__<image>_west`), shows the `nros build: next
for …` block, and names raw `cmake` / `west build` as the escape hatch (linking
"Build as a CMake subdirectory"). Per chapter: `deployment.md` Zephyr section
rewritten around `nros build` + `west … -d` (the "not yet generated / issue
1288" claim and the `zephyr-workspace/env.sh` + `nros/examples/…` checkout path
are gone); `integration-zephyr.md` — `nros build` is the flow, "One image per
RMW" is `nros build <image>` with `-DNROS_IMAGE` as the escape hatch, the BYO
`west build` block is a named "Driving west yourself" section, the five-command
transcript runs `west build -d … -t run`; `build-artifacts.md` — a C/C++
single-package row, the printed `next` block, and `-d` on every follow-up
`west` verb (its own `./build/zephyr/zephyr.exe` is gone);
`workflow-by-platform.md` — the cmake and west shapes lead with `nros build`
and name the escape hatch; `freertos.md` — exact C/C++ artifact, the printed
block, the cmake escape-hatch line (from `nros build --dry-run`);
`first-node-c.md` / `first-node-cpp.md` — the printed block and a cmake line
that matches `--dry-run`; `your-own-msg-package.md` — `nros build` builds the
no-bringup workspace colcon-style (`build/my_app/cmake/pkg/my_app/my_app`, the
`local_msg_pkg` compile-check row's output), the hand umbrella is the escape
hatch and names the SDK root instead of `/path/to/nano-ros`;
`workspace-entry-pkg.md` — "plain west stays the primary flow" reversed.
Measured: `nros new … --platform linux --lang c` + `nros build` prints the
quoted block; `nros build --dry-run` for FreeRTOS / NuttX C packages and a
Zephyr C package prints the quoted cmake / west lines.

**Left:**

* `nros new <name> --platform zephyr --lang c` writes a `find_package(nano_ros)`
  CMakeLists, not a Zephyr application: `nros sync && nros build` reaches
  `west build` and the configure fails (`nros_feature_set: unknown PLATFORM
  'zephyr'`, measured). The chapter's "Start from an example" still offers that
  scaffold. A CLI fix (template), not a docs one.
* `nros new`'s own "Next steps" for a C package say `cargo build`.
* `user-guide/build-artifacts.md` and `deployment.md` still describe an
  `idf.py` road that RFC-0065 D3 deleted; their ESP32 sections are dormant
  text (issue 1525) and were left unchanged.

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

**Design note (2026-10-25): ACCEPTED 2026-10-26** —
[RFC-0098, "a `<depend>` is one edit for Rust too"](../../design/0098-generated-leaf-build-config.md).
`nros sync` owns `# nros-managed` rows in the package's own `[dependencies]`
(the issue-0457 marker precedent). The three candidates above each fail on a
cargo fact: `--config` cannot add a dependency, a manifest has no `include`, and
a wrapper crate changes every `use` path. The selection dep is already generated
(RFC-0098 D6 amendment, 2026-10-11). Open questions resolved with the
acceptance (recommended, maintainer may override): the `nros` row stays
AUTHORED; sync WRITES the rows rather than only checking them.

**Landed (2026-10-26):**

- `nros sync` writes one `# nros-managed` row per `<depend>` / `<build_depend>`
  / `<exec_depend>` that resolves to a message package
  (`cmd/managed_deps.rs`, scan in `cmd/ws.rs`), into every Rust package the
  scan reads — including a single-package leaf's path-dep `node/` — with the
  path relative from that package. Untagged rows are never touched; a tagged
  row whose `<depend>` is gone is removed; a second sync writes no byte.
  A package whose `src/` holds C/C++ sources (a C application with a Rust link
  root, issue 1512) is skipped.
- `nros build` runs the same step before planning, and first GENERATES a
  message crate a new `<depend>` names, so `<depend>` + `nros build` is the
  whole change in an already-synced workspace. `nros ws managed-deps
  [--check]` runs the step alone.
- Migration: 117 tracked manifests' path rows tagged in place (examples,
  templates, `packages/testing`, CLI testing workspaces, `tests/`), 7 gained
  a row for a `<depend>` they declared but did not list (`linux/rust/
  lifecycle-node`, the six Zephyr Rust entry crates), and three `package.xml`
  gained the `builtin_interfaces` `<depend>` their manifest already used.
  Deliberately untagged: the three PX4 companions' `px4_msgs` (generated by
  `nros generate-px4-msgs`, not sync) and the dormant ESP32 leaves (issue 1525).
  The `nros new` Rust scaffolds no longer write a registry-named `std_msgs`.
- Gate `just check managed-msg-deps` (fast line): no untagged path row into a
  `generated/` tree in a tracked manifest; exemptions keyed on (manifest,
  crate) with reasons, ESP32 by the registry's `tier = "scaffold"`.
- Measured: `examples/workspaces/rust` — `nros sync` then `nros build linux`
  rc 0 with every manifest's mtime unchanged; adding `<depend>geometry_msgs</depend>`
  to `talker_pkg` and running only `nros build linux` generated the crate,
  wrote the row and built a node importing `geometry_msgs::msg::Point`;
  removing the `<depend>` removed the row, byte-identical to before.

**Not done:** the scaffold still names `nros` registry-style (by the
resolution above), and the selection dep is unchanged (already generated).

**Done (confirmed 2026-10-29).** Both remaining lines are the accepted
resolution, not gaps. Measured against the done-when on a fresh `nros new
pkgr --platform linux --lang rust`: the `Cargo.toml` names no message crate
and no selection dep, and `nros sync` writes `std_msgs = { path =
"generated/std_msgs", default-features = false }  # nros-managed` from the
`package.xml`'s `<depend>`. Adding a `<depend>` is one edit.

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
* **NuttX:** both QEMU targets (ARM `-M virt` and RISC-V `rv-virt`; rv-virt's
  `qemu-system-riscv32` is the host's, not the store's — W6 follow-up), booting the
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

**Done (2026-10-25)** — each tier 1–3 row of `board-support.toml` now names
its chapter section: `book_run = "<chapter>.md#<anchor>"`, plus
`real_hardware` (required on an `execution_class = "hardware"` row, because
that class covers both the S32Z270 and the FVP, and only the first is flashed)
and `book_flash` (required when `real_hardware = true`). Chapters:
`freertos.md` — "Run on QEMU" (Rust and C/C++ ELF, store QEMU, LAN9118), "Run
on the POSIX simulator" (`freertos-posix`), "Flash a real board" (no in-tree
board is flashed; S32Z270 build-only, flash with your S32 tooling; your own
board via the porting guide) — the STM32F4/LPC55/MSP432 claim is deleted;
`bare-metal.md` — "Run on QEMU", "Flash a real board" (probe-rs / OpenOCD on a
board crate of your own, STM32F4 worked example); `threadx.md` — "Run on the
Linux simulator", "Run on QEMU (RISC-V 64)", C/C++ paths added, "Flash a real
board" says none is supported; `integration-nuttx.md` — `nros build` road
added beside NuttX's own `make`, "Run on QEMU (Arm virt)" with the store QEMU,
"Run on QEMU (RISC-V rv-virt)", "Flash a real board" (NuttX's own flow); the
CMake line was wrong twice (`-DBOARD=… -DCONFIG=…`; NuttX's CMakeLists takes
`-DBOARD_CONFIG=<board>:<config>`, and `qemu-armv7a` has no `nsh_smp` config —
read off the pinned NuttX tree), now `-DBOARD_CONFIG=qemu-armv7a:nsh`;
`integration-zephyr.md` — "Run on native_sim", "Run on QEMU (mps2-an385)",
"Flash a real board" (`west flash -d`), `qemu_cortex_a9` is called a QEMU
machine; `arm-fvp.md` — a Build section (`nros build demo_bringup:fvp`) and
"Run on the FVP" (`west build -d … -t run`); `px4.md` — one tree: every step is
`make -C PX4-Autopilot <target> EXTERNAL_MODULES_LOCATION=…` (the old
`cmake -B build -S PX4-Autopilot` configured a tree the following `make` never
used), the `-DNROS_RMW` knob it named is read by nothing (backends are the
module's `BACKENDS` list), SITL is `gz_x500` (this PX4 has no Gazebo-classic
`gazebo` target), flash is `make … upload`; `first-project.md` — the run step
names the printed `run:` line. Gate `check-book-board-run-sections`
(`scripts/check-book-board-run-sections.py`, fast line via
`just/check/docs.just`): the named heading exists under that mdbook id, reads
"Run …"/"Flash …", and its section holds a fenced command; dormant chapters
(SUMMARY "(…, dormant)") are skipped. Self-test: a board's run block removed
from its chapter fails.

**Where the book and `next_steps` disagree** (the docs say the right thing; the
CLI is the side to fix):

* (Not a disagreement, measured.) MPS2-AN385: `next_steps` prints the
  descriptor `runner`, with no `-nic` and no `-icount`; the chapters said that
  boots "without networking". It does not: QEMU's default user-mode network
  lands on the LAN9118, and both that line and the chapter's harness line
  (`-icount shift=auto … -nic user,model=lan9118`) took the bare-metal Rust
  scaffold to `Publishing: 'Hello World: N'` against a local `rmw_zenohd`. The
  chapters now say both work and why theirs adds `-icount`.
* `rv-virt-nuttx`: `next_steps` prints `qemu-system-riscv32`, which the
  store's QEMU (`--target-list=arm-softmmu,riscv64-softmmu`) does not build, so
  after `nros setup rv-virt-nuttx` the printed `nros locate tool
  qemu-system-riscv32` answers nothing. The chapter says to install a distro
  `qemu-system-riscv32` and set `QEMU_SYSTEM_RISCV32` (the ladder's env rung).
* `build_next::chapter()` names `book/src/getting-started/zephyr.md` and
  `…/nuttx.md`, which do not exist (the chapters are `integration-zephyr.md`
  and `integration-nuttx.md`).

**Left:**

* Booted here: the Linux C talker (its printed block), and the bare-metal
  Rust scaffold under both QEMU lines against `rmw_zenohd`. Every other QEMU,
  FVP, SITL and flash line is unverified until W10's per-chapter probes run
  it (the FreeRTOS C build stopped at configure on this host for want of the
  FreeRTOS kernel source, not on a chapter step).
  → **Settled by W10's runs (2026-10-26):** the QEMU lines of
  `bare-metal.md`, `freertos.md` (Rust workspace image) and `threadx.md`
  (riscv64), `threadx.md`'s Linux simulator line and `integration-zephyr.md`'s
  native_sim quick start, boot on the installed
  track and print the output their chapters now show, and the chapters'
  `ros2 topic echo` lines deliver. The `-icount shift=auto -nic
  user,model=lan9118` line is measured on both MPS2-AN385 chapters. The NuttX
  QEMU lines boot on the installed track too (issue 2064, fixed). The FVP,
  SITL and flash lines, and the C/C++ "same flags, its own ELF" alternatives
  the tracks do not take, stay unverified.
* ARM FVP: the spec asked for "license-gated; say so". It is not
  licence-gated (index `[tool.arm-fvp]`, measured 2026-09-06); the chapter
  says so instead.
* Zephyr has no tier-3 hardware board to give a flash block; the chapter's
  "Flash a real board" is the generic `west flash -d` flow for a board
  package of the reader's own.
* The `mps2-an385-zephyr` image's network (address, locator) is not supplied
  by its board package the way the fixture lanes' `packages/boards/nros-board-mps2-an385-zephyr/prj.conf`
  supplies it; the chapter tells the reader to state it, unmeasured.
* The two `next_steps` disagreements above (rv-virt QEMU, chapter paths) are CLI fixes.

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

**Status (2026-10-26) — harness, recipes, weekly lane and gate landed; six of
eight tracks GREEN — Linux, bare-metal, FreeRTOS, ThreadX (both flavours) and
Zephyr native_sim; both NuttX tracks went green after issue 2064 was fixed.** Every number
below is a run of the track on the installed asset this branch builds, in a
clean `ros:humble` container (the chapters' router is ROS's `rmw_zenohd`);
the last sweep ran all eight in parallel.

*Harness — one, with a platform parameter.* `PROBE_TRACK=installed
PROBE_PLATFORM=<p>` (`scripts/probe/run-bootstrap-probe.sh`) installs the
release asset exactly as the Linux track does, keeps its no-checkout assertions
(`check-installed-sdk-root.sh` after the install,
`verify-installed-platform.sh` at the end), and replays `installation.md` plus
the platform's chapter; a chapter that documents several runs selects one with
`variant=` (NuttX `arm`/`riscv`, ThreadX `linux`/`riscv64`). The book states
the assertion: a Run step is `probe=NN probe-run=SECS` (plus `probe-keep` for
the router a later step needs) and the chapter's output block beside it is
`probe-expect=NN`, so the track passes only if the command prints what the
chapter says it prints (`scripts/probe/probe-run.sh`, which runs the step in its
own session and kills it as a group). `extract-book-steps.py` refuses a run
step with no output block, an output block with no step, and a `--variant` that
selects nothing, and the harness parses the assembled script (`bash -n`) before
any container starts. `PROBE_ASSET_OUT=<dir>` builds the asset once for several
tracks.

*Recipes and lane.* `just probe {baremetal,freertos,threadx-linux,
threadx-riscv64,nuttx-arm,nuttx-riscv,zephyr}` (one `just probe platform
<name>` underneath; `just probe platform-asset`), opt-in, in no tier.
`.github/workflows/probe-platforms.yml`: Mondays 09:00 UTC + dispatch, hosted
`ubuntu-latest` only, the asset built once and one job per chapter
(`fail-fast: false`), in no merge-gating lane. Per push, gate
`check-book-probe-tracks` (fast line, ~0.5 s) extracts every track from the
harness's own platform table, so a broken tag fails at review instead of a week
later; its self-test is a run step stripped of its output block.

*Measured (installed track):*

| Track | Result | What the run asserted |
|---|---|---|
| `linux` (first-project.md) | GREEN, 291 s | unchanged by the harness rework |
| `baremetal` | GREEN, 291 s | router up; QEMU prints `Publishing: 'Hello World: 1'`/`2`; `ros2 topic echo` prints `data: 'Hello World: 5'` |
| `freertos` | GREEN, 313 s | Rust workspace image: `[INFO] Publishing: 0` / `[INFO] I heard: 0` …; `ros2 topic echo … Int32` delivers |
| `threadx-linux` | GREEN, 123 s solo | `Publishing: 0` / `I heard: 0` from the host process. Red ONCE in the 8-way sweep: `zpico Session -> ConnectionFailed` 0.13 s after the router printed its start line, and this image opens its session once rather than retrying as the QEMU boards do; green solo before and after |
| `threadx-riscv64` | GREEN, 314 s | QEMU riscv64 prints both sides; `ros2 topic echo` delivers |
| `nuttx-arm`, `nuttx-riscv` | GREEN (2026-10-26, after issue 2064) | was RED: the sizes probe read a fat-LTO bitcode rlib with no `llvm-nm`, then the image dialled `192.0.3.1` on QEMU's 10.0.2.0/24 — both fixed |
| `zephyr` (native_sim) | GREEN, 599 s | Zephyr SDK (1.4 GiB) + a 3.7 west workspace + `west build` of `nros new --platform zephyr --lang c`; `zephyr.exe` prints `Published: 0`/`1`/`2` against the router |

*A planted wrong command fails its track.* `bare-metal.md`'s QEMU line with
`-kernel …/release/bm_talker` (the debug ELF is what `nros build` writes):
`PROBE FAIL: step 70 exited before printing its documented output; missing:
Publishing: 'Hello World: 1' Publishing: 'Hello World: 2'`, rc 1, after
`qemu-system-arm: Could not load kernel …`.

*Chapter fixes the runs forced.* Every chapter told the reader to `cd` into the
package twice (the scaffold step already had). FreeRTOS/ThreadX documented the
in-tree example's output (`Publishing: 'Hello World: 1'`, `std_msgs/String`) for
a scaffold that prints `Publishing: 0`/`I heard: 0` over `std_msgs/Int32`, so
the chapters' `ros2 topic echo … String` received nothing. FreeRTOS still said
the scaffold writes `board = "freertos"` and must be edited (it writes
`mps2-an385-freertos` since W3). ThreadX's `nros new … --platform threadx`
REFUSES without `--board` (two boards). NuttX had no `nros new` step at all,
claimed "no ROS 2 install" (the zenoh router is ROS's) and a NuttX checkout for
the `nros build` road (only the `make` road needs one), and needs
`python3-venv` + `unzip` on the host (kconfiglib venv; NuttX's own build
unzips `dtc`/`libmetal`). Zephyr gained a linear "Quick start: native_sim from
nothing" — host tools (incl. `wget`, which the SDK's own `setup.sh` requires),
`nros setup zephyr`, a west workspace with the same module allowlist nano-ros
uses, `nros new --platform zephyr --lang c`, `west build`, run.

*CLI / build fixes the runs forced* (each a dead end for every installed user,
invisible from a checkout because `activate.sh` and the root
`.cargo/config.toml` supplied what was missing):
`nros` puts `~/.cargo/bin` on its own PATH when PATH has no `cargo` (setup
installs rustup there; the cargo road died `No such file or directory`);
`nros setup <board>` installs the board's Rust target, `rust-src`, and the
pinned nightly a build-std board builds with (preflight asked for
`thumbv7m-none-eabi`, Corrosion for `nightly-2026-04-11`), and preflight now
asks `rust-src` of that nightly; `nros build` appends the board's store tool
`bin/` dirs to the build's PATH (cc-rs found no `arm-none-eabi-gcc`); the
generated entry of a `board-run` board aborts on panic (threadx-linux: "unwinding
panics are not supported without std") and the mps2 boards' descriptors state
`[board.entry] panic = "own"` (the FreeRTOS workspace image linked two
`panic_impl`s, E0152); the rv-virt ThreadX descriptor names
`riscv-none-elf-gcc` for cc-rs (it derived `riscv64-unknown-elf-gcc`); the
rv-virt NuttX provision script gets the board's `NUTTX_CROSS`; the NuttX FFI
cargo command carries the `nros-minsizerel` profile definition; `[tool.zephyr-sdk]`
declares `system = ["wget"]` so setup asks before the 1.4 GiB fetch.

`integration-zephyr.md` also needed the `native_sim` NSOS overlay on its
`west build` line (`-DEXTRA_CONF_FILE="$(nros sdk-root)/cmake/zephyr/native-sim-line-3.7.conf"`):
without it the image tried a TAP interface (`eth_posix: Cannot create zeth`)
and SEGFAULTED at boot; and the Zephyr module's cargo build now resolves
`cargo` like every other cargo invocation there (`nros_rust_tool`) — the
reader's own `west build` ran in a shell with no `~/.cargo/bin` and died
`FileNotFoundError: … 'cargo'`.

**Left:** `nros setup` for a NuttX board still does not ask for
`python3-venv` / `unzip` (issue 2068; the chapter names them as a step).
`threadx-linux`'s one-shot session open is a race against a router started
seconds earlier (a user's second terminal) that the QEMU boards' retry hides —
recorded, not fixed. The tracks probe one road per chapter (the Rust workspace
for FreeRTOS/ThreadX, the Rust package for bare-metal, the C package for
NuttX/Zephyr); the other language's "same flags, its own ELF" lines stay
unreplayed.

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

**Already provided (found 2026-10-25), so no new design:** RFC-0095 D7–D9,
phase-440 W7. `nros-toolchain.toml` (`[toolchain] version = "<store version>"`)
is the project's pin; the first `nros build` writes it and says "Commit it"
(`nros-launcher/src/pin.rs`), CI refuses to write one (RFC-0097 D11), and the
launcher reads it before clap and runs the pinned toolchain, fetching it through
the release's own `install.sh` when it is absent (`nros-launcher/src/dispatch.rs`).
What remains is evidence, not mechanism: the W10 installed-track probe should
build a project pinned to an older release, and the book (W2) should name the
file. `nros new` does not write it, by D9's design (pin on first BUILD, not on
scaffold), which this phase keeps.

**Done (2026-10-30): the evidence.** `just probe pinned` (track `installed`,
variant `pinned`; a `probe-platforms.yml` matrix row) runs installation.md's
"Pinning a version" blocks in a container with no checkout: the probe's own
asset is installed, a project writes `nros-toolchain.toml` pinning
`0.6.0-nros4`, and inside it `nros toolchain manifest` answers from
`sdk/nros/0.6.0-nros4/` (the launcher fetched that release through its own
`install.sh`); then `nros new --workspace`, `nros setup linux`, `nros sync` and
`nros build` run under the pin. `verify-installed-pinned.sh` checks that the pin
file is unchanged, the pinned toolchain still answers, the global front is the
newest installed release (install.sh's rule, not "the one just fetched"), and no
nano-ros checkout reached the host. Green on run 40928169451. One difference
from W16's wording: the installed CLI here is the probe build (`0.0.0-probe`),
which sorts BELOW the pin, so this proves "a project pinned to a release other
than the installed one builds with the pinned one". The "newer CLI installed"
shape needs the W16 release; it is the same track with only the installed
asset changed.

### Decisions on the open "Left" items (maintainer, 2026-10-29)

The work items above landed; each carries a **Left** list. The maintainer
ruled on the ones that needed a design choice, and the rest are recorded as
small items. Each becomes one of W12–W17 below.

### W12 — a real template per node role, named like `ros2 pkg create` (M)

**Decision.** Every node role gets its own template. The flag is renamed,
and the scaffold also takes the `ros2 pkg create` flags that apply to it, so
a ROS developer's muscle memory carries over.

`--use-case` was the wrong name and the wrong promise. It accepted `talker`,
`listener`, `service` and `action`, wrote a talker for all four, and printed
"template diversification: TODO". `ros2 pkg create` has no role flag: it
takes `--node-name` and writes one hello-world node. colcon has no scaffold
verb at all (`nros build` already mirrors colcon's `--packages-select` /
`--packages-up-to`). So the role flag is ours to name, and the names come
from the ROS demos a reader already runs (`ros2 run demo_nodes_cpp talker`,
`add_two_ints_server`, `fibonacci_action_server`):

* **`--template <role>`**, with roles `talker`, `listener`, `server`,
  `client`, `action-server` and `action-client`. The default is `talker`.
  Interfaces are the demos' own: `std_msgs/String` on `/chatter`,
  `example_interfaces/srv/AddTwoInts` on `/add_two_ints`, and
  `example_interfaces/action/Fibonacci` on `/fibonacci`. The scaffold's
  `package.xml` depends on the package the role uses.
* **`--use-case` is refused, naming `--template`.** It is parsed but hidden,
  the way `--platform native` is (phase-500 W5). An alias that silently maps
  would keep a word whose only behaviour was a TODO.
* **Taken from `ros2 pkg create`:**
  * `--node-name <name>`: the node's ROS name. The default is the package
    name.
  * `--dependencies <pkg>…`: extra `<depend>` rows, which `nros sync`
    turns into message crates for Rust.
  * `--license`, `--description`, `--maintainer-name` and
    `--maintainer-email`: written into `package.xml`. The maintainer falls
    back to `git config`, as it does today.
  * `--destination-directory <dir>`: the parent directory to create the
    package in.
* **Not taken.**
  * `--build-type`: `--lang` already decides it (`nros_cargo` or
    `nros_cmake`), and a second spelling could disagree with it.
  * `--package-format`: always 3.
  * `--library-name`: `--component` is the library shape.

Each role is a real program, not a renamed talker:

* **The timer-driven roles publish or call on a timer.** These are
  `talker`, `client` and `action-client`. The C/C++ talker no longer
  publishes once and exits.
* **The other roles serve or log what they receive.** These are
  `listener`, `server` and `action-server`.
* **The code comes from the in-tree role examples** under
  `examples/linux/{rust,c,cpp}/`, the same logic with the scaffold's names.

Every scaffold shape takes every role: a Rust package, a C/C++ package on
each offered platform, a Zephyr C/C++ app, and `--component`. Where a shape
cannot take a role, `nros new` refuses it before writing a file and names the
shapes that can.

**Gate:** `check-scaffold-builds` discovers the roles from `nros new --help`
the way it discovers languages, and compiles every language × role. Unit
tests pin the refusal of `--use-case` and that each role's `package.xml`
names its interface package.

**Done when** each role builds in each language on Linux through
`check-scaffold-builds`, a scaffolded talker and listener exchange messages
(one probe), and the book's first-node chapters scaffold the listener with
`--template listener` instead of pasting a full example over a talker.

**Done (2026-10-29).** `nros new --template <role>` and the `ros2 pkg create`
flags landed as decided.

* **Templates.** The roles are files under
  `packages/cli/cargo-nano-ros/templates/roles/{rust,c,cpp}/`: the Rust
  declarative node, the C/C++ executable and the C/C++ typed component, each
  copied from the in-tree role example (`examples/linux/<lang>/<role>`,
  `examples/workspaces/{c,cpp}/src/<role>_pkg`) with the names replaced by
  tokens. The C/C++ role mains are byte-identical across platforms, which is
  why one executable template serves every non-Zephyr platform.
  * The typed talker and listener use `std_msgs/String`, like every other
    shape. The workspace components they came from use `Int32`, so a
    scaffolded talker would not have reached a scaffolded listener of the
    other shape.
  * The Rust nodes log through `nros_log`, as the Rust scaffold always did.
  * Test-only knobs in the examples (`NROS_FIB_STEP_TICKS`,
    `NROS_TEST_GOAL_ORDER`) are not carried into the templates.
* **`nros-codegen.toml` for a C/C++ talker or listener.** A C/C++ talker or
  listener scaffold writes the `std_msgs/String.data = 256` capacity, beside
  `package.xml`, where the in-tree examples state it. The C++ listener does
  not compile without it (issue 0964's static assertion), and the gate below
  caught exactly that.
* **Node names.** A package's node defaults to the package name. A component's
  node defaults to the role's demo node name (`talker`,
  `add_two_ints_server`, …), which is what `--component` wrote before.
* **Refusals.**
  * `--use-case` is refused, naming the matching `--template` (`service` →
    `server` or `client`).
  * A package flag passed to a mode that writes no package (`--workspace`,
    `--image`, `nros new node`, …) is refused rather than ignored.
* **Gate.** `check-scaffold-builds` discovers the roles from `nros new --help`
  and compiles every role in project mode for every language, plus the talker
  in component mode: 21 variants, all OK. Any other
  `<lang>_component_<role>` builds by name. Measured on 2026-10-29: the
  ten remaining component roles (C and C++) all build, after the C client
  lost the `/sum` republish its workspace original carried (it needed
  `std_msgs`, which the role does not depend on).
* **Probe.** `verify-first-node.sh` scaffolds `--template talker` and
  `--template listener` as two Rust packages on cyclonedds and asserts that
  the listener prints the talker's payload. Measured locally:
  `I heard: [Hello World: 1]`, then `[Hello World: 2]`.
* **Book.** These chapters drop the "publishes once and exits" output and the
  "paste the example over it" step:
  * `first-node-c.md`, `first-node-cpp.md`;
  * `freertos.md`, `threadx.md`;
  * `integration-nuttx.md` (both `probe-expect` blocks now
    `Publishing: 'Hello World: N'`);
  * `integration-zephyr.md` (its `probe-expect` block);
  * `ros2-interop.md` (the standalone listener is
    `--template listener`).

  `reference/cli.md` documents the roles and the `ros2 pkg create` flags.

### W13 — Rust off Linux and bare metal stays a workspace; Zephyr Rust gets an image (S)

**Decision.** There is no single-package Rust scaffold for FreeRTOS, NuttX,
ThreadX or Zephyr. Those platforms use a node library plus an entry crate,
and the book already teaches that as a workspace plus `nros new --image`. A
second road on four platforms would double what W10 must replay.

* **The refusal names the commands.** `nros new --platform freertos --lang
  rust` keeps refusing, and now prints the exact `nros new <ws> --workspace
  --lang rust` and `nros new --image <id> --board <board>` lines, with the
  platform's default board filled in. Today it names an in-tree example
  directory, which a user with no checkout does not have.
* **Zephyr Rust gets an `--image` scaffold.** It is the one platform where
  the workspace road has no Rust entry scaffold, and its chapter points at a
  GitHub example instead.

**Done when** the refusal's printed commands run as printed (a unit test
asserts their text against the catalog), and `nros new --image zephyr
--board <zephyr board>` in a Rust workspace writes an entry `nros build`
plans.

**Done (2026-10-29).** The refusal prints the commands, and the "Zephyr Rust
gets a scaffold" half was already there.

* **Zephyr Rust already has a scaffold.** `nros new entry <name> --platform
  zephyr` writes a Zephyr entry package for a Rust workspace, with its
  `[image.*]` row, and `integration-zephyr.md` § The entry already teaches it.
  W2's **Left** line ("Zephyr Rust has no scaffold at all") was stale, so no
  new `--image` scaffold was written. The refusal names `nros new entry`
  instead.
* **The refusal.** `nros new <name> --platform <p> --lang rust` on FreeRTOS,
  NuttX, ThreadX or Zephyr prints
  `nros new <name> --workspace --lang rust`, `cd <name>`, then one
  `nros new --image <id> --board <board>` per board the catalog has for the
  platform, or `nros new entry zephyr_entry --platform zephyr`. It also names
  the platform's chapter. The boards come from
  `BoardCatalog::boards_for_platform`, the list `default_board_for_platform`
  already chose from. ThreadX has two boards, so it gets two lines, each with
  its own image id.
* **Test.** `a_rust_workspace_platform_prints_the_commands_with_catalog_boards`
  checks that every printed board resolves back to itself through the same
  `--board` resolution `nros new` uses.
* **Measured.** The printed commands were run as printed for all four
  platforms, and each wrote its image. `nros build <image> --dry-run` then
  planned the FreeRTOS, ThreadX (`threadx_linux`) and NuttX images.

### W14 — one QEMU ladder, shared by the CLI and the test harness (S)

**Decision.** One small crate holds the QEMU binary lookup and the per-board
argument lines. The test harness used to keep its own copy, and the CLI
copied the harness's lines out (W6 **Left**). It is a library dependency, so
tests do not shell out to the `nros` binary, which would make them depend on
the CLI being built.

**Done when**
* `nros build`'s printed QEMU commands and `nros_tests::qemu` call one
  function;
* a test asserts that both produce the same command for NuttX arm `virt`,
  NuttX rv-virt and ThreadX riscv64;
* `orchestration::qemu_bin` and `nros_tests::qemu::qemu_system_path` are
  re-exports of the crate's functions, not second spellings.

**Done (2026-10-29).** New crate `packages/tooling/nros-qemu-ladder`, with no
dependencies and host-only. It holds `locate` (`QEMU_SYSTEM_<ARCH>`, then the
checkout's pinned project prefix, then the pinned store, plus the
store-target-list `Host` answer) and `board_line` for NuttX arm `virt`, NuttX
rv-virt and ThreadX riscv64. `orchestration::qemu_bin` now re-exports the crate,
keeping only `for_sdk` (the CLI's inputs, read from the index) and
`store_targets`. `nros_tests::qemu::qemu_system_path` builds a `Ctx` and calls
the same `locate`. `QemuProcess::start_nuttx_virt`, `start_nuttx_riscv` and
`start_riscv64_virt` launch `board_line`, and `build_next::derived_qemu`
returns it. The differences are now policy inputs, not code: a stale prefix is
`StalePrefix::Skip` in the CLI and `Refuse` (panic) in the harness, and only
the harness falls back to `$PATH`. Two drifts were reconciled. First, the CLI
printed `-netdev … -kernel <img>` while the harness ran `-kernel <img> -netdev
…`; both now use the harness order. Second, the CLI's ThreadX line had no MAC;
it now pins peer 0's `52:54:00:12:34:56`, which is the firmware's
`Config::default()` and QEMU's own first-NIC default, so the guest sees the
same address. The harness's `QEMU_SYSTEM_*` override now also ignores an empty
value, as the CLI did. Tests: the crate pins the three lines verbatim;
`nros_tests::qemu::launchers_boot_the_shared_board_lines` and
`build_next::boards_without_a_runner_get_the_harness_qemu_or_a_chapter` each
assert that their side's full command equals the crate's line. **Left:** the
harness's multicast and `dgram` NuttX/ThreadX launchers (sibling-pair
networking that the CLI never prints) still spell their machine blocks by
hand.

### W15 — `threadx-linux` retries its session open like the QEMU boards (S)

**Decision.** This is fixed in the platform, not by documenting "start the
router first". The `threadx-linux` image opens its zenoh session once, so a
router started seconds earlier, as in the reader's second terminal, races it.
That race showed as one red in the W10 eight-way sweep. The QEMU boards retry
their open, which hides the same race there. All of them get the same bounded
retry policy.

**Done when** the `threadx-linux` image opens its session against a router
started after it (a test that starts the router second), and its W10 track
stays green.

**Done (2026-10-29).** The premise was half right: every board, ThreadX
included, already opens through ONE bounded retry
(`nros-rmw-zenoh`'s `Context::connect_with_retry`, 10 attempts 300 ms apart).
What ThreadX lacked was the wait. Its backoff was `zpico_spin_once` on the
session being opened, which returns `ZPICO_ERR_SESSION` at once on a session
that is not open, so the ten attempts ran back to back and the ~3 s budget
became the 0.13 s the sweep saw. ThreadX has had a real `z_sleep_ms`
(`tx_thread_sleep`) since the platform alias TU, so the backoff and the open
lock's wait are now the platform sleep on every platform and the ThreadX
special case is deleted. That fixes the ThreadX riscv64 QEMU board too, which
shares the code. New cell `test_rtos_session_open_outlasts_a_late_router`
(`rtos_e2e`, threadx-linux) leases the listener image's baked router port,
starts the image, waits 1 s, then starts the router. Measured: it fails 3/3
before the fix (`Executor::open failed: Transport(ConnectionFailed)`) and
passes 5/5 after it in about 2.4 s. The threadx-linux Rust pub/sub, service
and action cells still pass. The W10 track itself was not re-run here.

### W16 — cut the next release; prove the project pin (maintainer dispatch, then S)

**Decision.** The next release is cut after W12–W15 land on `main`, so it
carries them. Two things wait for it:

* **`integrations/` for installed users.** The PX4 and NuttX chapters copy
  from `$(nros sdk-root)/integrations/`, which `0.6.0-nros4` predates (W2
  **Left**).
* **W11's evidence.** With a release newer than `0.6.0-nros4`, a probe track
  builds a project whose `nros-toolchain.toml` pins `0.6.0-nros4` while the
  newer CLI is installed. The launcher must fetch the pinned toolchain and
  build with it.

**Done when** the release is published, `[tool.nros]` pins it, and the
pinned-project probe is green in `probe-platforms.yml`.

**Done (2026-10-30).** Release `0.6.0-nros5` is published (tag
`nros-0.6.0-nros5`, cut from `main` at `375fd91f`, after a `publish=false`
dry run passed). Its manifest records `codegen = 21`, and its asset carries
`bin/nros-build-profile` (W17) and `share/nano-ros/integrations/`, so the PX4
and NuttX chapters' copy-from-the-SDK-root steps now work for installed users.
`[tool.nros]` pins it: `version`, the smoke `--codegen-version` expectation
(18 to 21) and `upstream`/`source.ref` all move together, and
`the_nros_smoke_check_expects_what_the_pinned_release_emits` passes against
the new pin. The pinned-project probe was green before the cut (W11, run
40928169451). Not yet run: the same track with `0.6.0-nros5` as the installed
CLI rather than the probe's own `0.0.0-probe` build. The probe always installs
the asset it builds, so that needs a probe option to install a published
release instead, and nothing in the current harness provides one.

### W17 — small items (S each)

* **`nros-build-profile` ships as a binary in the release asset.**
  `build-profiling.md` stops telling the reader to `cargo install` it from
  the SDK root.
* **Sources with no checkout.** Measure `nros setup --source px4-rs`,
  `cyclonedds-src` and `tonbandgeraet` with no checkout, as W10 did for the
  NuttX sources. Record where each lands, and fix or refuse any whose `dest`
  needs a checkout.
* **`west nros` from the installed SDK.** Either give it a spelling that
  works there, or have `board-crate-import.md` say it is checkout-only.
  `west.yml`'s `self: west-commands:` needs a literal path, and the SDK root
  is versioned.
* **The other language's road per chapter.** W10 replays one road per
  chapter: the Rust workspace for FreeRTOS and ThreadX, the C package for
  NuttX and Zephyr. Each chapter's "same flags, its own ELF" line for the
  other language gets its own probe variant (`variant=` blocks, as the
  workspace track does), so both roads are replayed.

**Progress (2026-10-29).**

* **`nros-build-profile` ships.**
  * `release-nros.yml` builds it (a root-workspace host tool, serde only),
    stages it beside `nros`, and smoke-runs it after the install.
  * `install.sh` fronts it the way it fronts `nros-launch-resolve`, when the
    asset carries one.
  * The installed track's `check-installed-sdk-root.sh` asserts it runs.
  * `build-profiling.md` says it is on PATH, and keeps `cargo install` for
    `0.6.0-nros4` and older.
* **Sources with no checkout.** The rule is in `sdk_store::source_dir_of`: a
  `[source.*]` with no `location` lands under the SDK root. That is the same
  path W10 measured for the NuttX sources. It cannot be measured on a host
  with a checkout, because the in-tree binary's own checkout outranks
  `NROS_REPO_DIR` and `NROS_WORKSPACE_ROOT` (tried: it provisioned into the
  worktree). So `verify-installed-workspace.sh` now runs
  `nros setup --source` for `tonbandgeraet`, `px4-rs` and `cyclonedds-src` on
  the installed track, and asserts each lands non-empty under
  `$(nros sdk-root)`.
* **`west nros` is checkout-only, said so.** West loads extension commands
  from a manifest project, and the installed SDK root is a versioned store
  directory no `west.yml` names. `integration-zephyr.md` now says so and gives
  the installed spelling, `nros build --zephyr-workspace <dir>`.
* **The other language road per chapter.** There are two new installed
  tracks, each its own `variant=` set of book blocks. Steps both languages
  share carry both tags.
  * `freertos-c`: the C package on MPS2-AN385, booted and echoed as
    `std_msgs/String`. The chapter's Rust blocks are now `variant=rust`.
  * `threadx-linux-c`: the C package on the Linux simulator.

  Both are in `just probe` and in `probe-platforms.yml`, and both are green
  on the branch: runs 40902931965, 40910156964, 40913246178, 40915996754 and
  40918377304. The `workspace` track's source assertions were green in the
  same runs.
* **Zephyr Rust: not replayable yet, issue 2152.** A third track,
  `zephyr-rust` (the workspace plus `nros new entry`, run as `zephyr.exe`),
  was added. It was then withdrawn after six runs, each of which got one
  step further:
  * **Run 40902931965** stopped at Kconfig, on two defects, both now fixed
    and each held by a test:
    * the chapter's west manifest did not fetch `zephyr-lang-rust`. It now
      names the module at the revision `west.yml` pins.
    * `nros new entry`'s native_sim conf turned on NSOS without
      `CONFIG_NET_DRIVERS=y`. The conf now carries every line of the SDK's
      `native-sim-line-3.7.conf`.
  * **Runs 40910156964–40921005534** found that the host prerequisites a
    Rust Zephyr image needs beyond a C one are installed by no `nros setup`
    path: the `x86_64-unknown-none` target, `clang` and `libclang-dev`.
    That is issue 2151. Until it lands, the chapter's Rust steps install
    them by hand (with the setup-installed rustup put on PATH).
  * **The last run** compiled through bindgen and stopped in `nros::main!`.
    The scaffolded entry never gets its `<pkg>_nros_selection` package,
    because the facade writer reads only path dependencies and the scaffold
    writes registry-style ones. That is issue 2152.

  Those Rust steps were plain text, with a pointer to issue 2152, and the
  track was out of the matrix until that landed.

  **Back (2026-10-30).** Issues 2151 and 2152 are resolved. The chapter's
  Rust steps are probe blocks again: `nros setup --sudo` in the workspace
  replaces the hand-installed packages and target. The `zephyr-rust` track
  is back in `probe-platforms.yml` and green on run 41050394546 (scaffold,
  sync, build, boot, and the talker and listener in one image). Getting
  there took three more runs, and each found something: the board catalog
  read from the index's directory, which is a fetched copy under
  `NROS_INDEX_URL`; then rustup's `clippy`, which zephyr-lang-rust runs as
  part of every Rust app's build.
* **NuttX Rust: not replayable yet, issue 2149.** Measured locally: the
  scaffolded workspace image failed at `core`, because the cargo road ran
  the default toolchain for a `toolchain = "nightly"` board. That is fixed:
  `board_rust_channel` gives `RUSTUP_TOOLCHAIN` to both cargo handoffs. The
  build then stops at `nuttx/config.h`, because nothing configures the
  kernel on the cargo road. The chapter said so, and no NuttX Rust track
  was added until issue 2149 landed.

  **Landed (2026-10-30).** The cargo road now runs the board's kernel
  configure and export first: `scripts/nuttx/build-nuttx.sh`, from
  `[board.net] kernel_config`. A second defect then came up. NuttX's
  `required_env` still demanded `NUTTX_DIR`, the one source tree phase-484
  W2c had left there. The chapter's Rust paragraph is a probe block now. The
  new installed `nuttx-arm-rust` track builds the workspace image, boots it
  on QEMU and expects the talker and listener. Green on run 41051046857.

**Progress (2026-10-30): the three Rust roads that were not replayable.**
Issues 2149, 2151 and 2152 are resolved (PRs #2863 and #2867). The
`zephyr-rust` track is back, and a new `nuttx-arm-rust` track builds and
boots the NuttX Rust workspace image. Both are green on the installed track
(runs 41050394546 and 41051046857). Each is recorded in the bullets above
and in its issue's Resolution.

## Issues homed here (2026-10-29)

* [2152](../../issues/archived/2152-zephyr-entry-scaffold-no-selection.md),
  resolved. A Zephyr entry from `nros new entry` never got its selection
  package, so it did not compile outside a checkout (found by W17's
  `zephyr-rust` track).
* [2151](../../issues/archived/2151-zephyr-rust-target-not-provisioned.md),
  resolved. `nros setup zephyr` did not install what a Zephyr Rust image
  needs beyond a C one: the target, `clippy`, `clang` and `libclang-dev`
  (found by W17's `zephyr-rust` track).
* [2149](../../issues/archived/2149-nuttx-rust-workspace-no-nightly.md),
  resolved. A NuttX Rust workspace image did not build from `nros build`.
  The nightly half was fixed in W17, and the kernel-configure half above.
* [2137](../../issues/archived/2137-nros-build-never-takes-the-census.md),
  resolved. Promise 2 (one build) did not hold for any cross image whose model
  folds in a contract. `nros build --all` refused it with `census missing`,
  because only the fixture pre-pass took a census. `nros build` now takes it
  itself before the plan generates anything.

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
* W12–W15 and W17 are independent of each other. W16's release cut waits for
  W12–W15 on `main`, and its pinned-project probe waits for the release.

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
