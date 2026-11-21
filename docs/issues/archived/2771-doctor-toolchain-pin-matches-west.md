---
id: 2771
title: "`nros doctor` does not report a commit pin that disagrees with west.yml's nano-ros revision, so a Zephyr consumer keeps its own lockstep script"
status: resolved
resolved: 2026-11-21
type: enhancement
severity: low
area: [cli, zephyr]
related: [issue-2645, issue-2770, phase-478]
found: 2026-11-20
---

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md) I35.**
Follow-up of issue 2645 (PR #4451): its design has `nros doctor` report a
toolchain pin and a west revision that disagree, and that check was not built.

## What happens

A Zephyr consumer names nano-ros twice: the toolchain pin in
`nros-toolchain.toml` (now `git` + `rev` or `path`, issue 2645) and the
`nano-ros` project's `revision` in its `west.yml`, which west fetches as a
module. Nothing in nano-ros compares them. The island keeps a script of its
own (`assert_pin_lockstep` in its Zephyr FVP CI script) that fails CI when its
`west.yml` revision and its nano-ros submodule disagree; with a commit pin it
would have to compare `rev` as well.

## Wanted

`nros doctor`, in a project with both a `git` + `rev` pin and a `west.yml`
that lists nano-ros, reports a mismatch as an error naming both files and
both shas. For a `path` pin it compares the checkout's HEAD. A `version` pin
compares against the release's commit if the index records one, and is
skipped otherwise. West stays the owner of its own pin; doctor only reports.

## Acceptance

Doctor tests for agree, disagree, a short sha in west (prefix match), and no
west.yml. The island's lockstep script can then go.

## Fix (2026-11-21)

`nros doctor`'s install report (the project-side report, run where no
nano-ros checkout is the workspace) finds the `west.yml` nearest the working
directory and reads its `nano-ros` project's revision (`defaults: revision`
when the project has none, and west's own `master` with neither). It
compares that with the pin, in `packages/cli/nros-cli-core/src/cmd/doctor_west_pin.rs`:

- `git` + `rev`: the pin's commit. `path`: the checkout's `git rev-parse
  HEAD`. Either way a west commit agrees by prefix, so a short sha matches.
  A disagreement is `[!!]`, counted, and names both files and both commits.
- `version`: the index records no commit for a release, so it is reported
  (`[--]`) and not compared.
- A west revision that is a branch or a tag cannot be compared with a commit
  without fetching, so it is reported and not counted.
- No `west.yml`, or one with no nano-ros project: nothing is printed.

Tests: the agree, disagree, short-sha, no-west.yml cases from the acceptance,
plus a path pin, a release pin, a branch revision, a manifest default and a
manifest found from a subdirectory (`doctor_west_pin::tests`). One
install-report case
(`a_west_module_disagreeing_with_the_pin_is_counted_in_the_report`) pins
that the report asks at all. The island's lockstep script can now go; that
is the consumer's change.
