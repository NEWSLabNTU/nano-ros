# Phase 567 — a release ships only what the slow lanes have seen

**Status (2026-11-16). OPEN: W1–W6 are open.** Approved by the user on
2026-11-16 ("Take your advice on regression prevention. Have a phase doc to
track the change"), after a review of the CI workflows on main against that
day's regressions. Implements no RFC; it closes gaps between the lanes that
already exist. Owners: nros 243 (W1–W3), NXP 175 (W4), nros 133 (W5a),
nros 118 (W5b, W6).

## Why

The merge queue's one required check, `CI` (`gate.yml`'s `ci-ok`), runs
compile, unit, lane-contract and source gates, plus the template copy-out,
scaffold journey, colcon parity and SDK-index jobs. Everything that boots an
image runs later: the nightly (05:00), tier 1 and tier 2 (`run-matrix.yml`,
06:00), the weekly book walkthroughs, and `merge-runtime.yml` on dispatch only.
That split is the user's ruling of 2026-11-12 (runtime cadence for linux,
zephyr and threadx-linux is daily), and this phase does not reverse it.

What the split leaves open is the release. `release-nros.yml` cuts from any
commit whose `CI` passed, so a regression the daily lanes find a day later can
already be in a published release. On 2026-11-16 every regression found got
past the merge queue:

| regression | where it could have been caught | state on 2026-11-16 |
| --- | --- | --- |
| executor backing RAM in C/C++ images | merge queue | fixed: nm step in the merge queue |
| a fresh C template refuses its own nucleo build (#4031 moved the walk) | merge queue: the copy-out builds the FreeRTOS C template but walks only a Rust row | W5a |
| `<param from>` a sibling package; a raised cap refused after link | no lane: no in-repo project has the island's shape | W4 |
| starter images failed at publish (a book block split) | release dry run | fixed (#4029): fast context check, dry run writes the context |
| a release re-cut pulled in an unannounced breaking change | release process | W2 |
| two images failed to build in the nightly drift job, log not printed | nightly | fixed: the job tails the failing build's log |
| a nightly fixture looked up under the wrong profile dir | tier 1 / nightly | fixed (workspace fixture lookup applies the platform profile) |

## Work items

### W1 — a release cuts only from a commit the slow lanes passed

- **Status:** OPEN

`release-nros.yml`'s first step refuses unless the nightly, tier 1 and tier 2
are green on the commit being released, or on an ancestor with only
documentation changes since. "Documentation only" is `scripts/ci/gate-tier.py`'s
docs predicate, the same one the changelog audit uses, never a second spelling.
The refusal names the run it read and the lane that is missing or red.

Acceptance: a dry run on a commit with no green nightly is refused with that
message; a dry run on a commit one docs-only commit past a green nightly
passes; a self-test covers both, and a red tier 2 is refused even when the
nightly is green.

### W2 — the release branch is frozen after its first cut

- **Status:** OPEN

After `release/<ver>` is cut, fixes are cherry-picked onto it; it is never
re-cut from main. The release workflow refuses a branch whose history is not
its first cut plus cherry-picks (each with a `(cherry picked from commit …)`
trailer whose source is on main).

Acceptance: a re-cut branch is refused with a message naming the commits that
are not cherry-picks; AGENTS.md's release section states the rule.

### W3 — the dry run is a rehearsal of everything except the push

- **Status:** OPEN

The dry run builds the starter images without pushing them, and runs the
starter replays against the images it built. Only the registry push, the tag
and the GitHub release are left for publish.

Acceptance: the nros11 failure (a book block the image reader could not join)
reproduced on a dry run fails there; a dry run's job list differs from a
publish's only by the push, tag and release steps.

### W4 — a consumer-shaped workspace in the tree

- **Status:** OPEN

One in-repo workspace shaped like the safety island: a `<param from>` file in a
sibling package (absolute and `$(dirname)`-relative), mixed C and Rust
components, pools raised above their derived counts, a board build and a
census. It builds in `check-workspace-all` (the merge queue) and in the release
dry run. Every layout a downstream project reports that the tree did not cover
is added to it in the same PR as its fix.

Acceptance: reverting the `<param from>` input-recording fix or the raised-cap
fix fails the merge queue on this workspace.

### W5 — cheap merge-queue additions

- **Status:** OPEN

- **W5a.** A C stack-walk row for the FreeRTOS C template that
  `check-template-copy-out` already builds, run with `--only` in the existing
  queue drift step. Needs the FreeRTOS cmake road to export compile commands
  first (the stack-floor fix's item (a), in flight). Acceptance: reverting the C template's stack re-measure
  fails the merge queue; warm and cold walk times recorded.
- **W5b.** The tier-2 build-and-link job in `queue.yml` stays advisory, but its
  failures reach the PR author the way a merge-queue ejection does
  (`queue-notify.yml`), so a link break is heard about at merge rather than at
  the next nightly. Acceptance: a seeded link failure on a queue batch posts a
  comment on each PR in the batch.

The nucleo Zephyr template walk stays nightly (it needs the Zephyr SDK); moving
it into the queue is a separate user decision.

### W6 — a nightly red names its suspects

- **Status:** OPEN

On a red nightly, tier 1 or tier 2, `just nightly-triage` (and its
`matrix-triage` sibling) lists the commits merged since that lane's last green
run, with each commit's PR and author session, and the triage posts it where
the team reads it. Today this is done by hand.

Acceptance: a red run's triage output lists exactly the commits between the
last green run and the red one; a run with no prior green says so rather than
listing all history.

## Not in this phase

- Per-merge runtime boots for linux, zephyr and threadx-linux: the 2026-11-12
  ruling stands.
- Replacing `config/stack-floors.toml`'s per-combination rows with per-layer
  declarations: raised with the user on 2026-11-16, not yet decided.
