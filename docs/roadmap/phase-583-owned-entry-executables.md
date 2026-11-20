# Phase 583 — An owned entry with several executables

**Status (2026-11-20). OPEN: design accepted (user ruling 2026-11-20); W0
in progress. Owner: nros 243.** Implements [RFC-0071 D20](../design/0071-rmw-backend-descriptor.md#amendment-2026-11-20-an-owned-entry-with-several-executables-d20):
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

- **Status:** OPEN
- **Progress:** code and unit tests are in this item's PR; the fixture-row
  acceptance below has not run yet. It is a bug, so it comes before W1
  (user ruling 2026-11-20, option A of the design note).
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
  workspace with no bringup still adds every package.
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

- **Status:** OPEN
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

### W2 — `nros run --exec` and the launch's `exec`

- **Status:** OPEN
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

### W3 — An owned-entry bringup may carry a launch file

- **Status:** OPEN
- **What:** D19's "no launch needed" becomes "no launch needed for one
  program". The configure and the fixture manifest accept a launch file in an
  owned-entry bringup whose nodes name the entry package, and use it only to
  choose the program (W2). A node naming another package is refused.
- **Acceptance:**
  - an owned-entry bringup with a launch file naming one of two programs
    configures and runs that one;
  - one naming a foreign package is refused.

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

### W5 — Book and docs

- **Status:** OPEN
- **What:**
  - The book's `nros run` chapter: `--exec`, and the launch `exec`.
  - The RTOS platform chapters: one firmware per executable, and what each
    costs (one kernel and network-stack link each).
  - RFC-0071 D20's status line moves to Implemented.
- **Acceptance:** `just book` builds, and `just probe` blocks (where a
  chapter has them) run.

## Not in this phase

- An `[image]` key choosing the program (ruled out by D20).
- Several programs linked into ONE RTOS firmware: a different shape (a
  generated entry composing components), which already exists.
