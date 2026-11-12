---
id: 2236
title: "A NuttX make app as a nros project needs a project identity and a knob channel"
status: open
type: limitation
area: [nuttx, build, configure]
found: 2026-11-04
related: [phase-530, phase-550, rfc-0120, rfc-0126, issue-1883, issue-0488, issue-2426]
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
and `apps/external/Kconfig`, which `scripts/nuttx/stage-external-apps.sh`
generates with one explicit `source` line per staged app. The
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

Update (2026-11-11): the candidate work, including the make-road defects of
issue 2426, is drafted in [phase-550](../roadmap/phase-550-nuttx-native-build-infrastructure.md),
each item gated on the questions here; it starts only once they are ruled.

## NuttX build-system study (2026-11-11)

How NuttX itself configures and builds an application, read at our pins, so
that the options above can be weighed against what NuttX already provides.
Every upstream citation below was read in a fresh clone of the pinned commit;
claims that could not be checked there are marked "reported, not verified".

### Pins

- Kernel: `third-party/nuttx/nuttx` pins our fork (`jerry73204/nuttx`, branch
  `nano-ros`) at aea00d736d698994a7c0a7339b56b04af30ec21e. That commit is the
  upstream `nuttx-12.13.0` tag (b9b5d9afbf5a88f3c52830d4ce88ae038be0fd35,
  "Documentation: add NuttX-12.13.0 release notes") plus six fork commits
  (signal constants, bulk TLB invalidation, virtio-net CTRL_VQ and its DMA
  buffers, the armv7a `.bss` program header). It is not a commit of
  `apache/nuttx` itself.
- Apps: `third-party/nuttx/nuttx-apps` pins `apache/nuttx-apps` at
  e4b84b29d47b98a666df524d6f2d38904c972de5, which is exactly the apps
  `nuttx-12.13.0` tag. The two pins agree: both are 12.13.0.
- Line numbers below are at those two commits unless a line says otherwise.
  Upstream `master` was read once for comparison: nuttx-apps
  ee3d67104d117bb3d932b29b6068f513d04109bf (2026-11-10) and nuttx
  7e5a770880c01b1a89673616b0410c8da846d3b2.

### How a NuttX build is configured and run

- **Configure.** `tools/configure.sh` does not copy the defconfig: it expands
  it into `.config` with `tools/process_config.sh` (`configure.sh:243-245`),
  which understands `#include` lines with search paths
  (`process_config.sh:29-54`), and then runs `tools/sethost.sh`
  (`configure.sh:357`), which runs `make olddefconfig`
  (`sethost.sh:232`, `:234`). `olddefconfig` uses `kconfig-conf` when
  kconfiglib's `menuconfig` is not on PATH and kconfiglib otherwise
  (`tools/Unix.mk:684-714`), after `clean_context` and `apps_preconfig`
  (`Unix.mk:733-736`).
- **Kconfig discovery.** The apps tree's `Kconfig` is generated at preconfig
  (`apps/Makefile:191-195`) by `tools/mkkconfig.sh`, which sources every
  `*/Kconfig` one level down (`mkkconfig.sh:62`, `:76-77`). So
  `apps/external/` needs its own `Kconfig` (our staging script writes one).
  The kernel-side `EXTERNALDIR` (`nuttx/Kconfig:2955-2964`,
  `Unix.mk:80-84`) is a different slot: `nuttx/external/Kconfig`, kernel
  code, not apps.
- **The out-of-tree app slot.** `apps/external` is the documented place for a
  third-party app: link it in, configure, build (`apps/README.md:226-236`),
  and it is gitignored (`apps/.gitignore:33`). `apps/Make.defs` includes
  every `*/Make.defs` (`Make.defs:32`, `:48-52`), which is how an app adds
  itself to `CONFIGURED_APPS`.
- **config.h.** `include/nuttx/config.h` is made from `.config` by
  `tools/mkconfig` (`Unix.mk:266-275`).
- **Context.** The top-level `context` target needs `config.h`, `version.h`,
  the dirlinks and `<dir>/.context` for every `CONTEXTDIRS` entry
  (`Unix.mk:457-459`); each runs `make -C <dir> context` (`Unix.mk:466-467`).
  `$(APPDIR)` is one of them (`tools/Directories.mk:64`). In the apps tree,
  `context` runs `context_all`, then `register_all`, then `context_wasm`
  (`apps/Makefile:186-189`), one sub-make per configured app
  (`apps/Makefile:57-58`, `:179-180`).
- **An app's hooks.** `Application.mk` leaves `context::` empty for the app
  to extend (`Application.mk:359`). `register::` writes one
  `$(BUILTIN_REGISTRY)/<PROGNAME>.bdat` per program (`:120`, rule
  `:363-369`, `register::` at `:371`) unless the app is built as a module
  (`DO_REGISTRATION`, `:141-152`). That registration is what makes an app an
  NSH command.
- **Starting an app at boot.** `CONFIG_INIT_ENTRYPOINT` names the function
  the kernel starts (`sched/Kconfig:444`); the default is `nsh_main`.
