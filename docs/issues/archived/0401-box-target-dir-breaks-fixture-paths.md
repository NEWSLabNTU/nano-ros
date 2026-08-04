---
id: 401
title: The distrobox's CARGO_TARGET_DIR and the fixture path contract are
  mutually exclusive — fixtures build into the wrong tree, tests look in target/
status: resolved
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

## RESOLVED (2026-08-04) — the box got its own tree

Both issues were the same premise: host and box sharing one checkout whose build
artifacts are glibc- and toolchain-specific. Fixing instances did not converge —
five in one session (build scripts, CMake caches, the CLI, the resolver, fixture
paths), each real, each a symptom.

`scripts/dev/ros2-box-sync.sh` mirrors the working tree (uncommitted edits
included, `.git` included, every build output excluded) into `<checkout>-box`,
and `ros2-box-env.sh` detects the `.nros-box-tree` marker and does NOT redirect
`CARGO_TARGET_DIR` there. Cargo then writes to the LEAF paths the fixture
contract names, inside a tree the host never touches — which is what 0401 said
redirection could never give: the two mechanisms are mutually exclusive, and the
tree split removes the conflict instead of trading one hole for another.

A mirror rather than `git worktree`, deliberately: a worktree cannot check out
the branch the host has, and carries only COMMITTED state — the loop here is
edit, build in the box, test, and a worktree would test the last commit.

Verified: box `just setup-cli` produced a working box CLI and left the host's
binary untouched (previously each overwrote the other); a fixture built in the
box landed at `examples/native/rust/talker/target/nros-fast-release/talker` —
the exact path the test-side resolver stats — inside the box tree.

A checkout WITHOUT the marker is still treated as shared and keeps the redirect:
there the alternative is host-built build scripts dying on glibc, so the old
behaviour remains correct for that case.

Caveat, documented in the script and the guide: `nros sync` writes absolute
paths into leaf `.cargo/config.toml` files, so a mirrored leaf still points at
the source tree until re-synced in the box. Same rule as any moved checkout.

The narrower guards from this session stay as defence in depth: the CMake
compiler-version cache guard, `nros_scoped_target_dir` for ephemeral dirs, the
per-side resolver path, and the SDK store honouring `NROS_HOME`.
