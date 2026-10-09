---
id: 1764
title: "`check-template-copy-out` runs `nros build` at the root of `cpp-port-minimal-publisher`, whose images live in per-board sub-projects, so `check::build` is red"
status: resolved
type: bug
area: [tooling, examples]
severity: medium
found: 2026-10-09
related: [phase-482, issue-1108, issue-1077]
resolved_in: "33a72e1116 (phase-483 W1): template-copy-out counts only host (`board = \"native\"`) images"
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

## Resolution

Fixed on `main` by `33a72e1116` (phase-483 W1), which landed while a separate
fix for this issue was in review. `image_declaring_manifest` now counts a
template only when one of its `system.toml` files has a HOST image
(`board = "native"`). This lane builds on the host with `nros build
--workspace`, and `cpp-port-minimal-publisher`'s two images are both
cross-board (FreeRTOS, Zephyr). So the template is reported skipped with that
reason ("declares no host (board = "native") image"), and its leaves stay with
their own lanes.

The template's README agrees, and it is the authority on how a user copies the
template out: `cmake` at the root, and `cmake` (FreeRTOS) or `west` (Zephyr)
in the sub-projects. It never says `nros build`. The fixtures agree as well:
the root is a `cmake-configure` compile-check row, `mps2-an385-freertos/` is a
`cmake` fixture row, and `zephyr/` is a `west` row. A second measurement
points the same way: `nros build --workspace` on a copy of
`mps2-an385-freertos/` fails at its generated workspace's configure, because
it is not an `nros build` project. So giving the template a root that declares
its images would have described a road the README does not offer, and the gate
was the thing to change.

The other branch (PR #1838) used a different rule, which was also correct: a
manifest counts only if `nros build`'s package walk can reach it, since
`provider_scan` stops at a package. It was dropped in favour of `main`'s rule,
which selects the same 6 templates. The host-board rule also covers a cross
image that IS reachable, which the reach rule did not. Two rules for one
selection would be the thing to avoid.

### Evidence (2026-10-10, on `main`'s rule)

| run | rc |
| --- | --- |
| `--list` | 0: the same 6 buildable; `cpp-port-minimal-publisher` skipped, no host image |
| mutation: `^board = "native"` back to `^\[image\.` (applied and checked with `bash -n`), gate on `cpp-port-minimal-publisher` | 1, `the copy does not build` (the original red) |
| restored | `just ci gate` on PR #1838's branch (see the PR) |
