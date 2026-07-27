# Phase 312: Three-layer launch toolchain (RFC-0060)

**Status:** In progress (started 2026-07-28). **W1–W3 complete.** W4 (closure) remains.
**Implements:** RFC-0060. **Closes:** the structural half of issue 0293 (one
implementation per contract); removes the vendoring drift behind issues
0285/0293.
**Spans three repos:** `ros-launch-manifest`, `ros-launch-resolve` (new),
`play_launch`, plus this one.

## Goal

```
ros-launch-manifest   spec, theory, proofs, algorithms      no ROS, no Python
        ↑
ros-launch-resolve    launch tree -> SystemModel   (NEW)    + CPython
        ↑
play_launch           Linux runtime + binary                + rclrs, colcon
        ↑
nano-ros              helper binaries                       pins layer 2 only
```

## Ordering principle

**Every intermediate state builds.** The move is additive-then-subtractive per
layer: stand the new repo up and prove it green *before* anything is deleted
from play_launch. No step leaves a consumer pointing at code that has moved.

## Work items

### W1 — stand up `ros-launch-resolve`

- **W1.1** Create the repo under NEWSLabNTU, MIT/Apache as play_launch.
  Workspace skeleton: `parser/`, `resolve/`, `cli/`, `third-party/`.
- **W1.2** Vendor `ros-launch-manifest` as the single submodule pin.
- **W1.3** Move `play_launch_parser` in (history-preserving; it is already its
  own submodule, so this is a re-point rather than a copy).
- **W1.4** Move the resolve pipeline: `ros/{manifest_loader, sched_loader,
  model_builder, sched_derive, chain_checks, causal_graph, causal_dag_global,
  manifest_graph, launch_dump}` + `commands/{resolve, common, contract, dump,
  plot}` + the `cli` option types they need.
- **W1.5** `cargo build && cargo test` green **under plain cargo** — no ROS
  sourced, no colcon. This is the invariant the whole RFC rests on; if it does
  not hold, stop.
- **W1.6** Ship the `ros-launch-resolve` binary with the `resolve` verb, so the
  repo is independently useful.

**Acceptance:** a clean container with rustc + CPython and no ROS resolves
`examples/workspaces/cpp/src/demo_bringup/launch/system.launch.xml` into a
SystemModel byte-identical to the one play_launch produces today.

### W2 — nano-ros consumes layer 2

- **W2.1** Re-point `packages/cli/nros-launch-resolve` at the new repo;
  drop the `play_launch` submodule and the second `ros-launch-manifest`.
- **W2.2** `just setup-launch-resolve` builds against it; `nros sync` resolves
  all six `demo_bringup` launch files.
- **W2.3** Confirm `vendor/{ros2_rust, rosidl_runtime_rs,
  rcl_interception_sys}`, `play_launch_msgs` and `play_launch_container` are
  gone from nano-ros's graph.

**Acceptance:** nano-ros carries ONE launch-toolchain pin; `just native
build-fixtures` green; the `nros` binary's dependency graph contains no z3, no
pyo3, no ROS.

### W3 — play_launch consumes layer 2

- **W3.1** Depend on `ros-launch-resolve`; delete the moved modules.
- **W3.2** Retire the `runtime` feature — the crate boundary now enforces what
  the flag simulated. `play_launch_msgs`/`rclrs` go back to unconditional.
- **W3.3** `resolve` verb either delegates to layer 2 or is dropped (RFC-0060
  open question 3).
- **W3.4** `just build-cpp && just build-rust` green; `play_launch launch` and
  `replay` unaffected.

**Acceptance:** play_launch builds and its runtime tests pass with the resolve
pipeline no longer in-tree.

### W4 — closure

- **W4.1** RFC-0060 Draft → Stable; record the answers to its open questions.
- **W4.2** Issue 0293's SSoT follow-up: one `system.toml` deploy schema, with
  nano-ros's `DeployTarget` an alias of rlm's `DeployBlock`, and
  `deny_unknown_fields` once the key audit is done.
- **W4.3** CLAUDE.md / AGENTS.md pointers updated for the new chain.

## Progress

**W1.1–W1.3 done (2026-07-28).** `NEWSLabNTU/ros-launch-resolve` created;
`ros-launch-manifest` and `play_launch_parser` vendored as its two submodules.

**W1.4 done.** ~12.3k lines moved with `git filter-repo` — history preserved,
`git log --follow` walks through the move (24 commits on `model_builder.rs`
alone; 106 in the repo). A second filtered extraction brought `cli/options`,
`python/` and the sched helpers with their history rather than copying them.

**W1.5 done — the load-bearing receipt.** The resolve library compiles under
`env -i` with no `ROS_DISTRO`, no `AMENT_PREFIX_PATH` and an empty target dir.
Layer 2 genuinely does not need ROS or colcon.

