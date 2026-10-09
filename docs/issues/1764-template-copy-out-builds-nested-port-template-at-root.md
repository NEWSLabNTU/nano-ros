---
id: 1764
title: "`check-template-copy-out` runs `nros build` at the root of `cpp-port-minimal-publisher`, whose images live in per-board sub-projects, so `check::build` is red"
status: open
type: bug
area: [tooling, examples]
severity: medium
found: 2026-10-09
related: [phase-482]
---

## What was measured

On 2026-10-09, `just ci gate` on a branch based on `8a16309bac` failed at step 4
(`check::build`). One gate of 25 failed, `template-copy-out`:

```
cpp-port-minimal-publisher: FAIL — the copy does not build
    Error: this workspace declares no `[image.*]`. An image is the buildable unit — see RFC-0065 D6.
    Location: nros-cli-core/src/cmd/build.rs:297:58
```

Commit `e57ca7d2be` (phase-482 W3) gave this template two sub-projects:
`mps2-an385-freertos/system.toml` (`[image.mps2-an385-freertos]`) and
`zephyr/system.toml` (`[image.zephyr]`). The template root has no
`system.toml`.

`image_declaring_manifest` in `scripts/check-template-copy-out.sh` searches
EVERY tracked `system.toml` under the template, so it classifies the template
as buildable. The build then runs at the copy's ROOT, where `nros build` finds
no image. The two halves of the gate disagree about where the project is.

The branch that hit this (issue 1759) does not touch templates or `nros build`'s
image discovery. `check::build` runs on no merge-gating event, which is how this
reached main.

## Direction

Make the gate build where the image-declaring manifest is (its directory), or
treat a template with no root `system.toml` as several projects. One caveat:
both sub-projects need a cross SDK (FreeRTOS, Zephyr) that this lane does not
provision. If a sub-project's SDK is absent, the gate must report it as a
named skip through the `nros_check_skip` ledger, never as a pass.
