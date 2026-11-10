---
id: 2236
title: "A NuttX make app as a nros project needs a project identity and a knob channel"
status: open
type: limitation
area: [nuttx, build, configure]
found: 2026-11-04
related: [phase-530, rfc-0120, issue-1883, issue-0488, issue-2426]
---

## Symptom

The NuttX `apps/external` make road (`integrations/nuttx/Makefile`,
`Make.defs`) cannot run `nros configure` for the application it builds, so
an image built there is sized by its Kconfig knobs, never by its census or
its declarations. RFC-0120 D5 lists NuttX make as a hooked host; phase-530
W5 found it has nothing to hook.

## Why (measured, phase-530 W5)

- The make road builds ONE thing: the runtime archive (`libnros_c.a` or
  `libnros_cpp.a`), from Kconfig alone -- `CONFIG_NROS_RMW_*`,
  `CONFIG_NROS_ROS_EDITION`, `CONFIG_NROS_C_API` / `CONFIG_NROS_CPP_API` --
  through `cmake/NanoRosImageCrate.cmake -P`.
- It carries no nros project: no `system.toml`, no image, and the user's
  application is not one of its inputs. The per-example staging that once
  made it one was retired (Phase 212.M-F.12; issue 1883 retired the
  per-bringup template).
- It has no channel for configure's outputs: no seed, no knob file and no
  `--config` reaches its cargo build.

So layer 0 has nothing to configure there. Ruling R36 (phase-530) records
the road as a GUEST road, sized by its Kconfig knobs.

## What making it a project needs

Two design choices, neither of which exists today:

1. **A project identity** -- where the application's `system.toml` is and
   which image it builds (for example a Kconfig string naming the app
   directory, plus an image id).
2. **A knob channel** -- how configure's answer reaches the archive's cargo
   build (for example the image's `nros-cargo.toml` passed with `--config`,
   as the cargo road does) and the app's own objects.

With both, `nros configure` becomes a prerequisite of the archive's
`context::` step, run once per build.

## Workaround

Build a NuttX image as a nros project on the CMake leaf road
(`nros build` of a leaf whose `[image.*] board` is a NuttX board), which is
hooked: phase-530 W5 measured `examples/qemu-armv7a-nuttx/c/talker` at one
configure, no re-run, census-tight knobs identical to `nros build`'s.

## Home

phase-530 (W5, Ruling R36) records the decision; the work belongs with the
hookless/IDE adapters RFC-0120 D8 leaves for a later phase.

## Design write-up (2026-11-10)

This needs a design ruling, not a fix: the two missing pieces each have
several workable shapes, and they trade against rules other RFCs already
hold (RFC-0098 D10's one source of image configuration, RFC-0103's one knob
ladder, RFC-0120 D1's layer rule and D4's census default). What follows is
what the road does today, read from the tree, the options, a
recommendation and the questions a ruling has to answer. No code changed.

### 1. What the make road does today

Read from the tree on 2026-11-10 (nothing was built: the make road needs the
NuttX toolchain and mutates the shared `.config`, issue 0525). One make
behaviour was measured with GNU Make 4.4.1, and is marked as measured.

**Inputs.** The archive is a function of four Kconfig symbols and the
kernel's arch:

- `integrations/nuttx/Kconfig` declares `NROS` (line 19), the RMW choice
  (lines 31-58), the edition choice and its derived `NROS_ROS_EDITION`
  string (lines 60-92), and `NROS_C_API` / `NROS_CPP_API` (lines 101-117).
  Nothing else: **no knob symbol** from the registry
  (`packages/tooling/nros-zephyr-build/knobs.toml`, 109 rows) is declared
  there.
- `integrations/nuttx/Makefile:76` reads the edition, `:91` derives the RMW
  from whichever `CONFIG_NROS_RMW_<X>=y` is set, `:99` fixes the API
  features, `:122-128` picks the C or C++ flavor.

**The image crate is generated at PARSE time.**
`integrations/nuttx/Makefile:105-111` calls
`cmake -P cmake/NanoRosImageCrate.cmake` from a `$(shell ...)`, and `:129-131`
runs it on every parse that has a configured tree (the issue 0551 guard,
`:52`). Script mode takes exactly `FLAVOR`, `RMW`, `EDITION`, `OUT_DIR`,
`API_FEATURES` and an optional `WORKSPACE_DIR`
(`cmake/NanoRosImageCrate.cmake:43-51`, `:333-361`) and prints
`<manifest> <features>`. It has no `--config`, seed or knob input, and needs
none: it writes a manifest, and knobs never ride a manifest. The one thing it
does beyond the manifest is seed the root's `Cargo.lock` from the registry
pins (`:302-321`).

