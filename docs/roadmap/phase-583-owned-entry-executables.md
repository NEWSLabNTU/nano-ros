# Phase 583 — An owned entry with several executables

**Status (2026-11-21). OPEN: design accepted (user ruling 2026-11-20); W0
done (`a49a3dcf82`); W1, W2 and W3 done. Owner: nros 243.** Implements [RFC-0071 D20](../design/0071-rmw-backend-descriptor.md#amendment-2026-11-20-an-owned-entry-with-several-executables-d20):
- an owned entry may build several executables, chosen at run time as in
  ROS (`nros run <image> --exec <name>`, or the bringup's launch `exec`);
- on RTOS, each executable is its own firmware, one carrier per
  `EXECUTABLE`;
- per-program facts are keyed per executable.

Each item is one PR, claimed with `just claim phase-583-wN` before it starts,
in the order below.

## Why

- `nros run` resolves a named owned entry's one executable from CMake
  (#4391), and refuses several.
- On the RTOS carrier roads, a second registered component used to be
  dropped silently, shipping a firmware with the first component only. It
  is a configure error since issue 2744, which stops the wrong image and
  supports nothing new.
- NXP's ASI work builds several test programs per package, and today each
  needs its own package and bringup.

## Work items

### W0 — A build root adds only its bringup's closure (issue 2754)

- **Status:** DONE (2026-11-21, `a49a3dcf82`)
- **Progress:** landed as `a49a3dcf82` (a generated CMake root adds only its
  bringup's closure). The fixture-row acceptance ran on that change's head: every
  scope built; the cmake-driver rows passed; the two linux reds were both
  pre-existing on main: an undersized parameter store in the
  `workspace-probe-seeds` row (exit 250, reproduced on main) and a
  custom-transport row that predated a zenoh-pico fix. `tests/bringup_scoped_root.rs` guards that
  a real `nros build` never takes the unscoped fallback. It was a bug, so it
  came before W1 (user ruling 2026-11-20, option A of the design note).
- **Contract.** The generated CMake root for one bringup's images adds:
  - the bringup package;
  - the packages each image on that configure reaches: its launch plan's
    nodes, and its owned entry (`[image.<id>] entry`);
  - the bringup's `[[component]]` rows;
  - every workspace package those `<depend>` on, transitively.

  Nothing else. This is colcon's `--packages-up-to`, applied by default, and
  the closure the west road already built (`package_closure`, issue 2594).
  Another bringup's node packages and owned entries are never configured, so
  they cannot fail this build or add executables to it. A root for a
  workspace with no bringup still adds every package. So does a plan made
  before any sync, when an image's launch has no model yet. It is wider,
  never a wrong image, and it says so. `nros build` resolves the models in
  its implicit configure first, so a real build is scoped.
- **Every road.** The cmake root was the only road that added the whole
  workspace:
  - a west application already adds its launch closure;
  - a cargo entry reaches `src/*` only through its path dependencies.

  Both are unchanged.
- **A missing `<depend>` fails by name.** A kept package whose
  `CMakeLists.txt` uses a left-out workspace package (`find_package(<pkg>` or
  a `<pkg>::` target) is refused before the configure. The error names the
  package, the file, and the `<depend>` line to add to its `package.xml`.
  Interface packages are never subdirectories, so leaving one out changes
  nothing.
- **Where:** `cmd::build::bringup_closure_dirs`, read by
  `cmake_root_excluded` (the one exclusion the root's SUBDIRS and the
  stage-3.5 seed both use); `builder::cmake_root::refuse_undeclared_workspace_uses`.
- **Not changed:** the C/C++ metadata probe stays one host project per
  workspace. Whether it trips on another bringup's package was not measured
  here.
- **Acceptance:**
  - unit tests: the issue's shape (two bringups with owned entries, a shared
    support package, a support package only the other entry uses), with a
    mutation check; the refusal and its non-matches;
  - `examples/workspaces/cpp`: `pubsub_bringup`'s root adds `listener_pkg`
    and `talker_pkg`, where it used to add all nine node packages;
  - the fixture rows of the eight multi-bringup workspaces, split by the
    driver each image's plan takes (`nros image-facts <bringup>:<image>`,
    measured 2026-11-20; manager ruling the same day narrowed the run to the
    cmake rows):

    | workspace | cmake | cargo | west |
    |---|---|---|---|
    | `examples/workspaces/c` | 68 | 0 | 19 |
    | `examples/workspaces/cpp` | 70 | 0 | 20 |
    | `examples/workspaces/rust` | 0 | 66 | 20 |
    | `examples/workspaces/realtime-c` | 4 | 0 | 2 |
    | `examples/workspaces/realtime-rust` | 0 | 6 | 2 |
    | `fixtures/connect_facts_ws` | 4 | 2 | 0 |

    `n9_workspace` (`cargo-check`) and `board_agnostic_run_plan`
    (`cargo-build`) are compile checks on the cargo road.
    - The 146 cmake rows build and their tests pass: **not run yet**.
    - The 137 cargo and west rows are untouched by construction:
      `bringup_closure_dirs` runs only when the plan's driver is cmake, so
      their roots, entries and builds are byte-for-byte what they were.
- **The C/C++ metadata probe has the same shape (found under this item).**
  One host project probes every C/C++ component declared in the workspace.
  A workspace sync covers every image, so a failed probe is judged
  workspace-wide: under `NROS_PROBE_FAILURE=refuse`, `nros build` of one
  bringup's image fails on another bringup's component. The fix is open,
  pending a design ruling (scope the verdict or the membership).

### W1 — One carrier per `EXECUTABLE`

- **Status:** DONE (2026-11-21, the change that marks this)
- **What:**
  - Every single-node carrier (NuttX, ThreadX, FreeRTOS, native typed) is
    built per registration. The target is `<pkg>__<exe>`, the `OUTPUT_NAME`
    is `<exe>`, and the generated entry TU goes in
    `nros-entry/<exe>/main.cpp`.
  - Issue 2744's refusal is lifted on those roads.
  - Zephyr builds one `app` per configure (measured: a second program fails
    to link with `multiple definition of 'main'`), so a second registration
    there stays refused, worded "Zephyr builds one program per package;
    split the package"; issue 2746 tracks the sysbuild route.
- **Fixture-locator migration, same commit (#393):** about 50 TYPED
  packages' carrier artifacts move from `<project>` to `<exe>`. Every test-side
  locator (`nros-tests` fixture resolvers), every `fixtures.toml`
  `build_subdir`/artifact expectation, and NuttX's
  `build-zenoh/<PROJECT_NAME>` copy move with them. Sweep:
  `git grep -n -E '(nros_components_register_node|nano_ros_node_register)\(' -- '*CMakeLists.txt'`
  for the packages, then each one's artifact name in `packages/testing`.
- **Acceptance:**
  - the scratch two-registration rv-virt-threadx package from RFC-0071 D20
    builds two firmware images, and each boots under QEMU;
  - **the cross-package `talker` collision is its own check:** a configure
    with two packages that both register `EXECUTABLE talker` builds, giving
    two targets and two artifacts, each in its own package dir;
  - every migrated fixture row resolves;
  - issue 2744's verdict tests change to the new rule (a second DEPLOY
    registration builds; the Zephyr one is refused).
- **Done (2026-11-21).** Carriers are `<pkg>__<exe>` with `OUTPUT_NAME`
  `<exe>` and the entry TU in `nros-entry/<exe>/`, on all four roads. NuttX,
  whose ELF comes from a cargo link rather than CMake's, takes the name through
  `nros_nuttx_build_example(OUTPUT_NAME)`. `_nros_carrier_claim` refuses the
  same `EXECUTABLE` twice (with a DEPLOY) and any second Zephyr registration.
  A package's own program of the same name still turns the carrier off, but
  only a program in THIS package's directory, since target names are global.
  Measured:
  - the scratch rv-virt-threadx package with two registrations builds two
    firmware images, `c_typed_carrier` and `c_typed_carrier_b`; each holds only
    its own component, and each boots under QEMU and publishes its own line;
  - two packages that both register `EXECUTABLE talker` (native, TYPED) build
    `pkg/talker_pkg/talker` and `pkg/listener_pkg/talker`;
  - the fixture migration was 2 rows, not about 50: generated-entry rows
    resolve the entry's own binary, and native carriers in generated workspaces
    are by-products no test loads. The NuttX and ThreadX typed-carrier rows
    move to `pkg/<pkg>/c_typed_carrier`; both build and both QEMU tests pass,
    as do `workspace-c-native` and `workspace-mixed-native`;
  - issue 2744's verdict cases became five, and all 11 tests in
    `cmake_node_register_misuse` pass.
- **For W2:** `nros run`'s refusal of an entry with several executables lists
  the CMake TARGET names (`<pkg>__<exe>`); it should list the program names.

### W2 — `nros run --exec` and the launch's `exec`

- **Status:** DONE (2026-11-21, the change that marks this)
- **What:**
  - `cmake_file_api::entry_executable` takes the chosen exec.
  - `nros run` gains `--exec <name>`.
  - The run plan threads the bringup's launch `<node pkg="<entry>"
    exec="…">` into the same choice when no flag is given.
  - The refusal for several executables with none named says to name one
    with `--exec`.
  - `nros build`'s next steps print one run line per executable.
- **Acceptance:**
  - an `owned_entry_run_verb` case per choice (flag, launch, the one
    default);
  - the refusal names the executables and the flag.
- **Done (2026-11-21).** `cmake_file_api::entry_executable` takes the chosen
  program and matches it by PROGRAM name, the artifact's `OUTPUT_NAME`, never
  the CMake target (`<pkg>__<exe>` since W1). `nros run --exec <name>` chooses
  one. `ResolvedBuild::launch_exec` carries the bringup launch's `exec` for the
  entry package, and `nros run`, `nros flash` and `nros monitor` take it when
  no flag is given. The refusal for several programs lists them and names both
  ways to choose. `nros build`'s next steps print `nros run <image> --exec
  <prog>` once per program. Measured on the new `linux-c-zenoh-two-programs`
  row (`fixtures/two_programs_ws`, one owned entry building `prog_a` and
  `prog_b`): the refusal and both `--exec` choices, beside the existing
  one-program case, all in `tests/owned_entry_run_verb.rs`.
- **The launch case is W3's first acceptance item, the same check.** An
  owned-entry bringup's launch file still fails configure today: its node goes
  through component planning, which asks for probe metadata a plain program
  has none of (`missing-source-metadata ... two_programs/prog_b`). W3 makes the
  configure accept it, and adds that launch-choice case to this test file.

### W3 — An owned-entry bringup may carry a launch file

- **Status:** DONE (2026-11-22, the change that marks this)
- **What:** D19's "no launch needed" becomes "no launch needed for one
  program". The configure and the fixture manifest accept a launch file in an
  owned-entry bringup whose nodes name the entry package, and use it only to
  choose the program (W2). A node naming another package is refused.
- **Acceptance:**
  - an owned-entry bringup with a launch file naming one of two programs
    configures and runs that one;
  - one naming a foreign package is refused.
- **Done (2026-11-22).** When every image of a configure builds an owned
  entry, the configure reads the bringup's resolved launch
  (`build::launched_nodes`). When every node it starts names an owned entry
  package, the launch only chooses the program: the root still takes
  `NO_PLAN`, as with no launch at all. A node from any other package is refused
  at configure, naming it and the owned entries. W2's `launch_exec` reads the
  same nodes. Measured: the new `linux-c-zenoh-two-programs-launch` row
  (`<node pkg="two_programs" exec="prog_b">`) configures, and `nros run` with no
  flag resolves `prog_b` (`tests/owned_entry_run_verb.rs`, 7/7 with W2's
  cases). The foreign-package refusal is an end-to-end verdict: the
  `owned_entry_foreign_launch_refused` row (builder `fixture-script`) runs
  `nros build` at build time and records its exit and stderr, since the refusal
  is the CLI's, before cmake. The no-model and two-programs-in-one-launch cases
  are unit tests (`owned_entry_launch_tests`).

### W4 — Per-executable stack-floor and sizing facts

- **Status:** OPEN
- **What:**
  - On the cmake road, an owned entry with several executables writes one
    stack-floor report and one sizing descriptor per executable, keyed
    `<entry>/<exe>@<board>`.
  - `config/stack-floors.toml` allowances take the same key.
  - An entry with one executable keeps today's key.
- **Acceptance:** measured on the two-program RTOS package, two reports and
  two descriptors, each naming its own program.
- **Design (draft, 2026-11-22; not started, pending file ownership with
  nros 133 (stack floor: the stock-template RAM fix and phase-557 W9c) and nros 175 (sizing
  descriptor)).** What exists, and what breaks with two programs:
  - **Stack floor.** `nros_stack_floor_check(<exe>)` writes ONE
    `nros/stack-floor/{check.cmake,report.json}` per build dir, keyed by
    `_nros_stack_floor_image_key`: the stated `stack_floor_image`, else
    `<bringup or entry>@<board>`. Two checked programs in one configure
    overwrite each other's report and share one allowance row, and the twin and
    drift lanes glob for exactly one report.
  - **Sizing descriptor.** One `nros/sizing/<entry>.toml` per entry, composed
    from the entry's census (issue 2433). The census runs THE program; with
    two, it has no answer for which.
- **Proposed rule:** one key per program when, and only when, the configure
  checks more than one program from the same entry. An entry with one program
  keeps today's key and paths byte for byte, so no row, lane or fixture moves.
  - The key is `<entry>/<exe>@<board>`, decided in `_nros_stack_floor_image_key`
    (still the one place). A stated `stack_floor_image` names one program, so
    with several programs it is refused, naming the programs, rather than
    applied to all of them.
  - The paths are `nros/stack-floor/<exe>/{check.cmake,report.json}`.
    "Several" is known only once every program has been seen. Measured on
    CMake 3.22, the minimum: neither a DEFER at the end of the top directory
    (`TARGET 'prog' was not created in this directory`) nor a DEFER scheduled
    into the program's directory (`may not be scheduled in directory`) can
    attach a POST_BUILD to a program in a subdirectory, which is where an owned
    entry's programs live. So each check emits its POST_BUILD at once, and only
    the report PATH is decided late. The top-directory DEFER writes a layout
    file from a global property listing the checked programs per entry (a file
    write is allowed there). Each check's driver reads that file at build time,
    through the path rule below.
  - `config/stack-floors.toml` rows take the per-program key verbatim. The
    walk's key parser (`scripts/nros-stack-floor.py`) and the lanes
    (`stack-floor-twin.py`, `stack-floor-drift.py`) accept a `/<exe>` segment.
    The lanes do not glob for reports (see the path rule below).
  - The sizing descriptor goes to `nros/sizing/<entry>/<exe>.toml`, from that
    program's census, which needs the census to run each program (issue 2433's
    runner takes the program; W2's `entry_executable(…, exec)` names it).
    `nros run --exec` / the launch's `exec` pick which descriptor an image's
    consumers read, by the same choice as its `elf`.
- **Agreed with the owners (2026-11-22).**
  - **Key grammar (nros 133).** Three key shapes, told apart by their number of
    slashes: a template key `<template>/<lang>/<rmw>@<board>` (two), a bringup
    key `<bringup>@<board>` (none), and the new owned-entry program key
    `<entry>/<exe>@<board>` (one). An exe name is `[A-Za-z0-9_-]+`: rlm's exe
    names, never `/` or `@`. There is ONE grammar function, in
    `scripts/nros-stack-floor.py`; the twin lane imports it rather than keeping
    `parse_key`, and `stack-floor-drift.py` uses it too. Its Rust twin sits
    beside the CLI's `stack_floor_template_key`, with no third regex.
    `--validate-table` refuses a key that parses as none of the three. A
    self-test holds that today's `[[observed]]` keys parse unchanged. The
    parser is `parse_image_key(key) -> (shape, parts)`, beside `parse_key`'s
    callers rather than inside a walk function, so W9c's band edits and W4's
    key edits stay in separate functions.
  - **Descriptor path (nros 175).** Computed ONLY in `nros_sizing_descriptor`:
    `descriptor_path_for(build, entry, Some(exe))` gives
    `<build>/nros/sizing/<entry>/<exe>.toml`, and `None` gives today's
    `<entry>.toml` byte for byte; no reader or cmake joins it by hand.
  - **Correction (2026-11-22, read from the code; pending nros 175).** There is
    no per-program cargo build to give its own `NROS_SIZING_DESCRIPTOR`: every
    program of a configure links ONE runtime staticlib, and
    `nros_sizing_descriptor_cargo_env` names exactly one file for that build.
    Several entries sharing a runtime already have a rule (RFC-0100 D12): the
    runtime's own descriptor, `nros/sizing/runtime/shared.toml`, folded from
    each entry's. Several programs of one entry take the same rule: each
    program's census writes `<entry>/<exe>.toml`, these are the fold's inputs,
    and cargo is named the runtime descriptor. So no program's store is sized
    from another program alone, which was the concern behind the per-program
    variable.
  - **Split (manager ruling 2026-11-22).** W4 carries the GUARD: an owned
    entry's census refuses when the host build states several programs,
    naming them. The FOLD (per-program censuses folded by D12's runtime rule)
    changes the census file layout and four of its readers (freshness, digest,
    the completeness check, the configure's existence check), so it is its own
    PR after the W chain, with its own gate and review.
  - **The defect this fixes.** An owned entry's census today runs ONE program,
    the one the host build states in `nros/census-program.txt`. A cross-board
    entry with two programs is therefore sized from one program's census, and
    the other may need more.
  - **Order.** 133's stock-template RAM fix (`stack-floor-twin.py`) lands first; W4
    then rebases onto it. `cmake/NanoRosStackFloor.cmake`, `stack_floor.rs` and
    `stack_floor_raise.rs` are free. In `nros-stack-floor.py`, W4 changes key
    parsing only and 133's W9c band work changes other functions; whoever lands
    second rebases. 175's fix-param-store-carve touches only `write_for_leaf` in
    `sizing_descriptor.rs`, so the two do not collide.
  - **Follow-up: drift still globs (2026-11-22).** `report_path` spells the
    cmake road's location only, and drift also reads cargo- and west-road
    reports, so retiring its glob needs a per-road rule, from the build dir
    `nros run --dry-run --json` states. It lands with drift's build route for
    program keys. Until then `--validate-table` refuses program keys in
    drift-read rows, so the glob never meets a per-program report, and
    `pick_report` matches on the report's own `image`, so it cannot take one
    program's report for another.
  - **One path rule, every reader (nros 133's review).**
    `report_path(build_dir, key)`, in the same grammar module, is the only
    spelling of where a key's report lives. The CLI's `stack_floor_raise` (which
    reads `nros/stack-floor/report.json` today), the twin and drift lanes, and
    the build's own re-walk all call it. None of them spells `<exe>/` or globs,
    so drift's `pick_report` and the twin's glob go too, and a report at the
    wrong path reads as missing, not as found.
  - **Drift's build route for the new key (nros 133's review).** A template
    key builds through `nros new`, and a bringup key through `bringup_rows`
    over `examples/fixtures.toml`. An `<entry>/<exe>@<board>` row builds
    through the `[[workspace_fixture]]` row whose image names `entry = <entry>`
    on `<board>`. One configure builds every program of the entry, so no `exe`
    field is needed: the walk of `<exe>` is found by `report_path`. Until that
    route lands with W4, `--validate-table` refuses the one-slash shape in
    `[[observed]]`, so the nightly drift step never meets a row it cannot
    build.

### W5 — Book and docs

- **Status:** OPEN
- **What:**
  - The book's `nros run` chapter: `--exec`, and the launch `exec`.
  - The RTOS platform chapters: one firmware per executable, and what each
    costs (one kernel and network-stack link each).
  - RFC-0071 D20's status line moves to Implemented.
- **Acceptance:** `just book` builds, and `just probe` blocks (where a
  chapter has them) run.
- **Draft (2026-11-22, local until W3 lands).** "Port a C++ node" gains
  "Several programs in one package": each `nano_ros_add_executable` is a
  program, `nros run --exec`, the launch's `exec`, the foreign-package
  refusal, one firmware per program on a board, and the two limits (Zephyr's
  one program per package; a census-sized board refusing several until the
  census fold lands). `just book` builds. RFC-0071 D20's status line moves to
  Implemented only after W4 and the census fold.

## Not in this phase

- An `[image]` key choosing the program (ruled out by D20).
- Several programs linked into ONE RTOS firmware: a different shape (a
  generated entry composing components), which already exists.
