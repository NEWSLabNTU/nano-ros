---
id: 2464
title: "In a fresh worktree, `just format` fails until codegen and `nros configure` have run in several leaves, and the px4 companion leaves block `linux::format` outright"
status: open
type: bug
area: [tooling, dev-experience]
severity: low
found: 2026-11-12
related: [0463, 1114]
---

Found in the phase-535 W8 worktree; no phase owns it yet. CLAUDE.md tells everyone to run `just format`
before broad changes, so a fresh worktree that cannot format is the first
thing a new session hits.

## Repro (measured 2026-11-12, on `10d35dc209`)

    git worktree add -b x <dir> origin/main
    cd <dir> && just setup-worktree && source ./activate.sh
    just format

Each failure below appeared only after the one before it was fixed.

1. `_require-leaf-includes`: 2 unresolved `include` targets (the gitignored
   central `nros-patch.toml`), in
   - `packages/testing/nros-bench/wcet-cycles-qemu/.cargo/config.toml`
   - `packages/testing/nros-tests/bins/cdr-roundtrip-qemu/.cargo/config.toml`

   The remedy it prints, `nros configure`, run at the repo root, refuses: it
   finds the same package name in several workspaces. Running it inside each
   of the two leaves works.
2. `_require-leaf-includes` again: 85 path deps into `generated/` absent,
   across 68 manifests. `just _codegen` (about 10 minutes here) fixes it.
3. `linux::format`: `cargo fmt` fails in `examples/mps2-an385-baremetal/cpp/talker`
   with `failed to load manifest for dependency baremetal_cpp_talker_nros_selection`
   (`build/nros/selection/Cargo.toml` absent). `_codegen` does not generate
   it. `nros configure` was needed in four leaves:
   - `examples/mps2-an385-baremetal/c/talker`
   - `examples/mps2-an385-baremetal/cpp/talker`
   - `examples/mps2-an385-baremetal/rust/listener`
   - `examples/mps2-an385-baremetal/rust/talker`
4. `linux::format`: `cargo fmt` fails on `failed to load manifest for
   dependency px4_msgs` in the px4 companion leaves
   (`examples/px4/rust/companion/{offboard-companion,px4-probe,px4-stub}`),
   whose `generated/px4_msgs` comes only from `nros generate-px4-msgs`
   (issue 1114). With no PX4 `.msg` tree provisioned, nothing short of
   `just setup px4` gets past it. `_require-leaf-includes` already reports
   these as "not required by this lane", and then `linux::format` requires
   them anyway.

`just _codegen` also leaves two `generated/` trees that no ignore rule covers
(`packages/testing/nros-bench/large-msg-baremetal/src/qemu_bsp_large_msg_test/generated/`,
`packages/testing/nros-bench/wake-latency-cortex-m3/src/qemu_wcet_bench/generated/`),
so they show up in `git status` as untracked files that a careless stage would commit.

`git clean -fdXq`, which the agent workflow runs at the end of a task, puts
the worktree back at step 1: the same state also turns the `pre-push` hook's
`check fast` red.

## Expected

Formatting reads sources, so it should not need generated trees. Either:

- `just format` formats only tracked sources (rustfmt per file, or
  `cargo fmt` skipped for a leaf whose manifest cannot load, reported), or
- `just setup-worktree` leaves the tree in a state `just format` and
  `check fast` accept (the configure + codegen above), and says how long
  that takes.

In both cases, the px4 leaves must not block a host that never asked for
PX4.
