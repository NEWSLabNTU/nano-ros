# Phase 568 — build arguments and profiles

**Status (2026-11-17). DRAFT, for review: nothing is built.** Implements the
draft amendment "build arguments and profiles" (D7–D11) of
[RFC-0117](../design/0117-cli-command-tiers.md). The design was approved by
the user on 2026-11-17 for writing up, and is put up for review before any
work item starts. Work items are listed so that the review can price them.
None starts until the amendment is accepted.

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

- **Status:** BLOCKED (awaits review of the RFC-0117 draft amendment)

One type for a validated native argument (`CmakeArg::Define { name, value }`,
`CmakeArg::Undefine`, `CargoArg::Features`, …), built from every source by one
parser. It carries:
- plain POSIX splitting: no expansion, no `~`, unbalanced quotes refused;
- the allowlist and the refusals of D9, each refusal naming the `system.toml`
  setting that owns the option;
- owned `-D` names taken from the one table of what `nros build` passes, which
  this item creates if it is not one table already;
- derived features (RMW, platform, capability) refused.

**Acceptance:**
- every refused spelling, joined and separate, has a test;
- `$(touch x)` in a value creates nothing;
- an unbalanced quote is refused.

### W2 — the sources: `system.toml`, the command line, `@file`

- **Status:** BLOCKED (awaits review of the RFC-0117 draft amendment)

- `[image.<id>.build] cmake_args` / `cargo_args`, read from the image's own
  section only.
- `--cmake-args=` / `--cargo-args=` / `--profile=`, with `=` required,
  repeatable, adding up in order.
- `@file`: one argument per line, `#` comments, resolved from the current
  directory, command line only, no nesting.

The space-separated `--cmake-args` form is refused, with colcon-compatible
wording that shows the user's arguments rewritten.

**Acceptance:**
- a dependency's `system.toml` adding arguments is refused or ignored, and
  says which;
- `@file` resolution has a test;
- a CMake argument containing `--packages-skip` stays a CMake argument.

### W3 — built-in routing, and `--` narrowed to west

- **Status:** BLOCKED (awaits review of the RFC-0117 draft amendment)

CMake arguments reach every CMake configure (cmake image, west's cmake step,
cmake package), and cargo arguments reach every cargo command. An argument
kind a road does not use is ignored and reported by `--dry-run`. After `--`,
only west's known options are accepted (`split_native_args` already holds
their list). Anything else is refused, naming its flag.

**Acceptance:**
- a test for each road receiving its argv;
- `--dry-run` prints the final argv per tool, the winning profile and its
  source.

### W4 — profiles

- **Status:** BLOCKED (awaits review of the RFC-0117 draft amendment)

- Built-ins `dev` and `release`.
- Workspace `[profile.<name>]` with `inherits`, `cargo_profile`,
  `cmake_build_type`, `cmake_args` and `cargo_args`.
- Precedence: CLI > image > board > default.
- The board default moves from `board_cargo_profile` / `board_build_type_arg`
  into the board descriptor, so a board that needs `MinSizeRel` states it.
- `cargo_profile` must exist in the project's `Cargo.toml`, and is refused
  early if not.
- A custom `cmake_build_type` needs its flags defined.

**Acceptance:**
- a missing `cargo_profile` is refused before any build step;
- `inherits` chains resolve parent first, with a test.

### W5 — per-profile build dirs and recorded arguments

- **Status:** BLOCKED (awaits review of the RFC-0117 draft amendment)

`build/<image>/<profile>/`. Each dir records the arguments it was configured
with. On a change, the removed variables are `-U`'d and the configure re-runs,
and the build says so. Arguments and the profile are build inputs: they go in
the plan, the profile summary, fixtures and size reports.

**Acceptance:**
- cache drift: `-DFOO`, then a build without it, leaves no `FOO` in the cache;
- a profile switch keeps both dirs, and switching back rebuilds nothing.

### W6 — migration and docs

- **Status:** BLOCKED (awaits review of the RFC-0117 draft amendment)

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
