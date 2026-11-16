# Phase 565 — a lane's claims are the one provisioning list

**Status (2026-11-16). OPEN: W1 and W2 are done; W3–W6 are open.** Implements
[RFC-0131](../design/0131-gates-are-nextest-tests.md), and closes its one open
question. Assigned to nros 118 by the nros 241 manager on 2026-11-15, which
relayed the user's approval of RFC-0131 and asked for phase docs that implement
what is still open in it.

## Why

RFC-0131 D3 makes a lane a capability claim: `.config/gate-lanes.toml` says
what each lane's environment provides, a claims preflight refuses a lane whose
host provides less, and CI provisions from the claims
(`scripts/ci/provision-gate-lane.sh`). Its open question is the one place this
does not hold: a contributor's worktree. `just setup-worktree` reads its own
list, `.config/worktree-provisioning.txt`, so the `dev` lane's claims and what
a worktree is given can disagree, and nothing would say so.

The measured cost, 2026-11-15. Several fresh agent worktrees were refused at
`pre-push` until both `just setup-cli` and `just setup-worktree` had run. The
order mattered: `setup-worktree` checks out `play_launch`, whose pin is a CLI
build input (issue 0561), so it re-staled the CLI that `setup-cli` had just
built. The recipe's own comment records the workaround (`setup-cli` before AND
after) as something "learned by failing".

The same measurement found three smaller gaps between RFC-0131's D1–D6 and
main. Each is an item below.