- **Exporting a built kernel.** `make export` (`tools/mkexport.sh`) ships the
  built libraries, the startup objects, the `.config` (`:158-161`) and a
  `scripts/Make.defs` that records `ARCHCPUFLAGS`, `CROSSDEV`, `CC` and the
  other tool flags (`:247-269`). Our CMake leaf road links against that
  export (`scripts/nuttx/build-nuttx.sh`, "Export NuttX").

### The CMake build

- `CMakeLists.txt:95-105` refuses to configure without kconfiglib's
  `olddefconfig`, and `:406` exports every Kconfig symbol as a CMake variable
  (`nuttx_export_kconfig`). Apps use `nuttx_add_application` (for example
  `apps/examples/rust/hello/CMakeLists.txt`). Reported, not verified: that
  this has held since nuttx-12.3.0, that make is still the default flow, and
  that openvela is moving to CMake.
- The apps `CMakeLists.txt` adds `external/` whenever the DIRECTORY exists
  (`apps/CMakeLists.txt:76-77`), not when `external/CMakeLists.txt` exists.
  A tree staged for the make road (an `external/` with no `CMakeLists.txt`)
  therefore stops a CMake configure. This corrects a claim that the
  subdirectory is optional; it agrees with
  [issue 2426](2426-nuttx-make-road-board-facts-drift.md) item 7.

### Rust

- **Make.** `apps/tools/Rust.mk` maps `LLVM_ARCHTYPE` / `LLVM_ABITYPE` /
  `LLVM_CPUTYPE` to a triple (`RUST_TARGET_TRIPLE`, `:47-77`) and runs
  `cargo build -Zbuild-std=std,panic_abort` with `NUTTX_INCLUDE_DIR` as the
  only environment (`:90-106`). Its header lists `armv7a` (`:31`), but the
  macro has no `armv7a` branch: only `thumb%`, `riscv32`, `riscv64`, `x86` and
  `x86_64`. The `CONFIG_DEBUG_FULLOPT` branch adds
  `-Zbuild-std-features=panic_immediate_abort` (`:93-94`).
- **CMake.** `apps/cmake/nuttx_add_rust.cmake` has the same table in
  `nuttx_rust_target_triple` (`:49-96`) and the same cargo line in
  `nuttx_add_rust` (`:113-175`, the command at `:150-159`). NuttX's CMake
  sets `LLVM_ARCHTYPE` to `armv7-a`, with a hyphen
  (`arch/arm/src/cmake/armv7-a.cmake:42`), where make sets `armv7a`
  (`arch/arm/src/armv7-a/Toolchain.defs:40`); neither spelling has a branch.
  A debug build passes `--${RUST_PROFILE}`, which is `--debug` (`:133`,
  `:155`).
- **What reaches cargo.** Only the include dirs and, through
  `CONFIG_DEBUG_FULLOPT`, the profile. No other Kconfig value reaches cargo on
  either flow.
- **The NuttX triples are rustc tier 3.** At our pinned nightly both
  `armv7a-nuttx-eabihf` and `riscv32imac-unknown-nuttx-elf` print
  `"tier": 3` and `"std": true` (measured, below).
- **Upstream since our pin.** On nuttx-apps `master` the immediate-abort
  flag became `RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort"`, the
  CMake debug build passes no profile flag, an unknown triple is a
  `FATAL_ERROR`, and x86 / aarch64 use JSON target specs that upstream
  ships in `nuttx/tools/` (`x86_64-unknown-nuttx.json` and four more). There is
  still no `armv7a` branch on `master`.
- **The upstream example** (`apps/examples/rust/hello`) is a `staticlib` whose
  `Cargo.toml` sets `panic = "abort"` in both profiles; its `Make.defs` adds
  the archive with `EXTRA_LIBS += $(call RUST_GET_BINDIR,...)` and its
  `Makefile` runs `RUST_CARGO_BUILD` from `context::`.

### Measured at the pins (2026-11-11)

All in a scratch clone (`/mnt/wd/nt/nx`, never the checkout's shared tree,
issue 0525), with the board's defconfigs
(`packages/boards/nros-board-nuttx-qemu/nuttx-config/{arm,riscv}/defconfig`)
and the pinned nightly (`nightly-2026-04-11`).

| what | qemu-armv7a (`arm/defconfig`) | rv-virt (`riscv/defconfig`) |
| --- | --- | --- |
| `LLVM_ARCHTYPE ABITYPE CPUTYPE` after `olddefconfig` | `armv7a eabihf cortex-a7` | `riscv32 ilp32 sifive-e31` |
| `RUST_TARGET_TRIPLE` (make) | empty | `riscv32imac-unknown-nuttx-elf` |
| `nuttx_rust_target_triple` (CMake, `cmake -P`) | empty for `armv7-a` and for `armv7a` | `riscv32imac-unknown-nuttx-elf` |
| `ARCHCPUFLAGS` | `-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4 -mlittle-endian` | `-march=rv32imac -mabi=ilp32` |
| `CROSSDEV` | `arm-none-eabi-` | `riscv-none-elf-` |
| upstream `examples/rust/hello`, `make context` | fails in 3 s: `error: "--target" takes a target architecture as an argument.` | fails in 9 s: building `core` stops at `panic_immediate_abort is now a real panic strategy!` |
| `cargo build --debug` (what `nuttx_add_rust` runs for a debug build) | `error: unexpected argument '--debug' found` | same |

