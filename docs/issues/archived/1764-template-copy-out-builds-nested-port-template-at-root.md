---
id: 1764
title: "`check-template-copy-out` runs `nros build` at the root of `cpp-port-minimal-publisher`, whose images live in per-board sub-projects, so `check::build` is red"
status: resolved
type: bug
area: [tooling, examples]
severity: medium
found: 2026-10-09
related: [phase-482, issue-1782, issue-1296, issue-1308, issue-1108, issue-1077]
resolved_in: "check-template-copy-out builds each template sub-project from a copy (issue 1764)"
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

**Between filing and this fix, main turned the gate green without checking
either sub-project.** phase-483 W1 (PR #1835) changed the discovery
predicate to "some `system.toml` says `board = \"native\"`", which classified
`cpp-port-minimal-publisher` as skipped and printed one plain line. That
made `check::build` green, but it checked neither sub-project, the skip was
not in the `nros_check_skip` ledger, and the gate still built at the template
ROOT. A template with a native image in a sub-project would have hit the same
"declares no `[image.*]`" error.

**The unit is now the PROJECT.** `scripts/lib/template_projects.py` decides
which project a `system.toml` belongs to. Under a `src/` component it belongs
to the directory above `src/`, which is a workspace bringup and the same place
`nros build`'s discovery looks. Anywhere else it belongs to its own directory,
which is a single-package leaf. The gate copies the whole template, because
`mps2-an385-freertos/` reaches `../src/minimal_publisher.cpp`, and builds in
the project's directory inside the copy:

| project | road | here |
| --- | --- | --- |
| the six workspace templates | `nros sync` + `nros build --workspace` (unchanged) | OK |
| `cpp-port-minimal-publisher/mps2-an385-freertos` | the leaf's own CMake with its board's `[board.cmake] toolchain_file` | OK, one ARM ELF |
| `cpp-port-minimal-publisher/zephyr` | none derivable (issue 1782) | NOT VERIFIED, in the ledger |

The cross road reads its parameters from data. `nros ws board-facts` gives the
descriptor and the SDK roots the leaf's own `[board_config.*] sdk` names. The
descriptor gives the toolchain file. `nros_cmake_toolchain_resolved_cc`
(issue 0706's probe) checks whether the cross compiler resolves. `nros build`
is not used for a single-package CMake leaf, because that road is issues
1296/1308. Measured on a copy: `nros build` fails at configure with "no
SystemModel".

**Missing preconditions are named skips.** These go through
`nros_check_unverified`: an `{env:VAR}` SDK root that is unset, an SDK root
that is empty (an uninitialised submodule), a toolchain that resolves no
compiler, and no Zephyr workspace. Each is recorded in the ledger and becomes
a FAIL under `NROS_CHECK_SKIP_STRICT=1`. All four were exercised. A board with
neither a cmake toolchain nor a west board is a FAIL, so a new kind of
sub-project forces a decision. A whole-tree run that built no project at all
is also a FAIL.

**The Zephyr sub-project cannot be derived from its data.** This was
measured, then filed as issue 1782. Its `board = "zephyr"` lowers to
`native_sim/native/64`, where a copy fails to compile. `mps2_an385` with only
`prj.conf` also fails. Only the README's `-b mps2/an385` with its three
Kconfig fragments builds.

**Controls.** `--self-test` gains two arms:

- The project rule is checked on synthetic paths and on the real tree.
- A cross-arm negative control appends a missing source to the FreeRTOS leaf
  and must see a FAIL that names that source. A failure for some other reason
  does not count.

Mutation check: making `project_of` return `.` for every manifest restores
the 1764 behaviour. The gate then goes red ("no system.toml at
…/cpp-port-minimal-publisher"), and so does the self-test (three
`SELF-TEST FAILED` lines).

**Class sweep.** All 12 templates were checked. Six are workspaces whose
bringups sit under `src/` and are built at the root. One is the port template
with two sub-projects. Four declare no image and are listed as skipped:
`rclcpp-compat-smoke`, `topic-state-monitor-port`, `workspace-shadowing` and
`zephyr-byo`. The last two port templates in phase-482 W3 will be discovered
when they gain sub-projects.

Run on 2026-10-10 in a worktree with the FreeRTOS submodules initialised and
no Zephyr workspace: `just check template-copy-out` reports 7 projects built
and 1 NOT VERIFIED, rc 0.

### Record: main's interim rule, superseded by this fix

Kept as history. Before this fix landed, `main` resolved the issue with the rule below. The project-unit fix above replaces it: the host-board rule skipped `cpp-port-minimal-publisher` without checking either sub-project.

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

#### Evidence for the interim rule (2026-10-10)

| run | rc |
| --- | --- |
| `--list` | 0: the same 6 buildable; `cpp-port-minimal-publisher` skipped, no host image |
| mutation: `^board = "native"` back to `^\[image\.` (applied and checked with `bash -n`), gate on `cpp-port-minimal-publisher` | 1, `the copy does not build` (the original red) |
| restored | `just ci gate` on PR #1838's branch (see the PR) |
