# Phase 567 — a release ships only what the slow lanes have seen

**Status (2026-11-16). OPEN: W4 and W6 are done; W1–W3 and W5 are open.** Approved by the user on
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

### W1 — a release cuts only from commits the slow lanes passed

- **Status:** OPEN

`release-nros.yml`'s first job, `lanes`, refuses unless the nightly, tier 1
and tier 2 are green, in scheduled runs on `main`, on each commit W2 names:
the cut and each cherry-pick's SOURCE commit on main. A commit also passes
when a green run's head is an ancestor of it with only documentation changed
since. The release head itself is not measured (user ruling 2026-11-16): the
lanes run on main and never on a release branch, so a cherry-picked fix is
measured where it landed. "Documentation only" is `scripts/ci/gate-tier.py`'s
`tier()`, the same one the changelog audit uses, never a second spelling.
The refusal names the lane, the run it read and why that run did not count.
(`scripts/release/release-lanes-green.py`, `just check release-lanes-green`.)

Acceptance: a dry run on a commit with no green nightly is refused with that
message; a dry run on a commit one docs-only commit past a green nightly
passes; a self-test covers both, and a red tier 2 is refused even when the
nightly is green.

### W2 — a published version's release branch is frozen

- **Status:** OPEN

User ruling 2026-11-16: `release/<ver>` may be re-cut from main under the
same version name while that version is unpublished, which is while no
`nros-<ver>` tag exists. Once the tag exists, the name is burned: the branch
is frozen at the tagged commit. Either way, every commit on top of the cut
must be a `git cherry-pick -x` of a commit on main, whose `(cherry picked
from commit …)` line is what the workflow reads. The refusal names the tag
and each commit that is not a cherry-pick.
(`scripts/release/release-branch-frozen.py`, `just check release-branch-frozen`.)

Acceptance: a re-cut of a published version is refused with a message naming
the commits that are not cherry-picks; an unpublished re-cut is allowed; a
self-test covers both; AGENTS.md's release section states the rule.

### W3 — the dry run is a rehearsal of everything except the push

- **Status:** OPEN

The dry run builds the starter images without pushing them, and runs the
starter replays against the images it built. Only the registry push, the tag
and the GitHub release are left for publish.

Acceptance: the nros11 failure (a book block the image reader could not join)
reproduced on a dry run fails there; a dry run's job list differs from a
publish's only by the push, tag and release steps.

### W4 — a consumer-shaped workspace in the tree

- **Status:** DONE (#4063). `examples/workspaces/island-shaped`, checked by
  `just check island-shaped-workspace` at the end of `check-workspace-all`.
  Building it found issue 2644 (a C + Rust image with no C++ source was
  refused at configure), fixed in the same PR.
- **How each layout is covered.** The `<param from>` files (issue 2615) and
  the C + Rust image are covered in the merge queue by a live configure,
  census and FreeRTOS board build. The raised-cap fix (issue 2618) lives only
  in `scripts/check-knob-delivery.py`, which reads a configured ZEPHYR build
  dir, and the merge queue has no west workspace (user ruling 2026-11-13). So
  the merge queue's coverage of it is a committed snapshot of the workspace's
  Zephyr build dir (`tests/fixtures/knob-delivery/island-shaped-zephyr`), and
  the live coverage is daily: nightly.yml's `template-copy-out-zephyr` job
  runs `just check island-shaped-zephyr`, which builds the Zephyr image,
  checks its live build dir, and fails when the snapshot differs from it. The
  FreeRTOS board build carries raised pools too, but no delivery check reads
  the CMake road, so issue 2618 is Zephyr-only: queue on the snapshot, daily
  on Zephyr.
- **Release dry run: not covered here.** `release-nros.yml` does not run
  `check-workspace-all` (its only reused workflow is `starter-image.yml`), so
  this leg rides on W3, whose dry run rehearses the publish.
- **Acceptance, measured locally** (the PR body has the output): reverting
  a002a58779's `model_gate.rs` hunk turns `just check island-shaped-workspace`
  red; reverting 2cde3951f2's `scripts/check-knob-delivery.py` turns it red;
  with both restored it is green.

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
  the next nightly. Acceptance: a replay against a real red L3 run posts to
  each PR in its batch.
  **W5b DONE (2026-11-16).** The notifier's not-an-ejection branch runs
  `scripts/ci/queue-l3-advisory.sh` for a failed `queue` run. It posts one
  advisory comment to each pull request in the merge group, with a per-run
  marker and `queue-triage`'s INFRA note. The batch is
  `scripts/ci/queue-batch.py`: the run's PR plus every entry down the
  `pr-<N>-<base>` chain still unmerged when the run started. A replay of real
  red L3 run 43836015138 named #4039 and #4038 and the failed step `just ci
  matrix build`. Selftests in `check queue-notify-decision`.
- **W5c.** The heap-free C++ image (`heap-free-cpp-poc-mps2`, thumbv7m, no C
  library) links in the merge queue. On 2026-11-16 it stopped linking
  (`undefined symbol: vsnprintf`, after issue 2634's fix put a printf-style
  log on its path), and only the advisory L3 saw it. Acceptance: dropping the
  bin's `libc-stubs` fails the merge queue.
  **W5c DONE (2026-11-17).** Gate `just check cpp-heap-free-image-link`
  (`needs:arm-gcc`, the pinned 13.2) runs as its own step on shard 2 of
  `check-template-copy-out`, which already provisions that compiler. Its cost
  is about 21 s cold (20.6 s wall, 61 s CPU on 32 cores) and 1 s warm. With
  `libc-stubs` dropped, it fails on `rust-lld: error: undefined symbol:
  vsnprintf`.

The nucleo Zephyr template walk stays nightly (it needs the Zephyr SDK); moving
it into the queue is a separate user decision.

### W6 — a nightly red names its suspects

- **Status:** DONE (2026-11-16)

On a red nightly, tier 1 or tier 2, `just nightly-triage` (and its
`matrix-triage` sibling) lists the commits merged since that lane's last green
run, with each commit's PR and author session, and the triage posts it where
the team reads it. Today this is done by hand.

Acceptance: a red run's triage output lists exactly the commits between the
last green run and the red one; a run with no prior green says so rather than
listing all history.

Done: `scripts/ci/lane_suspects.py`, printed by `nightly-triage.py` and
`lane-stage.py --history` (`just matrix-triage`) for their newest completed run
when it failed.
- The lane is a JOB, because the nightly as a whole had not been green in 30
  runs. Each failed job finds its own last green run on `main`; a run that did
  not schedule the job is not green for it. Jobs sharing one green share a
  block.
- Commits come from the compare API, since a CI checkout is shallow. Each
  carries its PR and its `Claude-Session:` trailer.
- No prior green is stated and lists nothing; a green and red on the same
  commit name no suspect. Both are self-test cases run on every invocation.

Measured: run-matrix `tier 2 (1-wise matrix)` last green 2026-11-05, 1225
commits back. FreertosMps2 had been red on every nightly since at least
2026-11-13.

## Not in this phase

- Per-merge runtime boots for linux, zephyr and threadx-linux: the 2026-11-12
  ruling stands.
- Replacing `config/stack-floors.toml`'s per-combination rows with per-layer
  declarations: raised with the user on 2026-11-16, not yet decided.
