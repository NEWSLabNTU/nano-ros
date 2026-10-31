---
id: 2176
title: "The book's FVP, PX4 SITL and real-hardware flash lines are run by no probe"
status: resolved
type: test-gap
severity: low
area: [testing, docs, installed-sdk]
related: [phase-517, 2169]
found: 2026-10-31
resolved: 2026-10-31
---

**Home: [phase-517](../../roadmap/archived/phase-517-user-flow-setup-build-flash.md)** — W9's **Left**: everything the installed probe
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

## Re-read against main, 2026-10-31

Two of the three lines changed standing on the day this was filed, so the
direction below is split by what each line is today.

* **FVP** — `fvp-aemv8r-smp` is a **Tier 3 build-only** board
  (`book/src/reference/board-support-tiers.md`): `execution_class =
  "hardware"`, its runtime maintainer-run through `just zephyr
  verify-fvp-runtime`, and `arm-fvp.md` itself says the standalone talker
  lanes were retired (issue 0537) and that support "is intended to grow back".
  An installed probe track would claim a verified run for a board whose tier
  says build-only, so no track until the board is promoted. What the chapter
  owes the reader is the statement that its run half is maintainer-run and
  unverified on the installed track.
* **PX4 SITL** — RFC-0113 D9 (2026-10-31) makes PX4 a Tier 1 framework with a
  GENERATED entry: phase-521 W8 replaces `examples/px4/cpp/*`, names PX4 SITL
  as the in-tree board and requires a delivering CI cell. `px4.md`'s
  `make px4_sitl_default` flow is rewritten by that item, so a probe track
  over today's text would replay a chapter about to change. The SITL track
  belongs to phase-521 W8's acceptance, and this issue holds no separate
  SITL work.
* **Flash to real hardware** — unchanged: a CI runner has no board.

## Fix direction

* `arm-fvp.md`: state that the run half is maintainer-run (`just zephyr
  verify-fvp-runtime`) and replayed by no installed probe; a track follows
  the board's promotion out of Tier 3, not the other way round.
* PX4 SITL: closed here into phase-521 W8 — its delivering cell is the SITL
  replay, and its chapter rewrite is where the probe tags go.
* Flash: have each chapter say the line is unverified until it has been run
  against a named board. If a self-hosted runner with hardware ever exists,
  its lane is where the line gets checked.

## Progress (2026-10-31, recorded before the re-read above)

The second fix direction has landed. Each line no probe runs now says so,
linking here: the FVP run (`arm-fvp.md`), the PX4 SITL builds (`px4.md`,
`integration-px4.md`), and the flash lines (`bare-metal.md`,
`integration-nuttx.md`, `integration-zephyr.md`, `px4.md`). `arm-fvp.md`'s
"Installer surface" also stops describing a gated, download-free model; the
download is `nros setup --tool arm-fvp`. At the time, the FVP and SITL
probe tracks were still listed as open; the re-read above
withdraws both. Each was a build-out of its own:

* FVP is a Cyclone DDS image on an aarch64 Zephyr model with its own
  networking, and the only worked example is in-tree
  (`examples/workspaces/realtime-cpp`'s `[image.fvp]`), not an installed
  scaffold.
* SITL is a full PX4 build.

## Resolution (2026-10-31)

Closed the way the fix direction says, with no new probe track:

* `arm-fvp.md` now states, in its status box and under "Run on the FVP", that
  the run half is maintainer-run (`just zephyr verify-fvp-runtime`) and
  replayed by no installed probe, and that a track follows the board's
  promotion out of Tier 3.
* PX4 SITL is phase-521 W8's: that item rewrites `px4.md` around the generated
  entry and requires a delivering SITL cell, which is the replay. `px4.md`'s
  flash section says so.
* Every flash line — `integration-zephyr.md`, `freertos.md`, `bare-metal.md`,
  `integration-nuttx.md`, `px4.md` — now carries the sentence that no probe
  runs it (a CI runner has no board) and that it stands unverified until run
  against a named board, next to the QEMU line the weekly tracks do boot.

The mps2-an385-zephyr half of the same audit is [issue 2177](2177-mps2-an385-zephyr-network-unmeasured.md),
closed separately.