**The cargo build runs in `context::`.** `integrations/nuttx/Makefile:274-279`
calls `NROS_CARGO_BUILD` (`:241-263`). Its environment is the `CC_*` /
`CXX_*` / `AR_*` toolchain names and `NUTTX_INCLUDE_DIR`; its `--config`
flags are three inline literals (`:229-231`): the `libc` patch, the armv7a
rustflags and the armv7a `CFLAGS`. No `DOTCONFIG`, no `NROS_*`, no
`--config <file>` reaches it.

**So the archive is NOT sized by Kconfig knobs.** The knob ladder
(`packages/tooling/nros-zephyr-build/src/lib.rs:374-380`) reads, highest
first: the exported variable, `CONFIG_<sym>` in the `.config` that
`$DOTCONFIG` names, a platform/board rung when the caller splices one in, and
the crate builtin. On the make road `DOTCONFIG` is unset and the NuttX Kconfig
declares no knob, so rung 2 is empty; no `NROS_BOARD_TOML` is set, so no board
rung either. What sizes the archive is the crate builtins, plus any `NROS_*`
the person exported in the shell that ran `make`. Ruling R36's wording
("sized by its Kconfig knobs", phase-530 W5 and RFC-0120 D5) describes the
INTENT; the mechanism is defaults. This matters for the options below: there
is no NuttX knob vocabulary yet for configure's answer to disagree with.

**A second statement of board facts already exists here.** The three inline
`--config` literals restate what the board descriptor states, and they have
drifted from it:

- `NROS_CARGO_BUILD` passes `-Zbuild-std=std,panic_abort`
  (`integrations/nuttx/Makefile:254`), while the board's `cargo_config`
  says `build-std = ["core", "alloc", "panic_abort"]`
  (`packages/boards/nros-board-nuttx-qemu/nros-board.toml`, the first
  `[[board]]`'s `[unstable]`, phase-359 W7) and explains that the `libc`
  fork the make road still patches in is unnecessary once `std` is not
  built;
- the rustflags and `CFLAGS` literals are armv7a-only, while the same apps
  tree serves `rv-virt` (issue 0525, comment at `:28-33`).

**Where the user's app sits.** NuttX discovers apps through
`$(APPDIR)/external/*/Make.defs` (`integrations/nuttx/external-Make.defs.in`)
and `osource "$APPSDIR/external/*/Kconfig"` (`external-Kconfig.in`). The
nano-ros shell is ONE of those apps (`Make.defs:15`,
`CONFIGURED_APPS += $(APPDIR)/external/nano-ros`); the user's app is a
sibling that the shell never names.

**Step ordering.** The apps tree builds `context` before anything compiles
(`nuttx-apps/Makefile:186-187` in the pinned `third-party/nuttx/` tree:
`context: | staging`, then `context_all`), and `context_all` is one prerequisite per configured app
(`nuttx-apps/Makefile:57`, `:179`). Under `make -j` those run with no order
between the nano-ros shell and the user's app. Two consequences:

- whatever runs `nros configure` must sit inside the SHELL's own `context::`
  recipe, before its cargo line, never in the app's;
- every sub-make re-reads `$(APPDIR)/Make.defs`, which includes every
  `external/*/Make.defs`, so a make VARIABLE an app's `Make.defs` sets IS
  visible when the shell's `Makefile` is parsed (that is how
  `CONFIGURED_APPS` itself works).

**Recipe expansion (measured).** GNU make expands EVERY line of a recipe
before it runs the first one. A two-line recipe whose first line writes a
file and whose second reads it with `$(shell cat f)` printed `MISSING`
(GNU Make 4.4.1). So a recipe cannot run `nros configure` on one line and
pick up its answer with `$(shell nros locate ...)` on the next: the answer
must be taken by the shell (`cfg=$$(...)`) in the same command line as the
cargo build, or be at a path fixed before configure runs.

**The app's own objects.** The C/C++ sizes reach the app's TUs through the
config headers the archive's cargo build places (`Make.defs:97-106`,
issue 1824), so whatever reaches the archive's cargo build reaches the app's
objects too, with no second channel, as long as the app compiles after
`context` (it does).

**What `nros configure` gives a host to read.** `nros locate configured
--project <dir> --image <id>` prints the stamp dir, the resolved dir,
whether the stamp is current, why not, and the input files the digest
covered, in `path`, `sh` and `cmake` formats
(`packages/cli/nros-cli-core/src/cmd/locate.rs:739-800`). There is no make
format. The cmake road's knob carrier is a cargo `--config` file with an
`[env]` table (`cmake/NanoRosImageEnv.cmake:1-32`, issue 1712): `[env]` rows
are not `force`, so an exported variable still wins, and cargo fingerprints
the values, so an edited row re-runs the build script.

