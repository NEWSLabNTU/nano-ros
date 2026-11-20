---
id: 2770
title: "`nros pin` moves only a release pin; a commit (`git` + `rev`) or checkout (`path`) pin is edited by hand"
status: open
type: enhancement
severity: low
area: [cli]
related: [issue-2645, issue-2771, phase-478, rfc-0097]
found: 2026-11-20
---

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md) I34.**
Follow-up of issue 2645 (PR #4451), which built the commit and checkout pin
forms but not the command that writes them.

## What is missing

`nros-toolchain.toml` takes one of `version`, `git` + `rev` (a full 40-hex
sha) or `path` (issue 2645). `nros pin <version>` (`packages/cli/nros-cli-core/src/cmd/pin.rs`)
writes only the first. Moving a consumer that tracks main to a newer commit
means editing the file by hand, and nothing checks the sha before the next
launcher run fetches it.

## Wanted

As 2645's design states:

- `nros pin --rev <sha> [--git <url>]` writes `git` + `rev`, refusing a short
  sha, a branch or a tag the way the launcher does, and defaulting `git` to
  the nano-ros repository when the file has none.
- `nros pin --path <dir>` writes `path`, relative to the pin file, refusing a
  directory that is not a nano-ros checkout root.
- Each form replaces the other two, so the file always holds exactly one.
- In CI the command refuses rather than writes (RFC-0097 D11, as 2645's
  design states).

## Acceptance

Unit tests for each form and each refusal, and the pin page
(`book/src/guide/build/pin-a-commit.md`) shows the command instead of a
hand edit.
