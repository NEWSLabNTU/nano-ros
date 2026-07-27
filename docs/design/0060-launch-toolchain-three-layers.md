# RFC-0060 — Three layers: spec, resolver, runtime

**Status:** Stable (2026-07-28)
**Supersedes nothing. Amends:** RFC-0059 (launch-toolchain-split) — this is the
repository-level answer to the same problem RFC-0059 framed at crate level.
**Motivated by:** issue 0285 (a PATH-resolved `play_launch` broke every
platform's fixture build), issue 0293 (two parsers for one file), and the
submodule drift that bit three times during the 0285 work.

## Summary

Split the launch toolchain into three repositories along the line of **what a
consumer must be able to link**:

```
ros-launch-manifest      spec, theory, proofs, and the algorithms over them
        ↑                serde/toml/yaml — no ROS, no Python
ros-launch-resolve       ROS launch tree -> SystemModel        (NEW)
        ↑                + CPython (for .launch.py) — no rclrs, no colcon
play_launch              the Linux runtime and its binary
                         + rclrs, generated msgs, colcon
```

Strictly linear. nano-ros and future descendants depend on layers 1–2 and
never on layer 3.

## Problem

Three concrete failures, all observed rather than hypothesised.

**1. Descendants must link the runtime to reach the resolver.** The
launch-tree → `SystemModel` pipeline lives inside play_launch's `play_launch`
crate, alongside `member_actor`, `web` and the ROS graph client. nano-ros wants
only the pipeline. Before the `runtime` feature gate landed (2026-07-27), a
nano-ros helper linking it required `rclrs` and `play_launch_msgs` — and
`play_launch_msgs` is not a registry crate at all: `colcon-cargo-ros2`
generates it from the ament environment, which is why it is pinned `"*"`. So
"use the resolver" implied "install ROS and run play_launch's colcon setup",
for users nano-ros targets who have neither.

**2. The spec is vendored twice.** nano-ros carries
`packages/cli/third-party/ros-launch-manifest`, and play_launch carries
`src/ros-launch-manifest`. The two pins drift. During the 0285 work this
produced three separate incidents, including one where play_launch's own source
referenced a `ros-launch-manifest` field its pin predated.

**3. The contract is a CLI surface.** Because the pipeline is not linkable,
nano-ros shelled out to a `play_launch` binary found on `$PATH` — where an
unrelated ROS 2 record/replay tool of the same name won, and every platform's
`build-examples` died inside a cmake configure (issue 0285). Issue 0293's
`system.toml` divergence is the same disease: two implementations of one
contract, drifting because nothing links them together.

## The boundary that matters

Not "spec versus implementation", and not "ROS-related versus not" — almost
everything here is ROS-shaped. The load-bearing question is:

> Can this be linked into a portable binary with no CPython, no ROS install
> and no colcon?

That line falls in a specific place, and the code already respects it.
`ros-launch-manifest-sched` documents the contract on `MapperInput`:

> *"already flattened by the caller against the launch DAG … tie-breaks are the
> caller's job — it has the DAG; this crate stays FQN-string-based and
> dependency-free"*

So the scheduling algorithms never see ROS launch data. Two adapters feed them:
`play_launch/ros/sched_loader.rs` (from a launch tree) and
`nros-orchestration-ir/mapper_input.rs` (from a `SystemModel`). That is
dependency inversion, already built and already relied upon — which is why
layering these repos introduces no cycle. Verified: **rlm has zero dependencies
on play_launch.**

## Design

### Layer 1 — `ros-launch-manifest` (unchanged repo, unchanged contents)

Spec, theory, and the algorithms and checkers over them.

| crate | LOC | role |
| --- | --- | --- |
| `types` | 4027 | format AST + the parser for **its own** manifest format |
| `model` | 2572 | `SystemModel` schema + placement semantics |
| `sched` | 3983 | scheduling spec + mappers (`chain_aware_rank`) |
| `check` | 7062 | contract checker (z3) |

`docs/` already holds `contract-theory.md`, `contract-verification.md` and
`scheduling.md` — the proofs live with the spec they discharge.

**`check` stays here** even though it carries z3, and the reasoning is worth
recording because the intuition points the other way. rlm is a four-crate
workspace, so a consumer that depends on `model`/`sched`/`types` never builds
z3 — verified: no nano-ros crate depends on `ros-launch-manifest-check`, and z3
never enters the `nros` binary's graph. The cost is paid by whoever *runs* the
checker, which is layer 2 either way (`nros-launch-resolve`'s lock already
contains `z3`, because a `SystemModel` is by definition a checked one). Moving
`check` would relocate source without removing weight, and would separate the
contract theory from the code that discharges it — the exact split that
produced issue 0293.

**`types::parse` also stays.** It parses rlm's *own* manifest format. The
ROS-coupled parser is `play_launch_parser` (launch XML / `.launch.py`, pyo3),
which is a layer-2 concern.

### Layer 2 — `ros-launch-resolve` (new)

The ROS-coupled adapter: a launch tree in, a checked `SystemModel` out. Needs
CPython for `.launch.py`; needs no ROS graph, no generated messages, no colcon.

```
ros-launch-resolve/
├── parser/      play_launch_parser (pyo3)      launch XML / .py -> LaunchDump
├── resolve/     ~12.3k lines lifted from play_launch:
│                  manifest_loader 3701 · sched_loader 3118 · model_builder 1280
│                  sched_derive 955 · chain_checks 826 · causal_graph 727
│                  manifest_graph 494 · launch_dump 353 · causal_dag_global 331
├── cli/         the resolve / contract / dump / plot verbs (~1.0k)
└── third-party/ros-launch-manifest    (the one rlm pin)
```

That this is separable is not a guess: the `runtime` feature gate added on
2026-07-27 already builds exactly this subset with
`--no-default-features`, and `nros-launch-resolve` links it today to resolve
real launch files with no ROS environment present. The feature flag becomes
unnecessary once the boundary is a crate boundary — it was simulating one.

### Layer 3 — `play_launch` (existing repo, narrowed)

The Linux runtime and its binary, focused on ROS execution:

```
member_actor 6160 · execution 3414 · runtime_enforcement 1893 · monitoring 1827
web 1807 · interception 861 · diagnostics 373
commands: run 653 · replay 1405 · signal_handler 578 · manifest 350
+ play_launch_{msgs,container,interception}, spsc_shm, wasm_*, vendor/*
```

It depends on layer 2 for resolution rather than owning it.

### Layer 4 — consumers

nano-ros pins **only** `ros-launch-resolve`; rlm arrives transitively. Its
helper binary (`nros-launch-resolve`, issue 0285) keeps its role and its
absolute-path invocation. `nros-macros` continues to link rlm's `sched`
directly, so the proc-macro path stays light.

## Consequences

- nano-ros's pins go 2 → 1, and `vendor/{ros2_rust, rosidl_runtime_rs,
  rcl_interception_sys}` plus `play_launch_msgs` leave its graph entirely.
- The colcon prerequisite disappears for descendants instead of being
  feature-gated around.
- One rlm pin in the tree below any consumer, so the drift class in problem 2
  cannot recur by construction.
- play_launch gets smaller and its purpose becomes statable in one line.

## Invariants to preserve

Two properties, both learned by violating them this week:

1. **Layer 2 must resolve under plain `cargo`.** play_launch's workspace cannot
   (`composition_interfaces` and friends are colcon-generated), which means a
   path dependency there cannot be `cargo test -p`'d from a consuming
   workspace. Code moved into a crate that cannot be tested standalone silently
   loses its tests — `input_path_string` was written twice for this reason
   before landing in rlm where it runs.
2. **`build_model` separable from `build_checked_model`,** so a consumer that
   cannot tolerate z3 can opt out at the point the cost is paid, without a
   repository move.

## Alternatives considered

**Move `sched`/`model` into play_launch.** Rejected: `nros-macros` calls
`chain_aware_rank` at proc-macro time in every nano-ros build, so play_launch
would become a compile-time dependency of every user, dragging pyo3, z3 and
colcon into a proc macro.

**Keep two layers, rely on the `runtime` feature.** This works today but leaves
the resolver's dependencies a property of a flag rather than of the package
graph — one `default = ["runtime"]` away from silently re-coupling, and it does
not address the double-vendored spec.

**Publish rlm to crates.io instead of vendoring.** Complementary, not
alternative, and worth doing later: rlm is dependency-light and would remove
submodule pins for the spec layer entirely. Out of scope here because it
constrains release cadence.

## Decisions (2026-07-28)

**1. rlm is vendored as a submodule; publishing is deferred behind a trigger.**

None of rlm's four crates is publishable as written — none carries a `license`,
and only `model` carries a `description`; crates.io requires both. Beyond
metadata, publishing imposes a release cadence, and rlm's schema moved four
times in two days (`DeployBlock.launch`, `machine=` placement,
`input_path_string`, ordered param sources). Each would have needed a version
bump and a publish before any consumer could move.

The drift that motivated this RFC came from **two copies**, not from vendoring:
three layers leave exactly one, inherited transitively, so the failure mode
closes either way. Metadata is added now as prep; the trigger to publish is W4
closing plus the 0293 SSoT work landing — i.e. when the schema stops moving
weekly.

Consequence to accept with it: nano-ros links rlm directly as well
(`nros-macros` → `nros-orchestration-ir` → `sched`), so until rlm is published
that path-dep runs through
`third-party/ros-launch-resolve/third-party/ros-launch-manifest`. One copy, no
drift, but an ugly path — and the strongest argument for publishing sooner
rather than later.

**2. The parser keeps its name, and the premise for renaming was wrong.**

An earlier draft of this RFC said renaming "touches the pyo3 module name". That
is not true: the Rust crate (`crates/play_launch_parser`) and the Python
extension (`crates/python`, package `play_launch_parser_python`) are separate
crates, and the resolver depends only on the former. They rename
independently.

The actual objection is ownership. `play_launch_parser` is its own repository
(`jerry73204/play_launch_parser`), vendored by play_launch and now by this
layer — it was never part of play_launch. It has Python users importing
`play_launch_parser` directly. Renaming an upstream project to suit our
repository layout would break them for our convenience. If the name should
change, the honest form is a neutral `ros_launch_parser` decided upstream on
its own timeline, not folded into phase-312.

**3. play_launch keeps a `resolve` verb that delegates to layer 2, with a
deprecation line.**

nano-ros no longer calls it, but simple-autoware-safety-island does — and not
only in prose: the command is embedded in
`sentinel_bringup/launch/pilot.launch.xml` as the documented recipe for
regenerating that model. Since play_launch depends on layer 2 after W3 anyway,
forwarding costs almost nothing, while dropping the verb breaks a live
workflow.

Issue 0285 is the argument from the other side: that whole bug presented as
`unrecognized subcommand 'resolve'` surfacing inside a cmake configure.
Removing a verb people invoke reproduces exactly that failure shape with us as
the cause. The verb stays, prints one line naming `ros-launch-resolve`, and is
dropped in a later release once the downstream recipe has moved.

## Open questions

1. **When to publish rlm to crates.io.** Trigger recorded above; the deciding
   input is whether nano-ros's nested path-dep proves painful enough to pull it
   forward.
