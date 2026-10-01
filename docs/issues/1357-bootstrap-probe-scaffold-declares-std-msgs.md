---
id: 1357
title: "`nros new` scaffolds a project whose `package.xml` declares `std_msgs`, and on
  a host with no ROS nothing provides it — so `nros build` refuses the project the
  bootstrap probe just created, and the book's first-project flow fails on exactly
  the host it is written for"
status: open
type: bug
area: cli, book, ci
severity: medium
found: 2026-09-12
related: [0204, 1108, 0333, 1248]
---

## What happens

Nightly run **34680021029** (schedule, 07:10), job **103516832543**
(`bootstrap-probe`), step `Run bootstrap probe`. The probe runs the book's
quickstart in a clean container (issue 0204's whole point). Scaffolding and sync
both succeed:

```
nros new --workspace: scaffolded probe_quickstart (17 files, lang=cpp, rmw=cyclonedds)
sync: resolved system.launch.xml → /tmp/probe_quickstart/build/nros/models/demo_bringup/system_model.yaml
sync: source metadata — 2 rebuilt, 0 already current
sync: done.
```

and then the build refuses the project sync just finished preparing:

```
Error: 1 <depend> name(s) resolve to nothing:
  std_msgs — declared by /tmp/probe_quickstart/src/talker_pkg/package.xml,
             /tmp/probe_quickstart/src/listener_pkg/package.xml
  … NROS_ALLOW_UNRESOLVED_DEPS=1  to continue with a warning.
Location: nros-cli-core/src/cmd/build.rs:3110:5
```

The container has no ROS, and says so earlier in the same log:

```
activate.sh: /opt/ros/humble/setup.bash not found — ROS-dependent recipes will fail
```

## Why it matters

The bootstrap probe exists to answer one question: does the book's documented
flow work on a pristine host? Today the answer is no, and the thing that breaks
it is our own scaffolder — `nros new` writes two `package.xml` files declaring a
message package that, on a host with no ament install, nothing can resolve. A
user following the book gets this error on their first build.

The refusal itself is correct behaviour and worth keeping; issue 1108 is the
neighbouring case where a template materialises packages nothing consumes. What
is wrong is the pairing: the scaffolder emits a dependency the documented
environment cannot satisfy.

## What this is NOT

- Not `NROS_ALLOW_UNRESOLVED_DEPS=1`. Setting it in the probe would make the
  probe pass while leaving the user's first build broken, which inverts what the
  probe is for.
- Not the installed-path probe's failure in the same run (issue 1304) — that one
  is `nros setup` failing to read a gitlink sha, on a different job.
- Not a ROS-on-this-host question (issue 1248). The scaffolded project should
  either not need ROS, or the book should say it does before the user runs
  `nros new`; the build system's one environment question is whether ament
  packages are discoverable, and here the answer is legitimately no.

## What would close it

Decide which of these the quickstart is, and make the scaffolder and the book
agree:

1. **The quickstart does not need `std_msgs`.** Scaffold the talker/listener
   against a message type that resolves with no ROS — `nros sync` already
   generates message crates, so a workspace-local `.msg` is a candidate — and
   the probe passes with no environment change.
2. **The quickstart does need ROS.** Then `nros new` should say so at scaffold
   time, naming the missing prereq, and the bootstrap probe should provision it
   rather than discovering the gap three commands later.

Acceptance is `just probe bootstrap` green on a container with no ROS, reached
through the book's own blocks, with no `NROS_ALLOW_UNRESOLVED_DEPS` anywhere in
the path.

## The acceptance as written is now MET, by work that never named this issue (2026-10-01)

This issue's acceptance is *"`just probe bootstrap` green on a container with no
ROS, reached through the book's own blocks, with no `NROS_ALLOW_UNRESOLVED_DEPS`
anywhere in the path."* Measured on nightly run **36829686786** (schedule, 07:19,
head `c7db50ad6`), both front doors:

| job | id | conclusion | `NROS_ALLOW_UNRESOLVED_DEPS` in the log |
| --- | --- | --- | ---: |
| `bootstrap-probe` (checkout track) | 110263209711 | **success** | **0 occurrences** |
| `installed-probe` (no checkout) | 110263209769 | **success** | **0 occurrences** |

Both green on the previous night too (run **36682994178**), and the probe was
`failure` on run **36535897637** two nights before that, so the transition is
dated rather than assumed.

### How `std_msgs` resolves now, which is the actual finding

On the **installed** track — the harder door, the one a book reader with no
checkout uses — the probe scaffolds and then resolves the dependency this issue
was opened about:

```
nros new --workspace: scaffolded my_robot (17 files, lang=cpp, rmw=cyclonedds)
```

`std_msgs` appears 367 times in that job, resolving through `interfaces/std_msgs`
with an IDL root at `/cyclonedds-ts/_idlroot/std_msgs/msg/*.idl` and a vendored
`rosidl/humble-5621b26`. So the release asset carries the message package and the
rosidl toolchain; nothing is reaching for an installed ROS, and nothing is
suppressing the resolution check.

That is effectively **option 1** of this issue's "what would close it" — the
quickstart does not need ROS — arrived at by bundling the interfaces rather than
by scaffolding against a workspace-local `.msg`.

### Nobody did this for this issue

`git log --grep=1357` on main returns exactly **one** commit, the one that FILED
this issue (`6f40985a5`). No fix commit names it. The commits that plausibly did
it belong to issue **1304** — `feat(#1304): an installed nros provisions and
builds with no checkout`, `fix(#1304): the installed probe lost its build step`,
`fix(#1304): the build coordinate carries the RMW it resolves` — and to the 1439
repair, neither of which claimed this one. So this is an open issue whose
acceptance was satisfied as a side effect, and it would have stayed open
indefinitely on the strength of a probe nobody re-read.

### What is NOT established

- **Not that the design question was decided.** This issue asks to "decide which
  of these the quickstart is, and make the scaffolder and the book agree". The
  probe passing shows the scaffolder's dep now resolves; it does not show that
  anyone chose a route or that the book says what it needs. A reader of
  `book/src/getting-started/first-project.md` should confirm it matches what the
  probe exercises before this is closed.
- **Not that the scaffold stopped declaring `std_msgs`.** It still does; what
  changed is that something now provides it. Option 1 as written ("scaffold
  against a message type that resolves with no ROS") was not taken.
- **Not two nights into three.** This issue states no multi-night requirement,
  unlike issue 1353's arms, so the bar is lower — but one more green night would
  rule out a lucky container state, which is cheap to wait for.

Recommended: close on the next green nightly, with a line saying which route the
book now teaches.