**No lane runs it.** `scripts/check-nuttx-integration-makefile.sh` (header)
records that no shipped defconfig sets `CONFIG_NROS=y` and no CI lane builds
this road; `just nuttx build-integration-app` is the manual build.

Two of the board facts the make road restates by hand have already drifted
from the board's `cargo_config` (`-Zbuild-std=std` against `core,alloc`, and
armv7a-only flags on a tree that also serves rv-virt); that is filed as
[issue 2426](2426-nuttx-make-road-board-facts-drift.md), held for this ruling.

### 2. Options

#### (a) Project identity: which `system.toml`, which image

**A1. Two Kconfig strings in the shell's Kconfig.**
`CONFIG_NROS_PROJECT` (a directory, relative to `$(APPDIR)`, for example
`external/my-talker`) and `CONFIG_NROS_IMAGE` (an image id; empty means the
project's `default_images`, as the cmake hook does).

- Cost: small. Two symbols in `integrations/nuttx/Kconfig`, read in the
  `Makefile` like `NROS_ROS_EDITION` is.
- For: menuconfig shows it; it lives in the defconfig a board already keeps.
- Against: a path inside a defconfig. Relative to `$(APPDIR)` it survives a
  moved checkout; absolute it does not. It also names the project from
  OUTSIDE the project, so renaming the app dir breaks the defconfig silently
  unless the shell refuses a missing `system.toml`.

**A2. The app declares itself, in its own `Make.defs`.** The app's
`Make.defs` adds `NROS_PROJECTS += $(APPDIR)/external/<app>` (and optionally
`NROS_IMAGE := <id>`), beside its own `CONFIGURED_APPS +=` line. The shell's
`Makefile` reads `NROS_PROJECTS`: none means a guest (today's road,
unchanged); one is the project; more than one is refused, naming them.

- Cost: small. One idiom in the app template, one read in the shell.
- For: the identity sits with the project it names, uses the mechanism NuttX
  apps already use, and needs no path in a defconfig. A guest road stays
  byte-identical because the variable is simply absent.
- Against: the app's `Make.defs` is a second place a nros-specific line
  lives, and it is make syntax a non-NuttX reader will not expect.

**A3. Discovery.** The shell looks for exactly one `system.toml` under
`$(APPDIR)/external/*/` whose image names a NuttX board.

- Cost: a walk on every parse (issue 0551: not every parse has a configured
  tree), plus a refusal for zero-or-many.
- Against: identity by inference. CLAUDE.md's "a count is only as scoped as
  the tool that produced it" applies, and a second nros app dropped into
  `external/` changes which project builds. Not recommended.

**Image cross-check (any of A1-A3).** The chosen image's board must be a
NuttX board, and its `[board.nuttx] board_config` (issue 1652) and target
must agree with the kernel being built (`CONFIG_ARCH`, the triple
`NROS_TARGET_TRIPLE` resolves). A mismatch is refused, naming both, never
built for the wrong arch.

#### (b) Knob channel: how configure's answer reaches the cargo build

**B1. A cargo `--config` file, as the cargo and cmake roads do.** The shell's
`context::` recipe runs `nros configure --project <dir> --image <id>`, then
cargo with `--config <file>`, where `<file>` is the image's generated cargo
config (RFC-0098 D1's `nros-cargo.toml`, or the cmake road's image-env file,
issue 1712). The path is taken in the SAME shell line (the recipe-expansion
measurement above), from configure's own output or from
`nros locate configured --format sh`.

- Precedence: `[env]` rows without `force`, so an exported `NROS_*` still
  wins (the ladder's top rung) and the file is the app rung, exactly as on
  the other two roads. One ladder, no new rule written in make.
- Freshness: cargo fingerprints `[env]` values (issue 1712's measurement);
  configure's own stamp covers `system.toml`, the board and `knobs.toml`
  (RFC-0120 D7). Make needs no new edge, because `context::` runs every build
  and a current configure is a digest comparison.
- The board facts come with it: the file carries the board's `cargo_config`,
  so the three inline literals (`Makefile:229-231`) and the stale
  `-Zbuild-std=std,...` can go, which removes the drift above instead of
  adding a third statement.
- RMW and edition come from the image too, so the image-crate call
  (`Makefile:105-131`) has to take them from configure's answer. That means
  moving it from parse time into the `context::` recipe (after configure),
  because a parse-time configure would run on `olddefconfig`'s parse
  (issue 0551). `EXTRA_LIBS` and the placed-header paths (`Make.defs:52-106`)
  depend only on the arch and the flavor, so they stay at parse time.
- Cost: medium. Mostly the `Makefile`, plus whatever lets `nros configure`
  state the file's path for a NuttX image (if the cmake leaf road's
  configure does not already write one per image).

**B2. A generated make fragment.** Configure writes `nros-knobs.mk`
(`export NROS_<ID> := <v>` lines) and the shell `include`s it.

- Precedence: a make `export` is indistinguishable from a person's shell
  export, so it lands on the TOP rung and outranks the person's own export
  for the same knob: issue 1712's `cmake -E env` inversion, again.
- Freshness: the file is produced in `context::` but read at parse time, so
  either the first build reads a stale or missing file, or make's
  remake-the-makefile restart is used, which is a re-read fixed point:
  the shape RFC-0120 deleted.
- Not recommended.

**B3. Render into NuttX Kconfig (RFC-0098 D11's Zephyr shape).**
`scripts/gen-knob-registry.py` also renders a prompt-less knob section into
`integrations/nuttx/Kconfig`, a guest gets prompts back, a project gets its
values as `default`s from a generated fragment, and the shell exports
`DOTCONFIG=$(TOPDIR)/.config` to cargo so the ladder's rung 2 reads them
(NuttX's `.config` has Zephyr's `CONFIG_X=value` shape).

- For: menuconfig shows the knobs; one vocabulary with Zephyr.
- Against: NuttX evaluates Kconfig at `olddefconfig` / `menuconfig`, before
  `context::`, and has no `module_ext_root`-style hook, so configure would
  have to run at preconfig time (`nuttx-apps/Makefile:195`, `preconfig:
  Kconfig`), outside any recipe NuttX gives an app. A census-derived value in
  `.config` also costs a re-`olddefconfig` on every entity-changing edit
  (RFC-0120 D7, last paragraph). Largest change, and the generator gains a
  second Kconfig dialect.
- It is also a GUEST improvement on its own: rendering the prompts alone (no
  project) would give today's guest road the knob vocabulary R36 assumed it
  had. That half does not need this issue's ruling.

**Precedent (A1 + B1).** micro-ROS's NuttX app has this shape: in
`micro-ROS/nuttx_apps` (branch `foxy`, commit
1f4c6f0b1c5c26268d7a7b01760124a96d75a432), `uros/Makefile` renders
`rmw_config.meta` from `CONFIG_UROS_MAX_*` with `$(TOPDIR)/.config` as a
prerequisite, and runs `colcon build --metas ... rmw_config.meta` from
`context`. So Kconfig is the surface, and a generated file is read in context.
Upstream's `tools/Rust.mk` (at the pinned nuttx-apps commit
e4b84b29d47b98a666df524d6f2d38904c972de5) passes no Kconfig to cargo; only
`CONFIG_DEBUG_FULLOPT` picks release or debug.

#### Where Kconfig and configure could disagree

With a project, two sources can state the same fact:

- **RMW, edition, API flavor.** Kconfig states them today
  (`CONFIG_NROS_RMW_*`, `CONFIG_NROS_ROS_EDITION`, `CONFIG_NROS_*_API`); a
  project states RMW and edition in `system.toml`, and its languages imply
  the flavor. RFC-0098 D10 says `system.toml` is the single source, so
  configure's answer is the one built. Ranking the two is the defect D10
  names ("ranking two sources only decides which one silently loses"), so a
  disagreement is REFUSED at the top of the shell's `context::`, naming both
  values and both files, the way `nros_ros_edition_agree`
  (`cmake/NanoRosRosEdition.cmake:42-50`, Ruling R34) refuses on the cmake
  and Zephyr roads. The alternative, hiding the Kconfig choices when a
  project is named, cannot be done from make: Kconfig is evaluated before the
  project is known.
- **Knob values.** Under B1 there is nothing to disagree with (the NuttX
  Kconfig has no knob symbols). Under B3 a defconfig line `CONFIG_NROS_<knob>=`
  in a project build is refused, as phase-484 W5b makes it a Kconfig error on
  Zephyr; an exported `NROS_<ID>` stays the top rung on every option.

#### Sizing a make app (RFC-0120 D4)

A cross C/C++ leaf that declares no entities is census-sized by default, and
the census builds the leaf's `CMakeLists.txt` for the host. A NuttX make app
has a `Makefile`, not a `CMakeLists.txt`, so the default refuses it. Unless
the census learns to build a make app's `CSRCS` for the host, a make-app
project must state `entities = [...]` (tight) or `entities = "defaults"`
(loose, written down) on its component. That is consistent with D4's
existing refusal for a leaf whose sources do not build for the host, but it
means a make app is never census-tight, which is a ruling to make on purpose.

### 3. Recommendation

**A2 + B1, with a refusal on any Kconfig/configure disagreement.**

- A2 keeps the identity inside the project, needs no path in a defconfig, and
  leaves the guest road unchanged by construction (no `NROS_PROJECTS`, no
  configure).
- B1 is the mechanism the cargo and cmake roads already use, so the make road
  joins the one ladder instead of adding a precedence rule in make, and it
  deletes the make road's second statement of board facts.
- The make road then becomes a HOOKED road in RFC-0120 D5's sense (its host
  configure is the `context::` step, and it calls configure before its first
  reader), not a hookless adapter. D5's NuttX row and Ruling R36 would be
  amended to "a guest road when no project is named, hooked when one is".
- Make-app sizing: declared entities required (no census for a make app) in
  the first cut; a make census is a separate decision.
- B3's guest half (prompts for the guest road) is worth filing separately; it
  is independent of this issue.

### 4. Questions for the maintainer

1. Should a NuttX `apps/external` make app be able to be a nros project at
   all, or does the CMake leaf road stay the only project road for NuttX
   (close this issue as wontfix, and correct R36's "sized by its Kconfig
   knobs" to "sized by crate defaults and exported variables")? yes / no
2. Project identity: A1 (Kconfig strings `CONFIG_NROS_PROJECT` +
   `CONFIG_NROS_IMAGE`), A2 (the app's `Make.defs` sets `NROS_PROJECTS` and
   optionally `NROS_IMAGE`), or A3 (discovery)? Choose one.
3. Knob channel: B1 (cargo `--config` file, configure run inside the shell's
   `context::`), B2 (generated make fragment) or B3 (rendered NuttX Kconfig
   plus `DOTCONFIG`)? Choose one.
4. When a project is named, RMW, edition and API flavor come from the image,
   and a differing `CONFIG_NROS_RMW_*` / `CONFIG_NROS_ROS_EDITION` /
   `CONFIG_NROS_*_API` is REFUSED (not overridden, not merged). yes / no
5. Under B1, the make road drops its inline `--config` literals and its
   `-Zbuild-std=std,panic_abort` in favour of the board's `cargo_config`
   (including `build-std = ["core", "alloc", "panic_abort"]`), even on the
   guest road. yes / no (no means only project builds take the board's file)
6. A make-app project must declare its entities (`entities = [...]` or
   `entities = "defaults"`); no host census for make apps in this work.
   yes / no
7. Does this road then count as HOOKED (RFC-0120 D5, its `context::` step is
   the host configure), with D5's NuttX row and Ruling R36 amended, rather
   than as one of D8's hookless adapters? yes / no
8. Should the knob prompts for the GUEST road (B3's guest half: registry
   knobs as NuttX Kconfig symbols plus `DOTCONFIG` on the cargo line) be
   filed as a separate issue now, whatever 2 and 3 decide? yes / no

### 5. Acceptance for the implementation (proposed)

- A guest build (`CONFIG_NROS=y`, no project named) produces a byte-identical
  cargo command to today's, unless question 5 is answered yes, in which case
  the only difference is the board's file replacing the three literals.
- A project build of a NuttX C talker app on `qemu-armv7a-nsh`, from a clean
  apps tree: `nros configure` runs once inside the shell's `context::`, a
  second `make` reports it up to date, and the archive's knobs (the placed
  `nros_config_generated.h` and the cargo `[env]` the build script saw) are
  identical to `nros build`'s for the same image on the CMake leaf road.
- An exported `NROS_<ID>` still wins over the image's row, on both roads.
- Editing a `[image.<id>.knobs.*]` row in the app's `system.toml` changes the
  archive on the next `make`, with no `clean`.
- A Kconfig RMW or edition that differs from the image's is refused with both
  values and both files named (if question 4 is yes); two apps declaring a
  project are refused naming both (A2); an image whose board does not match
  the kernel's arch is refused.
- `just nuttx build-integration-app` (or a successor) builds the project case
  and is reachable from a lane, so the road stops being one nothing runs
  (`scripts/check-nuttx-integration-makefile.sh`'s note).
- RFC-0120 D5 and phase-530 W5's R36 text are amended to match.

### Proposed home

Not phase-530: its W5 ruled this out of scope, and the phase is otherwise
closing. Proposed: a new phase opened with `just phase-new` once the
questions above are ruled, implementing the amended RFC-0120 D5 NuttX row.
If question 7 is answered no, it belongs instead with the later phase
RFC-0120 D8 names for the hookless adapters.
