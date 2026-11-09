---
id: 2391
title: "Tutorial chapter 10's `ros2 topic echo` asks a `ros2` daemon left over from chapter 9, which fails with `!rclpy.ok()`"
status: resolved
type: bug
severity: medium
area: [book, probe]
related: [phase-548]
found: 2026-11-10
---

**Home: [phase-548](../../roadmap/phase-548-three-languages-every-board.md)**, W8,
which replays every starter and Tutorial track with its `awaits` lifted.

## Measured

The `tutorial-10` probe track (installed track, `ros:humble`, asset built from
`w8-zr-try`) fails at step 1030 of `book/src/tutorial/talk-to-ros2.md`, twice
in a row on 2026-11-10:

```
File "/opt/ros/humble/lib/python3.10/site-packages/ros2topic/verb/echo.py", line 146, in choose_qos
...
xmlrpc.client.Fault: <Fault 1: "<class 'RuntimeError'>:!rclpy.ok()">
```

Every step before it prints its documented output, step 1020's
`I heard: reading 2` included, so the board and its router are up.

## Cause

`ros2 topic echo` with no `--qos-*` asks the `ros2` daemon what QoS the
publisher offers. The daemon was started by an earlier chapter and still holds
a session to a router that `nros run` has since replaced; asked, it answers
`!rclpy.ok()`. Chapter 9 (`parameters-and-launch-files.md`, step 760) already
says this and passes `--no-daemon`; chapter 10 does not.

## Fix

Step 1030's `ros2 topic echo` and step 1040's `ros2 topic info -v` take
`--no-daemon`, as chapter 9's graph queries do (issue 1333), and the prose
says why. Not `ros2 daemon stop`: the daemon is keyed on `ROS_DOMAIN_ID`
alone, so stopping it ends one other sessions on the host may share (issue
0763). Step 1050's `ros2 topic pub` rejects the flag and needs no graph query.

## Verified

With the fix committed on the try branch, `tutorial-10` replays green
(`PROBE PASS`, 2026-11-10). The track's `awaits` (issue 2350) is a separate
matter, left to phase-548 W8.