So upstream's Rust support at our pins builds neither board with our pinned
nightly: armv7a has no triple, and the release path's
`panic_immediate_abort` feature was retired by the nightly. The arm
`ARCHCPUFLAGS` match the four places we restate them (the make road's
literals, the board's `cargo_config` rustflags and `CFLAGS`, and
`cmake/toolchain/armv7a-nuttx-eabihf.cmake`), plus `-mlittle-endian`; the
committed arm export snapshot's `scripts/Make.defs` carries the same line.
The rv-virt flags match the board's `-march=rv32imac -mabi=ilp32`.

### Other projects (how they integrate with NuttX)

- **micro-ROS** (`micro-ROS/nuttx_apps`, branch `foxy`, commit
  1f4c6f0b1c5c26268d7a7b01760124a96d75a432), verified: `uros/Kconfig`
  declares `UROS_MAX_NODES` ... `UROS_MAX_HISTORY` as the sizing surface;
  `uros/Makefile:41-54` renders `rmw_config.meta` from those `CONFIG_`
  values with `$(TOPDIR)/.config` as a prerequisite; `:33-39` renders a CMake
  toolchain file from `$(CROSSDEV)` and `$(ARCHCPUFLAGS)`; `:59-70` runs
  `colcon build` with both; `:75` makes `context:` depend on them. It does not
  include `Application.mk`; its `context:` is its own rule.
- **PX4** (our pin of `third-party/px4/PX4-Autopilot`,
  d6f12ad1c4f70ad3230afd7d86e971421e02fef4), verified: PX4's CMake owns
  `.config`. `platforms/nuttx/NuttX/CMakeLists.txt:54-67` concatenates the
  board defconfig into `.config` and runs `olddefconfig`, `:71-84` runs
  NuttX's `context`, and `:137-155` writes its own builtin registry
  (`px4.bdat`) before building `libapps.a`. Its own board Kconfig is
  evaluated with kconfiglib (`cmake/kconfig.cmake:4-14`) and `genconfig`
  writes `px4_boardconfig.h` (`:68`).
- **Spresense** (`sonydevworld/spresense`, `master` at
  7fd61b2c03f06a4ff0302b84c755e58c338788b2, not a pin), verified:
  `sdk/tools/config.py` collects defconfig fragments (`:82-93`), applies
  several in order (`:256`) and then runs `olddefconfig` (`:322`).
- **openvela**: a `build.sh` over defconfigs, with make and CMake. Reported,
  not verified.

The common shape: the project owns `.config` (or a fragment merged into it),
derives everything from it, and runs inside NuttX's own `context` step. None
of them restates CPU flags by hand: micro-ROS reads `ARCHCPUFLAGS`.

### Corrections to this issue's own text

- Section 1 cited an `osource`-glob Kconfig template in `integrations/nuttx/`
  as how the user's app is discovered (corrected, and the template deleted,
  in phase-550 W12). Nothing installed that file: `stage-external-apps.sh`
  writes its own `external/Kconfig` (`:89-103`) because, its comment says,
  the bundled `kconfig-conf` lacks `osource`. Only
  `external-Make.defs.in` is installed (`:76`).
- The make road needs the kernel's own tree for more than headers. Measured:
  with only `NUTTX_INCLUDE_DIR` set, as `NROS_CARGO_BUILD` sets it, the
  archive's build stops in `zpico-sys`'s build script: `[port.nuttx]
  include_paths {nuttx_include}: nuttx-kernel: not provisioned anywhere this
  build looks` (env `$NUTTX_DIR` unset, checkout
  `third-party/nuttx/nuttx` not initialised). So a make build in a user's
  own NuttX tree either fails or compiles zenoh-pico against the nano-ros
  checkout's kernel headers, not `$(TOPDIR)`'s. With `NUTTX_DIR` exported to
  the scratch kernel the armv7a make road builds:
  `make -j` 69 s from a configured tree, of which cargo
  (`-Zbuild-std=std,panic_abort`, `std` compiled) is 39.8 s; a 2.3 MB
  `libnros_c.a`; a 4.5 MB `nuttx` ELF with no `nros_*` text symbol, because
  no app references the archive.
  On rv-virt, with `NUTTX_DIR` exported too, it stops in 19 s in the same
  build script: the zenoh-pico C sources are compiled by the HOST `cc`
  (`cc: error: unrecognized argument in option '-mabi=ilp32'`), because
  `NROS_CARGO_BUILD` sets `CC_`/`CXX_`/`AR_` for the armv7a triples only.
  That is issue 2426 item 2, measured.

## Using NuttX's own infrastructure (exploration, 2026-11-11)

What it would take for nano-ros to build NuttX images with NuttX's own build
machinery and retire what we invented beside it. This section explores; it
decides nothing. The decision shape is recorded in
[RFC-0126](../design/0126-nuttx-builds-on-nuttx-infrastructure.md) and the
candidate work in [phase-550](../roadmap/phase-550-nuttx-native-build-infrastructure.md);
the questions are numbered on from Q8 at the end.

