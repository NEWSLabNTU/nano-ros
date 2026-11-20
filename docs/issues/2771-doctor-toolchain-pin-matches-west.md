---
id: 2771
title: "`nros doctor` does not report a commit pin that disagrees with west.yml's nano-ros revision, so a Zephyr consumer keeps its own lockstep script"
status: open
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
