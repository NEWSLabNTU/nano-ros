---
id: 1359
title: "All 22 nightly `zephyr` jobs die in `setup`, not in Zephyr provisioning: the
  container has no `unzip`, so `nros setup --tool clang-format` cannot unpack its
  prebuilt — and the step NAME says `Set up Zephyr … workspace`, which is how this got
  read as issue 1158 for days"
status: resolved
type: bug
area: ci, tooling
severity: high
resolved: 2026-09-24
found: 2026-09-13
related: [1158, 0368, 1353]
---

## What happens

Nightly run **34739657223** (schedule, 2026-09-13T05:10) has 23 failing jobs. Twenty-two
of them are the `zephyr *` matrix, every one failing at the step named
`Set up Zephyr 3.7 workspace` / `Set up Zephyr 4.4 workspace`. The step name is the
only thing about them that concerns Zephyr. Job **103677080644**
(`zephyr 3.7 / rust/talker`) is representative:

```
nros setup --tool clang-format: prebuilt 17.0.6-nros1 (dist linux-x86_64) → /github/home/.nros/sdk/clang-format/17.0.6-nros1
  [FAILED]  clang-format — not installed — needs system package(s) this host is missing: unzip
Error: 1 package(s) failed to install (see [FAILED] above)
Location:
    nros-cli-core/src/cmd/setup/session.rs:837:23
error: recipe `setup-clang-format` failed on line 3474 with exit code 1
error: recipe `_setup-common` failed with exit code 1
error: recipe `setup` failed with exit code 1
```

`setup-clang-format` runs inside `_setup-common`, which the Zephyr setup step invokes
before it touches a Zephyr workspace. So the lane never reaches Zephyr at all: the
container image these jobs run in has no `unzip`, and the clang-format prebuilt is a
zip.

## Why it matters, and what it cost

The whole Zephyr half of the nightly — 3.7 and 4.4, C, C++ and Rust, talker, listener,
service client/server, action client/server — reports `failure` every night for a
missing system package. Nothing about the code under test is exercised. Worse, the
RTOS jobs downstream of it do not run either: in this same run `bootstrap-probe` and
`installed-probe` are `skipped`, and no `threadx_*` / `nuttx` / `freertos` / `esp32`
job appears at all.

**This was misattributed for several days, and the step name is why.** The MAIN-HEALTH
runbook lists "nightly `zephyr *` jobs failing at `Set up Zephyr 3.7/4.4 workspace`" as
the provisioning class of issue 1158, with an instruction to verify the step name still
matches before attributing. The step name matched every time; the error underneath it
did not. That is the uniformly-red-lane hazard exactly — the lane had no signal
capacity, so a second, different cause read as the first one.

## What this is NOT

- **Not issue 1158.** That is tier 2 / `run-matrix` not reaching its cells, and the
  stage axis `matrix-triage` reports. This is `_setup-common` failing before any
  Zephyr work begins, in a different workflow.
- **Not issue 1353.** No `No space left on device`, no truncated log; this job fails
  in seconds with a complete diagnostic that names the missing package.
- **Not a nano-ros regression.** `nros setup` behaves correctly: it detects the missing
  prereq, names it, prints the exact `apt-get install` line, and refuses rather than
  half-installing. Issue 0368 is the archived case that made it behave that way.

## What would close it

The provisioner already prints the remedy it wants; the question is only where that
belongs, and that is a decision rather than a guess:

1. **The container image** — add `unzip` to the image these nightly jobs run in. Right
   if the image is meant to satisfy `_setup-common`'s declared prereqs.
2. **The workflow** — an apt step before `just setup`, the way other lanes provision
   their own prereqs. Right if the image is meant to stay minimal.
3. **The provisioner** — ship the clang-format dist as a tarball, or unpack the zip
   with something already present. Right if "a prebuilt tool must not need a system
   package to unpack" is the rule we want; that is the widest fix and the only one
   that also helps a user on a bare host.

Acceptance is the `zephyr 3.7 / rust/talker` nightly job reaching a Zephyr build —
green or red on its own cell — and the RTOS jobs downstream of it running at all.

## What landed (phase-466, 2026-09-23)

**Option 1, widened into option 1+2 of the class.** Adding `unzip` to the zephyr
image alone would have been the fix-at-the-reported-site the repo has paid for six
times, and issue 1364 is the proof: the SAME two Dockerfiles had drifted by a
SECOND package, in the same direction, and nobody had connected them.

