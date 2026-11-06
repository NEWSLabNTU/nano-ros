---
id: 2260
title: "A humble-edition image and a Jazzy peer exchange nothing with no error anywhere, and `nros new` scaffolds `ros_edition = \"humble\"`"
status: resolved
type: bug
area: cli, interop, ux
purpose: [industrial, education]
related: [phase-523, rfc-0116]
found: 2026-11-05
resolved_in: "issue-2260 branch: scaffold follows $ROS_DISTRO; nros run warns on a mismatch"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I10/E8, first-project.md "See it from ROS 2": the board prints `Publishing: 'Hello World: N'`, `ros2 topic list` shows `/chatter [std_msgs/msg/String] 1 publisher`, and `ros2 topic echo /chatter … --qos-reliability best_effort` prints nothing, with no error on either side. Cost 8 min (education) before the edition was suspected.
- Setting `ros_edition = "jazzy"` in the scaffold, then sync and build, delivers at once.

The scaffold's default edition (humble) is one the book's own support page steers a 24.04 user away from (issue 2259), and the mismatch is silent (the type hash in the key expression differs, so nothing matches).

## Next

Scaffold the edition of the ROS 2 on the host when one is sourced, and make a mismatch observable (a warning when a matching topic with another type hash is seen, or a doctor check).

## Resolution (2026-11-06)

Both halves of "Next" are done:

- **Scaffold.** `nros new <ws> --board …` states the edition of the sourced
  ROS 2 when `$ROS_DISTRO` is one nano-ros generates for (humble, iron,
  jazzy). Otherwise it uses the default. `--ros-edition` still wins. The
  scaffold prints which edition it chose and why:
  `ROS 2 edition: jazzy (the sourced ROS 2, $ROS_DISTRO)`.
- **Mismatch.** `nros run` compares the image's resolved edition with
  `$ROS_DISTRO` (`[image]` > `[image_defaults]` > `[system]`, via
  `SystemToml::image_ros_edition`, carried on `ImageTables`). When they differ,
  it warns before the run:
  `nros run: warning: this image speaks ROS 2 humble, and the shell's ROS 2 is
  jazzy ($ROS_DISTRO): a jazzy node will see its topics and receive nothing,
  with no error on either side. Set ros_edition = "jazzy" …`

Measured on a scaffolded `mps2-an385-baremetal` workspace. Scaffolding under
`ROS_DISTRO=jazzy` wrote `ros_edition = "jazzy"`. Under an unset
`$ROS_DISTRO` it wrote humble and said it was the default. `nros run --dry-run`
on the humble image warned under `ROS_DISTRO=jazzy` and said nothing under
`humble`.

Left: a peer started from a different shell than `nros run` is still only as
visible as that shell's `$ROS_DISTRO`. The type hash makes a mismatch
invisible on the wire, and no runtime check sees one.
