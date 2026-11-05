---
id: 2275
title: "`just probe installed` stops at first-project.md's `nros run`: configure refuses an unset `AMENT_PREFIX_PATH`"
status: resolved
type: bug
area: probe, cli, book
purpose: [education]
related: [phase-530, phase-527, phase-447, phase-532]
found: 2026-11-05
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)** — the implicit configure inside `nros run`.

## Measured

`just probe installed` (track `first-project`) on branch
`phase-527-w6-release-underlay`, rebased onto `main` at the phase-530 W1
merges (2026-11-05). The asset built and installed; the chapter's
`nros setup mps2-an385-baremetal --rmw zenoh` and `nros new sensor_node
--board mps2-an385-baremetal --template talker` passed; step 50,
`nros run --expect 'Publishing:'`, exited at once with:

```text
Error: configure: `AMENT_PREFIX_PATH` is unset and no configure of `/root/sensor_node` has recorded one to reuse. Run the setup step once from a shell that has ROS 2: `source /opt/ros/<distro>/setup.bash` (or your overlay's `install/setup.bash`),
Location:
    nros-cli-core/src/cmd/configure.rs:383:13
```

The same branch BEFORE that rebase passed the whole track (chapter, then the
phase-527 W6 underlay checks, then a native talker `nros run`). The branch's
diff touches neither `configure` nor the chapter, so this is main's.

The probe's user container is `ros:humble` started with `--entrypoint ''`,
so ROS is installed but not sourced, exactly as the chapter's reader is
until its "See it from ROS 2" step: first-project.md sources ROS only for
`ros2 topic echo`. A bare-metal talker's message crates come from the
release's bundled interfaces, so a configure that requires
`AMENT_PREFIX_PATH` refuses a build that does not need it.

## Next

Decide which is wrong: the chapter (source ROS before `nros run`), or
configure (fall back to the toolchain's bundled interfaces when
`AMENT_PREFIX_PATH` is unset, as the pre-configure `sync` verb did). Then re-run
`just probe installed`.

## Resolution (2026-11-05)

**Maintainer ruling: the chapter was wrong, not configure.** Configure keeps
its refusal of an unset `AMENT_PREFIX_PATH` when the project has messages to
generate (`configure.rs`, phase-530 Ruling R44); there is NO fallback to the
release's bundled interfaces. Message code is generated from the host's ROS
install because that is what keeps the message layout and the type hash —
part of the key expression in `rmw_zenoh` from Jazzy on — compatible with the
ROS peer. A bundled copy would silently pick a distro.

The chapter fix is on `main`: `book/src/get-started/first-project.md` sources
ROS (`source /opt/ros/humble/setup.bash`) before its first `nros run`, on
both the QEMU and the Linux variant, and its troubleshooting list names this
refusal (35494e2a4a, phase-532; 3e85895960, "building needs a sourced ROS 2,
once per project").

Not re-measured by the session that archived this: `just probe installed`
(track `first-project`) after those commits. Reopen if it still stops at
step 50.
