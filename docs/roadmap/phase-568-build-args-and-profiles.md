# Phase 568 — build arguments

**Status (2026-11-17). Design approved; W1 done, W2 next.** Implements the
amendment "build arguments" (D7–D9) of
[RFC-0117](../design/0117-cli-command-tiers.md). The user approved it as
revised after review on 2026-11-17. The review kept D7–D9 and moved build
profiles (D10–D11, here W4–W5) to a later amendment with its own phase. W1
is next. The RFC stays Draft until this work lands.

## Motivation

The user raised two concerns, recorded in the amendment:
- **colcon argument confusion:** `--cmake-args` takes "one or more values, up
  to the next flag or `--`";
- **plugin interpretation and injection:** each road, or a plugin, could read
  an argument its own way, and a project or a dependency could re-wire a build
  silently.

Today `nros build` has three channels, each with its own rules (all in
`packages/cli/nros-cli-core/src/cmd/build.rs`):
- `--cmake-args` (`num_args = 1..`, `allow_hyphen_values`);
- `native_args` after `--`, which is split by west's own flag list on the west
  road;
- `[image.<id>] profile`, read through `cargo_profile_arg` and
  `image_build_type_args`, with `board_cargo_profile` as the default.

A `--profile` / `--release` / `-DCMAKE_BUILD_TYPE` after `--` wins over the
stated profile.

## Work items

### W1 — one argument model and its allowlist