| gap | measured on main, 2026-11-15 |
| --- | --- |
| a worktree's provisioning is its own list | `.config/worktree-provisioning.txt`, read by `justfile` (`setup-worktree`), `just/ci.just`, `just/check/platform.just`, `provision-gate-lane.sh`, `gate-lane-preflight.py`, `check-store-source-readers.py`, `submodule-provision.sh` |
| `setup-worktree` provisions one claim of nine | `[lane.dev] claims` = cli, cmake, cc, bindgen, ros2, colcon, sources, submodules, git-history; the recipe provisions `submodules` (and their store-first sources) only |
| a declared lane no job runs | `[lane.queue]`: no workflow sets `NROS_GATE_SCOPE: queue` |
| lanes CI runs without the lane provisioner | `runner` (merge-runtime, queue, run-matrix ×3) and `merge` (merge-runtime) set `NROS_GATE_SCOPE` and never call the `provision-gate-lane` action; only `fast` and `build` do |
| a capability nothing needs | `nuttx`: `check-gate-needs` prints "NO LANE CLAIMS `nuttx` … no gate needs it" (`fsp` is claimed by no lane too, and that is D3's stated case: a provider no runner has, shown as running nowhere) |

## Work items

### W1 — one provisioning table, read by one helper

- **Status:** DONE (2026-11-16)

Done: the 11 submodule rows are `[[provision.submodule]]` in
`.config/gate-lanes.toml` (`path`, `tiers`, `reads`), and
`scripts/lib/lane_provision.py` is their one reader (`submodules [--tiers
...]`, `lane <name>`). `setup-worktree`, `_require-worktree-provisioning`
(and so the lane preflight) and `provision-gate-lane.sh` call it;
`check-store-source-readers` and `submodule-provision.sh` name the table.
`.config/worktree-provisioning.txt` is deleted. Gate:
`check-lane-provisioning-single-source` (fast tier). Measured: the helper's
lists for `fast`, for `fast build` and for every row equal the old file's,
by name and in order. Not part of W1: the two workflows' own submodule lists
(`gate.yml`, `live-peer.yml`) are W4's, and RFC-0131's open question is
W6's.

What each claim provisions moves into `.config/gate-lanes.toml`. The submodule
rows of `worktree-provisioning.txt` keep their per-tier tags and reasons. One
helper, `scripts/lib/lane_provision.py`, reads the table, and every current
reader goes through it: CI's provisioner, the preflight, `setup-worktree`,
`check-store-source-readers` and `submodule-provision.sh`.
`.config/worktree-provisioning.txt` is deleted.

Acceptance:
- The file is gone.
- No tracked file outside the helper parses a provisioning list. A gate
  (`check-lane-provisioning-single-source`) refuses one; its self-test's
  negative control is a second reader of the table.
- `provision-gate-lane.sh fast` and `provision-gate-lane.sh build` provision
  the same submodules as before, compared by name.

### W2 — `just setup-worktree` provisions the `dev` lane's claims

- **Status:** DONE (2026-11-16)

Done: every capability has a `[provision.claim]` entry in
`.config/gate-lanes.toml`: a `remedy`, and a `provisioner` when nano-ros can
install it (`submodules`, `git-history`, `zephyr-tree`, `cli`, `sources`,
`arm-gcc`). `[provision] order` runs them CLI inputs first, then `cli`, then
`sources`, which needs the CLI. `scripts/lib/claim-provision.sh` holds one
`claim_<name>` per provisioner, and both `just setup-worktree` (lane `dev`)
and `scripts/ci/provision-gate-lane.sh` (`--ci`: depth 1, and the `cli`
claim, marked `ci = false`, left to the job's own step) run it. Host tools are
named with their remedy, never installed. The lane preflight takes its
remedies from the same table, and `setup-worktree` ends with the `dev`
preflight. Gate: `lane-claims-provisioned`, whose negative control is a claim
added to `dev` with no provisioner and no host remedy; it also refuses either
caller dropping the library. Measured on a fresh `git worktree add` at the
change: `just setup-worktree` alone (2 min 32 s) checked out all 11 submodules,
built the CLI and the resolver, provisioned the rosidl clone, passed the `dev`
preflight, and left `check cli-fresh` OK.

`setup-worktree` walks `[lane.dev] claims` and, for each claim, either runs that
claim's provisioner (the same code CI's provisioner runs) or names the host
tool it cannot install and its remedy (`cmake`, `cc`, `ros2`, `colcon`;
**never sudo**). It ends with the claims preflight for `dev`, so a worktree
learns every unmet claim at once, the way a lane does.

Acceptance:
- On a fresh `git worktree add`, `just setup-worktree` alone leaves a tree
  where `pre-push` (`check fast`) passes its preflight and `check cli-fresh`
  is OK. No other setup step is needed.
- A gate compares `setup-worktree`'s claim set with `[lane.dev] claims` by
  deriving both from the table, so the two cannot disagree. Its negative
  control is a claim added to `dev` with no provisioner and no named host tool.

### W3 — provisioning never re-stales what it provisioned

- **Status:** OPEN

The order is part of the claim table, not of a recipe's comment. A submodule
that is a CLI build input (`play_launch`, issue 0561) is provisioned before the
`cli` claim, and the `cli` claim is provisioned last, through `setup-cli`'s own
freshness question (`source-stamp`).

Acceptance:
- On a fresh worktree, `just setup-worktree` followed by `just setup-worktree`
  rebuilds nothing the second time.
- `check cli-fresh` is OK after one run.
- The comment in `setup-worktree` about "`setup-cli` before AND after" is
  deleted with the workaround.

### W4 — every lane runs somewhere, provisioned by its own claims

- **Status:** OPEN

The `queue` lane is either run by the job that runs on its claims
(`zephyr`, `west`, `build-tree`: the queue's L3 job) or deleted, with the
reason recorded in RFC-0131 D3. The `runner` and `merge` jobs provision
through the `provision-gate-lane` action, or each records why its job provides
its claims another way.

Acceptance:
- A gate (an extension of `check-lane-contracts` or a new
  `check-gate-lane-jobs`) holds three things:
  - every `[lane.<name>]` is the `NROS_GATE_SCOPE` of at least one job;
  - every such job runs the lane provisioner, or carries a named reason;
  - every `NROS_GATE_SCOPE` names a declared lane.
- Its self-test covers an unrun lane and an unprovisioned job.

### W5 — no capability that nothing needs

- **Status:** OPEN

`nuttx` is claimed by no lane and needed by no gate, so it is vocabulary with
no subject. Delete it, or give it the gate that needs it. `fsp` stays: D3
names it as the visible "runs nowhere" case.

Acceptance:
- `check-gate-needs` prints no "no gate needs it" line.
- The capability check refuses a listed capability that no gate needs and no
  lane claims. A ratchet with an empty baseline is enough.

### W6 — RFC-0131 records the answer

- **Status:** OPEN

RFC-0131's open question 1 moves into D3 as decided: the claims are the one
provisioning list, and `setup-worktree` is the `dev` lane's provisioner.
`implements-tracked-by` names this phase.

Acceptance:
- The RFC has no open question that this phase answered.
- `check-rfc-tracked-by` (or the equivalent RFC/phase link gate) passes.

## Acceptance (phase)

- Every item DONE.
- A fresh worktree needs exactly one setup command before its first push.
- No provisioning list exists outside `.config/gate-lanes.toml`.
- Every declared lane runs in some job, and that job provisions from the lane's
  claims.

## Not in scope

- What a runner IMAGE contains. A missing system package is an image fix
  (issues 1457/1482), and a claim the image already meets has no provisioner
  here.
- Sharing build output between worktrees (phase-559 W10).
