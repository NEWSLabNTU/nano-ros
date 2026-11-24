---
id: 2909
title: "A workspace with an image whose SDK is absent never reads as configured, so every bare `nros configure` redoes every image"
status: resolved
type: limitation
area: [cli, build, ux]
severity: medium
found: 2026-11-23
resolved: 2026-11-24
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

## Resolution

Fixed 2026-11-24. The refusal stamp the direction asks for already existed
(Ruling R15 records a skip stamp; R19 re-checks it with the image's preflight
and stales it once the preflight passes). The defect was that a missing
Zephyr never reached it:

- Planning refused a Zephyr image on a host with no Zephyr tree with a plain
  `eyre::bail!("no Zephyr found, ...")`. That is not a `PreflightRefusal`, so
  `nros configure` filed it under R22 ("not configured", no stamp) and
  re-planned the workspace on every bare configure. The live-peer fixture
  log of run 44672701128 shows the line for each Zephyr image 51-52 times.
- Now it is a typed `ZephyrAbsent` (same message, so `nros build <image>`
  prints what it did), and one helper, `preflight::host_refusal`, decides
  "this host cannot build the image" for both the configure loop and the
  R19 re-check. A stage-3 `PreflightRefusal` or a `ZephyrAbsent` is
  skipped with a stamp; any other planning error is still R22's.
- The R19 re-check runs stages 1-3 only (`preflight_query`), which passed
  without a Zephyr, so the first fix alone read "toolchain now present"
  every run. The workspace road's stage 3 now also refuses a West-driven
  image with no Zephyr tree in `Planning::Preflight`.

Measured on `examples/workspaces/island-shaped` (3 images, one Zephyr), with
Zephyr hidden by `NROS_ZEPHYR_VERSION=9.9`:

| run | result |
| --- | --- |
| 1 (fresh) | 217 s; `zephyr: skipped -- missing a Zephyr tree (run: nros setup zephyr, or pass --zephyr-workspace <dir>)` |
| 2, nothing changed | 1 s, `configure: up to date (3 image(s))` |
| 3, Zephyr found (variable unset) | 3 s, `zephyr: toolchain now present`, configured |
| 4, nothing changed | 1 s, `up to date` |

The skip line still prints on every configure that plans the image. Test:
`a_missing_zephyr_is_a_host_refusal_and_other_errors_are_not` (a
`ZephyrAbsent` through `wrap_err` is a host refusal and keeps its full text;
a `PreflightRefusal` gives its summary; any other error is none).
