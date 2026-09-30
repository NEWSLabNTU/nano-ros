---
id: 1453
title: "`check-template-copy-out` sat red on `main` unseen because it runs on
  `schedule` only — the defect's class is gated cheaply now, the lane's
  reachability is an open cost decision"
status: open
type: tech-debt
area: testing, build
severity: medium
found: 2026-09-22
related: [issue-1108, issue-1429, issue-1226, issue-0319, issue-1040, issue-1412, phase-452]
---

## What was measured

`just check template-copy-out` was **RED on clean `origin/main`, on three of its
six buildable templates** — `pure-c-workspace`, `c-and-cpp-mixed-workspace`,
`multi-node-workspace-cpp`, i.e. every template a user copies out which builds a
C or C++ workspace. Every one of them died the same way:

```
thread 'main' panicked at packages/rmw/zenoh/nros-zpico-build/src/runner.rs:304:23:
NROS_DECLARED_TL_PUBLISHERS="" is neither a count nor `refused`.
```

That defect is archived issue 1429 and was fixed in PR #1151: an UNSET cmake
property leaves `get_property`'s output variable **undefined**, so an unquoted
`if(NOT _tl STREQUAL "")` compares the literal string `"_tl"` against `""`,
which is never equal — the guard that exists to SKIP an absent fact takes its
branch and emits a carrier with no value.

**Nobody saw the red**, and the reason is placement, not attention.
`template-copy-out` is a member of the `check build` gate list
(`just/check/fixtures.just`), and `check build` runs on `schedule` /
`workflow_dispatch` only — the step in `.github/workflows/gate.yml` is named
"just check build (nightly / manual only)". No `pull_request` and no
`merge_group` event asks the question, so the three templates a stranger copies
were unbuildable on `main` and every merge-gating lane stayed green over it.

This is issue 1226's shape exactly (a gate that WORKS is not a gate that RUNS)
and issue 0319's before it (a backend's own suite that `just check` never
reached, red on main for two days).

## Why it matters more than an ordinary red lane

`examples/templates/` is the one part of the tree whose contents are **copied by
a stranger**. Being wrong there is replicated rather than merely observed: the
user's first build of their own project fails, in code they now own, with a
panic naming an environment variable they have never heard of. That is the
argument `scripts/check-template-copy-out.sh` was written on (issue 1108), and
it is undiminished by the gate existing — an unreachable gate protects nobody.

It also has no signal capacity while red. A uniformly-red lane cannot report a
regression, because the new failure looks exactly like yesterday's; that is the
mechanism by which issue 0876 rode into the nightly unnoticed.

## The class is gated now — cheaply, and that is the good half

PR #1151 did not stop at the site. `check-declared-fact-carriers` gained **rule
4 (VALUED)**: a `get_property` output variable may not be compared against `""`
unquoted. Checked as the IDIOM rather than as a name, with five self-test cases
including the two-character difference between the broken and the correct
spelling.

That rule runs on the **fast line**, statically, in milliseconds, and it is
merge-gating. So the specific defect cannot recur silently, and it cannot recur
at any of its siblings either — the sweep found three sites sharing the idiom,
two of which were correct only by accident (one because every road emits a node
count, one because an abstaining road sets a `..._UNKNOWN` companion and the
`AND` short-circuits).

**This is the right shape of answer and it is already taken.** It is recorded
here so that nobody re-opens this issue believing the 1429 class is unguarded.

## The residual question — defects that are NOT in a checkable class

Rule 4 covers one cmake idiom. What `check-template-copy-out` catches is the
category "a template a user copies does not build", and the members of that
category found so far were each unreachable by any static predicate:

* an unresolvable `<depend>nano-ros</depend>` in two `package.xml` files — a
  name that is not even a legal ROS package name, over which two static gates
  and a colcon-parity job had been green for the template's whole life (issue
  1108);
* a Rust manifest path-depping five levels up into this checkout, so the
  template built in place and nowhere else (same first run);
