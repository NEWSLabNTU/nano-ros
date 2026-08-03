---
id: 401
title: The distrobox's CARGO_TARGET_DIR and the fixture path contract are
  mutually exclusive — fixtures build into the wrong tree, tests look in target/
status: open
type: bug
area: testing
related: [0400, 0375, 0285]
---

## Problem

Two mechanisms in this repo both need to win and cannot:

- **`scripts/dev/ros2-box-env.sh` exports `CARGO_TARGET_DIR`** so the box never
  re-runs a HOST-built `build-script-build` against its older glibc. Without it
  you get `GLIBC_2.39 not found` from a build script — the failure that
  variable exists to prevent (#0400).
- **The fixture contract is LEAF-RELATIVE.** `examples/fixtures.toml` rows and
  `nros_tests::fixtures::binaries` agree on paths like
  `examples/native/rust/talker/target/nros-fast-release/talker`, with per-row
  variants pinned by explicit `--target-dir target-{zenoh,xrce,cyclonedds,…}`.

With `CARGO_TARGET_DIR` set, every fixture cargo build writes into the box's
shared tree instead, and NOTHING at the leaf path changes. The build reports
success — truthfully, it built everything — and then the tests fail on binaries
that were never written where they look.

The symptom is maximally misleading, because a developer who has ALSO built on
the host has stale leaf binaries sitting there from that host run. The tests
then find them and report

    Test fixture is STALE — a source is newer than the built binary:
      binary: examples/native/rust/talker/target/nros-fast-release/talker
      newer:  examples/native/rust/talker/generated/builtin_interfaces/src/lib.rs

which reads as an ordering bug in the fixture builder. It is not: the binary is
simply from a different machine image, and no amount of rebuilding IN THE BOX
will ever update it. 138 of 1244 tests failed this way before the cause was
found, and three separate "fix the staleness" theories were wrong first.

## Repro

```sh
DBX_CONTAINER_MANAGER=docker distrobox enter ros2 -- bash -c '
    . scripts/dev/ros2-box-env.sh
    just build-test-fixtures lane=native          # reports success
    ls examples/native/rust/talker/target/nros-fast-release/talker'   # absent
```

Unsetting `CARGO_TARGET_DIR` puts the binary where the tests look — and
immediately reintroduces the glibc hazard the variable exists for, because the
box then re-runs host-built build scripts under `packages/cli/**/target/`
(observed on `z3-sys`).

## Why neither side can simply give way

Dropping the redirect means host and box share every build tree: host-built
build-script executables, host-configured CMake caches (#0400 already documents
three instances), and mixed-glibc objects. Making the fixture paths follow
`CARGO_TARGET_DIR` means the test-side resolvers, `fixtures.toml`'s per-row
`--target-dir` values, and every gate that stats those paths must all learn the
same redirect — and `nros sync` writes absolute paths into leaf configs, which
is the #0375 split-brain hazard.

## Fix sketch

Preferred: make the fixture path contract a FUNCTION of the environment rather
than a constant, in ONE place. A single helper (`nros_fixture_target_root`)
consulted by the builder, `fixtures.toml` row expansion, and the test-side
resolvers, defaulting to the leaf `target/` and honouring `CARGO_TARGET_DIR`
when set. The per-row `--target-dir target-<rmw>` values become suffixes under
that root.

Cheaper interim: have `ros2-box-env.sh` NOT redirect, and instead give the box
its own checkout (a second worktree). Costs disk, but removes every
shared-tree hazard in #0400 at once rather than one instance at a time.

Whatever is chosen, the builder should FAIL when a lane finishes without the
artifacts its rows name, instead of reporting success for files it wrote
elsewhere — that check is what would have caught this in one run.

## Notes

Found finishing tier 1 in the box for the issue-0383 `-Werror` work
(2026-08-03). Not caused by that change. `just check` passes in the box; this
only affects the fixture/test half.
