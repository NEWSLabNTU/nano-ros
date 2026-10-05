---
id: 1592
title: "`examples/workspaces/features/src/custom_msgs` declares
  `rosidl_default_generators` and `rosidl_default_runtime`, which resolve to
  nothing on a ROS-less host — so the `native` module of the tier-2 build refuses
  the fixture"
status: open
type: bug
area: ci, examples, cli
severity: high
related: [1158, 1357, 0662, 0967, rfc-0033]
---

## What happens

`run-matrix` (tier 2, 1-wise) run **36825211926** (schedule, 2026-10-01T06:31),
job **110249254587**, step 6 `just build tier2`. The `native` module fails, and
the decisive error is in its own log (`native.log:1224`, reached from the
`build-workspace-fixtures` recipe):

```
  nros build demo_bringup:native_rust_qos --workspace . --offline -- …
Error: 2 <depend> name(s) resolve to nothing:
  rosidl_default_generators — declared by
    examples/workspaces/features/src/custom_msgs/package.xml
  rosidl_default_runtime — declared by
    examples/workspaces/features/src/custom_msgs/package.xml
…
  NROS_ALLOW_UNRESOLVED_DEPS=1  to continue with a warning.
Location: nros-cli-core/src/cmd/build.rs:3578:5
make[1]: *** [… ws-linux-….mk:19: ws-group-3] Error 1
```

The refusal lists what a name may be: a package in this workspace, a message
package `nros sync` generates, a `[prereq.*]` key in `nros-sdk-index.toml` whose
`role` is `package`, or a package the ambient ROS install provides. On this
runner there is no ambient ROS, and `rosidl_default_generators` /
`rosidl_default_runtime` are none of the other three.

## Why it matters

`custom_msgs` is an in-tree fixture package under
`examples/workspaces/features/`, so this is not a user's mistake — the repo ships
a workspace that cannot build on a host without ROS, and the tier-2 lane is such
a host. It takes `ws-group-3` down, which takes the `native` module down, which
is one of the three reasons that run produced no cell verdict (issue 1158).

It is also the second place the same mechanism has bitten: issue **1357** is a
`package.xml` declaring `std_msgs` with nothing to provide it, in the project
`nros new` scaffolds. Same refusal, same escape hatch, different declarant — one
authored by the scaffolder, one committed in `examples/`. A fix for either should
be checked against the other (the 0196 rule).

## What this is NOT

- **Not issue 1457.** That is `msg2idl.py failing on
  `builtin_interfaces/msg/Duration.msg`, an interpreter/importability failure
  inside codegen. This one never reaches codegen for the package: the dependency
  NAME does not resolve.
- **Not `NROS_ALLOW_UNRESOLVED_DEPS=1`.** Setting it in the lane would make the
  lane pass while leaving the fixture's declaration wrong, which is the inversion
  issue 1357's "What this is NOT" already argues against.
- Not the `leaf-fetch: could NOT warm the cargo cache` warning printed six lines
  above it (issue 0967). That warning is about `--frozen` downloads and names
  itself; the failure here is dependency resolution, before any download.

## What would close it

1. Decide what `custom_msgs` needs those two names FOR. A generated message
   package under `nros sync` does not need ament's rosidl generators declared —
   if the declaration is vestigial from a colcon-built ancestor, deleting it is
   the fix and the gate is that the workspace builds with no ROS.
2. If they are genuinely needed, they belong in `nros-sdk-index.toml` as
   `[prereq.*]` keys with `role = "package"`, which is what the refusal text
   already tells a reader — and then the ROS-less path must say so before the
   build, not three commands in.
3. Acceptance is the `native` module of a tier-2 build reaching a verdict with no
   `NROS_ALLOW_UNRESOLVED_DEPS` anywhere in the path, and a check that would catch
   a new in-tree `package.xml` declaring an unresolvable name.

## A second site: `workspace-shadowing` declares `rclcpp` (2026-10-05)

Scheduled `nightly` run **37267865689** (head `40fb98d8b`), job **111628381118**
"tier 2 nightly (pairwise cover)", step `just build tier2-nightly`. For the
first time in this episode every fixture module built (`== zephyr == OK` …
`== native == OK`, `All test fixtures built.`), and the lane reached the
compile-check stage (`compile-check: 51 unit(s), jobserver pool=20`). One unit
failed, and the jobserver stopped the rest:

```
== cmake-fixture: shadowing ==
Error: 1 <depend> name(s) resolve to nothing:
  rclcpp — declared by .../build/cmake-fixtures/shadowing/src/consumer/package.xml
make: *** [.../build/jobserver-pool/compile-check-2228265.mk:83: u25] Error 1
error: recipe `build-compile-check-fixtures` failed on line 1826 with exit code 2
```

The row is `[[compile_check_fixture]] id = "shadowing"` (`builder =
"cmake-configure"`, `dir = "examples/templates/workspace-shadowing"`), and
`src/consumer/package.xml` declares `<depend>rclcpp</depend>`. That is this
issue's class at a second site: a template's ROS-only `<depend>` resolves to
nothing on a ROS-less runner, so `nros build` refuses it. It was latent until
now because no earlier night got past the module stage.

**Reading note for this lane:** the job log does not name the failing unit.
The unit's `Error:` text is interleaved with 25 other units' output, and only
`make: *** [...] u25] Error 1` marks it; there is no per-unit status line. The
`fixtures built (...)` summary is printed once per finished unit (25 times
here), so a reader of the tail sees 25 successes followed by `exit code 2`.

Not issue 1665 (the Zephyr module built), not 1277 (esp32 built), not disk.