* the sibling lane's first real finding: `check-scaffold-builds` caught the Rust
  component template still built on the `Component*` trait family retired in
  212.N.12, **broken for three and a half months** (#1412).

Those are what an end-to-end build finds and a source-reading gate does not.
Nothing in the tree currently asks that question on any event that gates a
merge, so the exposure window for the next one is a nightly's latency at best
and "until somebody reads the nightly" in practice.

## Why it cannot simply be made merge-gating

Two constraints, and they are not the same constraint.

**1. Cost.** The gate really builds: it copies each template out of the TRACKED
file set into a temp dir and runs `nros sync` + `nros build --workspace` on it,
six times. It is costed two ways in the tree and they disagree, which is itself
worth knowing: `just/check/fixtures.just` records **~10 min per template**,
while the run behind this filing put the whole set nearer **~20 min warm**.
Either way the unit is tens of minutes, and that is before provisioning — a C or
C++ workspace template needs the in-tree `nros` CLI built, the launch resolver
built, and the vendored `-sys` sources present.

**2. Lane contracts.** `check-lane-contracts` enforces the rule that came out of
the 2026-08-28 incident: **a gate in an affordability tier may only resolve
artifacts the JOB ITSELF builds.** `check-build` was on the merge group once and
could never pass there, because it resolved generated bindings and prebuilt
`.compile-ok` stamps that no CI job built — which left the required check red
for EVERY pull request for a day. A required check that cannot produce a pass is
a deadlock, not a slow gate.

Here the two constraints meet in a way worth stating precisely, because the
tree's two notes about it read as contradictory and are not:

* `just/check/fixtures.just` says the gate "resolves nothing it did not build
  itself, so it satisfies `check-lane-contracts` on any lane that can afford
  it";
* PR #1151's own note says it "cannot join an affordability tier".

Both are true. The gate is lane-contract-CLEAN *provided the job pays for the
CLI, the resolver and the submodule sources*, and that payment is exactly what
an affordability tier is defined as not doing. So the blocker is not a rule
forbidding the placement — it is that the placement is only legal at a price,
and nobody has decided whether the price is worth paying.

## The candidate that was suggested and not implemented

A **`merge_group`-only SUBSET: one template, `pure-c-workspace`** — not all six.

The case for it: the three reds were ONE defect with one cause, and
`pure-c-workspace` exhibits it. A single C workspace template is the cheapest
carrier of the "a copy-out template does not build" question, so one sixth of
the cost buys most of the class. The mechanism already exists and costs nothing
to build — `scripts/check-template-copy-out.sh` takes template names as
positional arguments (`check-template-copy-out.sh [--list | --self-test]
[template-name ...]`), so a subset lane is an argument, not a new code path.

The case against, stated so it is not lost:

* A subset is a **reach narrower than the rule** (issue 0196), and this
  neighbourhood has been bitten by that repeatedly. Whichever five templates are
  left off the merge lane keep exactly today's exposure, and the next defect has
  five places to land where the nightly is still the only reader.
* It still costs the provisioning, which is most of the fixed overhead. One
  template does not cost one sixth of six templates — the CLI build, the
  resolver build and the submodule checkout are paid once either way.
* A partially-covered class invites the reading that the class is covered. The
  honest alternative is "not merge-gating, and we know it" rather than
  "merge-gating for one of six".

Other roads deliberately not costed here, so the decision has a shape rather
than a single option: run the full set on `merge_group` and accept the batch
latency (the queue's whole economic argument is that expensive verification runs
once per batch rather than once per push, and the measured L1 figure in
`.github/workflows/gate.yml` — 587 s of an 878 s gate — is precedent for
tolerating minutes there); or keep it nightly and make the nightly's verdict
*reach* someone, which is what `just nightly-triage` exists for and is a
different repair to a different defect.

## This is a cost decision, and it is not taken here

No recommendation, on purpose. The inputs are: the exposure is a template a
stranger copies; the specific 1429 defect is already gated statically at no
cost; the unguarded remainder is the end-to-end class, whose three known members
were each invisible to static analysis; and the price of merge-gating any part
of it is tens of minutes plus provisioning per batch.

Whoever takes it should also decide the reporting half, because a nightly gate
that nobody reads and a merge gate that everyone waits on are the two ends of
the same trade, and there is a middle (a `schedule` lane whose red is surfaced
the way `just queue-triage` surfaces an ejection) that costs no build time at
all.


## A second, different template is red — and the lane DOES run on push now (2026-09-28)

`multi-package-workspace` fails, on two consecutive `host-tests` runs of the
PUSH event, inside `just ci tier1`:

| run | integration job | elapsed | gate |
| --- | --- | --- | --- |
| 36451336301 | 108974046015's lane, 16:51:39 → 18:51:04 | 1 h 59 m | `FAIL (template-copy-out, rc=1, 1555109ms)` |
| 36466773816 | 109078713781, 18:51:07 → 21:15:08 | 2 h 24 m | `FAIL (template-copy-out, rc=1, 1767314ms)` |

Both print the same one-line verdict, with the other five templates passing:

```
check-template-copy-out: building 6 template(s) from a copy of the tracked file set
  c-and-cpp-mixed-workspace: OK — 2 artifact(s) ...
  local-msg-package: OK — 2 artifact(s) ...
  multi-node-workspace: OK — 1 artifact(s) ...
  multi-node-workspace-cpp: OK — 1 artifact(s) ...
  multi-package-workspace: FAIL — the copy does not build
```

Two things this changes about this issue as filed.

**The reachability premise moved.** This issue says the gate "runs on `schedule`
only", which is why it sat red unseen. It is now reached from `just ci tier1` on
the **push** event, via `host-tests.yml` — which is how these two were found. So
the lane is no longer invisible; what it is not is FAST (the gate alone took
1,555 s and 1,767 s, inside a job that runs ~2 h), and it sits behind the disk
pressure of issue 1353 in the same job.

**The template set that fails is different.** The three this issue measured
(`pure-c-workspace`, `c-and-cpp-mixed-workspace`, `multi-node-workspace-cpp`)
died on archived issue 1429's `NROS_DECLARED_TL_PUBLISHERS=""` carrier and now
pass. `multi-package-workspace` is a new red with a different shape.

**The cause, reproduced locally.** The sentence that stood here said the log was
"truncated by the log writer rather than by the build" and that the cause was
unknown. The truncation was neither — it was **this gate**, on purpose:

```sh
sed -n '1,12p' "$log" | sed 's/^/      /' >&2
```

The first twelve lines of a copy's build are `nros sync`'s progress, so the
twelve shown were always preamble and the error — which cargo, cmake and the
CLI's own refusals all put LAST — was never among them. Both CI runs carried
their reason below the cut. This commit prints the last 40 lines instead.

With the tail visible, `multi-package-workspace` fails at dependency resolution:

```
Error: 2 <depend> name(s) resolve to nothing:
  cmake — declared by …/src/pkg_c_talker/package.xml, …
  nros  — declared by …/src/pkg_c_talker/package.xml, …
```

and the mechanism is the copy-out itself. `[prereq.nros]` **does** exist in
`nros-sdk-index.toml` with `role = "package"`, which the refusal's own rule says
is sufficient, and `[prereq.cmake]` exists with `role = "buildtool"`. Neither is
found because the index is discovered by **walking ancestors** of the workspace
(`store::lock_path_for` over `PIN_FILE_NAMES`), and the copy is
`/tmp/nros-template-copy-out.*/copy/examples/templates/<tmpl>` — no ancestor of
it holds an `nros-sdk-index.toml`. Verified: the copy's root contains `examples`
and nothing else.

`store.rs` names this blind spot twice in its own doc comments — *"in-tree every
ancestor of a test's cwd is this checkout, which HAS an `nros-sdk-index.toml`, so
a cwd-keyed derivation could never observe the fallback here."* Copying out is
the one place in the tree where that stops being true, which is exactly what this
gate exists for.

Why the other five templates pass: they declare only `std_msgs` (plus
`ament_cmake`/`rclcpp`/msg packages in `local-msg-package`), all of which resolve
as message packages `nros sync` generates or from the ambient ROS install.
`multi-package-workspace` is the only template that exercises prereq resolution
at all.

**Two defensible fixes, and this commit chooses neither.** Either the template
should not declare `nros`/`cmake` — the five that work declare neither, including
`c-and-cpp-mixed-workspace`, which also builds C and C++ — or an `nros` invoked
outside any checkout should carry its own index, in which case `[prereq.nros]`
having `role = "package"` is the intent and the discovery walk is too narrow. A
real user copying the template out has no index above it either way, so the gate
is reporting a genuine user-facing breakage rather than an artefact of being
copied. Deciding between them is a design call for whoever owns RFC-0098 D3's
`<depend>` contract.

**A precondition here reads as a template defect, three times over.** Getting to
that error took three local runs, each stopped by a different missing tool and
each reported identically as `multi-package-workspace: FAIL — the copy does not
build`: no `nros-launch-resolve` beside the CLI; then a stale in-tree CLI,
because initialising the `play_launch` submodule moved a pin that is a CLI build
input (issue 0409/1018). The gate already special-cases a missing `nros` with a
named remedy and a comment saying why that distinction matters — *"the first
spelling reported 'the copy does not build' … which blames the template for a
missing tool"* — and the same class reaches it through two more doors. The tail
now makes each self-describing, which is the cheap half; a precondition arm for
the resolver would be the thorough half and is not in this commit.

**What this is NOT**: not issue 1353. Both runs also end with the disk at 100 %,
and that is what misled the first reading (retracted in 1353's own text): tier 1
builds the world, so 100 % is what the run DOES, not what failed.

## 2026-09-30 — seen on `host-tests`, and NOT separable from the disk exhaustion beside it

`host-tests` run **36679366972** (push, 06:39), job **109771243653**, step 15
`just ci tier1`:

```
===== FAIL (template-copy-out, rc=1, 846088ms) =====
  local-msg-package: FAIL — the copy does not build
```

The gate ran for 14 minutes and then failed on this template. What it does not
give is a compiler error: the captured output ends

```
cargo:rerun-if-env-changed=ARFLAGSerror: recipe `build` failed on line 233 with exit code 1
```

— a build-script line and the recipe's own failure concatenated with no
diagnostic between them, which is the truncation shape of issue **1353**. One
line earlier in the same job:

```
##[warning]You are running out of disk space. … Free space left: 0 MB
```

timestamped within a second of the gate's verdict.

So this instance is **not** evidence that the copy-out template is broken. It is
consistent with that, and equally consistent with a build that was killed by a
full disk, and the log cannot distinguish them. Recorded so nobody reads it as a
reproduction. The sibling `host-tests` red the same night (run **36672424283**,
job **109750091198**, `Free space left: 33 MB`) failed on a different gate
entirely, `workspace-features` — two different first failures under the same
disk pressure, which is what 1353 does to a lane.

A clean measurement of this issue needs a run with disk headroom.
