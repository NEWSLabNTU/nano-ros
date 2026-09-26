---
id: 1475
title: "`check-copy-out.sh` defaults to `../nano-ros-workspace-4.4`, which `just
  zephyr setup` has not written for some time — so the nightly copy-out check fails
  on a path rather than on the copy-out it exists to test"
status: resolved
type: bug
area: ci, zephyr, testing
severity: medium
found: 2026-09-24
resolved: 2026-09-26
related: [1474]
---

## What happens

`nightly` run **35958890602** (schedule, 05:12), job **107502987723**
(`zephyr copy-out check (4.4)`), step `Copy-out build check`:

```
./scripts/zephyr/check-copy-out.sh "c/talker" "zenoh" "native_sim/native/64"
FAIL: Zephyr 4.4 workspace not set up at ../nano-ros-workspace-4.4
  run: NROS_ZEPHYR_VERSION=4.4 just zephyr setup
error: recipe `check-copy-out` failed on line 162 with exit code 1
```

The workspace **is** set up. The same job's earlier step provisioned it, and
its own log says where:

```
Workspace: /github/home/.nros/workspaces/zephyr/4.4
```

`scripts/zephyr/check-copy-out.sh:59` reads

```sh
WORKSPACE_DEFAULT="../nano-ros-workspace-4.4"
WORKSPACE="${NROS_ZEPHYR_WORKSPACE:-$WORKSPACE_DEFAULT}"
```

and nothing in `nightly.yml` sets `NROS_ZEPHYR_WORKSPACE`, so the default is
what runs. Setup writes workspaces under `$HOME/.nros/workspaces/zephyr/<line>`;
the sibling-directory layout the default names is a older convention.

## Why it matters

The job is named for the copy-out check and never performs one. Its advice
(`run: NROS_ZEPHYR_VERSION=4.4 just zephyr setup`) is also wrong for this
failure, because setup already ran and succeeded — a reader who follows it
gets a second workspace in the same place and the same message.

## What this is NOT

- **Not a setup failure.** Setup completed; the workspace and its SDK are
  present at the path quoted above.
- **Not issue 1474.** That is `cargo clippy` missing a component during a
  fixture build on a different lane. This job never reaches a build.
- **Not the `env.sh` heredoc defect** fixed alongside this filing. That one
  made setup emit a broken environment script while still exiting 0; it is
  upstream of nothing here, since `check-copy-out.sh` does not source `env.sh`.
  Both were found in this job, which is why they are easy to conflate.

## What would close it

Decide who states the workspace path, then make the two agree:

1. **The caller states it** — `nightly.yml` exports `NROS_ZEPHYR_WORKSPACE`
   for this step, from the same value the setup step used. Narrow, and leaves
   the stale default for the next caller to trip on.
2. **The script asks** — resolve the workspace the way setup does
   (`$HOME/.nros/workspaces/zephyr/<line>`, honouring the same overrides), so
   the default is derived rather than written down twice.

Acceptance is the job performing a copy-out build and reporting on it.

## UNMASKED — this is now the whole of the job's failure (2026-09-25 nightly)

Nightly **36097564895** (the 05:12 zephyr line), job **107952943423**,
`zephyr copy-out check (4.4)`:

```
./scripts/zephyr/check-copy-out.sh "c/talker" "zenoh" "native_sim/native/64"
FAIL: Zephyr 4.4 workspace not set up at ../nano-ros-workspace-4.4
  run: NROS_ZEPHYR_VERSION=4.4 just zephyr setup
