# Phase 583 — An owned entry with several executables

**Status (2026-11-20). OPEN: design accepted (user ruling 2026-11-20); no
item started. Owner: nros 243.** Implements [RFC-0071 D20](../design/0071-rmw-backend-descriptor.md#amendment-2026-11-20-an-owned-entry-with-several-executables-d20):
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

### W1 — One carrier per `EXECUTABLE`

- **Status:** OPEN
- **What:**
  - Every single-node carrier (NuttX, ThreadX, FreeRTOS, native typed) is
    built per registration. The target is `<pkg>__<exe>`, the `OUTPUT_NAME`
    is `<exe>`, and the generated entry TU goes in
    `nros-entry/<exe>/main.cpp`.
  - Issue 2744's refusal is lifted on those roads.
  - Zephyr builds one `app` per configure, so a second registration there
    stays refused, with that reason.
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
