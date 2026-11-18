---
id: 2722
title: "A feature set a lane builds can be linted by no lane, so its clippy errors sit on main"
status: resolved
type: bug
severity: medium
area: [ci, lint, testing]
related: [issue-1532, issue-0196, issue-1040]
found: 2026-11-19
resolved_in: "the change that archives this issue"
---

## What

`nros-cpp` with `--no-default-features --features alloc,env,log,rmw-cffi,
metadata-mode,param-services,lifecycle-services` was built only by
`census-hooks-complete`, with `cargo test`, and no lane ran `cargo clippy` over
it. Two `-D warnings` errors sat on `main`:

- `packages/api/nros-cpp/src/metadata_hooks.rs:963`: `map_or(rest.len(), |e| e)`,
  an identity closure ("expression can be simplified using
  `Option::unwrap_or()`");
- `packages/api/nros-cpp/src/lib.rs:10674`: `assert!` on a constant
  (`clippy::assertions_on_constants`).

The class is wider than one set. A crate built with an explicit `--features`
set compiles code no other set reaches: its `cfg(feature = ...)` arms, and,
for `cargo test`, the test modules behind them. Whether that code is linted
depended on someone having written a matching `cargo clippy` line beside the
build. Nothing compared the two lists.

## Measured

`scripts/check-built-feature-sets-linted.py` harvests every build site with
an explicit set from:

- every `just` recipe (through `just --dump --dump-format json`, so `just`
  resolves `{{C_API_SHIPPED_FEATURES}}` itself);
- `scripts/*.sh`;
- `.config/feature-lanes.toml`, whose runs `scripts/feature-lanes.py` spells
  in Python, so they appear in no recipe text.

That gives 73 build sites, 12 clippy sites, and **53 sets no clippy site
covered**. With this issue's two fixes, all 53 lint clean (`-D warnings`,
test targets included where the site builds tests), in about 5.5 minutes
warm. So the census set was the only red one, and the other 52 were green
by luck rather than by a lane.

## Fix

- The two errors: `unwrap_or(rest.len())`, and the constant assertion moved
  into `const { assert!(..) }`, so the build itself refuses it.
- The lint list is derived from the build sites, never written down beside
  them:
  - `just check built-feature-sets-linted` (fast line) fails on any build
    site whose set cannot be resolved statically, naming it, so nothing goes
    unlinted silently. It runs its self-test on every run.
  - `just check built-feature-sets-clippy` (merge group, beside
    `cpp-ffi-tests` in `check-cli-tests`) runs clippy `-D warnings` over
    every uncovered set. It carries each site's environment (one set needs
    `NROS_PARAM_STORE=1`) and reports every red set, not just the first.
- A set is "covered" by a clippy site with the same crate,
  default-features switch, feature set, manifest and target, and with test
  targets linted when the site builds tests.
- Two sites whose set exists only at run time now run `cargo clippy` in
  place of `cargo check`, at the same compile cost: the Zephyr Cyclone set
  in `check cpp` (read from `zephyr/CMakeLists.txt`), and
  `scripts/check-feature-gated-modules.sh`'s one-feature-at-a-time sweep.
- Exempt, each with its reason in the script:
  - play_launch's Python extension and zenoh-c's CAN build: not this tree's
    crates, built as host tools;
  - `scripts/stage-px4-posix-archives.sh`: the same set that
    `px4::build-sitl-example` spells literally;
  - `scripts/debug/`: ad hoc aids no lane runs.

Sweep: `python3 scripts/check-built-feature-sets-linted.py --list`
