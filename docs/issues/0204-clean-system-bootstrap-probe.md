---
id: 204
title: "No automated clean-system bootstrap verification — book setup steps are never executed on a pristine host"
status: open
type: task
area: ci
related: [rfc-0014, issue-0200]
---

## Problem

The book's bootstrap + per-platform setup instructions (clone → `direnv
allow`/`source ./activate.sh` → `just setup` → platform SDK provisioning per
RFC-0014) are only ever exercised on already-provisioned developer hosts.
Nothing runs them on a clean system, so missing-prereq regressions (a new
implicit host dependency, a setup recipe that assumes prior state, a doc step
that rotted) surface only when a new user hits them.

The `/audit` skill's F3 check (added 2026-07-16) statically cross-reads the
book against `activate.sh` / `justfile` / `nros-sdk-index.toml` — that
catches doc drift but NOT real-world breakage. This issue is the dynamic
half.

## Wanted

A containerized probe (fresh Ubuntu LTS image, no toolchains) that executes
the book's documented setup steps verbatim and asserts the documented
outcome:

1. Bootstrap: clone, activate, `just doctor` green, `just setup-cli`.
2. At least one cheap platform lane end-to-end: e.g. native fixture build +
   one runtime test (`just native build-fixtures` is disk-heavy — pick the
   smallest honest slice, or gate the fat lanes to the #200-class big runner).
3. Fail loud on any undocumented prerequisite (the probe image gets NOTHING
   the book doesn't install).

## Constraints

- Runner-class work: disk/network budget overlaps issue #200's CI-runner
  campaign — likely the same infrastructure.
- Steps must be extracted FROM the book (or the book generated from the
  probe script) so the probe can't drift from what users actually read —
  a probe with its own hand-rolled steps re-creates the F3 drift problem
  one level down.