### What we invented, and NuttX's own counterpart

| ours | where | NuttX's counterpart | status of the counterpart at our pins |
| --- | --- | --- | --- |
| armv7a triple synthesis, and the empty-triple fallback that calls upstream's macro | `integrations/nuttx/Makefile:201-206`, `:274-276` | `RUST_TARGET_TRIPLE` / `nuttx_rust_target_triple` | no armv7a branch, measured; the fallback cannot build (issue 2426 item 4) |
| `NROS_NUTTX_TRIPLE := armv7a-nuttx-eabihf` | `integrations/nuttx/Make.defs:65` | the same macro | as above |
| CPU and C flags as literals | `Makefile:229-231`, `:242-248`; board `cargo_config` rustflags, `CFLAGS_*`, `NUTTX_*_CFLAGS`; `cmake/toolchain/armv7a-nuttx-eabihf.cmake` | `$(ARCHCPUFLAGS)` / `$(CROSSDEV)` from `Make.defs`, or the export's `scripts/Make.defs` | arm value equals ours plus `-mlittle-endian`; rv-virt equal; measured |
| our own cargo command (`NROS_CARGO_BUILD`) | `Makefile:241-263` | `RUST_CARGO_BUILD` / `nuttx_add_rust` | broken at our pins with our nightly on both boards, measured; fixed on upstream `master` except armv7a |
| parse-time image crate generation, guarded for `olddefconfig`'s parse | `Makefile:105-131`, issue 0551 | `context::` (a recipe, so it runs once per build, after configure) | available |
| a second nightly pin | `Makefile:144` | none (upstream reads the ambient toolchain) | -- |
| a CMake shell that reads an undeclared `CONFIG_NROS_RMW` and is never staged | `integrations/nuttx/CMakeLists.txt` | `external/CMakeLists.txt` + `nuttx_add_application` / `nuttx_add_extra_library` | available; the staged tree breaks a CMake configure today (2426 item 7) |
| an unused `external/Kconfig` template | deleted in phase-550 W12 | `mkkconfig.sh` | our staging writes its own |
| configure by `cp defconfig .config` + `olddefconfig` ("replicates what tools/configure.sh does") | `scripts/nuttx/build-nuttx.sh:161-163`, `:292` | `tools/configure.sh <config-dir>`, with `#include` fragments through `process_config.sh` | available; needs `kconfig-tweak` (build-nuttx.sh's stated reason) |
| the app runs by overriding the `nsh_main` symbol | `packages/boards/nros-board-nuttx-qemu/src/entry.rs:37`, `CONFIG_INIT_ENTRYPOINT="nsh_main"` in the board defconfig | `CONFIG_INIT_ENTRYPOINT="<prog>_main"` | available |
| the image link restated as 47 `link-arg`s across the two boards | board `cargo_config` (`-Tdramboot.ld`, the kernel lib group, `-lgcc`) | NuttX's own final link (`LD: nuttx`) | available on the apps road; the leaf road links the export itself |

### E1. One triple table, upstream's

Upstream's table is the natural single owner, but it lacks armv7a in both
flows, on our pin and on `master`, and the make and CMake flows spell the arch
differently (`armv7a`, `armv7-a`). Three shapes:

- contribute the branch upstream (both files), and carry it until then as a
  patch on a nuttx-apps fork (today `third-party/nuttx/nuttx-apps` is an
  untouched upstream pin, so this changes its shape to "fork with a patch
  branch", CLAUDE.md's second legitimate shape);
- keep our own fallback, in one place, with a check that it agrees with
  upstream's table for every triple upstream knows;
- read the board's `[build] target` instead and never ask NuttX.

The last is what the leaf road does today, and it is the opposite of this
exploration's direction: the board would keep stating a fact the kernel
configuration already implies.

### E2. CPU flags from `ARCHCPUFLAGS`

Measured: NuttX's own value equals our restated literals on both boards
(arm adds `-mlittle-endian`). micro-ROS takes them from `$(ARCHCPUFLAGS)` and
`$(CROSSDEV)` in its `context` step. For us the make road has them at parse
time (the app `Makefile` includes `$(APPDIR)/Make.defs`, which includes the
kernel's), and the leaf road has them in the export's `scripts/Make.defs`.
The open part is who turns them into cargo's `CFLAGS_<triple>`,
`CC_<triple>` and link args: the make recipe itself, or `nros configure`
reading the export (which would make layer 0 read a host build's product;
RFC-0120 D1's layer rule has to say whether that is allowed).

### E3. `context::` and `register::`

Our shell already runs cargo from `Application.mk`'s `context::`. What it
adds beside it: the parse-time `$(shell cmake -P ...)` that generates the
image crate (and the issue 0551 guard it needs), and the `PREFIX` object dir.
Moving the image crate into the `context::` recipe removes the parse-time
guard. The shell has no `PROGNAME`, so it registers nothing; the user's app
registers itself through its own `PROGNAME`, which is NuttX's mechanism
already. Nothing to retire there; the open part is whether the runtime archive
stays a separate library app (micro-ROS's shape) or is built inside the
user's app's own `context::`.

### E4. `nros configure` in `context::`

This is option B1 above, unchanged by the study, with two additions the
measurements force: the cargo command must carry `NUTTX_DIR=$(TOPDIR)`
(otherwise `zpico-sys` resolves the nano-ros checkout's kernel or fails), and
the per-target `CC_` / `AR_` names come from `$(CROSSDEV)`, not from an
armv7a list (otherwise rv-virt compiles zenoh-pico with the host `cc`).

### E5. The CMake flow

To make NuttX's CMake build reach us: the staging writes
`external/CMakeLists.txt` (one `add_subdirectory` per staged app), and the
shell's `CMakeLists.txt` becomes an app that builds the archive with
`nuttx_add_rust`-style custom commands and hands it to
`nuttx_add_extra_library`, reading only the Kconfig symbols that exist
(`CONFIG_NROS_RMW_<X>`, not `CONFIG_NROS_RMW`). Not measured: whether our pinned
kernel's CMake build boots `qemu-armv7a` with our defconfig at all. The
alternative is to delete the CMake shell and the book's one sentence until a
lane builds it.

### E6. The board's build-std and target spec on both roads

The board's `cargo_config` is the one statement of `build-std`,
`build-std-features` and the target spec; the make road would take it
through the image's generated cargo config (E4), so the
`-Zbuild-std=std,panic_abort` literal goes. The rv-virt JSON target spec:

- Measured (`rustc +nightly-2026-04-11 -Z unstable-options --print
  target-spec-json`): the built-in `riscv32imac-unknown-nuttx-elf` has no
  `panic-strategy` (unwind) and no `eh-frame-header` (default true). The
  board's JSON is the built-in spec plus exactly those two keys, field for
  field. Both deltas are still needed for the leaf road at the pinned nightly.
- The built-in `armv7a-nuttx-eabihf` already says `"panic-strategy":
  "abort"`, and the arm leaf road uses the built-in triple.
- On the apps road the archive is a `staticlib` that NuttX's own link
  consumes, so rustc never passes `--eh-frame-hdr` there, and
  `panic = "abort"` can come from the profile (the upstream example does
  this). Reasoned from the build shape, not measured.
- Upstream nuttx `master` already ships JSON target specs in `tools/` for
  x86 and aarch64, so an upstream home for a riscv spec has precedent.

### E7. One road or two

Today a NuttX image can be built three ways: the CMake leaf road
(`examples/qemu-armv7a-nuttx`, `examples/rv-virt-nuttx`), the workspace road
(`nros build` of a workspace image on a NuttX board, cargo for Rust and
CMake for C/C++), and the make road. The first two link the ELF themselves
from the `make export` snapshot; only the make road lets NuttX link.

| capability | leaf and workspace roads today | apps road (make or CMake) |
| --- | --- | --- |
| sizing by census and configure | yes, hooked; phase-530 W5 measured one configure, census-tight knobs | none today; with a project (A2+B1) configure can run in `context::`, but the census builds a host `CMakeLists.txt`, so a make app must declare its entities (Q6) |
| fixtures and tests | 35 NuttX fixture rows in `examples/fixtures.toml` (27 workspace, 7 leaf and test bins, 1 compile check); 24 test files under `packages/testing/nros-tests/tests` name NuttX | none; a lane would have to build a kernel tree per configuration |
| kernel builds | one per arch, exported once and linked by every image (`build-nuttx.sh` short-circuit) | one configured tree per image configuration, because the tree holds one `.config` (issue 0525); measured 69 s for one armv7a make road build from a configured tree on 4 jobs, 39.8 s of it cargo |
| QEMU runs | the ELF starts the image through the `nsh_main` override | the ELF boots NSH; the harness types the program name, or the defconfig names `<prog>_main` in `CONFIG_INIT_ENTRYPOINT` |
| the final link | restated by the board (47 `link-arg`s, `dramboot.ld`, vectortab, the lib group) and re-checked per arch | NuttX's |
| a user's own NuttX tree and app | not the user's tree: the leaf road builds against the nano-ros checkout's kernel export | the user's tree (`$(TOPDIR)`), which is what a NuttX user expects |

Not measured: a leaf-road build time to compare with the 69 s above (the leaf
road needs the checkout's shared kernel tree, issue 0525), and any apps-road
build of a real app (no app references the archive yet, so the measured ELF
has no `nros_*` symbol).

So the leaf and workspace roads carry everything the test system rests on,
and the apps road is the one a NuttX user would recognise. Collapsing to one
road either moves 35 fixture rows and the census onto a road that has neither
today, or drops the road NuttX users know. Keeping both but building each
through NuttX's own pieces (E1, E2, E6) removes the restated facts without
choosing.

### Questions for the maintainer (continued)

9. Direction: should NuttX builds move onto NuttX's own infrastructure
   (upstream's triple table, `ARCHCPUFLAGS` / `CROSSDEV`, `context::`,
   `nuttx_add_application`, `tools/configure.sh`), retiring the inventions
   in the table above as the phase lists? yes / no
10. Roads: (a) keep the leaf and workspace roads and the apps road, each built
    from NuttX's pieces; (b) make the apps road the only NuttX road, moving
    examples, fixtures and tests onto it and deleting the leaf road; or
    (c) keep only the leaf and workspace roads and drop `apps/external` as
    unsupported. Choose one.
11. The armv7a triple: (a) contribute the branch upstream and carry it on a
    nuttx-apps fork patch branch until it lands; (b) keep our own fallback in
    one place, checked against upstream's table; or (c) take the triple from
    the board's `[build] target` and never ask NuttX. Choose one.
12. Upstream's cargo invocation (`RUST_CARGO_BUILD`, `nuttx_add_rust`) does
    not build at our pins with our nightly (measured). (a) move the
    nuttx-apps pin to an upstream commit that fixes it and use upstream's
    invocation; (b) keep our own cargo command and take only the triple table
    from upstream; or (c) fix it upstream first, then (a). Choose one.
13. CPU and C flags come from NuttX (`ARCHCPUFLAGS`, `CROSSDEV`: the
    configured tree on the apps road, the export's `scripts/Make.defs` on the
    leaf road), and the restated copies are deleted (board `cargo_config`
    rustflags and `CFLAGS`, the riscv `NUTTX_*_CFLAGS` env rows, the CMake
    toolchain file's flags, the make-road literals). yes / no
14. If 13 is yes: may `nros configure` (layer 0) read the kernel export's
    `scripts/Make.defs` to render them, or must the host build hand them over
    (RFC-0120 D1's layer rule)? read / hand over
15. The rv-virt target spec: (a) keep the board's JSON for every road;
    (b) keep it for the leaf road only and use the built-in triple plus a
    `panic = "abort"` profile on the apps road; (c) get the two deltas into
    rustc's built-in target; or (d) propose the spec to upstream NuttX's
    `tools/`. Choose one or an order.
16. NuttX's CMake flow: (a) support it now (stage `external/CMakeLists.txt`,
    rewrite the shell over `nuttx_add_application` /
    `nuttx_add_extra_library`, add a lane); or (b) delete the CMake shell and
    the book's sentence until there is a lane. Choose one.
17. The board's kernel configuration becomes a defconfig fragment over
    upstream's `qemu-armv7a:nsh` / `rv-virt:netnsh` (`#include`, read by
    `process_config.sh`), configured with `tools/configure.sh` instead of
    `build-nuttx.sh`'s copy and `olddefconfig`. yes / no
18. Boot entry: name the app in `CONFIG_INIT_ENTRYPOINT` (NuttX's mechanism)
    instead of overriding the `nsh_main` symbol, on every road. yes / no
19. The make-road lane (both armv7a and rv-virt, each in its own scratch
    kernel tree, never the checkout's shared tree): which cadence: pull
    request, merge queue, or daily?
20. Is retiring the vendored `libc` fork (`third-party/nuttx/libc`) in this
    phase's scope once no road builds `std` (2236 Q5 and 2426 item 1), or a
    separate phase? this phase / separate

## Decision brief (2026-11-12)

This section puts Q1 to Q20 in a form that can be ruled in one pass. It adds
no new measurement. Each fact cited here is measured in a section above, and
the section is named. The questions above stay as they were written.

**Direction (ruled 2026-11-12, relayed through the manager session).** The
direction is RFC-0126:

- NuttX owns every NuttX fact, and nano-ros reads it.
- One cargo carrier serves every road.
- No road is offered without a lane that builds it.

The direction settles some questions by itself. Section A lists them, so the
ruling only needs to confirm them. Section B lists the questions the direction
leaves open, each with a recommendation that fits the direction.

### A. Settled by the direction (confirm as a group)

| Q | Question | Answer the direction gives | Why it follows | Measured fact |
| --- | --- | --- | --- | --- |
| 9 | Move NuttX builds onto NuttX's own infrastructure? | **yes** | This is the direction itself. | "Using NuttX's own infrastructure" table: each invention beside its NuttX counterpart. |
| 13 | CPU and C flags from `ARCHCPUFLAGS` / `CROSSDEV`, with the restated copies deleted? | **yes** | NuttX owns the flags. | "Measured at the pins": the arm `ARCHCPUFLAGS` equal our four copies plus `-mlittle-endian`, and rv-virt's are equal. |
| 3 | Knob channel: B1, B2 or B3? | **B1** (cargo `--config` file, with configure run in `context::`) | B1 is the one cargo carrier the cargo and cmake roads already use. B2 adds a make-side carrier, and its `export`s land on the ladder's top rung. B3 adds a Kconfig dialect and cannot run before `olddefconfig`. | Section 2(b). Issue 1712 measured the `[env]` fingerprinting B1 relies on. |
| 5 | The make road drops its inline `--config` literals and `-Zbuild-std=std,panic_abort` for the board's `cargo_config`, on the guest road too? | **yes** | One carrier on every road. A guest that keeps the literals keeps a second statement of the board. | Section 1 and the study: the make road's `std` build is 39.8 s of a 69 s armv7a build. |
| 7 | Does the make road count as HOOKED (RFC-0120 D5), with D5's NuttX row and R36 amended? | **yes**, given Q1 yes and Q3 B1 | The host configure becomes the shell's `context::` step, which calls configure before cargo reads anything. | Section 3. |
| 17 | Configure the kernel as a defconfig fragment over upstream's `qemu-armv7a:nsh` / `rv-virt:netnsh`, with `tools/configure.sh`? | **yes** | NuttX owns its configure. `build-nuttx.sh` copies a defconfig and runs `olddefconfig`, which restates `configure.sh`. | Exploration table, "configure by `cp defconfig .config`" row. |
| 18 | Name the app in `CONFIG_INIT_ENTRYPOINT` instead of overriding `nsh_main`? | **yes** | NuttX owns the boot entry. | Exploration table, "the app runs by overriding the `nsh_main` symbol" row. |
| 16 | NuttX's CMake flow: support it now, or delete the shell? (ruled in the second step, see "Ruling" below) | **(b) delete** the CMake shell and the book's sentence | No road is offered without a lane, and nothing builds this flow. It can come back as a new road with its own lane. | Issue 2426 item 7: the staged tree breaks a CMake configure today. E5: whether the pinned kernel's CMake build boots `qemu-armv7a` was not measured. |
| 4 | When a project is named, refuse a Kconfig RMW, edition or API flavor that differs from the image's? | **yes** | This follows from rulings already in force, not from RFC-0126: RFC-0098 D10 (one source of image configuration) and Ruling R34 (the cmake and Zephyr roads already refuse an edition disagreement). | Section 2, "Where Kconfig and configure could disagree". |

### B. Open: the direction does not decide these

| Q | Question | Options | Measured facts | Recommendation |
| --- | --- | --- | --- | --- |
| 1 | Can an `apps/external` make app be a nros project? | yes, or no (the leaf road stays the only project road) | Section 1: the make road has no project, no image and no knob channel today. E7: it is the only road that builds in the user's own NuttX tree (`$(TOPDIR)`). | **yes.** Under Q5's one carrier the make road needs configure's cargo config anyway, and "no" leaves a road with no configure in a phase whose point is one carrier. |
| 2 | Project identity | A1: Kconfig strings `CONFIG_NROS_PROJECT` + `CONFIG_NROS_IMAGE`. A2: the app's `Make.defs` sets `NROS_PROJECTS`. A3: discovery. | Section 2(a). micro-ROS's NuttX app uses Kconfig as its surface (`uros/Kconfig`, `uros/Makefile:41-54`). PX4 and Spresense also keep the project in `.config` or a fragment merged into it ("Other projects"). | **A1** (the manager's lean), plus a refusal when the named directory holds no `system.toml`. Section 3 recommended A2, before the direction was ruled. The direction puts NuttX-side settings in NuttX's own configuration surface, so A1 now fits better: the identity sits in the defconfig fragment Q17 creates, and menuconfig shows it. A2's case was that it keeps paths out of a defconfig. A1 meets that with a path relative to `$(APPDIR)`. A3 is not recommended. |
| 6 | Must a make-app project declare its entities (no host census for make apps)? | yes, or no (teach the census to build `CSRCS` for the host) | Section 2, "Sizing a make app": the census builds a host `CMakeLists.txt`, and a make app has none. | **yes** for this phase. A make census can be a separate issue. |
| 8 | File the guest road's knob prompts (B3's guest half) as a separate issue now? | yes / no | Section 2(b), B3: the NuttX Kconfig has no knob symbols today. | **no.** Kconfig knob symbols read through `DOTCONFIG` would be a second carrier beside Q3's `--config` file. With Q1 yes, a user who wants sized knobs names a project. Revisit only if a guest-only user asks. |
| 10 | Which roads exist? | (a) leaf + workspace + apps, each from NuttX's pieces. (b) apps road only. (c) leaf + workspace only. | E7: 35 NuttX fixture rows and 24 test files rest on the leaf and workspace roads. The apps road has no fixtures and no census, and only it builds in the user's own tree. | **(a)**, with the apps road gaining W1's lane (the direction's lane rule). (b) moves 35 rows onto a road that has no fixtures today. (c) drops the road a NuttX user recognises. |
| 11 | Where the armv7a triple comes from | (a) upstream PR, carried on a nuttx-apps fork patch branch. (b) our own fallback, in one place, checked against upstream's table. (c) the board's `[build] target`. | E1: upstream lacks an armv7a branch in both flows, on our pin and on `master`, and the two flows spell the arch differently. | **(a).** It keeps NuttX as the owner. The cost is that `nuttx-apps` changes shape from an untouched upstream pin to a fork with a patch branch. (c) is ruled out by the direction, because the board would restate a kernel fact. |
| 12 | Whose cargo invocation | (a) move the nuttx-apps pin to a fixed upstream commit and use `RUST_CARGO_BUILD`. (b) keep our cargo command and take only the facts (triple, flags) from NuttX. (c) fix it upstream, then (a). | "Measured at the pins": upstream's invocation fails on both boards with our nightly. Its environment carries no `NUTTX_DIR`, no knobs and no board build-std (RFC-0126 Alternatives). | **(b).** The invocation is the carrier, and the carrier is ours (Q3, Q5). The facts it consumes are NuttX's. |
| 14 | On the leaf road, may `nros configure` read the export's `scripts/Make.defs`, or must the host build hand the flags over? | read / hand over | E2: the make road has the flags at parse time. The leaf road has them only in the export's `scripts/Make.defs`. | **read**, through `nros locate`, with one sentence added to RFC-0120 D1: the export is a provisioned input, built once per arch and shared by every image (`build-nuttx.sh` short-circuit), not a host-build product of the image being configured. On the apps road the recipe passes them, so nothing is read there. |
| 15 | Where the rv-virt target spec lives | (a) board JSON for every road. (b) board JSON on the leaf road only, built-in triple plus `panic = "abort"` on the apps road. (c) the two deltas into rustc's built-in target. (d) the spec into upstream NuttX's `tools/`. | E6: the board's JSON is the built-in spec plus `eh-frame-header: false` and `panic-strategy: abort`. Both are still needed where rustc links the image. The apps-road case is reasoned, not measured. | **(b) now, then (d).** (b) follows the direction where it can be checked. (d) moves the spec to NuttX's own tree; upstream already ships JSON specs for x86 and aarch64. |
| 19 | Cadence of the make-road lane (armv7a and rv-virt, each in its own scratch kernel tree) | pull request, merge queue, or daily | E7: one armv7a make build from a configured tree takes 69 s on 4 jobs. The study measured no kernel configure time. | **daily** to start. A per-PR or merge-queue lane would configure two kernels per entry. Move it up once W1 has measured the whole lane. |
| 20 | Retire the vendored `libc` fork in this phase, or in a separate one? | this phase / separate | 2426 item 1: the fork exists only because a road builds `std`. With Q5 yes, no road does. | **separate**, opened once W5's `std` removal has landed and been measured. The retirement is a submodule removal with its own acceptance, and it should not hold up W0. |

### What the ruling then unblocks

- With A confirmed and B answered as recommended, every "Gate:" line in
  [phase-550](../roadmap/phase-550-nuttx-native-build-infrastructure.md) clears,
  except W13's road deletion. Q10 (a) deletes no road.
- RFC-0126 is then rewritten from a decision shape into decisions.
- RFC-0120 D5/D8 and phase-530 R36 are amended for Q1 and Q7.

### Ruling (2026-11-12)

The user ruled in two steps, both relayed through the manager session. All
twenty questions are now ruled.

**Step 1**, on section A, verbatim:

> Accept the settled group. Hold Q10 and Q16 until 241's unified-build design

**Step 2**, on the rest. The user asked for a review of the open questions
that preferred "better UX and maintainability, and design cleanliness", then
answered "Accept" to the list below. The list includes Q10 and Q16, because
the unified-build design (RFC-0130, one ROS-CMake build, a draft from the
241 session) now places NuttX.

| Q | Ruled | Against the brief |
| --- | --- | --- |
| 3, 4, 5, 7, 9, 13, 17, 18 | as section A states them (step 1) | same |
| 1 | yes: a make app may NAME a project, for its configuration and sizing | same |
| 2 | **A2**: the app declares itself in its own `Make.defs` (`NROS_PROJECTS += ...`, optional `NROS_IMAGE`). No path in a defconfig. A guest is unchanged by construction. More than one project is refused. | changed from A1 |
| 6 | yes: a make app declares its entities. A make census is a separate issue. | same |
| 8 | no | same |
| 10 | ONE road for nano-ros packages: the unified ROS-CMake road (RFC-0130 D7), building against NuttX's exported make tree. The leaf and workspace roads become that road. The `apps/` make app is a GUEST road (nano-ros entering a NuttX user's own tree), not a second way to build a nano-ros package. | new: neither (a), (b) nor (c) |
| 11 | **(b)**: one armv7a fallback in ONE place, checked against upstream's table, PLUS an upstream contribution. No nuttx-apps fork. The fallback is deleted when the pin gains the branch. | changed from (a) |
| 12 | (b): our cargo command, with the facts from NuttX | same |
| 14 | read the export's `scripts/Make.defs` (a provisioned input), and add the RFC-0120 D1 sentence | same |
| 15 | **(a)**: the board JSON on EVERY road, one statement. Upstreaming to rustc or NuttX stays possible later. | changed from (b) then (d) |
| 16 | (b): delete the NuttX CMake shell and the book's sentence. RFC-0130 Q-N1: the exported make tree now, NuttX's upstream CMake flow only after it is measured booting both boards and a lane builds it. | same |
| 19 | daily | same |
| 20 | a separate phase | same |

Where it is recorded:

- [RFC-0126](../design/0126-nuttx-builds-on-nuttx-infrastructure.md)'s
  "Decided" section, and its D1 to D8.
- [RFC-0120](../design/0120-nros-configure-one-resolver.md) D1 gains the
  provisioned-input sentence (Q14).
- [phase-550](../roadmap/phase-550-nuttx-native-build-infrastructure.md)'s
  items and gates.

This issue closes when phase-550 W6 lands: a make app as a project, which is
the road it was filed about.