- **Status:** DONE (2026-11-17, #4115, 7ee3f7fdd6). `builder::owned_defines`
  holds the 35 owned names, plus documented `USER_SET` (`NROS_OVERRIDE_<ID>`)
  and `COMPILER_DEFINES` rows. `every_emitted_define_is_owned` harvests the
  `nros` binary's whole source closure (420 files) and fails closed on a name
  it cannot read. `builder::build_args` is the unwired parser.
  `ARMFVP_EXTRA_FLAGS` is registered by the FVP PR that replaces #4108.

One type for a validated native argument (`CmakeArg::Define { name, value }`,
`CmakeArg::Undefine`, `CargoArg::Features`, …), built from every source by one
parser. It carries:
- plain POSIX splitting: no expansion, no `~`, unbalanced quotes refused;
- the allowlist and the refusals of D9, each refusal naming the setting that
  owns the option;
- `-DCONFIG_*` refused (the west road's Kconfig bypass);
- owned `-D` names GENERATED from the code that emits them. Each
  configure-argument builder takes its names from one exported table, and a
  test runs every road's builder and asserts each `-D` it produced is in that
  table. That covers `ARMFVP_EXTRA_FLAGS`, the board heap `-D` and the
  `NROS_*` / `_NANO_ROS_*` names;
- knob-registry names refused in every spelling of the row (env, `CONFIG_*`,
  legacy). The remedy names the row's user-facing spelling (the `knobs.toml`
  field `nros knob show` reads, #4082);
- derived features (RMW, platform, capability) refused, derived from the
  descriptors: `nros-rmw.toml` and `[system] rmw`, the board descriptor's
  `platform`, and the board and system declarations;
- `--features` must be `<pkg>/<feat>` in a workspace of more than one
  package.

**Acceptance:**
- every refused spelling, joined and separate, has a test;
- the owned-names test fails if a builder emits a `-D` missing from the
  table (negative control: add one unregistered `-D` to a builder);
- `$(touch x)` in a value creates nothing;
- an unbalanced quote is refused.

### W2 — the sources: `system.toml`, the command line, `@file`

- **Status:** DONE (2026-11-17), in two PRs:
  - W2a, #4124 (08f2847bea): `--cmake-args=`, `@file`, the space-form
    refusal, `[image.<id>.build] cmake_args`. It also ships
    `builder::knob_roads`, the measured knob-carriage table, and the knob
    refusal OFF.
  - W2b, this PR: `--cargo-args=` with the per-image derived-feature and
    `<pkg>/<feat>` checks, and `--profile=`. `--profile=` replaces each
    image's `profile` before anything reads it, and now also reaches the
    single-package cargo road, which read no profile at all before.

  The acceptance's `--dry-run` listing of a dependency's ignored arguments
  moves to W3, with the rest of `--dry-run`'s output. The arguments are
  already ignored by construction, since only the image's own block is read.

### W2c — the knob refusal

- **Owner:** nros 243 (assigned by the nros 241 manager, 2026-11-18).
- **Status:** OPEN (in progress). Every row the measured table proves carried
  is on, in one PR (ruled 2026-11-18). The 12 unproven rows below are the
  rest.
- **Rule.** `build_args::knob_refusal_on()` is not a list: it is every row of
  `knob_road_coverage.tsv` (pinned from the code by `knob_roads::tests`) that
  reads `yes yes yes`. That is 99 of 111 today. A row the table cannot prove
  carried stays accepted, so the refusal never points at a remedy that does
  nothing. A row that becomes carried is refused in the same commit that
  records it.
- **In-tree effect: none.** The refusal is a pure function of the arguments
  `nros build` is given (`--cmake-args=`, `@file`,
  `[image(_defaults).build] cmake_args`). Swept on 2026-11-18: no tracked
  `system.toml` states `cmake_args`; no script, recipe, workflow or test
  calls `nros build` with a knob in `--cmake-args=`; and the fixture builds
  run `cmake` directly, not through `nros build`. Sweep:
  `git grep -n cmake_args -- '*.toml'` and
  `git grep -n -- '--cmake-args=-D' -- ':!packages/cli/nros-cli-core/src/builder/build_args.rs'`.
- **The 12 rows still `? ? ?`.** Each has a real reader. The table cannot see
  it, because the reader does not spell `"NROS_<ID>"`:

  | Rows | Where the value is read | What the row lacks |
  | --- | --- | --- |
  | `ZENOH_BATCH_MULTICAST_SIZE`, `ZENOH_GET_POLL_INTERVAL_MS`, `ZENOH_GET_REPLY_BUF_SIZE`, `ZENOH_RX_TASK_BUDGET_FRAMES`, `ZENOH_RX_TASK_BUDGET_REST_US` | `nros-zpico-build` resolves them by IMAGE KEY through `platform_config::wire_env_key("<key>")`, which looks the env name up in the registry (`registry_env("zenoh.wire", …)`) | the scan must credit a `wire_env_key`/`registry_env` call by image key as a Rust reader |
  | `ZENOH_TX_BATCH`, `ZENOH_TX_BATCH_FLUSH_MS`, `ZENOH_TX_SPLIT_LOCK` | the same ladder, through `platform_config::tx_env_key("<key>")` | the same credit, for `tx_env_key` |
  | `XRCE_MAX_SUBSCRIBERS`, `XRCE_TRANSPORT_MTU`, `XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE` | `nros-rmw-xrce-cffi/build.rs` reads the env names from rows of `packages/rmw/xrce/xrce-config.txt` (`define`/`knob` lines) | the scan must read that table, which is `.txt` and outside both corpora |
  | `ZENOH_SOCKET_TIMEOUT_MS` | only Zephyr's C lane, as `CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS` → `Z_CONFIG_SOCKET_TIMEOUT` (`zephyr/cmake/nros_rmw_zenoh.cmake`). The cargo and cmake roads have no reader at all | a ruling: is it Zephyr-only (its cargo/cmake cells are then "not applicable", and the table needs that state), or do the other roads read it too? |

  Teaching the scan the first three shapes turns 11 rows to measured. Until
  then they stay accepted.

- `[image.<id>.build] cmake_args`, read from the image's own section only.
  There is no `cargo_args` key: committed features belong in the manifest.
- `--cmake-args=` / `--cargo-args=` / `--profile=`, with `=` required,
  repeatable, adding up in order.
- `@file`: one argument per line, `#` comments, resolved from the current
  directory, command line only, no nesting.

The space-separated `--cmake-args` form is refused, with colcon-compatible
wording that shows the user's arguments rewritten.

**Acceptance:**
- a dependency's `system.toml` arguments are IGNORED (not refused: a
  package is not this build's to refuse) and listed by `--dry-run`;
- `@file` resolution has a test;
- a CMake argument containing `--packages-skip` stays a CMake argument.

### W3 — built-in routing, and `--` narrowed to west

- **Status:** DONE (2026-11-17, W3a #4131 and W3b)

The `--` refusal moved to W6, ruled that day: the fixture harness passes about
470 rows' `-D`s and cargo's `--profile`/`--target-dir` after `--`, so refusing
them now would break the lane.

CMake arguments reach every CMake configure (cmake image, west's cmake step,
cmake package), and cargo arguments reach every cargo command. An argument
kind a road does not use is ignored and reported by `--dry-run`. After `--`,
only west's known options are accepted (`split_native_args` already holds
their list). Anything else is refused, naming its flag.

The build dir records the arguments it was configured with. On a change, the
removed CMake variables are `-U`'d and the configure re-runs, and the build
says so.

**Acceptance:**
- a test for each road receiving its argv. **Done** in W3b:
  `each_road_hands_the_callers_arguments_to_its_own_tool_in_order`
  (cmake, west, cargo), in `build_verb_pipeline.rs`;
- the source-metadata probe reads `[image_defaults.build] cmake_args` and
  nothing else (RFC-0117 D8's exception). A package whose
  `find_package(<Pkg> … NO_DEFAULT_PATH)` path is given there both probes
  and builds; removing it re-probes and fails, saying where the path goes.
  The book's bring-your-own-library page found the gap on 2026-11-17, with
  a guarded probe that failed and an unguarded one that found the host's
  copy. **Done** in W3a, measured end to end on a copy of `image_config_ws`;
  the probe's dropped `-D`s are `-U`'d, so the cache cannot keep them;
- the probe adds each node package's build-time WORKSPACE dependency
  packages to its one project, in ws-order before the node, and keys
  freshness on them, so a node including a sibling library's header
  probes without an `add_subdirectory` fallback of its own (issue 2680).
  **Done**: dependency closure over the build-time tags, added once in
  topological order, cycle and west-dependency refusals, blame per member,
  freshness key over the closure; `probe_workspace_dep_ws` configures under
  `refuse` with no fallback (`archived/2680-*`);
- a dependency's `system.toml` arguments are listed by `--dry-run` as
  ignored (moved from W2). **Open**: a dependency package has no
  `[image.<id>.build]` of its own today, so there is nothing yet to ignore;
- cache drift: `-DFOO`, then a build without it, leaves no `FOO` in the cache.
  **Done** in W3b for the cmake image road: the caller's arguments are
  recorded beside the image's `CMakeCache.txt` (`nros-cmake-args.used`) and a
  dropped `-D` is `-U`'d; a name nano-ros owns is left to nano-ros. Measured
  on a copy of `image_config_ws`: `-DMY_X=1 -- -DMY_Z=3`, then a bare build,
  leaves no `MY_*` in the cache;
- `--dry-run` prints the final argv per tool, and the source of each
  argument. **Done** in W3b on the cmake road: the configure line, then the
  caller's arguments by source in the order they reach CMake, and the probe
  rule;
- `--` carrying anything but west's own options warns, ONCE per run, naming
  the flag that carries each kind (`--cmake-args=`, `--cargo-args=`,
  `--profile=`, `CARGO_TARGET_DIR`). **Done** in W3b; the refusal is W6's.

### W4 — profiles

- **Status:** DROPPED (2026-11-17, review): moved to a later amendment with its
  own phase. That amendment starts from the review's three points, recorded in
  RFC-0117's "Deferred: build profiles": rename away from cargo's
  `[profile.*]` and name the file; one locator plan, priced, before any
  artifact path moves; no custom `CMAKE_BUILD_TYPE`.

### W5 — per-profile build dirs

- **Status:** DROPPED (2026-11-17, review): moved with W4. The recorded
  arguments and the `-U` on a change, which are not about profiles, moved into
  W3.

### W6 — migration and docs

- **Status:** BLOCKED (on W1–W3: it moves callers onto the new flags)

- **First: the fixture harness.** `scripts/check/report-fixture-image-defs.py`
  (report only, no build) classifies each image fixture row's `-D`s after `--`
  against its image. Measured 2026-11-17 over 270 image cmake rows: 0
  contradict; 395 repeat the image (`NROS_RMW` 270, `NROS_BOARD` 84,
  `NROS_PLATFORM` 41); 43 give `NROS_PLATFORM` at family level (`threadx` for
  `threadx-linux`), which nano-ros does not pass itself, so those are built on
  each side before they drop; 15 add something (`NROS_ENTRY_LOCATOR` x11,
  `CMAKE_BUILD_TYPE` x3, one find-package switch) and need a home first. The
  cargo rows move to `--profile=` and `CARGO_TARGET_DIR`. Only then does the
  W3b warning become the refusal.

- The book's `nros build` page and RFC-0117 D4's table name the new flags.
- Every in-tree caller of `--cmake-args <space form>` and of `--`-carried CMake
  or cargo arguments moves (recipes, scripts, fixtures, tests). The sweep is
  `git grep -n -e '--cmake-args ' -e 'nros build.* -- '`.
- The logs note says secrets passed as arguments appear in logs.

**Acceptance:**
- the sweep finds no old form outside the refusal tests;
- the book block for the new flags is probed.

## Threat model

From the amendment: the arguments come from the user or their own project,
and a build already runs project code. So the guard is against misuse
(silently re-wiring or corrupting a build), and the allowlist also closes code
execution through CMake (`-C`, `-P`) and cargo (`--config`, `-Z`). Secrets in
arguments show in logs, which is documented, not prevented.

## Not in scope

- colcon's `--cmake-target`, `--cmake-clean-cache` and `--cmake-force-configure`.
- Per-package arguments for a dependency (refused by design, D7).
- Changing the nano-ros cargo presets (`nros-relwithdebinfo`, …): a user
  profile names one as its `cargo_profile`.
