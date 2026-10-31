---
id: 2176
title: "The book's FVP, PX4 SITL and real-hardware flash lines are run by no probe"
status: open
type: test-gap
severity: low
area: [testing, docs, installed-sdk]
related: [phase-517, 2169]
found: 2026-10-31
---

**Home: [phase-517](../roadmap/archived/phase-517-user-flow-setup-build-flash.md)** — W9's **Left**: everything the installed probe
tracks do not boot stays unverified.

## What

The 13 `probe-platforms.yml` tracks boot every QEMU and simulator line the
platform chapters show. Three kinds of line are run by nothing:

* **ARM FVP** (`book/src/getting-started/arm-fvp.md`, and the FVP rows in
  `integration-zephyr.md`). The model is a public CDN download
  (`[tool.arm-fvp]`, measured 2026-09-06), so this is not a licensing
  limit. Nothing has replayed the chapter.
* **PX4 SITL** (`px4.md`, `integration-px4.md`). The `make px4_sitl`
  flow is documented and never replayed on the installed track.
* **Flash to real hardware** (`west flash -d …`, openocd, PX4 `upload`,
  in `integration-zephyr.md`, `freertos.md`, `bare-metal.md`,
  `integration-nuttx.md`, `px4.md`). A CI runner has no board.

Issue 2169's check proves a printed run command equals its chapter's line
only where a probe boots that line, so these boards fall outside it.

## Fix direction

* FVP and SITL can run in a container. Give each an installed probe track,
  as `zephyr-rust` and `nuttx-arm-rust` got, and add their boot lines to
  2169's table.
* Flash cannot run in CI. Have each chapter say the line is unverified
  until it has been run against a named board. If a self-hosted runner with
  hardware ever exists, its lane is where the line gets checked.

## Progress (2026-10-31)

The second fix direction has landed. Each line no probe runs now says so,
linking here: the FVP run (`arm-fvp.md`), the PX4 SITL builds (`px4.md`,
`integration-px4.md`), and the flash lines (`bare-metal.md`,
`integration-nuttx.md`, `integration-zephyr.md`, `px4.md`). `arm-fvp.md`'s
"Installer surface" also stops describing a gated, download-free model; the
download is `nros setup --tool arm-fvp`. Still open are the FVP and SITL
probe tracks. Each is a build-out of its own:

* FVP is a Cyclone DDS image on an aarch64 Zephyr model with its own
  networking, and the only worked example is in-tree
  (`examples/workspaces/realtime-cpp`'s `[image.fvp]`), not an installed
  scaffold.
* SITL is a full PX4 build.
