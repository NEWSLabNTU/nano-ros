---
id: 2594
title: "A generated Zephyr west app cannot get the workspace's own Kconfig symbols"
status: resolved
type: design-question
area: [cli, zephyr]
severity: medium
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "phase-478 I10 and I11"
related: [phase-478]
---

GitHub: NEWSLabNTU/nano-ros#3906

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md).**
Seen on main 37232ecff1 with the Autoware Safety Island port.

## What happens

A workspace whose Zephyr image needs its own Kconfig symbols cannot get them
into the west application `nros build <image>` generates. The only route is
`nros materialize`, which makes one hand-owned app per image.

The generated app (`build/zephyr-<rmw>/<image>_entry/`, `builder/west_app.rs`)
is missing three things:
- It has no `Kconfig` beside its `CMakeLists.txt`. It holds only a generated
  `prj.conf` (issue 2177).
- It `add_subdirectory`s only the node packages the launch file names.
- It gets `-DZEPHYR_EXTRA_MODULES=<nano-ros>` and nothing else
  (`builder/zephyr.rs`, `module_arg`).

`[image.<id>] os_config` takes `.conf` / `.overlay` fragments, which can assign
symbols but cannot define them, so `CONFIG_CONTROL_CMD_OUTPUT_DDS_ONLY=y` is
refused by Kconfig as undefined.

The island's hand-written app defines 37 application symbols (control-output
mode, SNTP, log level, a thread-stats reporter, a tracing backend, and more),
read by its support package `asi_common` on Zephyr. Its three Zephyr images
(fvp, fvp-tap, s32z) are the last of its images that `nros build` does not
build.

## The question

Where does an image's Kconfig DEFINITION live, and how does the generated app
include it? Assigning a value is already answered: `os_config`.

## Options

**A. A workspace package that is a Zephyr module joins `ZEPHYR_EXTRA_MODULES`.**
A package carrying `zephyr/module.yml` with a `kconfig:` (and optionally a
`cmake:`) entry is passed as an extra module whenever an image's package
closure includes it.
- For: Zephyr's own mechanism for "code that brings Kconfig symbols". It
  needs no nano-ros grammar. The support package that READS the symbols owns
  their definitions, and the same package works in a plain west app.
- Against: an image's closure must be computed to decide inclusion (today
  only launch-named packages are known). A module's `cmake:` hook also runs,
  which changes what a support package builds.

**B. A `Kconfig` in the bringup (or one named by the image row) is placed
beside the generated `CMakeLists.txt`.** It is the app-level `Kconfig`,
reached the way `prj.conf` reaches it today through `APPLICATION_CONFIG_DIR`,
for example `[image.<id>] kconfig = "Kconfig.island"`, or `<bringup>/Kconfig`
by convention.
- For: the smallest change, and the closest to the hand-written app the
  island has now. One file per image.
- Against: the definitions live in the bringup, away from the package that
  reads them. Two bringups sharing a support package each restate them. An
  app-level `Kconfig` must also `source "Kconfig.zephyr"`, which the
  generator would have to arrange.

**C. The generated app adds every package in the image's dependency closure,
not only the launch file's node packages.** A support package's own
`zephyr_*` calls then run.
- For: fixes the sibling gap, where a support library's CMake never runs in a
  generated app.
- Against: on its own it defines no Kconfig symbol, because Kconfig is read
  before any `add_subdirectory`. So it must pair with A or B. It also widens
  what every generated Zephyr app builds.

**Recommendation: A, with C's closure computation as its prerequisite.** The
symbols belong to the package that reads them. Zephyr modules are the one
mechanism that carries Kconfig and CMake together, so a support package is
then the same thing in a generated app and a hand-written one. B is the
fallback if a ruling prefers a per-image file over making packages Zephyr
modules.

## Workaround

`nros materialize controller_bringup:<image>` per image, then add the Kconfig
and the support package by hand. Or keep the hand-written app.

## Ruling (2026-11-15)

User ruling, relayed by the phase-478 manager: **option A**. No B.

- A workspace package that reads the symbols ships `zephyr/module.yml` with a
  `kconfig:` entry (and `cmake:` if it builds Zephyr code). It joins
  `ZEPHYR_EXTRA_MODULES` whenever the image's package closure includes it.
- C's closure computation is the prerequisite. The generated app adds the
  image's whole package closure, not only the node packages the launch file
  names.
- Built after issue 2593 (the CMake road's image `profile`).

Work items: phase-478 I10 (closure) and I11 (modules). The book pages that
change are listed under I11.

## Resolution (2026-11-15)

Option A, built as ruled.

- **I10, the closure.** `launch_closure` in `cmd/build.rs` now returns the
  launch-named packages plus every workspace package they `<depend>` on,
  transitively, over the package graph `builder::discover` finds. A
  dependency joins when it is a CMake package (`routing::cmake_node`, the
  native cmake root's predicate, so interface packages stay out) or a Zephyr
  module. The same closure feeds the registered population and the
  message-bound seed, so the three readers still agree.
- **I11, the modules.** `builder::west_app::resolve_cmake` reads each closure
  package's `zephyr/module.yml`. A package that has one is appended to
  `ZEPHYR_EXTRA_MODULES` in the generated `CMakeLists.txt`, before
  `find_package(Zephyr)`, beside the nano-ros module the `west build` line
  already passes. A module whose `build.cmake` is the package's own directory
  is built by Zephyr and is not `add_subdirectory`'d again. A package with a
  module and no `CMakeLists.txt` is a module only. A `module.yml` that does not
  parse is refused, naming the file.
- Tests: `package_closure_tests` in `cmd/build.rs` and four `west_app` tests
  (module before `find_package`, no double add, Kconfig-only module and a bad
  `module.yml`, no modules means no lines).
- Book: `guide/platforms/zephyr.md` gains "A workspace package that is a
  Zephyr module"; `configure-an-image.md` and
  `organize-a-larger-workspace.md` point at it. `zephyr-west-app.md` is
  unchanged: it covers the hand-written road, where a module already works
  the Zephyr way, and the page is at its how-to word budget.
