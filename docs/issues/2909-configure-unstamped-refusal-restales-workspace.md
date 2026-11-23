---
id: 2909
title: "A workspace with an image whose SDK is absent never reads as configured, so every bare `nros configure` redoes every image"
status: open
type: limitation
area: [cli, build, ux]
severity: medium
found: 2026-11-23
---

## What happens

In a workspace that declares an image this host cannot configure (its SDK is
not installed), a bare `nros configure` never reports `up to date`. Every run
repeats C1 and stages 2-4 for every image in the workspace.

Measured on 2026-11-23 on `examples/workspaces/rust` (96 images). The stamp of
`action_bringup:zephyr_action_client` was moved aside to reproduce a host with
no Zephyr SDK. The workspace-wide configure then took 87 s, every time. With
the stamp in place it is a digest comparison.

The hosted live-peer runner is such a host. Its fixture script ran this bare
configure once per row, which cost 91-133 s per language-workspace row and
106-121 min of a 226-min fixture loop (runs 44456527279, 44672701128). That
was nros12 blocker B16, and #4715 works around it on the fixture side by
naming the row's image.

A user who builds only Linux images, in a workspace that also declares Zephyr
or NuttX ones, pays the same cost on every bare configure.

## Why

`cmd/configure.rs` `configure_project`:

- the currency check walks every target, and one stale key makes the whole
  project stale;
- Ruling R22: an image nobody named that fails stages 2-4 (a
  `PreflightRefusal`, such as a missing SDK) is reported and left unstamped,
  "so the next configure tries it again".

R22 is right that the image must be retried once its SDK appears. But
"retry" is implemented as "never current", so the cost lands on every image
and every run, not on that one image when something has changed.

`nros build <image>` is not affected. `configure_implicitly` passes the named
image, and `targets()` scopes the check to it (RFC-0120 D2).

## A direction (for the CLI owner)

Record a refusal stamp for an image skipped by a `PreflightRefusal`. It would
key on the image's ordinary inputs plus the refusal's precondition, e.g.
the SDK's resolved path or its absence as `nros locate` answers it. The
currency check then treats the image as current while nothing it depends on
has changed, and retries it as soon as the SDK appears or an input moves.
The report line (`configure: <image>: skipped — <why>`) should still print on
every run, so the skip stays visible.

Acceptance:
- In a workspace with an image whose SDK is absent, a second bare
  `nros configure` with nothing changed reports `up to date`.
- Installing (or pointing at) the SDK makes that image stale and configures
  it on the next run.
