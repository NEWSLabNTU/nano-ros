---
id: 2279
title: "configure's unset-AMENT_PREFIX_PATH refusal tells the reader to run `nros configure`, the command that printed it"
status: open
type: bug
area: cli
purpose: [education]
related: [2275, phase-530]
found: 2026-11-05
---

**Home: [phase-530](../roadmap/phase-530-nros-configure-single-pass.md)**.

## Measured

`packages/cli/nros-cli-core/src/cmd/configure.rs` (around line 383) refuses
with:

```text
configure: `AMENT_PREFIX_PATH` is unset and no configure of `<dir>` has recorded one to reuse. Run the setup step once from a shell that has ROS 2: `source /opt/ros/<distro>/setup.bash` (or your overlay's `install/setup.bash`), then `nros configure`; later runs without a sourced shell reuse what it records.
```

The reader usually meets this from `nros run` or `nros build`, which run the
configure implicitly: the book's first chapter does, and so does issue 2275's
probe. Two things read wrong:

- "Run the setup step" names no command. The fix is to source ROS, not to
  run `nros setup`, which a reader may take "setup step" to mean.
- "then `nros configure`" sends the reader to a command they did not type.
  When the refusal came from `nros configure`, it names the command that just
  failed.

## Next

Name the remedy as the command to retype. One wording:

```text
configure: `AMENT_PREFIX_PATH` is unset, and `<dir>` has no earlier configure that recorded one. Source ROS 2 in this shell (`source /opt/ros/<distro>/setup.bash`, or your overlay's `install/setup.bash`) and run `nros <verb that failed>` again; later shells need not source it.
```

The verb comes from the caller, which knows whether it is `run`, `build` or
`configure`. Keep the first-10-minutes entry in the book (#3132) in step with
the new text.
