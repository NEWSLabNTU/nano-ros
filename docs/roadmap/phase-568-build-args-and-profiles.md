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

- **Status:** OPEN (W2 landed)

CMake arguments reach every CMake configure (cmake image, west's cmake step,
cmake package), and cargo arguments reach every cargo command. An argument
kind a road does not use is ignored and reported by `--dry-run`. After `--`,
only west's known options are accepted (`split_native_args` already holds
their list). Anything else is refused, naming its flag.

The build dir records the arguments it was configured with. On a change, the
removed CMake variables are `-U`'d and the configure re-runs, and the build
says so.

**Acceptance:**
- a test for each road receiving its argv;
- the source-metadata probe reads `[image_defaults.build] cmake_args` and
  nothing else (RFC-0117 D8's exception). A package whose
  `find_package(<Pkg> … NO_DEFAULT_PATH)` path is given there both probes
  and builds; removing it re-probes and fails, saying where the path goes.
  The book's bring-your-own-library page found the gap on 2026-11-17, with
  a guarded probe that failed and an unguarded one that found the host's
  copy. **Done** in W3a, measured end to end on a copy of `image_config_ws`;
  the probe's dropped `-D`s are `-U`'d, so the cache cannot keep them;
- a dependency's `system.toml` arguments are listed by `--dry-run` as
  ignored (moved from W2);
- cache drift: `-DFOO`, then a build without it, leaves no `FOO` in the cache;
- `--dry-run` prints the final argv per tool, and the source of each
  argument.

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