```

The setup step **succeeded** immediately above it, and said where the workspace
actually is: `/github/home/.nros/workspaces/zephyr/4.4` — venv created, west and
zephyr-build present, all five 4.4 NSOS patches applied. So the job now provisions
correctly and fails only on the path this issue is about.

That matters for two reasons.

**It confirms the diagnosis rather than restating it.** On the 2026-09-24 nightly
this same job died earlier, in `create_env_script`, on the unquoted heredoc that
executed its own comment text — a different defect, fixed in `f3a55d21f`. While
that stood, the stale default was unreachable and this issue was an inference
from reading the script. It is now the observed failure.

**It is the last wall for this cell.** The 05:12 nightly ran 23 green, 5 skipped
and exactly ONE failure, and this is it. Closing 1475 should turn the job green
or produce the first real copy-out verdict — which is the acceptance already
written above, unchanged.

Nothing here changes either remedy.

## Fix applied 2026-09-25 — option 2, via the resolver that already exists

`scripts/zephyr/check-copy-out.sh` no longer writes a path down. It sources
`scripts/lib/zephyr-workspace.sh` — the ONE Zephyr workspace resolver
(RFC-0095 D4, phase-440 W1) — and asks it for the 4.4 line:

```sh
. "$NROS_ROOT/scripts/lib/zephyr-workspace.sh"
ZEPHYR_LINE=4.4
workspace="$(nros_zephyr_ws_resolve_abs "$ZEPHYR_LINE" "$NROS_ROOT" || true)"
```

That ladder is `$NROS_ZEPHYR_WORKSPACE` -> `$NROS_STORE/workspaces/zephyr/4.4`
-> the legacy sibling, so the tree `just zephyr setup` actually writes is now on
it and the sibling layout still resolves for a host provisioned before W4.
Option 1 (export the variable from `nightly.yml`) was not taken: it fixes this
caller and leaves the stale constant for the next one, which is the
second-spelling shape CLAUDE.md says this repo keeps paying for.

The remedy text was wrong for the failure it printed, so it moved too. The
check can now only fail here when nothing is provisioned at all, and it says
which candidates it tried:

```
FAIL: no Zephyr 4.4 workspace on the resolver ladder. Candidates tried:
    /nonexistent-store/workspaces/zephyr/4.4
    ../nano-ros-workspace-4.4
  run: NROS_ZEPHYR_VERSION=4.4 just zephyr setup
```

`.config/zephyr-workspace-resolvers.txt` shrinks by one line — its
`scripts/zephyr/check-copy-out.sh  # a WORKSPACE_DEFAULT constant for the 4.4
line, not a ladder walk` entry. The reason on that line was accurate about the
code and wrong about the consequence: a constant naming a rung of the ladder is
a spelling of the ladder, and this is what it cost. 28 spellings remain.

### Negative control

With a store populated at `$NROS_STORE/workspaces/zephyr/4.4/zephyr` — the CI
shape — the fixed script resolves it and moves on to the next precondition,
while the same environment against the previous revision reproduces the nightly
failure verbatim:

```
$ git stash && NROS_STORE=…/store bash scripts/zephyr/check-copy-out.sh
FAIL: Zephyr 4.4 workspace not set up at ../nano-ros-workspace-4.4
```

### Still open

Acceptance is unchanged and this does not meet it: a copy-out BUILD reported by
the `zephyr copy-out check (4.4)` job. That job has never performed one, so what
it finds past this rung is unmeasured — the next 05:12 nightly is the first run
that can say. Keep this issue open until it does.

## RESOLVED 2026-09-26 — the acceptance is met: the job built a copy-out and said so

Nightly run **36220045896** (schedule, 05:11), job **108343456536**
(`zephyr copy-out check (4.4)`): **success**, 14 steps, and the step that has
never once reached a build now reports one:

```
[copy-out] example   : examples/zephyr/c/talker  (zenoh, native_sim/native/64)
[copy-out] copied to : /tmp/nros-copy-out.zBWENx/talker  (OUTSIDE repo tree)
[copy-out] workspace : /github/home/.nros/workspaces/zephyr/4.4 (4.4 line)
Built: /tmp/nros-copy-out.zBWENx/build/zephyr/zephyr.elf
PASS: copied-out example built from OUTSIDE the repo tree via the nano-ros Zephyr module.
```

The `workspace :` line is the fix working: the resolver ladder picked the STORE
tree that `just zephyr setup` actually writes, where the retired constant named a
sibling directory nothing had created for some time.

Acceptance as written was "the job performing a copy-out build and reporting on
it". Met. And the thing the check exists for — the copy-out promise, that an
`examples/zephyr/<lang>/<example>` dir copied out of the repo still builds
against the nano-ros Zephyr module — is now EXERCISED rather than asserted: it
reached `zephyr.elf` from `/tmp`, outside the tree.

Note what this does and does not say about the lane. The same nightly run has
21 jobs `success`, 5 skipped and 0 failed so far, with only the tier-2 pairwise
job still running — so this job is no longer the thing hiding the rest. What it
found once it could run is that the contract holds; had it been broken, that
would have been a different issue, and the two were indistinguishable for as
long as the check died on a path.