Two layering leaks were fixed rather than carried across, both found by the
compiler once the crate boundary was real:

- `util::logging::init_verbose` took the consumer's entire `Options` enum and
  matched every subcommand to read one `bool`.
- `sched/{plan,apply}` pulled in `play_launch::sched`, whose `apply_tier` /
  `has_sched_privilege` are `sched_setscheduler` wrappers. Applying a schedule
  is runtime work; nothing in layer 2 used `SchedPlan`, only the
  `SchedApplyMode` enum. Both files stayed in play_launch.

That second one is the useful signal: a crate boundary catches layer violations
that a feature flag cannot.

**W1.6 done. W1 ACCEPTANCE MET.** `ros-launch-resolve resolve` produces a
SystemModel byte-identical to play_launch's for
`demo_bringup/system.launch.xml` — the only differences are the two provenance
fields that must differ:

```
- tool: play_launch      + tool: ros-launch-resolve
- version: 0.8.2         + version: 0.1.0
```

Built and run under `env -i`. 274 tests pass.

`Command` is trimmed to the four launch-tree verbs; `launch`, `run`, `replay`,
`setcap` and `verify` drive processes and stayed in play_launch, as did
`CleanupGuard` (it reaps spawned nodes via `kill_all_descendants`, and the
resolver spawns none).

One real bug surfaced by the move: the model's provenance stamp was the literal
`"play_launch"`, so every model this crate resolved would have misreported
which tool produced it — the field exists precisely so a consumer can tell.
Now `env!("CARGO_PKG_NAME")`.

Still owed here: argument-parsing tests for the four remaining verbs. The ones
that existed covered `launch`/`run`/`replay`/`check` and went with them.

**W2 done (2026-07-28).** Three pins became one:
`third-party/{play_launch, ros-launch-manifest, play_launch_parser}` →
`third-party/ros-launch-resolve`, with rlm arriving transitively. The drift
class behind 0285/0293 is closed by construction — one copy of the spec below
any consumer.

Receipts: `nros sync` resolves 6/6; regenerated models differ ONLY in the
provenance stamp; `rclrs`, `play_launch_msgs` and `rosidl_runtime_rs` are gone
from both `packages/cli/Cargo.lock` and the helper's lock; 453 cli-core tests
pass.

One design correction on the way: `build_checked_model` lived in the resolver's
CLI crate, so a program LINKING the library could not build a model without
reimplementing argument parsing. It moved to `resolve/src/model.rs`; file IO
and progress output stayed in the binary.

**W3 done (2026-07-28).** play_launch is layer 3 and nothing else: 12,570
lines of resolution left, replaced by a dependency on `ros-launch-resolve`.
Its `src/{ros-launch-manifest, play_launch_parser}` submodules are gone —
both arrive transitively, so one copy of the spec exists in the tree instead
of two.

The `runtime` feature is **retired**. It had been simulating a crate boundary;
now there is one, so the ROS deps go back to unconditional and every
`#[cfg(feature = "runtime")]` is gone.

`resolve` stays as a deprecating delegate (RFC-0060 decision 3) because ASI
embeds it in `pilot.launch.xml`; `dump`/`plot`/`contract` had no such user and
were dropped. `SchedApplyMode` was deduplicated — play_launch held a second
definition of the same enum, the shape that produced issue 0293.

Receipts: `just build-rust` green under colcon; the delegate emits a model
byte-identical to layer 2's apart from provenance.

**Next: W4** — RFC-0060 Draft → Stable, the 0293 SSoT follow-up (one
`system.toml` deploy schema), and CLAUDE.md/AGENTS.md pointers for the new
chain.

## Risks

- **Test execution follows the code.** A crate that cannot resolve under plain
  cargo cannot be `cargo test -p`'d from a consumer, so moved code can silently
  lose its tests (this already happened twice — `DispatchGuard`,
  `input_path_string`). W1.5 is the gate that prevents it.
- **Three-repo pin ordering.** Push layer 1 before layer 2 before layer 3, and
  bump pins in that order, or a nested checkout resolves against code that does
  not exist yet. This drifted three times during the 0285 work.
- **History.** `git mv` within a repo preserves it; moving across repos does
  not, unless done with `git filter-repo`. W1.4 should preserve history for
  ~12.3k lines of load-bearing code.
- **Concurrent sessions.** Other agents are active in all three repos; land
  each W in small pushed steps rather than one large drop.

## Receipts to collect

| Step | Receipt |
| --- | --- |
| W1.5 | plain-cargo `cargo test` green with no ROS on PATH |
| W1 acceptance | byte-identical SystemModel vs today's play_launch |
| W2 | `nros sync` resolves 6/6; one pin; no z3/pyo3/ROS in `nros` |
| W3 | play_launch colcon build green, runtime tests pass |
