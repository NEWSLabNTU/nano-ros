---
id: 2882
title: "No release past nros11: a consumer on the released CLI cannot take any fix since 2026-11-16, and no item tracks the next release"
status: open
type: chore
severity: medium
area: [release, cli]
related: [phase-478, 2645, 2770, 2771, phase-567]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
I42. Consumer: the safety island (autoware-safety-island fork), branches
`nano-ros` (release) and `nano-ros-next` (main).

## What happens

The latest release is `nros-0.6.0-nros11` (`09219edc75`, 2026-11-16). Main is
1062 commits past it (2026-11-23, `a1cbbb6b04`). An nros12 candidate was cut
and blocked once (phase-567: a red `host-tests` run, 2026-11-18); no open
issue or phase item tracks cutting the next one, so "when does a consumer get
these fixes" has no answer in the tree.

A consumer that follows the book (`book/src/guide/build/install-options.md`,
"Pinning a version") runs the released prebuilt CLI. The island does: its
default branch pins `0.6.0-nros11` in `actuation_module/nros-toolchain.toml`,
and `scripts/nros-cli.sh` refuses a `modules/nros` that is not that tag's
commit.

## Island workaround

Two branches. Everything that needs main waits on `nano-ros-next`, where
`modules/nros` is a main commit and `scripts/nros-cli.sh` builds the CLI and
the launch resolver from source on every CI run (with `NROS_SKIP_STALE_CHECK`
on a clean checkout). Waiting there (the island's phase-15 doc, "Pin on
nano-ros-next" and "Dropped on nano-ros-next"): the parameter, logging,
clock, network, CAN, census and test-lane work of its steps P3-P11, built on
nano-ros fixes 2629-2634, 2636-2640, 2645, 2678, 2679, 2690, 2702, 2752,
2754, 2756, 2805, 2822, 2861-2865 and phase-582.

A commit pin (issue 2645, `nros pin --rev`, phase-478 I26 and I34) would let
the default branch track main without a source build, but the prebuilt nros11
launcher cannot read one, so it too waits for a release.

## Wanted

The next release cut (nros12 or later) once the release lanes are green
(`just check release-lanes-green`), with the date or the blocking lanes
recorded here while it is pending.

## Acceptance

- A release past nros11 is published with its prebuilt CLI.
- The island moves `nros-toolchain.toml`, `modules/nros` and `west.yml` to it
  together (or to a commit pin, I26), folds `nano-ros-next` into its default
  branch, and deletes the source-CLI path's use in CI.