So the defect treated is "two hand-written apt lists must agree and nothing makes
them agree", not "this image lacks unzip":

- **`ci/docker/apt-packages.txt`** is now the one apt closure every CI image
  installs. Both Dockerfiles `COPY` it and pipe it through
  `xargs apt-get install`; `unzip` and `python3-tomli` are in it. For everything
  named there, drift is not detected — it is unrepresentable.
- `images.yml` builds the zephyr image with the repository root as its context
  (`context: .` + `file:`), the way it has built ci-base since [[issue-1201]],
  with a matching `Dockerfile.dockerignore`, because a `COPY` of a repository
  path against a Dockerfile-dir context fails at "failed to calculate checksum of
  ref: not found" every time.
- **`check-ci-image-apt-packages`** (fast lane, 0.14 s) refuses: an image that
  stops consuming the shared list, an image that restates a shared package in its
  own list, a shared list that does not cover what `[tool.clang-format] system` +
  `[prereq.*].apt` demand (this issue, derived from the index rather than
  asserted), a shared list with no TOML parser while the tree still uses the
  `import tomllib` -> `import tomli` chain ([[issue-1364]]), and a published image
  TAG that its `container:` consumers do not spell. It enumerates
  `ci/docker/*/Dockerfile` by GLOB, so a third image is covered without editing
  the gate.

**And the COUPLING, which is what turned one package into 22 dead jobs.**
`_setup-common` ran `just setup-clang-format` under `set -e`. That recipe is the
prelude EVERY `just setup <scope>` takes, so a code formatter held a veto over a
cross-compile: none of the 22 jobs formats anything. Provisioning there is
best-effort now (the spelling the TIER arm of `setup` already used — there were
two call sites of one step, one fatal and one not, and the fatal one was the one
CI took), and the ASSERTION moved to the consumers that need the binary:
`check-tier-preconditions` reports it in the batch at the head of `just ci`,
beside the other unmet preconditions, and `c-fmt`/`cpp-fmt` fail on it inside
`check fast`. A lane that needs clang-format still hears in the first minute; a
lane that does not is no longer answerable for it.

## What is NOT done, and why this stays open

**A Dockerfile edit publishes nothing.** The image is built only by `images.yml`
on a push to `main` touching these paths, and the tag consumers pin moved to
`humble-sdk0.17.4-r5` — which does not exist in the registry until that workflow
has run. Acceptance is unchanged and still remote: the `zephyr 3.7 / rust/talker`
nightly job reaching a Zephyr build, and the RTOS jobs downstream of it running at
all. Close this when a scheduled run shows that, not when the merge lands.

## RESOLVED 2026-09-24 — acceptance met, measured on two scheduled runs

Both clauses of the acceptance above are satisfied by the first nightly runs
after the image republished (`images.yml` run 35843238854, 2026-09-23 09:29Z,
which fired on the merge push and needed no dispatch).

**Clause 1 — `zephyr 3.7 / rust/talker` reaching a Zephyr build.** Nightly run
**35958890602** (05:12Z): `zephyr 3.7 / rust/talker` **success**, and so is
`zephyr 4.4 / rust/talker`. Across that run, **22 of 23 zephyr jobs pass** and
**zero** fail in `Set up Zephyr 3.7/4.4 workspace` — the step that killed all of
them for twelve days. The single failure is `zephyr copy-out check (4.4)` at
`Copy-out build check`, which is a verdict about the code rather than about the
lane, and is what this issue existed to make possible.

**Clause 2 — the downstream jobs running at all.** Nightly run **35968523188**
(07:14Z, the `0 7 * * *` cron that gates them): `installed-probe` **success**,
`bootstrap-probe` **runs and fails at its own `Run bootstrap probe` step**. Both
were unreachable before. Note for anyone re-checking: their `skipped` status in
the 05:12 run is correct and not a symptom — they are gated on the 07:00
schedule.

**What fixed it** (PR #1211, phase-466 W1): `unzip` was absent from the Zephyr
image, so `setup-clang-format` failed and took `_setup-common` with it. The fix
is not the package — it is one shared `ci/docker/apt-packages.txt` both
Dockerfiles COPY, so the drift that caused this is unrepresentable rather than
merely detected, plus moving the clang-format assertion out of the provisioning
path into `check-tier-preconditions`, where a caller has said it is about to run
a tier.

The lane is still RED overall, and that is the point worth recording: 22 jobs
went from dead to passing underneath an unchanged red. Read the failing step,
never the colour.
